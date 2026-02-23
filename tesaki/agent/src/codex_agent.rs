//! Codex CLI agent.

use anyhow::{bail, Result};
use std::io::Write;
use std::process::{Command, Stdio};

use crate::base_runner::run_cli_runner;
use crate::llm_backend::{LLMBackend, LLMRequest, LLMResponse};
use crate::runner::RunnerInvocation;

pub struct CodexAgent {
    command_template: String,
}

impl CodexAgent {
    pub fn new(command: Option<String>) -> Self {
        let command_template = command.unwrap_or_else(|| {
            "codex exec --dangerously-bypass-approvals-and-sandbox -C {working_dir} -".to_string()
        });
        Self { command_template }
    }

    pub fn check_available() -> Result<()> {
        let output = Command::new("codex")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        match output {
            Ok(status) if status.success() => Ok(()),
            Ok(_) => bail!("Codex CLI returned non-zero exit code"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                bail!("Codex CLI not found. Please install Codex CLI.")
            }
            Err(e) => bail!("Failed to check Codex CLI availability: {}", e),
        }
    }

    fn expand_command(&self, request: &LLMRequest) -> String {
        let mut cmd = self.command_template.clone();
        if let Some(model) = &request.model {
            cmd = format!("{} --model {}", cmd, model);
        }
        
        let mission_dir = request.input_file.as_ref()
            .and_then(|p| p.parent())
            .unwrap_or(&request.working_dir);
            
        cmd.replace("{mission_dir}", &mission_dir.display().to_string())
           .replace("{working_dir}", &request.working_dir.display().to_string())
    }
}

impl LLMBackend for CodexAgent {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn execute(&self, request: &LLMRequest) -> Result<LLMResponse> {
        let cmd = self.expand_command(request);
        
        let wants_input_file = self.command_template.contains("{input_file}");
        let wants_output_file = self.command_template.contains("{output_file}");

        let mut temp_input = None;
        let mut input_path = request.input_file.clone();
        
        if wants_input_file && input_path.is_none() {
            let mut file = tempfile::NamedTempFile::new()?;
            file.write_all(request.prompt.as_bytes())?;
            input_path = Some(file.path().to_path_buf());
            temp_input = Some(file);
        }

        let mut temp_output = None;
        let mut output_path = None;
        if wants_output_file {
            let file = tempfile::NamedTempFile::new()?;
            output_path = Some(file.path().to_path_buf());
            temp_output = Some(file);
        }

        let mission_dir = input_path.as_deref()
            .and_then(|p| p.parent())
            .unwrap_or(&request.working_dir);

        let outcome = run_cli_runner(
            &cmd,
            mission_dir,
            &crate::runner::RunnerConfig {
                working_dir: request.working_dir.clone(),
                max_runtime_seconds: request.timeout.map(|d| d.as_secs() as u32).unwrap_or(300),
                model: request.model.clone(),
                stream_output: request.stream_output,
            },
            false,
            if temp_input.is_some() || request.input_file.is_some() { None } else { Some(request.prompt.clone()) },
            input_path.as_deref(),
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

    fn planned_invocation(&self, request: &LLMRequest) -> Option<RunnerInvocation> {
        let cmd = self.expand_command(request);
        let parts: Vec<String> = cmd.split_whitespace().map(|s| s.to_string()).collect();
        if parts.is_empty() { return None; }

        let mission_dir = request.input_file.as_ref()
            .and_then(|p| p.parent())
            .unwrap_or(&request.working_dir);
        let mission_dir_abs = std::fs::canonicalize(mission_dir).unwrap_or_else(|_| mission_dir.to_path_buf());

        Some(RunnerInvocation {
            program: parts[0].clone(),
            args: parts[1..].to_vec(),
            working_dir: request.working_dir.display().to_string(),
            env: vec![("TESAKI_MISSION_DIR".to_string(), mission_dir_abs.display().to_string())],
        })
    }
}
