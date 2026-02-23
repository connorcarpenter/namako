//! Agent abstraction traits and implementations for Namako Tesaki orchestrator.
//!
//! This crate provides the core abstractions and implementations for agent-based task execution:
//! - `Runner` trait for mission execution
//! - `ChatPlanner` trait for conversation-based planning
//! - Concrete implementations (Claude Code, Codex, Copilot)
//! - Fallback and factory logic for agent selection

pub mod agent_fallback;
pub mod base_runner;
pub mod chat_plan;
pub mod chat_planner;
pub mod claude_code_agent;
pub mod codex_agent;
pub mod copilot_agent;
pub mod runner;
pub mod runner_test;
pub mod token_usage;

// Re-export key types for convenient access
pub use runner::{OutcomeClassification, Runner, RunnerConfig, RunnerInvocation, RunnerOutcome};
pub use runner_test::MockRunner;
pub use token_usage::TokenUsage;
pub use chat_plan::{AllowedCommand, ChatPlan, ChatTurnInput, CommandResult, MissionProposal};
pub use chat_planner::ChatPlanner;
pub use agent_fallback::{
    build_planner, build_runner, describe_candidates, describe_planner_candidates,
    normalize_model_for_runner, outcome_from_error, planner_candidates, runner_candidates,
    should_fallback_on_outcome, FallbackChatPlanner, PlannerCandidate, RunnerCandidate,
};
