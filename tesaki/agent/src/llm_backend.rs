//! Core LLM backend trait and blanket implementations.
//!
//! This module provides the central `LLMBackend` trait. Any struct that implements 
//! this trait automatically gains `Runner` and `ChatPlanner` capabilities via 
//! blanket implementations.

use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::chat_plan::{ChatPlan, ChatTurnInput};
use crate::chat_planner::{format_planner_prompt, strip_markdown_code_fences, ChatPlanner};
use crate::runner::{OutcomeClassification, Runner, RunnerConfig, RunnerInvocation, RunnerOutcome};
use crate::token_usage::TokenUsage;
use crate::base_runner::run_cli_runner;

/// The core trait for any AI agent.
///
/// Implement this to define how to talk to a specific LLM (via CLI, API, etc.).
/// You get `Runner` and `ChatPlanner` for free.
pub trait LLMBackend: Send + Sync {
    /// Execute a raw prompt against the LLM and return a standardized response.
    fn execute(&self, request: &LLMRequest) -> Result<LLMResponse>;
    
    /// The display name of this backend.
    fn name(&self) -> &'static str;
    
    /// Optional: Describe how this backend would be invoked as a CLI command.
    fn planned_invocation(&self, _request: &LLMRequest) -> Option<RunnerInvocation> {
        None
    }
}

/// Blanket implementation: Every LLMBackend is a Runner.
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
            config: config.clone(),
            // Mission context
            input_file: Some(mission_path),
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
            config: config.clone(),
            input_file: Some(mission_dir.join("MISSION.md")),
        };
        self.planned_invocation(&request)
    }
}

/// Blanket implementation: Every LLMBackend is a ChatPlanner.
impl<T: LLMBackend> ChatPlanner for T {
    fn name(&self) -> &'static str {
        self.name()
    }

    fn plan_turn(&self, input: &ChatTurnInput) -> Result<ChatPlan> {
        let prompt = format_planner_prompt(input);
        let request = LLMRequest {
            prompt,
            config: RunnerConfig {
                working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                max_runtime_seconds: 60,
                model: None,
                stream_output: false,
            },
            input_file: None,
        };

        let resp = self.execute(&request)?;
        let json_text = strip_markdown_code_fences(&resp.text);
        
        let plan: ChatPlan = serde_json::from_str(&json_text)
            .with_context(|| format!("LLM returned invalid JSON for plan: {}", resp.text))?;
        Ok(plan)
    }
}

/// A standardized request to an LLM.
#[derive(Debug, Clone)]
pub struct LLMRequest {
    pub prompt: String,
    pub config: RunnerConfig,
    /// Optional: If the prompt is already stored in a file (for CLI efficiency).
    pub input_file: Option<PathBuf>,
}

/// A standardized response from an LLM.
#[derive(Debug, Clone)]
pub struct LLMResponse {
    pub text: String,
    pub classification: OutcomeClassification,
    pub exit_code: Option<i32>,
    pub token_usage: Option<TokenUsage>,
    pub elapsed_seconds: f64,
    /// Optional paths if the output was captured to files (standard for CLI).
    pub stdout_path: Option<String>,
    pub stderr_path: Option<String>,
}

/// A unified workhorse for any CLI-based LLM agent.
///
/// Handles command expansion, temp files, and result extraction.
pub struct CliBackend {
    pub name: &'static str,
    pub command_template: String,
    pub extract_error: bool,
}

impl CliBackend {
    pub fn execute_with_expansion(&self, request: &LLMRequest, model_expander: Option<fn(&str) -> String>) -> Result<LLMResponse> {
        let mut cmd = self.command_template.clone();
        
        // 1. Expand model
        if let Some(model) = &request.config.model {
            let model_arg = model_expander.map(|f| f(model)).unwrap_or_else(|| model.clone());
            cmd = format!("{} --model {}", cmd, model_arg);
        }

        // 2. Handle temp files if template needs them but they aren't provided
        let mut temp_input = None;
        let mut input_path = request.input_file.clone();
        
        if input_path.is_none() && (cmd.contains("{input_file}") || cmd.contains("{mission_dir}")) {
            let mut file = tempfile::NamedTempFile::new()?;
            file.write_all(request.prompt.as_bytes())?;
            input_path = Some(file.path().to_path_buf());
            temp_input = Some(file);
        }

        let mut temp_output = None;
        let mut output_path = None;
        if cmd.contains("{output_file}") {
            let file = tempfile::NamedTempFile::new()?;
            output_path = Some(file.path().to_path_buf());
            temp_output = Some(file);
        }

        // 3. Final expansion
        let mission_dir = input_path.as_deref()
            .and_then(|p| p.parent())
            .unwrap_or(&request.config.working_dir);
            
        let expanded_cmd = cmd
            .replace("{input_file}", &input_path.as_deref().map(|p| p.display().to_string()).unwrap_or_default())
            .replace("{mission_dir}", &mission_dir.display().to_string())
            .replace("{working_dir}", &request.config.working_dir.display().to_string())
            .replace("{output_file}", &output_path.as_deref().map(|p| p.display().to_string()).unwrap_or_default());

        // 4. Execution
        let outcome = run_cli_runner(
            &expanded_cmd,
            mission_dir,
            &request.config,
            self.extract_error,
            if temp_input.is_some() || request.input_file.is_some() { None } else { Some(request.prompt.clone()) },
            input_path.as_deref(),
            output_path.as_deref(),
        )?;

        // 5. Result extraction
        let text = if let Some(out_p) = output_path {
            std::fs::read_to_string(out_p).unwrap_or_default()
        } else {
            outcome.stdout_path.as_ref()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .unwrap_or_default()
        };

        drop(temp_input);
        drop(temp_output);

        Ok(LLMResponse {
            text,
            exit_code: outcome.exit_code,
            classification: outcome.classification,
            stdout_path: outcome.stdout_path,
            stderr_path: outcome.stderr_path,
            token_usage: outcome.token_usage,
            elapsed_seconds: outcome.elapsed_seconds,
        })
    }

    pub fn planned_invocation(&self, request: &LLMRequest, model_expander: Option<fn(&str) -> String>) -> Option<RunnerInvocation> {
        let mut cmd = self.command_template.clone();
        if let Some(model) = &request.config.model {
            let model_arg = model_expander.map(|f| f(model)).unwrap_or_else(|| model.clone());
            cmd = format!("{} --model {}", cmd, model_arg);
        }

        let mission_dir = request.input_file.as_ref()
            .and_then(|p| p.parent())
            .unwrap_or(&request.config.working_dir);
            
        let expanded = cmd
            .replace("{input_file}", &request.input_file.as_deref().map(|p| p.display().to_string()).unwrap_or_default())
            .replace("{mission_dir}", &mission_dir.display().to_string())
            .replace("{working_dir}", &request.config.working_dir.display().to_string());

        let parts: Vec<String> = expanded.split_whitespace().map(|s| s.to_string()).collect();
        if parts.is_empty() { return None; }

        let mission_dir_abs = std::fs::canonicalize(mission_dir).unwrap_or_else(|_| mission_dir.to_path_buf());

        Some(RunnerInvocation {
            program: parts[0].clone(),
            args: parts[1..].to_vec(),
            working_dir: request.config.working_dir.display().to_string(),
            env: vec![("TESAKI_MISSION_DIR".to_string(), mission_dir_abs.display().to_string())],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_agent::MockAgent;

    #[test]
    fn test_blanket_impls() {
        let agent = MockAgent::success();
        
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
