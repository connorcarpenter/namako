//! Agent abstraction traits and implementations for Namako Tesaki orchestrator.

pub mod agent_fallback;
pub mod chat_planner;
pub mod runner;

// Re-export key types for convenient access
pub use chat_planner::{
    AllowedCommand, ChatPlan, ChatTurnInput, CommandResult, MissionProposal, ChatPlanner,
    SurfaceLock, SurfacePolicy,
};
pub use servling::{ClaudeAgent, CodexAgent, CopilotAgent, MockAgent, Servling, LLMRequest, LLMResponse, EfficiencyRating, MissionTokenStats, SessionTokenStats, TokenUsage, MissionTypeStats};
pub use runner::{OutcomeClassification, Runner, RunnerConfig, RunnerInvocation, RunnerOutcome, MockRunner, check_surface_violations, matches_any_pattern};
pub use agent_fallback::{
    build_planner, build_runner, describe_candidates, describe_planner_candidates,
    normalize_model_for_runner, outcome_from_error, planner_candidates, runner_candidates,
    should_fallback_on_outcome, FallbackChatPlanner, PlannerCandidate, RunnerCandidate,
};
