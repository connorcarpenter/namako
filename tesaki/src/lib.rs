//! Tesaki - AI-friendly task orchestrator for Namako spec-driven development
//!
//! This library module exports the core abstractions for the Tesaki orchestrator.

// Re-export agent abstractions and implementations from tesaki_agent
pub use tesaki_agent::{
    chat_planner, runner,
    // Key types
    AllowedCommand, ChatPlan, ChatPlanner, ChatTurnInput, OutcomeClassification, Runner, RunnerConfig,
    RunnerInvocation, RunnerOutcome, TokenUsage, MockRunner, SessionTokenStats, MissionTokenStats,
    // Agent Registry & Factory
    AgentCandidate, agent_candidates, build_coding_agent, build_servling, describe_candidates,
    normalize_model, MissionProposal,
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
