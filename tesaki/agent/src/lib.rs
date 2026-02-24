//! Agent abstraction traits and implementations for Namako Tesaki orchestrator.

pub mod chat_planner;
pub mod runner;

// Re-export core roles
pub use chat_planner::{
    AllowedCommand, ChatPlan, ChatTurnInput, CommandResult, MissionProposal, ChatPlanner,
    SurfaceLock, SurfacePolicy, MockChatPlanner,
};
pub use runner::{
    OutcomeClassification, Runner, RunnerConfig, RunnerInvocation, RunnerOutcome, MockRunner,
    MockAgent, check_surface_violations, matches_any_pattern,
};

// Re-export the entire agent engine and factory from servling
pub use servling::{
    agent_candidates, build_coding_agent, build_servling, describe_candidates, normalize_model,
    AgentCandidate, ClaudeAgent, CodexAgent, CodingAgent, CodingAgentBuilder,
    CopilotAgent, EfficiencyRating, LLMRequest, LLMResponse, MissionTokenStats, MissionTypeStats,
    Servling, SessionTokenStats, TokenUsage,
};

/// Helper to convert a Result error into a generic EnvironmentError outcome.
pub fn outcome_from_error(err: anyhow::Error) -> RunnerOutcome {
    RunnerOutcome {
        exit_code: None,
        classification: OutcomeClassification::EnvironmentError,
        elapsed_seconds: 0.0,
        stdout_path: None,
        stderr_path: None,
        error_message: Some(err.to_string()),
        token_usage: None,
    }
}

/// Factory to build a Runner from agent candidates.
pub fn build_runner(candidates: Vec<AgentCandidate>) -> anyhow::Result<Box<dyn Runner>> {
    let agent = build_coding_agent(candidates)?;
    // Use a manual wrap to force dyn Runner coercion
    struct RunnerWrap(Box<dyn Servling>);
    impl Runner for RunnerWrap {
        fn run(&self, mission_dir: &std::path::Path, config: &RunnerConfig) -> anyhow::Result<RunnerOutcome> {
            self.0.run(mission_dir, config)
        }
        fn name(&self) -> &'static str {
            Servling::name(&*self.0)
        }
        fn planned_invocation(&self, mission_dir: &std::path::Path, config: &RunnerConfig) -> Option<RunnerInvocation> {
            // Create a temporary LLMRequest for planned_invocation
            let request = LLMRequest {
                prompt: String::new(),
                model: config.model.clone(),
                working_dir: config.working_dir.clone(),
                max_runtime_seconds: config.max_runtime_seconds,
                stream_output: config.stream_output,
                input_file: Some(mission_dir.join("MISSION.md")),
            };
            Servling::planned_invocation(&*self.0, &request).map(|inv| RunnerInvocation {
                program: inv.program,
                args: inv.args,
                working_dir: inv.working_dir,
                env: inv.env,
            })
        }
    }
    Ok(Box::new(RunnerWrap(agent)))
}

/// Factory to build a ChatPlanner from agent candidates.
pub fn build_planner(candidates: Vec<AgentCandidate>) -> anyhow::Result<Box<dyn ChatPlanner>> {
    let agent = build_coding_agent(candidates)?;
    struct PlannerWrap(Box<dyn Servling>);
    impl ChatPlanner for PlannerWrap {
        fn plan_turn(&self, input: &ChatTurnInput) -> anyhow::Result<ChatPlan> {
            self.0.plan_turn(input)
        }
        fn name(&self) -> &'static str {
            Servling::name(&*self.0)
        }
    }
    Ok(Box::new(PlannerWrap(agent)))
}
