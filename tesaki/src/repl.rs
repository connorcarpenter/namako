//! Interactive REPL session for Tesaki v1.8.

use anyhow::{Context, Result};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::allowlist::{validate_command, AllowedTool};
use crate::chat_planner::{CmdChatPlanner, MockChatPlanner};
use crate::config::{self, ConfigDiscoveryResult};
use crate::packet_parser::{parse_gate_json, parse_review_json, parse_status_json};
use crate::repo_state::RepoState;
use crate::runner::{ChatPlan, ChatPlanner, ChatTurnInput, CommandResult, MissionProposal};
use crate::session::{PendingMission, SessionState};
use crate::stage::{detect_stage, Stage, StageConstraint};
use crate::surface_policy::{SurfaceLock, SurfacePolicy};

const MAX_TURN_STEPS: usize = 5;

pub fn run_repl(start_dir: PathBuf) -> Result<()> {
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
    let namako_cmd = crate::resolve_namako_cli(None, &spec_root);

    let planner: Box<dyn ChatPlanner> = match config.planner.as_deref().unwrap_or("mock") {
        "mock" => Box::new(MockChatPlanner::new(ChatPlan {
            say: "Planner not configured. Ask a question or request a mission.".to_string(),
            run: vec![],
            mission_proposal: None,
            done: true,
        })),
        "cmd" => {
            let cmd = config.planner_cmd.clone()
                .ok_or_else(|| anyhow::anyhow!("planner_cmd required when planner = \"cmd\""))?;
            Box::new(CmdChatPlanner::new(cmd, spec_root.clone()))
        }
        other => anyhow::bail!("Unsupported planner backend: {}", other),
    };

    let mut session = SessionState::default();
    refresh_repo_state(&spec_root, &adapter, &namako_cmd, &mut session)?;

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

            let plan = match planner.plan_turn(&planner_input) {
                Ok(plan) => plan,
                Err(err) => {
                    if planner_hint.is_none() {
                        planner_hint = Some("Return ONLY valid JSON matching the ChatPlan schema.".to_string());
                        println!("Planner returned invalid response; retrying with strict JSON requirement.");
                        continue;
                    }
                    println!("Planner failed to return valid JSON: {}", err);
                    break;
                }
            };
            print_plan(&plan);

            if let Some(proposal) = handle_mission_proposal(&plan, &mut session) {
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
                let result = execute_allowed_command(
                    &command,
                    &spec_root,
                    &adapter,
                    &namako_cmd,
                )?;
                recent_results.push(result);
            }

            if plan.done {
                break;
            }
        }
    }

    Ok(())
}

fn refresh_repo_state(spec_root: &Path, adapter: &str, namako_cmd: &str, session: &mut SessionState) -> Result<()> {
    let gate_json = crate::run_namako_gate_json(namako_cmd, adapter, &spec_root.to_path_buf())?;
    let gate_packet = parse_gate_json(&gate_json)?;

    let out_dir = spec_root.join("target/namako_artifacts/tesaki");
    std::fs::create_dir_all(&out_dir)?;
    let status_path = out_dir.join("status.json");
    let review_path = out_dir.join("review.json");

    crate::run_namako_status(namako_cmd, adapter, &spec_root.to_path_buf(), &status_path, None)?;
    crate::run_namako_review(namako_cmd, adapter, &spec_root.to_path_buf(), &review_path)?;

    let status_json = std::fs::read_to_string(&status_path)?;
    let review_json = std::fs::read_to_string(&review_path)?;

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
        Some(namako_cmd.to_string()),
        max_cert_updates,
        runner_name,
        runner_cmd,
        max_runtime_seconds,
        max_files_changed,
        max_retries,
        "CONSUMPTION",
        Some(stage_to_arg(stage)),
        None,
        constraint.surface_overrides,
        None,
    )?;

    refresh_repo_state(spec_root, adapter, namako_cmd, session)?;
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

fn convert_surface_policy(input: &crate::runner::SurfacePolicy) -> SurfacePolicy {
    SurfacePolicy {
        spec: convert_lock(input.spec),
        tests_bindings: convert_lock(input.tests),
        sut: convert_lock(input.sut),
    }
}

fn convert_lock(lock: crate::runner::SurfaceLock) -> SurfaceLock {
    match lock {
        crate::runner::SurfaceLock::Locked => SurfaceLock::Locked,
        crate::runner::SurfaceLock::Unlocked => SurfaceLock::Unlocked,
    }
}

fn print_plan(plan: &ChatPlan) {
    if !plan.say.trim().is_empty() {
        println!("{}", plan.say.trim());
    }
    for cmd in &plan.run {
        if let Some(reason) = &cmd.reason {
            println!("> Running {} {} ({})", cmd.tool, cmd.args.join(" "), reason);
        } else {
            println!("> Running {} {}", cmd.tool, cmd.args.join(" "));
        }
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
    use crate::runner::{AllowedCommand, ChatPlan, MissionProposal, SurfaceLock};

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
                surfaces: crate::runner::SurfacePolicy {
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
    command: &crate::runner::AllowedCommand,
    spec_root: &Path,
    adapter: &str,
    namako_cmd: &str,
) -> Result<CommandResult> {
    let tool = validate_command(command)?;
    let (program, base_args) = match tool {
        AllowedTool::Namako => split_command(namako_cmd)?,
        AllowedTool::Tesaki => {
            let exe = std::env::current_exe().context("Failed to locate tesaki binary")?;
            (exe.to_string_lossy().to_string(), vec![])
        }
    };

    let mut args = base_args;
    args.extend(command.args.clone());

    if tool == AllowedTool::Namako {
        args = augment_namako_args(args, adapter);
    }

    let output = Command::new(&program)
        .args(&args)
        .current_dir(spec_root)
        .output()
        .context("Failed to execute allowlisted command")?;

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
