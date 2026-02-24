//! Tesaki - AI-friendly task orchestrator for Namako spec-driven development
//!
//! This library module exports the core abstractions for the Tesaki orchestrator.

pub mod chat_planner;
pub mod runner;

// Re-export EVERYTHING from integrated agent modules into the crate root
pub use crate::chat_planner::*;
pub use crate::runner::*;

// Re-export types from servling that were previously in tesaki_agent
pub use servling::{
    agent_candidates, build_coding_agent, build_servling, describe_candidates, normalize_model,
    AgentCandidate, ClaudeAgent, CodexAgent, CodingAgent, CodingAgentBuilder,
    CopilotAgent, EfficiencyRating, LLMRequest, LLMResponse, MissionTokenStats, MissionTypeStats,
    Servling, SessionTokenStats, TokenUsage,
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
pub mod diagnosis;
pub mod escalation;
pub mod lessons;
pub mod logging;
pub mod plan_validator;
