//! Core LLM backend trait and blanket implementations for Runner and ChatPlanner.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::chat_plan::{ChatPlan, ChatTurnInput};
use crate::chat_planner::{ChatPlanner, format_planner_prompt, strip_markdown_code_fences};
use crate::runner::{OutcomeClassification, Runner, RunnerConfig, RunnerInvocation, RunnerOutcome};
use crate::token_usage::TokenUsage;

/// The core trait for any LLM provider.
///
/// If you implement this, you get `Runner` and `ChatPlanner` for free via
/// blanket implementations.
pub trait LLMBackend: Send + Sync {
    /// Execute a raw prompt against the LLM.
    fn execute(&self, request: &LLMRequest) -> Result<LLMResponse>;
    
    /// The display name of this agent.
    fn name(&self) -> &'static str;
    
    /// Optional: Describe how to invoke this as a CLI command.
    fn planned_invocation(&self, _request: &LLMRequest) -> Option<RunnerInvocation> {
        None
    }
}

/// Blanket implementation: Any LLMBackend is automatically a Runner.
impl<T: LLMBackend> Runner for T {
    fn name(&self) -> &'static str {
        self.name()
    }

    fn run(&self, mission_dir: &Path, config: &RunnerConfig) -> Result<RunnerOutcome> {
        let mission_path = mission_dir.join("MISSION.md");
        let prompt = std::fs::read_to_string(&mission_path)
            .with_context(|| format!("Failed to read mission at {}", mission_path.display()))?;

        let request = LLMRequest {
            prompt,
            input_file: Some(mission_path),
            output_file: None,
            model: config.model.clone(),
            working_dir: config.working_dir.clone(),
            timeout: Some(Duration::from_secs(config.max_runtime_seconds as u64)),
            stream_output: config.stream_output,
        };

        let resp = self.execute(&request)?;
        Ok(RunnerOutcome {
            exit_code: resp.exit_code,
            classification: resp.classification,
            elapsed_seconds: resp.elapsed_seconds,
            stdout_path: resp.stdout_path,
            stderr_path: resp.stderr_path,
            error_message: None,
            token_usage: resp.token_usage,
        })
    }

    fn planned_invocation(&self, mission_dir: &Path, config: &RunnerConfig) -> Option<RunnerInvocation> {
        let request = LLMRequest {
            prompt: String::new(),
            input_file: Some(mission_dir.join("MISSION.md")),
            output_file: None,
            model: config.model.clone(),
            working_dir: config.working_dir.clone(),
            timeout: None,
            stream_output: config.stream_output,
        };
        self.planned_invocation(&request)
    }
}

/// Blanket implementation: Any LLMBackend is automatically a ChatPlanner.
impl<T: LLMBackend> ChatPlanner for T {
    fn name(&self) -> &'static str {
        self.name()
    }

    fn plan_turn(&self, input: &ChatTurnInput) -> Result<ChatPlan> {
        let prompt = format_planner_prompt(input);
        let request = LLMRequest {
            prompt,
            input_file: None,
            output_file: None,
            model: None,
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            timeout: Some(Duration::from_secs(60)),
            stream_output: false,
        };

        let resp = self.execute(&request)?;
        let json_text = strip_markdown_code_fences(&resp.text);
        
        let plan: ChatPlan = serde_json::from_str(&json_text)
            .with_context(|| format!("LLM returned invalid JSON for plan: {}", resp.text))?;
        Ok(plan)
    }
}

/// Request sent to an LLM backend
#[derive(Debug, Clone)]
pub struct LLMRequest {
    pub prompt: String,
    pub input_file: Option<PathBuf>,
    pub output_file: Option<PathBuf>,
    pub model: Option<String>,
    pub working_dir: PathBuf,
    pub timeout: Option<Duration>,
    pub stream_output: bool,
}

/// Response from an LLM backend
#[derive(Debug, Clone)]
pub struct LLMResponse {
    pub text: String,
    pub exit_code: Option<i32>,
    pub classification: OutcomeClassification,
    pub stdout_path: Option<String>,
    pub stderr_path: Option<String>,
    pub token_usage: Option<TokenUsage>,
    pub elapsed_seconds: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_agent::MockAgent;

    #[test]
    fn test_agent_name() {
        let agent = MockAgent::success();
        assert_eq!(LLMBackend::name(&agent), "mock");
    }

    #[test]
    fn test_blanket_impls() {
        let agent = MockAgent::success();
        
        // Test Runner blanket impl
        let mission_dir = tempfile::tempdir().unwrap();
        let mission_path = mission_dir.path().join("MISSION.md");
        std::fs::write(&mission_path, "test mission").unwrap();
        
        let config = RunnerConfig {
            working_dir: PathBuf::from("."),
            max_runtime_seconds: 30,
            model: None,
            stream_output: false,
        };
        
        let outcome = Runner::run(&agent, mission_dir.path(), &config).unwrap();
        assert_eq!(outcome.classification, OutcomeClassification::Ok);
        
        // Test ChatPlanner blanket impl
        let input = ChatTurnInput {
            user_message: "hi".to_string(),
            session_state_json: serde_json::json!({}),
            recent_command_results: vec![],
            planner_hint: None,
            system_prompt: None,
        };
        
        let plan = ChatPlanner::plan_turn(&agent, &input).unwrap();
        assert_eq!(plan.say, "Mock success");
    }
}
