//! Interactive REPL session for Tesaki v1.8.

use anyhow::Result;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::chat_plan::{
    ChatPlan, ChatTurnInput, MissionProposal,
    SurfaceLock as PlanSurfaceLock, SurfacePolicy as PlanSurfacePolicy,
};
use crate::chat_planner::{ChatPlanner, MockChatPlanner};
use crate::claude_code_agent::ClaudeCodeAgent;
use crate::codex_agent::CodexAgent;
use crate::copilot_agent::CopilotAgent;
use crate::config::{self, ConfigDiscoveryResult};
use crate::packet_parser::{parse_gate_json, parse_review_json, parse_status_json};
use crate::repo_state::RepoState;
use crate::session::{PendingMission, SessionState};
use crate::stage::{detect_stage, Stage, StageConstraint};
use crate::surface_policy::{SurfaceLock as RepoSurfaceLock, SurfacePolicy as RepoSurfacePolicy};
use crate::token_usage::{MissionTokenStats, TokenUsage};

const DEFAULT_PLANNER_TIMEOUT_SECONDS: u64 = 60;

/// Run the autonomous loop directly without REPL (headless mode).
/// Usage: `tesaki --loop 10` or `tesaki -l 10`
pub fn run_loop_headless(start_dir: PathBuf, max_iterations: u32, logger: &crate::logging::JsonlLogger) -> Result<()> {
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
        None,
    );

    println!("Tesaki v1.8 Autonomous Mode");
    println!("Spec root: {}", spec_root.display());
    println!("Runner: {}", runner_name);
    println!();

    // Initialize session state
    let mut session = SessionState::default();
    refresh_repo_state(&spec_root, &adapter, &namako_cmd, &mut session, logger)?;

    // Run the autonomous loop
    run_autonomous_loop(
        max_iterations,
        &spec_root,
        &adapter,
        &namako_cmd,
        &runner_name,
        runner_cmd,
        max_retries,
        max_cert_updates,
        max_runtime_seconds,
        max_files_changed,
        &mut session,
        logger,
    )
}

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
            "Planner not configured. Set `planner = \"mock\" | \"codex\" | \"claude\" | \"copilot\"` in .tesaki/config.toml."
        );
    } else if planner_name == "mock" {
        println!(
            "Planner is set to \"mock\". To enable interactive planning, set `planner = \"codex\" | \"claude\" | \"copilot\"` in .tesaki/config.toml."
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
        "copilot" => {
            if let Err(err) = CopilotAgent::check_available() {
                anyhow::bail!("Copilot planner unavailable: {}", err);
            }
            Box::new(CopilotAgent::new_with_timeout_and_stream(
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

        // Handle `loop N` command - DIRECT algorithmic mission selection (no planner LLM)
        if let Some(count) = parse_loop_command(line) {
            run_autonomous_loop(
                count,
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
            )?;
            continue;
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

        let planner_hint: Option<String> = None;
        
        // Build compact planner input
        let planner_input = ChatTurnInput {
            user_message: line.to_string(),
            session_state_json: serde_json::to_value(&session)?,
            recent_command_results: vec![], // No longer used
            planner_hint: planner_hint.clone(),
            system_prompt: None,
        };

        println!("Planner ({}) running...", planner.name());
        let plan_start = Instant::now();
        let plan_result = planner.plan_turn(&planner_input);
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
                // Retry once with strict hint
                let retry_input = ChatTurnInput {
                    user_message: line.to_string(),
                    session_state_json: serde_json::to_value(&session)?,
                    recent_command_results: vec![],
                    planner_hint: Some("Return ONLY valid JSON. No explanation.".to_string()),
                    system_prompt: None,
                };
                println!("Retrying with strict JSON requirement...");
                match planner.plan_turn(&retry_input) {
                    Ok(plan) => plan,
                    Err(err2) => {
                        println!("Error: {}", err2);
                        continue;
                    }
                }
            }
        };
        
        if let Ok(plan_json) = serde_json::to_string(&plan) {
            logger.log_event(crate::logging::LogEvent::PlannerPlan {
                parse_status: "ok".to_string(),
                plan_json: Some(plan_json),
                error: None,
            });
        }
        
        // Display the planner's response
        println!("{}", plan.say);

        // Handle mission proposal if present
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
        true,  // allow_dirty - REPL missions don't require clean workspace
        None,  // model_overrides - use mission type defaults
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
    use crate::chat_plan::{ChatPlan, MissionProposal, SurfaceLock, SurfacePolicy};

    #[test]
    fn repl_sets_pending_mission_from_plan() {
        let mut session = SessionState::default();
        let plan = ChatPlan {
            say: "test".to_string(),
            run: vec![],
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

/// Parse `loop N` command and return the count, or None if not a loop command.
fn parse_loop_command(line: &str) -> Option<u32> {
    let trimmed = line.trim().to_ascii_lowercase();
    if trimmed.starts_with("loop") {
        let rest = trimmed.strip_prefix("loop")?.trim();
        if rest.is_empty() {
            // Just "loop" with no number means loop until done (use a high number)
            return Some(100);
        }
        rest.parse::<u32>().ok()
    } else {
        None
    }
}

/// Run the autonomous loop: algorithmic mission selection → runner → gate → repeat.
/// No planner LLM is used - task selection is fully deterministic.
fn run_autonomous_loop(
    max_iterations: u32,
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
) -> Result<()> {
    use crate::mission_selector::select_with_constraints;
    
    println!("Starting autonomous loop ({} missions max)...", max_iterations);
    println!("Task selection is ALGORITHMIC (no planner LLM).");
    println!("Loop continues while PROGRESS is being made.\n");
    
    let mut stall_count = 0;
    const MAX_STALLS: u32 = 3;
    let loop_start = Instant::now();
    
    // Record initial issue count for session summary
    {
        let gate_json = crate::run_namako_gate_json(namako_cmd, adapter, &spec_root.to_path_buf(), logger)?;
        let gate_packet = parse_gate_json(&gate_json)?;
        let status_json = crate::run_namako_status(namako_cmd, adapter, &spec_root.to_path_buf(), None, logger)?;
        let review_json = crate::run_namako_review(namako_cmd, adapter, &spec_root.to_path_buf(), logger)?;
        let status_packet = parse_status_json(&status_json)?;
        let review_packet = parse_review_json(&review_json)?;
        let initial_state = RepoState::compute(&status_packet, &review_packet, &gate_packet, None)?;
        session.initial_issue_count = initial_state.spec_issues.len() + initial_state.binding_issues.len()
            + initial_state.sut_issues.len() + initial_state.structure_issues.len();
    }
    
    for iteration in 1..=max_iterations {
        // Refresh state from Namako
        refresh_repo_state(spec_root, adapter, namako_cmd, session, logger)?;
        
        // Get current RepoState
        let gate_json = crate::run_namako_gate_json(namako_cmd, adapter, &spec_root.to_path_buf(), logger)?;
        let gate_packet = parse_gate_json(&gate_json)?;
        let status_json = crate::run_namako_status(namako_cmd, adapter, &spec_root.to_path_buf(), None, logger)?;
        let review_json = crate::run_namako_review(namako_cmd, adapter, &spec_root.to_path_buf(), logger)?;
        let status_packet = parse_status_json(&status_json)?;
        let review_packet = parse_review_json(&review_json)?;
        let state = RepoState::compute(&status_packet, &review_packet, &gate_packet, None)?;
        
        // Snapshot issue counts BEFORE mission
        let before_spec = state.spec_issues.len();
        let before_binding = state.binding_issues.len();
        let before_sut = state.sut_issues.len();
        let before_structure = state.structure_issues.len();
        let before_total = before_spec + before_binding + before_sut + before_structure;
        
        // Check if truly done (all gates pass AND no issues)
        if state.all_gates_pass() && !state.has_work() {
            println!("🎉 All gates pass, no issues remaining. DONE!");
            break;
        }
        
        // Algorithmic mission selection (NO LLM)
        let constraint = StageConstraint {
            stage: session.intent.stage,
            surface_overrides: None,
        };
        
        let selection = match select_with_constraints(&state, &constraint) {
            Some(s) => s,
            None => {
                println!("No actionable work found. Done!");
                break;
            }
        };
        
        let (mission_type, stage, surface_policy) = selection;
        
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("MISSION {}/{}", iteration, max_iterations);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("Type:    {}", mission_type.name());
        println!("Target:  {}", mission_type.target_label().unwrap_or_else(|| "(auto-selected)".to_string()));
        println!("Stage:   {}", stage.name());
        println!("Surfaces: Spec {:?} • Tests {:?} • SUT {:?}",
            surface_policy.spec, surface_policy.tests_bindings, surface_policy.sut);
        println!("Before:  Spec:{} Bind:{} SUT:{} Struct:{} (total: {})",
            before_spec, before_binding, before_sut, before_structure, before_total);
        println!();
        
        // Execute mission directly (bypass planner)
        let start = Instant::now();
        let result = crate::run_run(
            &spec_root.to_path_buf(),
            adapter,
            crate::NamakoCliResolution::explicit(namako_cmd.to_string()),
            max_cert_updates,
            runner_name,
            runner_cmd.clone(),
            max_runtime_seconds,
            max_files_changed,
            max_retries,
            None,  // model - use default
            false, // stream_output
            Some(stage_to_arg(stage)),
            None,
            Some(surface_policy),
            true,  // allow_dirty
            None,  // model_overrides - use mission type defaults
            logger,
        );
        
        let elapsed = start.elapsed();
        
        // Check result - but don't stop on "gate failed" if progress was made
        let runner_succeeded = result.is_ok();
        if let Err(e) = &result {
            println!("Runner reported: {}", e);
        }
        println!("Elapsed: {:.1}s", elapsed.as_secs_f64());
        
        // Read token usage from latest mission and display/record it
        let token_usage = read_latest_token_usage(spec_root);
        if let Some(ref usage) = token_usage {
            println!("{}", usage.to_display_line());
            
            // Record in session stats
            let mission_stats = MissionTokenStats {
                mission_type: mission_type.name().to_string(),
                tokens_in: usage.tokens_in,
                tokens_out: usage.tokens_out,
                tokens_cached: usage.tokens_cached,
                premium_requests: usage.premium_requests,
                model: usage.model.clone(),
                elapsed_seconds: elapsed.as_secs_f64(),
            };
            session.token_stats.record_mission(&mission_stats, runner_succeeded);
        }
        
        // Refresh state AFTER mission
        refresh_repo_state(spec_root, adapter, namako_cmd, session, logger)?;
        
        // Get new counts
        let gate_json = crate::run_namako_gate_json(namako_cmd, adapter, &spec_root.to_path_buf(), logger)?;
        let gate_packet = parse_gate_json(&gate_json)?;
        let status_json = crate::run_namako_status(namako_cmd, adapter, &spec_root.to_path_buf(), None, logger)?;
        let review_json = crate::run_namako_review(namako_cmd, adapter, &spec_root.to_path_buf(), logger)?;
        let status_packet = parse_status_json(&status_json)?;
        let review_packet = parse_review_json(&review_json)?;
        let after_state = RepoState::compute(&status_packet, &review_packet, &gate_packet, None)?;
        
        let after_spec = after_state.spec_issues.len();
        let after_binding = after_state.binding_issues.len();
        let after_sut = after_state.sut_issues.len();
        let after_structure = after_state.structure_issues.len();
        let after_total = after_spec + after_binding + after_sut + after_structure;
        
        // Calculate deltas
        let spec_delta = after_spec as i32 - before_spec as i32;
        let binding_delta = after_binding as i32 - before_binding as i32;
        let total_delta = after_total as i32 - before_total as i32;
        
        println!();
        println!("After:   Spec:{} Bind:{} SUT:{} Struct:{} (total: {})",
            after_spec, after_binding, after_sut, after_structure, after_total);
        println!("Delta:   Spec:{:+} Bind:{:+} Total:{:+}",
            spec_delta, binding_delta, total_delta);
        
        // Determine if we made progress
        // Progress = any category decreased, even if others increased
        // (e.g., adding scenarios decreases spec issues but increases binding issues)
        let made_progress = spec_delta < 0 || binding_delta < 0 || 
            (after_sut as i32) < (before_sut as i32) ||
            (after_structure as i32) < (before_structure as i32);
        
        let gates_now_pass = after_state.all_gates_pass();
        
        // Show mission-specific success message
        let mission_success_msg = format_mission_success(&mission_type, before_binding, after_binding, before_sut, after_sut, before_spec, after_spec);
        if let Some(msg) = mission_success_msg {
            println!("{}", msg);
        }
        
        if gates_now_pass && !after_state.has_work() {
            println!("🎉 All gates pass, no issues remaining. DONE!");
            break;
        } else if made_progress {
            println!("✅ Progress made - continuing");
            stall_count = 0;
        } else if runner_succeeded && total_delta == 0 {
            stall_count += 1;
            println!("⚠️  No net progress (stall {}/{})", stall_count, MAX_STALLS);
            if stall_count >= MAX_STALLS {
                println!("🛑 Too many stalls - stopping to avoid infinite loop");
                break;
            }
        } else if total_delta > 0 {
            println!("⚠️  Regression detected (issues increased) - continuing anyway");
            // Don't stop - the next mission type might fix it
            stall_count += 1;
        }
        
        println!();
    }
    
    // Final summary
    refresh_repo_state(spec_root, adapter, namako_cmd, session, logger)?;
    
    // Get final issue count for summary
    let gate_json = crate::run_namako_gate_json(namako_cmd, adapter, &spec_root.to_path_buf(), logger)?;
    let gate_packet = parse_gate_json(&gate_json)?;
    let status_json = crate::run_namako_status(namako_cmd, adapter, &spec_root.to_path_buf(), None, logger)?;
    let review_json = crate::run_namako_review(namako_cmd, adapter, &spec_root.to_path_buf(), logger)?;
    let status_packet = parse_status_json(&status_json)?;
    let review_packet = parse_review_json(&review_json)?;
    let final_state = RepoState::compute(&status_packet, &review_packet, &gate_packet, None)?;
    let final_issues = final_state.spec_issues.len() + final_state.binding_issues.len() 
        + final_state.sut_issues.len() + final_state.structure_issues.len();
    
    // Calculate session duration (use wall clock time from loop_start)
    let session_duration = loop_start.elapsed().as_secs_f64();
    
    // Print token stats summary if we have any
    if session.token_stats.missions_completed > 0 || session.token_stats.missions_failed > 0 {
        println!("{}", session.token_stats.format_summary(
            session.initial_issue_count,
            final_issues,
            session_duration
        ));
    } else {
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("AUTONOMOUS LOOP FINISHED");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        if let Some(summary) = &session.last_repo_state_summary {
            println!("Final state: {}", summary);
        }
    }
    
    Ok(())
}

/// Format a mission-specific success message based on what the mission type was trying to achieve.
fn format_mission_success(
    mission_type: &crate::mission_type::MissionType,
    before_binding: usize,
    after_binding: usize,
    before_sut: usize,
    after_sut: usize,
    before_spec: usize,
    after_spec: usize,
) -> Option<String> {
    use crate::mission_type::MissionType;
    
    match mission_type {
        MissionType::CreateMissingBindings { .. } => {
            let bindings_created = before_binding.saturating_sub(after_binding);
            if bindings_created > 0 {
                let cascade_msg = if after_sut > before_sut {
                    format!(" → {} SUT issue(s) surfaced (expected cascade)", after_sut - before_sut)
                } else {
                    String::new()
                };
                Some(format!("📝 Created {} binding(s){}", bindings_created, cascade_msg))
            } else {
                None
            }
        }
        MissionType::ImplementBehaviorForScenario { .. } | MissionType::FixRegressionFromGateFailure { .. } => {
            let sut_fixed = before_sut.saturating_sub(after_sut);
            if sut_fixed > 0 {
                Some(format!("🔧 Fixed {} SUT issue(s)", sut_fixed))
            } else {
                None
            }
        }
        MissionType::AddOrClarifyScenario { .. } => {
            let specs_improved = before_spec.saturating_sub(after_spec);
            if specs_improved > 0 {
                let cascade_msg = if after_binding > before_binding {
                    format!(" → {} binding(s) now needed (expected cascade)", after_binding - before_binding)
                } else {
                    String::new()
                };
                Some(format!("📋 Improved {} spec issue(s){}", specs_improved, cascade_msg))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Read the most recent token_usage.json from the mission directory.
/// Looks in .tesaki/missions/ for the latest mission bundle.
fn read_latest_token_usage(spec_root: &Path) -> Option<TokenUsage> {
    let missions_dir = spec_root.join(".tesaki/missions");
    if !missions_dir.exists() {
        return None;
    }
    
    // Find the most recent mission directory (sorted by name, which includes timestamp)
    let mut entries: Vec<_> = std::fs::read_dir(&missions_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    
    entries.sort_by_key(|e| e.path());
    let latest = entries.last()?;
    
    let token_path = latest.path().join("RUNNER_OUTPUT/token_usage.json");
    if !token_path.exists() {
        return None;
    }
    
    let content = std::fs::read_to_string(&token_path).ok()?;
    serde_json::from_str(&content).ok()
}
