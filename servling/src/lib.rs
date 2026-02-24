//! Core AI agent trait and CLI engine.

pub mod outcome;
pub mod token_usage;
pub mod core;
pub mod cli_backend;
pub mod coding_agent;
pub mod runner;
pub mod claude_agent;
pub mod codex_agent;
pub mod copilot_agent;
pub mod factory;

pub use outcome::OutcomeClassification;
pub use token_usage::{TokenUsage, MissionTokenStats, SessionTokenStats, MissionTypeStats, EfficiencyRating};
pub use crate::core::{Servling, LLMRequest, LLMResponse, RunnerInvocation};
pub use cli_backend::CliBackend;
pub use coding_agent::{CodingAgent, CodingAgentBuilder};
pub use runner::{run_cli_runner, CliRunnerConfig, CliRunnerOutcome};
pub use claude_agent::ClaudeAgent;
pub use codex_agent::CodexAgent;
pub use copilot_agent::CopilotAgent;
pub use factory::{AgentCandidate, agent_candidates, build_servling, build_coding_agent, describe_candidates, normalize_model, should_fallback};
