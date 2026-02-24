//! Core AI agent trait and CLI engine.

pub mod outcome;
pub mod token_usage;
pub mod backend;
pub mod runner;
pub mod claude_agent;
pub mod codex_agent;
pub mod copilot_agent;
pub mod mock_agent;

pub use outcome::OutcomeClassification;
pub use token_usage::{TokenUsage, MissionTokenStats, SessionTokenStats, MissionTypeStats, EfficiencyRating};
pub use backend::{Servling, LLMRequest, LLMResponse, CliBackend, RunnerInvocation};
pub use runner::{run_cli_runner, CliRunnerConfig, CliRunnerOutcome};
pub use claude_agent::ClaudeAgent;
pub use codex_agent::CodexAgent;
pub use copilot_agent::CopilotAgent;
pub use mock_agent::MockAgent;
