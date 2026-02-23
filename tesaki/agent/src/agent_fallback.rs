use anyhow::{bail, Result};
use std::path::PathBuf;

use crate::chat_planner::{ChatPlanner, MockChatPlanner};
use crate::claude_agent::ClaudeAgent;
use crate::codex_agent::CodexAgent;
use crate::copilot_agent::CopilotAgent;
use crate::runner::{OutcomeClassification, Runner, RunnerOutcome};
use crate::runner_test::MockRunner;
use crate::chat_plan::ChatPlan;

const AI_RUNNERS: [&str; 3] = ["claude", "copilot", "codex"];

#[derive(Debug, Clone)]
pub struct RunnerCandidate {
    pub name: String,
    pub runner_cmd: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PlannerCandidate {
    pub name: String,
    pub runner_cmd: Option<String>,
    pub planner_cmd: Option<String>,
}

pub fn runner_candidates(preferred: &str, runner_cmd: Option<String>) -> Vec<RunnerCandidate> {
    let preferred = preferred.to_lowercase();
    let mut candidates = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut push = |name: &str, cmd: Option<String>| {
        if seen.insert(name.to_string()) {
            candidates.push(RunnerCandidate {
                name: name.to_string(),
                runner_cmd: cmd,
            });
        }
    };

    if AI_RUNNERS.contains(&preferred.as_str()) {
        push(&preferred, runner_cmd);
        for name in AI_RUNNERS {
            push(name, None);
        }
    } else {
        push(&preferred, runner_cmd);
    }

    candidates
}

pub fn planner_candidates(
    preferred: &str,
    runner_cmd: Option<String>,
    planner_cmd: Option<String>,
) -> Vec<PlannerCandidate> {
    let preferred = preferred.to_lowercase();
    let mut candidates = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut push = |name: &str, rcmd: Option<String>, pcmd: Option<String>| {
        if seen.insert(name.to_string()) {
            candidates.push(PlannerCandidate {
                name: name.to_string(),
                runner_cmd: rcmd,
                planner_cmd: pcmd,
            });
        }
    };

    if AI_RUNNERS.contains(&preferred.as_str()) {
        push(&preferred, runner_cmd, planner_cmd);
        for name in AI_RUNNERS {
            push(name, None, None);
        }
    } else {
        push(&preferred, runner_cmd, planner_cmd);
    }

    candidates
}

pub fn describe_candidates(names: &[RunnerCandidate]) -> String {
    let chain = names
        .iter()
        .map(|c| c.name.as_str())
        .collect::<Vec<_>>();
    chain.join(" -> ")
}

pub fn describe_planner_candidates(names: &[PlannerCandidate]) -> String {
    let chain = names
        .iter()
        .map(|c| c.name.as_str())
        .collect::<Vec<_>>();
    chain.join(" -> ")
}

pub fn build_runner(
    candidate: &RunnerCandidate,
    _spec_root: &PathBuf,
    _max_runtime_seconds: u32,
) -> Result<Box<dyn Runner>> {
    match candidate.name.as_str() {
        "mock" => Ok(Box::new(MockRunner::success())),
        "claude" => {
            ClaudeAgent::check_available()?;
            Ok(Box::new(ClaudeAgent::new(candidate.runner_cmd.clone(), true)))
        }
        "codex" => {
            CodexAgent::check_available()?;
            Ok(Box::new(CodexAgent::new(candidate.runner_cmd.clone())))
        }
        "copilot" => {
            CopilotAgent::check_available()?;
            Ok(Box::new(CopilotAgent::new(candidate.runner_cmd.clone())))
        }
        other => bail!("Unknown runner: {}", other),
    }
}

pub fn build_planner(
    candidate: &PlannerCandidate,
    _spec_root: &PathBuf,
    _timeout_seconds: u64,
) -> Result<Box<dyn ChatPlanner>> {
    match candidate.name.as_str() {
        "mock" => Ok(Box::new(MockChatPlanner::new(ChatPlan {
            say: "Planner not configured. Set `agent` or `planner` in .tesaki/config.toml.".to_string(),
            run: vec![],
            mission_proposal: None,
            done: true,
        }))),
        "claude" => {
            ClaudeAgent::check_available()?;
            Ok(Box::new(ClaudeAgent::new(candidate.planner_cmd.clone(), true)))
        }
        "codex" => {
            CodexAgent::check_available()?;
            Ok(Box::new(CodexAgent::new(candidate.planner_cmd.clone())))
        }
        "copilot" => {
            CopilotAgent::check_available()?;
            Ok(Box::new(CopilotAgent::new(candidate.planner_cmd.clone())))
        }
        other => bail!("Unsupported planner backend: {}", other),
    }
}

pub struct FallbackChatPlanner {
    candidates: Vec<PlannerCandidate>,
    spec_root: PathBuf,
    timeout_seconds: u64,
    state: std::sync::Mutex<PlannerState>,
}

struct PlannerState {
    current_index: usize,
    current: Box<dyn ChatPlanner>,
}

impl FallbackChatPlanner {
    pub fn new(
        candidates: Vec<PlannerCandidate>,
        spec_root: &PathBuf,
        timeout_seconds: u64,
    ) -> Result<Self> {
        let (current_index, current) = select_first_planner(&candidates, spec_root, timeout_seconds)?;
        Ok(Self {
            candidates,
            spec_root: spec_root.clone(),
            timeout_seconds,
            state: std::sync::Mutex::new(PlannerState { current_index, current }),
        })
    }

    fn advance(&self) -> Result<()> {
        let mut state = self.state.lock().expect("planner state lock poisoned");
        let mut next_index = state.current_index + 1;
        while next_index < self.candidates.len() {
            let candidate = &self.candidates[next_index];
            match build_planner(candidate, &self.spec_root, self.timeout_seconds) {
                Ok(next) => {
                    eprintln!(
                        "  ⚠️  Planner {} unavailable or rate-limited. Switching to {}.",
                        state.current.name(),
                        candidate.name
                    );
                    state.current = next;
                    state.current_index = next_index;
                    return Ok(());
                }
                Err(err) => {
                    eprintln!(
                        "  ⚠️  Planner {} unavailable: {}",
                        candidate.name, err
                    );
                    next_index += 1;
                }
            }
        }
        bail!("No planner candidates available");
    }
}

impl ChatPlanner for FallbackChatPlanner {
    fn plan_turn(&self, input: &crate::chat_plan::ChatTurnInput) -> Result<ChatPlan> {
        loop {
            let result = {
                let state = self.state.lock().expect("planner state lock poisoned");
                state.current.plan_turn(input)
            };
            match result {
                Ok(plan) => return Ok(plan),
                Err(err) => {
                    if should_fallback_on_planner_error(&err) {
                        self.advance()?;
                        continue;
                    }
                    return Err(err);
                }
            }
        }
    }

    fn name(&self) -> &'static str {
        let state = self.state.lock().expect("planner state lock poisoned");
        state.current.name()
    }
}

fn select_first_planner(
    candidates: &[PlannerCandidate],
    spec_root: &PathBuf,
    timeout_seconds: u64,
) -> Result<(usize, Box<dyn ChatPlanner>)> {
    let mut last_err: Option<anyhow::Error> = None;
    for (idx, candidate) in candidates.iter().enumerate() {
        match build_planner(candidate, spec_root, timeout_seconds) {
            Ok(planner) => return Ok((idx, planner)),
            Err(err) => {
                last_err = Some(err);
            }
        }
    }
    let err = last_err.unwrap_or_else(|| anyhow::anyhow!("No planner candidates available"));
    Err(err)
}

fn should_fallback_on_planner_error(err: &anyhow::Error) -> bool {
    let message = err.to_string().to_lowercase();
    let patterns = [
        "rate limit",
        "rate-limit",
        "ratelimit",
        "quota exceeded",
        "too many requests",
        "429",
        "hit your limit",
        "usage limit",
        "api limit",
        "insufficient credits",
        "out of credits",
        "payment required",
        "billing",
        "not found",
        "authentication",
        "auth",
        "login",
        "api key",
        "permission",
        "forbidden",
    ];
    patterns.iter().any(|p| message.contains(p))
}

pub fn should_fallback_on_outcome(outcome: &RunnerOutcome) -> bool {
    outcome.classification == OutcomeClassification::RateLimited
}

pub fn normalize_model_for_runner(
    runner_name: &str,
    model: Option<String>,
) -> Option<String> {
    let model = model?;
    if runner_name == "claude" {
        return Some(model);
    }
    if is_claude_tier_model(&model) {
        return None;
    }
    Some(model)
}

fn is_claude_tier_model(model: &str) -> bool {
    let lower = model.to_lowercase();
    matches!(lower.as_str(), "haiku" | "sonnet" | "opus") || lower.contains("claude-")
}

pub fn outcome_from_error(err: anyhow::Error) -> RunnerOutcome {
    RunnerOutcome {
        exit_code: None,
        classification: OutcomeClassification::EnvironmentError,
        elapsed_seconds: 0.0,
        stdout_path: None,
        stderr_path: None,
        error_message: Some(err.to_string()),
        token_usage: None,
    }
}
