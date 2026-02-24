//! Claude Code agent.

use anyhow::{bail, Result};
use std::process::{Command, Stdio};
use crate::llm_backend::{LLMBackend, LLMRequest, LLMResponse, CliBackend};
use crate::runner::RunnerInvocation;

pub struct ClaudeAgent {
    cli: CliBackend,
}

impl ClaudeAgent {
    pub fn new(command: Option<String>, stream_output: bool) -> Self {
        let template = command.unwrap_or_else(|| {
            if stream_output {
                "claude --print --dangerously-skip-permissions --output-format stream-json --include-partial-messages --verbose".to_string()
            } else {
                "claude --print --dangerously-skip-permissions".to_string()
            }
        });
        Self {
            cli: CliBackend {
                name: "claude",
                command_template: template,
                extract_error: true,
            },
        }
    }

    pub fn check_available() -> Result<()> {
        let output = Command::new("claude")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        match output {
            Ok(status) if status.success() => Ok(()),
            Ok(_) => bail!("Claude CLI returned non-zero exit code"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                bail!("Claude CLI not found. Please install Claude Code CLI.")
            }
            Err(e) => bail!("Failed to check Claude CLI availability: {}", e),
        }
    }
}

impl LLMBackend for ClaudeAgent {
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
