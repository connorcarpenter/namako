//! Codex CLI agent.

use anyhow::{bail, Result};
use std::process::{Command, Stdio};
use crate::llm_backend::{LLMBackend, LLMRequest, LLMResponse, CliBackend};
use crate::runner::RunnerInvocation;

pub struct CodexAgent {
    cli: CliBackend,
}

impl CodexAgent {
    pub fn new(command: Option<String>) -> Self {
        let template = command.unwrap_or_else(|| {
            "codex exec --dangerously-bypass-approvals-and-sandbox -C {working_dir} -".to_string()
        });
        Self {
            cli: CliBackend {
                name: "codex",
                command_template: template,
                extract_error: false,
            },
        }
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
}

impl LLMBackend for CodexAgent {
    fn name(&self) -> &'static str {
        self.cli.name
    }

    fn execute(&self, request: &LLMRequest) -> Result<LLMResponse> {
        self.cli.execute_with_expansion(request, None)
    }

    fn planned_invocation(&self, request: &LLMRequest) -> Option<RunnerInvocation> {
        self.cli.planned_invocation(request, None)
    }
}
