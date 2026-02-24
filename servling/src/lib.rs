//! Core AI agent trait and CLI engine.

pub mod token_usage;
pub mod core;
pub mod cli_backend;
pub mod coding_agent;
pub mod runner;
pub mod claude_agent;
pub mod codex_agent;
pub mod copilot_agent;

pub use token_usage::{TokenUsage, MissionTokenStats, SessionTokenStats, MissionTypeStats, EfficiencyRating};
pub use crate::core::{Servling, LLMRequest, LLMResponse, RunnerInvocation, normalize_model, OutcomeClassification};
pub use cli_backend::CliBackend;
pub use coding_agent::{CodingAgent, CodingAgentBuilder, AgentCandidate, agent_candidates, describe_candidates};
pub use runner::{run_cli_runner, CliRunnerConfig, CliRunnerOutcome};
pub use claude_agent::ClaudeAgent;
pub use codex_agent::CodexAgent;
pub use copilot_agent::CopilotAgent;

/// Build a single Servling backend.
pub fn build_servling(name: &str, command: Option<String>) -> anyhow::Result<Box<dyn Servling>> {
    match name {
        "claude" => {
            ClaudeAgent::check_available()?;
            Ok(Box::new(ClaudeAgent::new(command, true)))
        }
        "codex" => {
            CodexAgent::check_available()?;
            Ok(Box::new(CodexAgent::new(command)))
        }
        "copilot" => {
            CopilotAgent::check_available()?;
            Ok(Box::new(CopilotAgent::new(command)))
        }
        other => anyhow::bail!("Unknown agent backend: {}", other),
    }
}

/// Build a CodingAgent (with fallbacks) from a list of candidates.
pub fn build_coding_agent(candidates: Vec<AgentCandidate>) -> anyhow::Result<Box<dyn Servling>> {
    if candidates.len() == 1 {
        return build_servling(&candidates[0].name, candidates[0].command.clone());
    }

    let mut builder = CodingAgent::builder();
    let mut count = 0;
    
    for candidate in candidates {
        match build_servling(&candidate.name, candidate.command.clone()) {
            Ok(s) => {
                builder = builder.register(s);
                count += 1;
            }
            Err(e) => log::warn!("Agent candidate {} unavailable: {}", candidate.name, e),
        }
    }
    
    if count == 0 {
        anyhow::bail!("No agent candidates available");
    }
    
    Ok(Box::new(builder.build()?))
}
