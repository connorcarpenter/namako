//! Factory and selection policy for Servling agents.

use anyhow::{bail, Result};
use crate::backend::{Servling};
use crate::coding_agent::CodingAgent;
use crate::claude_agent::ClaudeAgent;
use crate::codex_agent::CodexAgent;
use crate::copilot_agent::CopilotAgent;
use crate::outcome::OutcomeClassification;

const AI_BACKENDS: [&str; 3] = ["claude", "copilot", "codex"];

#[derive(Debug, Clone)]
pub struct AgentCandidate {
    pub name: String,
    pub command: Option<String>,
}

/// Generate a prioritized list of agent candidates.
pub fn agent_candidates(preferred: &str, custom_command: Option<String>) -> Vec<AgentCandidate> {
    let preferred = preferred.to_lowercase();
    let mut candidates = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut push = |name: &str, cmd: Option<String>| {
        if seen.insert(name.to_string()) {
            candidates.push(AgentCandidate {
                name: name.to_string(),
                command: cmd,
            });
        }
    };

    if AI_BACKENDS.contains(&preferred.as_str()) {
        push(&preferred, custom_command);
        for name in AI_BACKENDS {
            push(name, None);
        }
    } else {
        push(&preferred, custom_command);
    }

    candidates
}

/// Format a candidate chain for display (e.g., "claude -> copilot -> codex").
pub fn describe_candidates(candidates: &[AgentCandidate]) -> String {
    candidates.iter()
        .map(|c| c.name.as_str())
        .collect::<Vec<_>>()
        .join(" -> ")
}

/// Build a single Servling backend.
pub fn build_servling(name: &str, command: Option<String>) -> Result<Box<dyn Servling>> {
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
        other => bail!("Unknown agent backend: {}", other),
    }
}

/// Build a CodingAgent (with fallbacks) from a list of candidates.
pub fn build_coding_agent(candidates: Vec<AgentCandidate>) -> Result<Box<dyn Servling>> {
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
        bail!("No agent candidates available");
    }
    
    Ok(Box::new(builder.build()?))
}

pub fn should_fallback(classification: OutcomeClassification) -> bool {
    classification == OutcomeClassification::RateLimited
}

pub fn normalize_model(backend_name: &str, model: Option<String>) -> Option<String> {
    let model = model?;
    // Claude models pass through to Claude backend
    if backend_name == "claude" {
        return Some(model);
    }
    // Generic tiers are stripped for non-claude backends unless they match
    if is_claude_tier(&model) {
        return None;
    }
    Some(model)
}

fn is_claude_tier(model: &str) -> bool {
    let lower = model.to_lowercase();
    matches!(lower.as_str(), "haiku" | "sonnet" | "opus") || lower.contains("claude-")
}
