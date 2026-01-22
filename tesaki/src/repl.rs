//! Interactive REPL session for Tesaki v1.8.

use anyhow::{Context, Result};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use crate::allowlist::validate_command;
use crate::chat_plan::{
    AllowedCommand, ChatPlan, ChatTurnInput, CommandResult, MissionProposal,
    SurfaceLock as PlanSurfaceLock, SurfacePolicy as PlanSurfacePolicy,
};
use crate::chat_planner::{ChatPlanner, MockChatPlanner};
use crate::claude_code_agent::ClaudeCodeAgent;
use crate::config::{self, ConfigDiscoveryResult};
use crate::codex_agent::CodexAgent;
use crate::packet_parser::{parse_gate_json, parse_review_json, parse_status_json};
use crate::repo_state::RepoState;
use crate::session::{PendingMission, SessionState};
use crate::stage::{detect_stage, Stage, StageConstraint};
use crate::surface_policy::{SurfaceLock as RepoSurfaceLock, SurfacePolicy as RepoSurfacePolicy};

const MAX_TURN_STEPS: usize = 5;
const DEFAULT_PLANNER_TIMEOUT_SECONDS: u64 = 60;

pub fn run_repl(start_dir: PathBuf, logger: &crate::logging::JsonlLogger) -> Result<()> {
    let config = match config::discover_config(&start_dir)? {
        ConfigDiscoveryResult::Found(config) => config,
        ConfigDiscoveryResult::NotFound { .. } => {
            config::print_config_error();
            anyhow::bail!("No .tesaki/config.toml found");
        }
    };

    let spec_root = config.specs_dir.clone();
    let adapter = config.adapter_cmd.clone();
    let runner_name = config.runner.clone().unwrap_or_else(|| "mock".to_string());
    let runner_cmd = config.runner_cmd.clone();
    let max_retries = config.max_retries.unwrap_or(0);
    let max_cert_updates = config.max_cert_updates.unwrap_or(0);
    let max_runtime_seconds = config.max_runtime_seconds.unwrap_or(600) as u32;
    let max_files_changed = config.max_files_changed.unwrap_or(10) as u32;
    let namako_resolution = crate::resolve_namako_cli(None, config.namako_cli.clone(), &spec_root);
    let namako_cmd = namako_resolution.command.clone();

    crate::log_session_start(
        logger,
        &spec_root,
        &adapter,
        &namako_resolution,
        Some(&runner_name),
        config.planner.as_deref(),
    );

    let planner_name = config.planner.as_deref().unwrap_or("mock");
    if config.planner.is_none() {
        println!(
            "Planner not configured. Set `planner = \"mock\" | \"codex\" | \"claude\"` in .tesaki/config.toml."
        );
    } else if planner_name == "mock" {
        println!(
            "Planner is set to \"mock\". To enable interactive planning, set `planner = \"codex\" | \"claude\"` in .tesaki/config.toml."
        );
    }

    let planner: Box<dyn ChatPlanner> = match planner_name {
        "mock" => Box::new(MockChatPlanner::new(ChatPlan {
            say: "Planner not configured. Set planner in .tesaki/config.toml.".to_string(),
            run: vec![],
            mission_proposal: None,
            done: true,
        })),
        "codex" => {
            if let Err(err) = CodexAgent::check_available() {
                anyhow::bail!("Codex planner unavailable: {}", err);
            }
            Box::new(CodexAgent::new_with_timeout_and_stream(
                config.runner_cmd.clone(),
                config.planner_cmd.clone(),
                spec_root.clone(),
                Some(std::time::Duration::from_secs(DEFAULT_PLANNER_TIMEOUT_SECONDS)),
                true,
            )?)
        }
        "claude" => {
            if let Err(err) = ClaudeCodeAgent::check_available() {
                anyhow::bail!("Claude planner unavailable: {}", err);
            }
            Box::new(ClaudeCodeAgent::new_with_timeout_and_stream(
                config.runner_cmd.clone(),
                config.planner_cmd.clone(),
                spec_root.clone(),
                Some(std::time::Duration::from_secs(DEFAULT_PLANNER_TIMEOUT_SECONDS)),
                true,
            )?)
        }
        other => anyhow::bail!("Unsupported planner backend: {}", other),
    };

    let mut session = SessionState::default();
    refresh_repo_state(&spec_root, &adapter, &namako_cmd, &mut session, logger)?;

    println!("Tesaki v1.8 REPL");
    println!("Spec root: {}", spec_root.display());
    if let Some(summary) = &session.last_repo_state_summary {
        println!("RepoState: {}", summary);
    }
    if let Some(summary) = &session.chat_summary {
        println!("{}", summary);
    }
    if let Some(stage) = session.intent.stage {
        println!("Stage: {}", stage.name());
    }
    println!("Type 'exit' to quit.");

    let mut input = String::new();
    loop {
        input.clear();
        print!("> ");
        io::stdout().flush()?;
        if io::stdin().read_line(&mut input)? == 0 {
            break;
        }
        let line = input.trim();
        if line.is_empty() {
            continue;
        }
        if matches!(line, "exit" | "quit") {
            break;
        }

        if handle_mission_approval(
            line,
            &spec_root,
            &adapter,
            &namako_cmd,
            &runner_name,
            runner_cmd.clone(),
            max_retries,
            max_cert_updates,
            max_runtime_seconds,
            max_files_changed,
            &mut session,
            logger,
        )? {
            continue;
        }

        session.intent.apply_user_message(line);

        let mut recent_results: Vec<CommandResult> = Vec::new();
        let mut attempts = 0;
        let mut planner_hint: Option<String> = None;
        loop {
            if attempts >= MAX_TURN_STEPS {
                println!("Planner exceeded max iterations for this turn.");
                break;
            }
            attempts += 1;

            let planner_input = ChatTurnInput {
                user_message: line.to_string(),
                session_state_json: serde_json::to_value(&session)?,
                recent_command_results: recent_results.clone(),
                planner_hint: planner_hint.clone(),
            };

            let simulate_bad_plan = std::env::var("TESAKI_SIMULATE_BAD_PLAN")
                .ok()
                .as_deref()
                == Some("1");
            println!("Planner ({}) running...", planner.name());
            let plan_start = Instant::now();
            let plan_result = if simulate_bad_plan {
                Err(anyhow::anyhow!("Simulated invalid JSON from planner"))
            } else {
                planner.plan_turn(&planner_input)
            };
            println!(
                "Planner ({}) finished in {:.1}s",
                planner.name(),
                plan_start.elapsed().as_secs_f64()
            );

            let plan = match plan_result {
                Ok(plan) => plan,
                Err(err) => {
                    logger.log_event(crate::logging::LogEvent::PlannerPlan {
                        parse_status: "invalid".to_string(),
                        plan_json: None,
                        error: Some(err.to_string()),
                    });
                    if planner_hint.is_none() {
                        planner_hint = Some("Return ONLY valid JSON matching the ChatPlan schema.".to_string());
                        println!("Planner returned invalid response; retrying with strict JSON requirement.");
                        continue;
                    }
                    let details = format!("Planner failed to return valid JSON after retry: {}", err);
                    logger.log_event(crate::logging::LogEvent::SessionEnd {
                        stop_reason: "FAILED".to_string(),
                        details: Some(details.clone()),
                    });
                    println!("FAILED: {}", details);
                    return Err(anyhow::anyhow!(details));
                }
            };
            if let Ok(plan_json) = serde_json::to_string(&plan) {
                logger.log_event(crate::logging::LogEvent::PlannerPlan {
                    parse_status: "ok".to_string(),
                    plan_json: Some(plan_json),
                    error: None,
                });
            }
            print_plan(&plan);

            if let Some(proposal) = handle_mission_proposal(&plan, &mut session) {
                logger.log_event(crate::logging::LogEvent::MissionProposed {
                    mission_type: proposal.mission_type.clone(),
                    stage: proposal.stage.clone(),
                    target: proposal.target.clone(),
                    surfaces: crate::logging::SurfaceLog {
                        spec: format!("{:?}", proposal.surfaces.spec),
                        tests: format!("{:?}", proposal.surfaces.tests),
                        sut: format!("{:?}", proposal.surfaces.sut),
                    },
                });
                show_mission_proposal(&proposal);
            }

            if plan.run.is_empty() {
                if !plan.done {
                    println!("Planner did not provide commands but marked done=false.");
                }
                break;
            }

            recent_results.clear();
            for command in plan.run {
                match execute_allowed_command(
                    &command,
                    &spec_root,
                    &adapter,
                    &namako_cmd,
                    logger,
                ) {
                    Ok(result) => recent_results.push(result),
                    Err(_) => {}
                }
            }

            if plan.done {
                break;
            }
        }
    }

    logger.log_event(crate::logging::LogEvent::SessionEnd {
        stop_reason: "DONE".to_string(),
        details: None,
    });
    Ok(())
}

fn refresh_repo_state(
    spec_root: &Path,
    adapter: &str,
    namako_cmd: &str,
    session: &mut SessionState,
    logger: &crate::logging::JsonlLogger,
) -> Result<()> {
    let gate_json = crate::run_namako_gate_json(namako_cmd, adapter, &spec_root.to_path_buf(), logger)?;
    let gate_packet = parse_gate_json(&gate_json)?;

    let status_json =
        crate::run_namako_status(namako_cmd, adapter, &spec_root.to_path_buf(), None, logger)?;
    let review_json =
        crate::run_namako_review(namako_cmd, adapter, &spec_root.to_path_buf(), logger)?;

    let status_packet = parse_status_json(&status_json)?;
    let review_packet = parse_review_json(&review_json)?;
    let state = RepoState::compute(&status_packet, &review_packet, &gate_packet, None)?;

    session.last_repo_state_summary = Some(state.summary());
    session.chat_summary = Some(format!(
        "Propagation: {}",
        state.propagation_summary().to_line()
    ));
    session.intent.stage = Some(detect_stage(&state));
    session.last_packets_fingerprint = Some(fingerprint_packets(&status_json, &review_json, &gate_json));

    Ok(())
}

fn fingerprint_packets(status: &str, review: &str, gate: &str) -> String {
    let mut data = String::new();
    data.push_str(status);
    data.push_str(review);
    data.push_str(gate);
    let hash = blake3::hash(data.as_bytes());
    hash.to_hex().to_string()
}

fn handle_mission_approval(
    line: &str,
    spec_root: &Path,
    adapter: &str,
    namako_cmd: &str,
    runner_name: &str,
    runner_cmd: Option<String>,
    max_retries: u32,
    max_cert_updates: u32,
    max_runtime_seconds: u32,
    max_files_changed: u32,
    session: &mut SessionState,
    logger: &crate::logging::JsonlLogger,
) -> Result<bool> {
    let approve = matches!(line, "run it" | "execute" | "go" | "run");
    let cancel = matches!(line, "cancel" | "skip" | "no");

    if cancel {
        session.pending_mission = None;
        println!("Mission proposal cleared.");
        return Ok(true);
    }

    if !approve {
        return Ok(false);
    }

    let pending = match session.pending_mission.take() {
        Some(pending) => pending,
        None => {
            println!("No mission proposal pending.");
            return Ok(true);
        }
    };

    execute_proposed_mission(
        spec_root,
        adapter,
        namako_cmd,
        runner_name,
        runner_cmd,
        max_retries,
        max_cert_updates,
        max_runtime_seconds,
        max_files_changed,
        &pending.proposal,
        session,
        logger,
    )?;
    Ok(true)
}

fn execute_proposed_mission(
    spec_root: &Path,
    adapter: &str,
    namako_cmd: &str,
    runner_name: &str,
    runner_cmd: Option<String>,
    max_retries: u32,
    max_cert_updates: u32,
    max_runtime_seconds: u32,
    max_files_changed: u32,
    proposal: &MissionProposal,
    session: &mut SessionState,
    logger: &crate::logging::JsonlLogger,
) -> Result<()> {
    println!("Executing mission...");

    let stage = stage_from_label(&proposal.stage)
        .or(session.intent.stage)
        .unwrap_or(Stage::ImplementTests);

    let surface_overrides = Some(convert_surface_policy(&proposal.surfaces));
    let constraint = StageConstraint {
        stage: Some(stage),
        surface_overrides,
    };

    crate::run_run(
        &spec_root.to_path_buf(),
        adapter,
        crate::NamakoCliResolution::explicit(namako_cmd.to_string()),
        max_cert_updates,
        runner_name,
        runner_cmd,
        max_runtime_seconds,
        max_files_changed,
        max_retries,
        None,  // model - use default
        false, // stream_output - don't stream in REPL context (planner is interactive)
        Some(stage_to_arg(stage)),
        None,
        constraint.surface_overrides,
        logger,
    )?;

    refresh_repo_state(spec_root, adapter, namako_cmd, session, logger)?;
    if let Some(summary) = &session.last_repo_state_summary {
        println!("RepoState: {}", summary);
    }
    if let Some(summary) = &session.chat_summary {
        println!("{}", summary);
    }

    Ok(())
}

fn stage_from_label(label: &str) -> Option<Stage> {
    let value = label.to_ascii_lowercase();
    if value.contains("refine") {
        Some(Stage::RefineSpec)
    } else if value.contains("structure") {
        Some(Stage::StructureSpec)
    } else if value.contains("tests") || value.contains("bindings") {
        Some(Stage::ImplementTests)
    } else if value.contains("sut") {
        Some(Stage::ImplementSut)
    } else if value.contains("finalize") {
        Some(Stage::Finalize)
    } else {
        None
    }
}

fn stage_to_arg(stage: Stage) -> String {
    match stage {
        Stage::RefineSpec => "refine",
        Stage::StructureSpec => "structure",
        Stage::ImplementTests => "tests",
        Stage::ImplementSut => "sut",
        Stage::Finalize => "finalize",
    }
    .to_string()
}

fn convert_surface_policy(input: &PlanSurfacePolicy) -> RepoSurfacePolicy {
    RepoSurfacePolicy {
        spec: convert_lock(input.spec),
        tests_bindings: convert_lock(input.tests),
        sut: convert_lock(input.sut),
    }
}

fn convert_lock(lock: PlanSurfaceLock) -> RepoSurfaceLock {
    match lock {
        PlanSurfaceLock::Locked => RepoSurfaceLock::Locked,
        PlanSurfaceLock::Unlocked => RepoSurfaceLock::Unlocked,
    }
}

fn print_plan(plan: &ChatPlan) {
    if !plan.say.trim().is_empty() {
        println!("{}", plan.say.trim());
    }
}

fn show_mission_proposal(proposal: &MissionProposal) {
    println!();
    println!("MISSION PROPOSAL");
    println!("Type: {}", proposal.mission_type);
    println!("Stage: {}", proposal.stage);
    println!("Target: {}", proposal.target);
    println!(
        "Surfaces: Spec {:?} • Tests {:?} • SUT {:?}",
        proposal.surfaces.spec, proposal.surfaces.tests, proposal.surfaces.sut
    );
    println!("Objective: {}", proposal.objective);
    if !proposal.validation.is_empty() {
        println!("Validation:");
        for (idx, item) in proposal.validation.iter().enumerate() {
            println!("  {}. {}", idx + 1, item);
        }
    }
    println!("Say \"run it\" to execute, or ask questions.");
    println!();
}

fn handle_mission_proposal(plan: &ChatPlan, session: &mut SessionState) -> Option<MissionProposal> {
    let proposal = plan.mission_proposal.clone()?;
    session.pending_mission = Some(PendingMission {
        proposal: proposal.clone(),
        approved: false,
    });
    Some(proposal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_plan::{AllowedCommand, ChatPlan, MissionProposal, SurfaceLock, SurfacePolicy};

    #[test]
    fn repl_sets_pending_mission_from_plan() {
        let mut session = SessionState::default();
        let plan = ChatPlan {
            say: "test".to_string(),
            run: vec![AllowedCommand {
                tool: "namako".to_string(),
                args: vec!["status".to_string(), "--json".to_string()],
                reason: None,
            }],
            mission_proposal: Some(MissionProposal {
                mission_type: "CreateMissingBindings".to_string(),
                stage: "Implement Tests".to_string(),
                target: "@Scenario(01)".to_string(),
                surfaces: SurfacePolicy {
                    spec: SurfaceLock::Locked,
                    tests: SurfaceLock::Unlocked,
                    sut: SurfaceLock::Locked,
                },
                objective: "Create bindings".to_string(),
                validation: vec![],
            }),
            done: true,
        };

        let proposal = handle_mission_proposal(&plan, &mut session).unwrap();
        assert_eq!(proposal.mission_type, "CreateMissingBindings");
        assert!(session.pending_mission.is_some());
    }
}

fn execute_allowed_command(
    command: &AllowedCommand,
    spec_root: &Path,
    adapter: &str,
    namako_cmd: &str,
    logger: &crate::logging::JsonlLogger,
) -> Result<CommandResult> {
    if let Err(err) = validate_command(command) {
        logger.log_event(crate::logging::LogEvent::AllowlistReject {
            tool: command.tool.clone(),
            args: command.args.clone(),
            reason: err.to_string(),
        });
        return Err(err);
    }

    let (program, base_args) = split_command(namako_cmd)?;
    let mut args = base_args;
    args.extend(command.args.clone());
    args = augment_namako_args(args, adapter);

    crate::log_command_run(logger, &command.tool, &command.args, spec_root, None);
    let output = Command::new(&program)
        .args(&args)
        .current_dir(spec_root)
        .output()
        .context("Failed to execute allowlisted command")?;
    crate::log_command_result(logger, &command.tool, &command.args, &output);

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    Ok(CommandResult {
        tool: command.tool.clone(),
        args: command.args.clone(),
        exit_code,
        stdout,
        stderr,
    })
}

fn split_command(command: &str) -> Result<(String, Vec<String>)> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        anyhow::bail!("Empty command");
    }
    Ok((parts[0].to_string(), parts[1..].iter().map(|s| s.to_string()).collect()))
}

fn augment_namako_args(mut args: Vec<String>, adapter: &str) -> Vec<String> {
    let has_spec = args.iter().any(|arg| arg == "-s" || arg == "--specs");
    let has_adapter = args.iter().any(|arg| arg == "-a" || arg == "--adapter");

    if !has_spec {
        args.push("-s".to_string());
        args.push(".".to_string());
    }
    if !has_adapter {
        args.push("-a".to_string());
        args.push(adapter.to_string());
    }

    args
}
