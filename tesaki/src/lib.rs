//! Tesaki - AI-friendly task orchestrator for Namako spec-driven development
//!
//! This library module exports the core abstractions for the Tesaki orchestrator.

// Re-export agent abstractions and implementations
pub use tesaki_agent::{
    agent_fallback, base_runner, chat_plan, chat_planner, claude_code_agent, codex_agent,
    copilot_agent, runner, runner_test, token_usage,
    // Key types
    ChatPlan, ChatPlanner, ChatTurnInput, OutcomeClassification, Runner, RunnerConfig,
    RunnerInvocation, RunnerOutcome, TokenUsage, FallbackChatPlanner, PlannerCandidate,
    RunnerCandidate, MockRunner,
};

pub mod binding_extractor;
pub mod config;
pub mod error_parser;
pub mod gate;
pub mod issue_classifier;
pub mod mission;
pub mod mission_selector;
pub mod mission_type;
pub mod model_tier;
pub mod packet_parser;
pub mod prompts;
pub mod repo_state;

pub mod scenario_extractor;
pub mod session;
pub mod stage;
pub mod stop_reason;
pub mod surface_policy;
pub mod spec_quality;
pub mod workspace;
