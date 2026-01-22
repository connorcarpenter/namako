//! Claude Code runner backend for v1.7 Runner Integration.

use crate::base_runner::run_cli_runner;
use crate::runner::{Runner, RunnerConfig, RunnerOutcome, RunnerInvocation};
use anyhow::{Result, bail};
use std::path::Path;
use std::process::{Command, Stdio};

/// Runner backend for Claude Code.
///
/// This executes Claude CLI with the mission directory as input,
/// allowing autonomous coding agents to work on the specs repository.
pub struct ClaudeCodeRunner {
    /// Optional custom command template (use {mission_dir} and {working_dir} placeholders).
    command_template: String,
}

impl ClaudeCodeRunner {
    /// Create a new ClaudeCodeRunner.
    ///
    /// If `command` is None, uses a default command.
    pub fn new(command: Option<String>) -> Result<Self> {
        let cmd = command.unwrap_or_else(|| {
            // Default Claude Code command
            // The runner reads MISSION.md and sends it via stdin.
            "claude --print --dangerously-skip-permissions".to_string()
        });

        Ok(Self {
            command_template: cmd,
        })
    }

    /// Check if the Claude CLI is available.
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

    /// Expand the command template with the mission directory and working directory.
    fn expand_command(&self, mission_dir: &Path, working_dir: &Path) -> String {
        self.command_template
            .replace("{mission_dir}", &mission_dir.display().to_string())
            .replace("{working_dir}", &working_dir.display().to_string())
    }

    /// Build the full command with optional model argument.
    fn build_command(&self, mission_dir: &Path, config: &RunnerConfig) -> String {
        let base = self.expand_command(mission_dir, &config.working_dir);
        match &config.model {
            Some(model) => format!("{} --model {}", base, model),
            None => base,
        }
    }
}

impl Runner for ClaudeCodeRunner {
    fn run(&self, mission_dir: &Path, config: &RunnerConfig) -> Result<RunnerOutcome> {
        let cmd = self.build_command(mission_dir, config);
        run_cli_runner(&cmd, mission_dir, config, true)
    }

    fn name(&self) -> &'static str {
        "claude"
    }

    fn planned_invocation(&self, mission_dir: &Path, config: &RunnerConfig) -> Option<RunnerInvocation> {
        let cmd = self.build_command(mission_dir, config);
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }
        let program = parts[0].to_string();
        let args = parts[1..].iter().map(|s| s.to_string()).collect();
        let mission_dir_abs = std::fs::canonicalize(mission_dir)
            .unwrap_or_else(|_| mission_dir.to_path_buf());
        Some(RunnerInvocation {
            program,
            args,
            working_dir: config.working_dir.display().to_string(),
            env: vec![
                ("TESAKI_MISSION_DIR".to_string(), mission_dir_abs.display().to_string()),
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_code_runner_expand() {
        let runner = ClaudeCodeRunner::new(Some("echo {mission_dir}/MISSION.md".to_string())).unwrap();
        let expanded = runner.expand_command(Path::new("/test/mission"), Path::new("/workspace"));
        assert_eq!(expanded, "echo /test/mission/MISSION.md");
    }
}
