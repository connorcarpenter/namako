//! GitHub Copilot CLI agent.

use anyhow::{bail, Result};
use std::process::{Command, Stdio};
use crate::llm_backend::{LLMBackend, LLMRequest, LLMResponse, CliBackend};
use crate::runner::RunnerInvocation;

pub struct CopilotAgent {
    cli: CliBackend,
}

impl CopilotAgent {
    pub fn new(command: Option<String>) -> Self {
        let template = command.unwrap_or_else(|| {
            "copilot -p @{input_file} --allow-all --add-dir {working_dir}".to_string()
        });
        Self {
            cli: CliBackend {
                name: "copilot",
                command_template: template,
                extract_error: false,
            },
        }
    }

    pub fn check_available() -> Result<()> {
        let output = Command::new("copilot")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        match output {
            Ok(status) if status.success() => Ok(()),
            Ok(_) => bail!("Copilot CLI returned non-zero exit code"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                bail!("Copilot CLI not found. Please install GitHub Copilot CLI.")
            }
            Err(e) => bail!("Failed to check Copilot CLI availability: {}", e),
        }
    }
}

impl LLMBackend for CopilotAgent {
    fn name(&self) -> &'static str {
        "copilot"
    }

    fn execute(&self, request: &LLMRequest) -> Result<LLMResponse> {
        self.cli.execute_with_expansion(request, Some(expand_model_name))
    }

    fn planned_invocation(&self, request: &LLMRequest) -> Option<RunnerInvocation> {
        self.cli.planned_invocation(request, Some(expand_model_name))
    }
}

fn expand_model_name(tier: &str) -> String {
    match tier.to_lowercase().as_str() {
        "opus" => "claude-opus-4.5".to_string(),
        "sonnet" => "claude-sonnet-4.5".to_string(),
        "haiku" => "claude-haiku-4.5".to_string(),
        _ => tier.to_string(),
    }
}
