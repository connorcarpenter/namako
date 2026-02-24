//! Interactive REPL session for Tesaki v1.8.

use anyhow::Result;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use tesaki::chat_planner::{
    ChatPlan, ChatTurnInput, MissionProposal,
    SurfaceLock as PlanSurfaceLock, SurfacePolicy as PlanSurfacePolicy,
    ChatPlanner, MockChatPlanner,
};
use servling::{agent_candidates, describe_candidates};
use tesaki::config::{self, ConfigDiscoveryResult};
use tesaki::diagnosis::StallDiagnosis;
use tesaki::escalation;
use tesaki::lessons::{self, LessonsDatabase};
use tesaki::mission_type::MissionType;
use tesaki::packet_parser::{parse_gate_json, parse_review_json, parse_status_json};
use tesaki::repo_state::RepoState;
use tesaki::session::{PendingMission, SessionState};
use tesaki::stage::{detect_stage, Stage, StageConstraint};
use tesaki::stop_reason::StopReason;
use tesaki::surface_policy::{SurfaceLock as RepoSurfaceLock, SurfacePolicy as RepoSurfacePolicy};
use servling::{MissionTokenStats, TokenUsage};

/// Run the autonomous loop directly without REPL (headless mode).
/// Usage: `tesaki --loop 10` or `tesaki -l 10`
pub fn run_loop_headless(start_dir: PathBuf, max_iterations: u32, logger: &tesaki::logging::JsonlLogger) -> Result<()> {
    let config = match config::discover_config(&start_dir)? {
        ConfigDiscoveryResult::Found(c) => c,
        ConfigDiscoveryResult::NotFound(path) => {
            anyhow::bail!("No .tesaki.toml found in any parent of {}", path.display());
        }
    };

    let spec_root = config.workspace_root.clone();
    let adapter = config.adapter.clone();
    let namako_cmd = config.namako_cli.clone();

    let namako_resolution = tesaki::resolve_namako_cli(None, config.namako_cli.clone(), &spec_root);
    tesaki::log_session_start(
        logger,
        &spec_root,
        &adapter,
        &namako_cmd,
        "headless",
        &namako_resolution,
    );

    let mut session = SessionState::default();
    let mut lessons_db = LessonsDatabase::load(&spec_root).unwrap_or_default();
    refresh_repo_state(&spec_root, &adapter, &namako_cmd, &mut session, logger)?;

    for i in 0..max_iterations {
        info!("Headless iteration {}/{}", i + 1, max_iterations);
        
        let preferred_agent = config.preferred_agent.clone();
        let custom_command = config.agent_command.clone();
        let planner_candidates = agent_candidates(&preferred_agent, custom_command);
        
        let planner = tesaki::chat_planner::build_planner(planner_candidates)?;
        
        let input = ChatTurnInput {
            user_message: "Make progress on the current goal.".to_string(),
            session_state_json: serde_json::to_value(&session)?,
            recent_command_results: vec![],
            planner_hint: None,
            system_prompt: None,
        };

        let plan = planner.plan_turn(&input)?;
        
        if let Some(proposal) = plan.mission_proposal {
            info!("Planner proposed mission: {}", proposal.objective);
            logger.log_event(tesaki::logging::LogEvent::MissionProposed {
                mission_id: "headless".to_string(),
                mission_type: proposal.mission_type.clone(),
                stage: proposal.stage.clone(),
                target: proposal.target.clone(),
                surfaces: tesaki::logging::SurfaceLog {
                    spec: format!("{:?}", proposal.surfaces.spec),
                    tests: format!("{:?}", proposal.surfaces.tests),
                    sut: format!("{:?}", proposal.surfaces.sut),
                },
                objective: proposal.objective.clone(),
            });

            session.proposal = proposal;
            session.pending_mission = Some(PendingMission {
                id: format!("headless-{}", i),
                objective: session.proposal.objective.clone(),
                created_at: chrono::Utc::now(),
            });

            // Headless execution would normally call run_mission here
            // For now, we just log and continue
        }

        if plan.done {
            info!("Planner signal DONE.");
            break;
        }
    }

    logger.log_event(tesaki::logging::LogEvent::SessionEnd {
        mission_token_stats: session.stats.missions.clone(),
        total_tokens: TokenUsage::default(), // TODO: Aggregate properly
    });

    Ok(())
}

/// Start the interactive REPL session.
pub fn run_repl(start_dir: PathBuf, logger: &tesaki::logging::JsonlLogger) -> Result<()> {
    let config = match config::discover_config(&start_dir)? {
        ConfigDiscoveryResult::Found(c) => c,
        ConfigDiscoveryResult::NotFound(path) => {
            anyhow::bail!("No .tesaki.toml found in any parent of {}", path.display());
        }
    };

    let spec_root = config.workspace_root.clone();
    let adapter = config.adapter.clone();
    let namako_cmd = config.namako_cli.clone();

    let namako_resolution = tesaki::resolve_namako_cli(None, config.namako_cli.clone(), &spec_root);
    tesaki::log_session_start(
        logger,
        &spec_root,
        &adapter,
        &namako_cmd,
        "repl",
        &namako_resolution,
    );

    println!("Tesaki v1.8 REPL (Root: {})", spec_root.display());
    
    let preferred_agent = config.preferred_agent.clone();
    let custom_command = config.agent_command.clone();
    let planner_candidates = agent_candidates(&preferred_agent, custom_command);
    println!("Agent: {}", describe_candidates(&planner_candidates));

    let planner = if std::env::var("TESAKI_OFFLINE").is_ok() {
        Box::new(MockChatPlanner::new(ChatPlan {
            say: "Offline mode active.".to_string(),
            run: vec![],
            mission_proposal: None,
            done: true,
        }))
    } else {
        tesaki::chat_planner::build_planner(planner_candidates)?
    };

    let mut session = SessionState::default();
    let mut lessons_db = LessonsDatabase::load(&spec_root).unwrap_or_default();
    refresh_repo_state(&spec_root, &adapter, &namako_cmd, &mut session, logger)?;

    loop {
        print!("> ");
        io::stdout().flush()?;

        let mut user_input = String::new();
        io::stdin().read_line(&mut user_input)?;
        let user_message = user_input.trim();

        if user_message.is_empty() {
            continue;
        }

        if user_message == "exit" || user_message == "quit" {
            break;
        }

        if user_message == "status" {
            refresh_repo_state(&spec_root, &adapter, &namako_cmd, &mut session, logger)?;
            println!("Stage: {:?}", session.stage);
            continue;
        }

        let input = ChatTurnInput {
            user_message: user_message.to_string(),
            session_state_json: serde_json::to_value(&session)?,
            recent_command_results: vec![],
            planner_hint: None,
            system_prompt: None,
        };

        let start = Instant::now();
        let plan = match planner.plan_turn(&input) {
            Ok(p) => p,
            Err(e) => {
                println!("Planner Error: {}", e);
                continue;
            }
        };
        let elapsed = start.elapsed();

        println!("\n{}", plan.say);
        
        for cmd in &plan.run {
            println!("Executing: {} {}", cmd.tool, cmd.args.join(" "));
            if cmd.tool == "namako" {
                if cmd.args.contains(&"gate".to_string()) {
                    let gate_json = tesaki::run_namako_gate_json(&namako_cmd, &adapter, &spec_root.to_path_buf(), logger)?;
                    println!("Gate outcome: {}", gate_json);
                } else if cmd.args.contains(&"status".to_string()) {
                    tesaki::run_namako_status(&namako_cmd, &adapter, &spec_root.to_path_buf(), None, logger)?;
                } else if cmd.args.contains(&"review".to_string()) {
                    tesaki::run_namako_review(&namako_cmd, &adapter, &spec_root.to_path_buf(), logger)?;
                }
            }
        }

        if let Some(proposal) = plan.mission_proposal {
            println!("\nProposed Mission:");
            println!("  Type:      {}", proposal.mission_type);
            println!("  Stage:     {}", proposal.stage);
            println!("  Target:    {}", proposal.target);
            println!("  Objective: {}", proposal.objective);
            println!("  Surfaces:  Spec={:?}, Tests={:?}, SUT={:?}", 
                proposal.surfaces.spec, proposal.surfaces.tests, proposal.surfaces.sut);

            print!("\nExecute mission? [y/N] ");
            io::stdout().flush()?;
            let mut confirm = String::new();
            io::stdin().read_line(&mut confirm)?;
            if confirm.trim().to_lowercase() == "y" {
                session.proposal = proposal;
                session.pending_mission = Some(PendingMission {
                    id: uuid::Uuid::new_v4().to_string(),
                    objective: session.proposal.objective.clone(),
                    created_at: chrono::Utc::now(),
                });

                logger.log_event(tesaki::logging::LogEvent::MissionProposed {
                    mission_id: session.pending_mission.as_ref().unwrap().id.clone(),
                    mission_type: session.proposal.mission_type.clone(),
                    stage: session.proposal.stage.clone(),
                    target: session.proposal.target.clone(),
                    surfaces: tesaki::logging::SurfaceLog {
                        spec: format!("{:?}", session.proposal.surfaces.spec),
                        tests: format!("{:?}", session.proposal.surfaces.tests),
                        sut: format!("{:?}", session.proposal.surfaces.sut),
                    },
                    objective: session.proposal.objective.clone(),
                });

                tesaki::run_run(
                    &config,
                    &spec_root,
                    &adapter,
                    &namako_cmd,
                    tesaki::config::PreGateBuildMode::None,
                    &mut session,
                    &mut lessons_db,
                    logger,
                )?;
                refresh_repo_state(&spec_root, &adapter, &namako_cmd, &mut session, logger)?;
            }
        }

        if plan.done {
            println!("Done.");
            break;
        }
    }

    logger.log_event(tesaki::logging::LogEvent::SessionEnd {
        mission_token_stats: session.stats.missions.clone(),
        total_tokens: TokenUsage::default(), 
    });

    Ok(())
}

fn refresh_repo_state(
    spec_root: &Path,
    adapter: &str,
    namako_cmd: &str,
    session: &mut SessionState,
    logger: &tesaki::logging::JsonlLogger,
) -> Result<()> {
    let gate_json = tesaki::run_namako_gate_json(namako_cmd, adapter, &spec_root.to_path_buf(), logger)?;
    let status_json = tesaki::run_namako_status(namako_cmd, adapter, &spec_root.to_path_buf(), None, logger)?;
    let review_json = tesaki::run_namako_review(namako_cmd, adapter, &spec_root.to_path_buf(), logger)?;

    session.last_repo_state_summary = Some(format!(
        "Gate: {}\nStatus: {}\nReview: {}",
        gate_json, status_json, review_json
    ));
    
    let stage = detect_stage(
        &parse_gate_json(&gate_json)?,
        &parse_status_json(&status_json)?,
        &parse_review_json(&review_json)?,
    );
    session.stage = Some(stage);

    Ok(())
}
