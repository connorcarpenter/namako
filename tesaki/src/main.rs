//! Tesaki task orchestrator v1.8.

use std::path::{Path, PathBuf};
use log::{info, warn, error, debug};
use anyhow::{Result, Context};
use serde_json::json;

use tesaki::config::{self, ConfigDiscoveryResult};
use tesaki::packet_parser::{
    parse_gate_json, parse_review_json, parse_status_json, 
    GatePacket, ReviewPacket, StatusPacket, StatusValue,
};
use tesaki::repo_state::RepoState;
use tesaki::session::{SessionState, PendingMission};
use tesaki::stage::{detect_stage, Stage, StageConstraint};
use tesaki::mission_type::MissionType;
use tesaki::mission_selector::{select_mission_type, select_with_constraints};
use tesaki::model_tier::select_model_for_attempt;
use tesaki::workspace::Workspace;
use tesaki::gate::{GateOutcome, ProcessInvoker, UpdateCertInvoker, PhaseStatus};
use tesaki::surface_policy::{SurfaceLock, SurfacePolicy, SurfaceDefinition};
use tesaki::error_parser::run_pre_gate_build;
use tesaki::plan_validator::{ProposedPlan, validate_plan};
use tesaki::runner::{
    Runner, RunnerConfig, RunnerOutcome, RunnerInvocation, 
    outcome_from_error, build_runner, check_surface_violations,
};
use servling::{
    agent_candidates, describe_candidates, normalize_model, 
    OutcomeClassification, TokenUsage,
};

/// Main entry point for `tesaki run`.
pub fn run_run(
    config: &tesaki::config::TesakiConfig,
    spec_root: &Path,
    adapter: &str,
    namako_cmd: &str,
    pre_gate_build_mode: tesaki::config::PreGateBuildMode,
    session: &mut SessionState,
    lessons_db: &mut tesaki::lessons::LessonsDatabase,
    logger: &tesaki::logging::JsonlLogger,
) -> Result<tesaki::stop_reason::RunResult> {
    info!("Starting Tesaki run loop...");
    
    let mut iterations = 0;
    let max_iterations = config.max_iterations.unwrap_or(10);

    loop {
        iterations += 1;
        if iterations > max_iterations {
            info!("Reached max iterations ({})", max_iterations);
            return Ok(tesaki::stop_reason::RunResult {
                stop_reason: tesaki::stop_reason::StopReason::MaxIterationsReached,
                iterations,
            });
        }

        info!("Iteration {}/{}", iterations, max_iterations);

        // 1. Get current repo state
        let gate_json = tesaki::gate::run_namako_gate_json(namako_cmd, adapter, spec_root, logger)?;
        let status_json = tesaki::repo_state::run_namako_status(namako_cmd, adapter, spec_root, None, logger)?;
        let review_json = tesaki::repo_state::run_namako_review(namako_cmd, adapter, spec_root, logger)?;

        let gate = parse_gate_json(&gate_json)?;
        let status = parse_status_json(&status_json)?;
        let review = parse_review_json(&review_json)?;

        let stage = detect_stage(&gate, &status, &review);
        session.stage = Some(stage.clone());
        info!("Current Stage: {:?}", stage);

        if matches!(stage, Stage::Finalize) {
            info!("All goals achieved. Stopping.");
            return Ok(tesaki::stop_reason::RunResult {
                stop_reason: tesaki::stop_reason::StopReason::GoalsAchieved,
                iterations,
            });
        }

        // 2. Select mission
        let mission_type = if let Some(proposal) = &session.pending_mission {
            // If we have a pending mission from REPL, use its type
            // This is a simplification; in reality we'd need to map proposal back to MissionType
            select_mission_type(&gate, &status, &review, &StageConstraint::None)
        } else {
            select_mission_type(&gate, &status, &review, &StageConstraint::None)
        };

        let mission_id = session.pending_mission.as_ref().map(|m| m.id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        // 3. Execute mission
        let outcome = run_mission(
            config,
            spec_root,
            adapter,
            namako_cmd,
            &mission_type,
            &mission_id,
            session,
            lessons_db,
            logger,
        )?;

        if outcome.classification.should_fallback() {
            warn!("Mission rate limited or failed with fallbackable error.");
        }

        if matches!(outcome.classification, OutcomeClassification::Failed) {
            warn!("Mission failed. Stopping loop.");
            return Ok(tesaki::stop_reason::RunResult {
                stop_reason: tesaki::stop_reason::StopReason::MissionFailed,
                iterations,
            });
        }
    }
}

pub fn run_mission(
    config: &tesaki::config::TesakiConfig,
    spec_root: &Path,
    adapter: &str,
    namako_cmd: &str,
    mission_type: &MissionType,
    mission_id: &str,
    session: &mut SessionState,
    _lessons_db: &mut tesaki::lessons::LessonsDatabase,
    logger: &tesaki::logging::JsonlLogger,
) -> Result<RunnerOutcome> {
    let preferred_agent = config.preferred_agent.clone();
    let custom_command = config.agent_command.clone();
    let runner_candidates = agent_candidates(&preferred_agent, custom_command);
    
    let runner = build_runner(runner_candidates)?;
    
    let mission_dir = spec_root.to_path_buf(); // Simplified
    let runner_config = RunnerConfig {
        working_dir: spec_root.to_path_buf(),
        max_runtime_seconds: 300,
        model: config.model_tier.as_ref().map(|t| t.to_string()),
        stream_output: true,
    };

    info!("Executing mission {} with runner {}", mission_id, runner.name());
    
    let outcome = runner.run(&mission_dir, &runner_config)?;
    
    logger.log_event(tesaki::logging::LogEvent::MissionExecuted {
        mission_id: mission_id.to_string(),
        runner: runner.name().to_string(),
        outcome: outcome.clone(),
    });

    Ok(outcome)
}

fn main() -> Result<()> {
    // Basic main implementation to satisfy compiler
    Ok(())
}
