//! Core Servling trait and implementations.

use anyhow::{bail, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use serde::{Deserialize, Serialize};

use crate::outcome::OutcomeClassification;
use crate::token_usage::TokenUsage;
use crate::runner::{run_cli_runner, CliRunnerConfig};

/// The core trait for any AI agent provider.
pub trait Servling: Send + Sync {
    /// Execute a raw prompt against the LLM and return a standardized response.
    fn execute(&self, request: &LLMRequest) -> Result<LLMResponse>;
    
    /// The display name of this agent.
    fn name(&self) -> &'static str;
    
    /// Optional: Describe how to invoke this as a CLI command.
    fn planned_invocation(&self, _request: &LLMRequest) -> Option<RunnerInvocation> {
        None
    }
}

/// Implement Servling for Boxed trait objects to allow delegation.
impl Servling for Box<dyn Servling> {
    fn execute(&self, request: &LLMRequest) -> Result<LLMResponse> {
        (**self).execute(request)
    }

    fn name(&self) -> &'static str {
        (**self).name()
    }

    fn planned_invocation(&self, request: &LLMRequest) -> Option<RunnerInvocation> {
        (**self).planned_invocation(request)
    }
}

/// A standardized request to a Servling.
#[derive(Debug, Clone)]
pub struct LLMRequest {
    pub prompt: String,
    pub working_dir: PathBuf,
    pub model: Option<String>,
    pub max_runtime_seconds: u32,
    pub stream_output: bool,
    /// Optional: If the prompt is already stored in a file.
    pub input_file: Option<PathBuf>,
}

/// A standardized response from a Servling.
#[derive(Debug, Clone)]
pub struct LLMResponse {
    pub text: String,
    pub classification: OutcomeClassification,
    pub exit_code: Option<i32>,
    pub token_usage: Option<TokenUsage>,
    pub elapsed_seconds: f64,
    pub stdout_path: Option<String>,
    pub stderr_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerInvocation {
    pub program: String,
    pub args: Vec<String>,
    pub working_dir: String,
    pub env: Vec<(String, String)>,
}

/// A unified workhorse for any CLI-based LLM agent.
pub struct CliBackend {
    pub name: &'static str,
    pub command_template: String,
}

impl CliBackend {
    pub fn expand_command(&self, 
        base_cmd: &str, 
        working_dir: &Path,
        input_path: Option<&Path>, 
        output_path: Option<&Path>,
        model: Option<&str>
    ) -> String {
        let mut cmd = base_cmd.to_string();
        
        if let Some(m) = model {
            cmd = format!("{} --model {}", cmd, m);
        }

        let mission_dir = input_path.and_then(|p| p.parent()).unwrap_or(working_dir);
            
        cmd.replace("{input_file}", &input_path.map(|p| p.display().to_string()).unwrap_or_default())
           .replace("{mission_dir}", &mission_dir.display().to_string())
           .replace("{working_dir}", &working_dir.display().to_string())
           .replace("{output_file}", &output_path.map(|p| p.display().to_string()).unwrap_or_default())
    }

    pub fn prepare_temp_files(&self, prompt: &str) -> Result<(Option<tempfile::NamedTempFile>, Option<PathBuf>, Option<tempfile::NamedTempFile>, Option<PathBuf>)> {
        let mut temp_input = None;
        let mut input_path = None;
        if self.command_template.contains("{input_file}") || self.command_template.contains("{mission_dir}") {
            let mut file = tempfile::NamedTempFile::new()?;
            file.write_all(prompt.as_bytes())?;
            input_path = Some(file.path().to_path_buf());
            temp_input = Some(file);
        }

        let mut temp_output = None;
        let mut output_path = None;
        if self.command_template.contains("{output_file}") {
            let file = tempfile::NamedTempFile::new()?;
            output_path = Some(file.path().to_path_buf());
            temp_output = Some(file);
        }

        Ok((temp_input, input_path, temp_output, output_path))
    }

    pub fn execute_with_expansion(&self, request: &LLMRequest, extract_error: bool, model_expander: Option<fn(&str) -> String>) -> Result<LLMResponse> {
        let (temp_input, input_path, temp_output, output_path) = self.prepare_temp_files(&request.prompt)?;
        
        let model = request.model.as_deref().map(|m| {
            if let Some(expander) = model_expander {
                expander(m)
            } else {
                m.to_string()
            }
        });

        let cmd = self.expand_command(
            &self.command_template,
            &request.working_dir,
            input_path.as_deref().or(request.input_file.as_deref()),
            output_path.as_deref(),
            model.as_deref()
        );

        let config = CliRunnerConfig {
            working_dir: request.working_dir.clone(),
            max_runtime_seconds: request.max_runtime_seconds,
            stream_output: request.stream_output,
        };

        let mission_dir = input_path.as_deref()
            .or(request.input_file.as_deref())
            .and_then(|p| p.parent())
            .unwrap_or(&request.working_dir);

        let outcome = run_cli_runner(
            &cmd,
            mission_dir,
            &config,
            extract_error,
            if temp_input.is_some() || request.input_file.is_some() { None } else { Some(request.prompt.clone()) },
            input_path.as_deref().or(request.input_file.as_deref()),
            output_path.as_deref(),
        )?;

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
}

/// A high-level agent that orchestrates one or more backends with fallback logic.
pub struct CodingAgent {
    backends: Vec<Box<dyn Servling>>,
    current_index: Mutex<usize>,
}

impl CodingAgent {
    /// Start building a new CodingAgent.
    pub fn builder() -> CodingAgentBuilder {
        CodingAgentBuilder::default()
    }

    fn should_fallback(classification: OutcomeClassification) -> bool {
        matches!(classification, OutcomeClassification::RateLimited)
    }
}

/// Builder for Configuring a CodingAgent.
#[derive(Default)]
pub struct CodingAgentBuilder {
    backends: Vec<Box<dyn Servling>>,
}

impl CodingAgentBuilder {
    /// Register a backend. Order of registration defines priority.
    pub fn register(mut self, backend: Box<dyn Servling>) -> Self {
        self.backends.push(backend);
        self
    }

    /// Convenience for registering multiple backends.
    pub fn with_backends(mut self, backends: Vec<Box<dyn Servling>>) -> Self {
        self.backends.extend(backends);
        self
    }

    pub fn build(self) -> Result<CodingAgent> {
        if self.backends.is_empty() {
            bail!("CodingAgent must have at least one backend");
        }
        Ok(CodingAgent {
            backends: self.backends,
            current_index: Mutex::new(0),
        })
    }
}

impl Servling for CodingAgent {
    fn name(&self) -> &'static str {
        let idx = *self.current_index.lock().unwrap();
        self.backends[idx].name()
    }

    fn execute(&self, request: &LLMRequest) -> Result<LLMResponse> {
        loop {
            let (idx, backend) = {
                let current = *self.current_index.lock().unwrap();
                if current >= self.backends.len() {
                    bail!("No backends available in CodingAgent");
                }
                (current, &self.backends[current])
            };

            match backend.execute(request) {
                Ok(resp) if CodingAgent::should_fallback(resp.classification) => {
                    let mut current = self.current_index.lock().unwrap();
                    if *current == idx {
                        *current += 1;
                        if *current >= self.backends.len() {
                            return Ok(resp);
                        }
                        log::warn!("Backend {} rate limited. Falling back to next.", backend.name());
                    }
                    continue;
                }
                Ok(resp) => return Ok(resp),
                Err(err) => {
                    let mut current = self.current_index.lock().unwrap();
                    if *current == idx {
                        *current += 1;
                        if *current >= self.backends.len() {
                            return Err(err);
                        }
                        log::warn!("Backend {} failed: {}. Falling back.", backend.name(), err);
                    }
                    continue;
                }
            }
        }
    }

    fn planned_invocation(&self, request: &LLMRequest) -> Option<RunnerInvocation> {
        let idx = *self.current_index.lock().unwrap();
        self.backends.get(idx).and_then(|b| b.planned_invocation(request))
    }
}
