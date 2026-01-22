//! Codex CLI runner backend for v1.7 Runner Integration.

use crate::base_runner::run_cli_runner;
use crate::runner::{Runner, RunnerConfig, RunnerOutcome, RunnerInvocation};
use anyhow::{bail, Result};
use std::path::Path;
use std::process::{Command, Stdio};

/// Runner backend for Codex CLI.
///
/// This executes Codex CLI with the mission directory as input,
/// allowing autonomous coding agents to work on the specs repository.
pub struct CodexRunner {
    /// Optional custom command template (use {mission_dir} and {working_dir} placeholders).
    command_template: String,
}

impl CodexRunner {
    /// Create a new CodexRunner.
    ///
    /// If `command` is None, uses a default command using `codex exec`.
    pub fn new(command: Option<String>) -> Result<Self> {
        let cmd = command.unwrap_or_else(|| {
            // Default Codex CLI command using exec subcommand for non-interactive mode
            // Uses --dangerously-bypass-approvals-and-sandbox for autonomous execution
            // Reads prompt from stdin via the `-` argument
            "codex exec --dangerously-bypass-approvals-and-sandbox -C {working_dir} -".to_string()
        });

        Ok(Self {
            command_template: cmd,
        })
    }

    /// Check if the Codex CLI is available.
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

impl Runner for CodexRunner {
    fn run(&self, mission_dir: &Path, config: &RunnerConfig) -> Result<RunnerOutcome> {
        let cmd = self.build_command(mission_dir, config);
        run_cli_runner(&cmd, mission_dir, config, false)
    }

    fn name(&self) -> &'static str {
        "codex"
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
    fn test_codex_runner_expand_default() {
        let runner = CodexRunner::new(None).unwrap();
        let expanded = runner.expand_command(Path::new("/test/mission"), Path::new("/workspace"));
        assert_eq!(
            expanded,
            "codex exec --dangerously-bypass-approvals-and-sandbox -C /workspace -"
        );
    }

    #[test]
    fn test_codex_runner_expand_custom() {
        let runner = CodexRunner::new(Some(
            "codex exec -C {working_dir} --full-auto < {mission_dir}/MISSION.md".to_string(),
        ))
        .unwrap();
        let expanded = runner.expand_command(Path::new("/test/mission"), Path::new("/workspace"));
        assert_eq!(
            expanded,
            "codex exec -C /workspace --full-auto < /test/mission/MISSION.md"
        );
    }

    #[test]
    fn test_codex_runner_name() {
        let runner = CodexRunner::new(None).unwrap();
        assert_eq!(runner.name(), "codex");
    }
}
