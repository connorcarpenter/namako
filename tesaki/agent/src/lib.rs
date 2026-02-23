//! Agent abstraction traits and implementations for Namako Tesaki orchestrator.

pub mod agent_fallback;
pub mod base_runner;
pub mod chat_plan;
pub mod chat_planner;
pub mod claude_agent;
pub mod codex_agent;
pub mod copilot_agent;
pub mod llm_backend;
pub mod mock_agent;
pub mod runner;
pub mod runner_test;
pub mod token_usage;

// Re-export key types for convenient access
pub use chat_plan::{AllowedCommand, ChatPlan, ChatTurnInput, CommandResult, MissionProposal};
pub use chat_planner::ChatPlanner;
pub use claude_agent::ClaudeAgent;
pub use codex_agent::CodexAgent;
pub use copilot_agent::CopilotAgent;
pub use llm_backend::{LLMBackend, LLMRequest, LLMResponse};
pub use mock_agent::MockAgent;
pub use runner::{OutcomeClassification, Runner, RunnerConfig, RunnerInvocation, RunnerOutcome};
pub use runner_test::MockRunner;
pub use token_usage::TokenUsage;
pub use agent_fallback::{
    build_planner, build_runner, describe_candidates, describe_planner_candidates,
    normalize_model_for_runner, outcome_from_error, planner_candidates, runner_candidates,
    should_fallback_on_outcome, FallbackChatPlanner, PlannerCandidate, RunnerCandidate,
};
