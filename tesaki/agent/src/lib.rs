//! Agent abstraction traits and implementations for Namako Tesaki orchestrator.

pub mod agent_fallback;
pub mod base_runner;
pub mod chat_plan;
pub mod chat_planner;
pub mod runner;
pub mod token_usage;

// Re-export key types for convenient access
pub use chat_plan::{AllowedCommand, ChatPlan, ChatTurnInput, CommandResult, MissionProposal};
pub use chat_planner::ChatPlanner;
pub use servling::{ClaudeAgent, CodexAgent, CopilotAgent, MockAgent, Servling, LLMRequest, LLMResponse};
pub use runner::{OutcomeClassification, Runner, RunnerConfig, RunnerInvocation, RunnerOutcome, MockRunner};
pub use token_usage::TokenUsage;
pub use agent_fallback::{
    build_planner, build_runner, describe_candidates, describe_planner_candidates,
    normalize_model_for_runner, outcome_from_error, planner_candidates, runner_candidates,
    should_fallback_on_outcome, FallbackChatPlanner, PlannerCandidate, RunnerCandidate,
};
