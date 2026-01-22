//! Claude Code agent backend (runner + chat planner).

use crate::base_runner::run_cli_runner;
use crate::chat_plan::{ChatPlan, ChatTurnInput};
use crate::chat_planner::{ChatPlanner, CmdChatPlanner};
use crate::runner::{Runner, RunnerConfig, RunnerOutcome, RunnerInvocation};
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// Agent backend for Claude Code.
///
/// Runs missions via `Runner` and provides chat planning via `ChatPlanner`.
pub struct ClaudeCodeAgent {
    runner_command_template: String,
    planner: CmdChatPlanner,
}

impl ClaudeCodeAgent {
    /// Create a new ClaudeCodeAgent.
    ///
    /// If `runner_command` is None, uses a default Claude CLI command.
    /// If `planner_command` is None, uses a default Claude CLI command (stdin input).
    pub fn new(
        runner_command: Option<String>,
        planner_command: Option<String>,
        planner_working_dir: PathBuf,
    ) -> Result<Self> {
        Self::new_with_timeout(
            runner_command,
            planner_command,
            planner_working_dir,
            None,
        )
    }

    pub fn new_with_timeout(
        runner_command: Option<String>,
        planner_command: Option<String>,
        planner_working_dir: PathBuf,
        planner_timeout: Option<Duration>,
    ) -> Result<Self> {
        let runner_cmd = runner_command.unwrap_or_else(|| {
            // Default Claude Code command.
            // The runner reads MISSION.md and sends it via stdin.
            "claude --print --dangerously-skip-permissions".to_string()
        });
        let planner_cmd = planner_command.unwrap_or_else(|| {
            // Default Claude planner command uses stdin input.
            "claude --print --dangerously-skip-permissions".to_string()
        });

        Ok(Self {
            runner_command_template: runner_cmd,
            planner: CmdChatPlanner::new_with_timeout(
                planner_cmd,
                planner_working_dir,
                planner_timeout,
            ),
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

    /// Expand the runner command template with the mission directory and working directory.
    fn expand_runner_command(&self, mission_dir: &Path, working_dir: &Path) -> String {
        self.runner_command_template
            .replace("{mission_dir}", &mission_dir.display().to_string())
            .replace("{working_dir}", &working_dir.display().to_string())
    }

    /// Build the full runner command with optional model argument.
    fn build_runner_command(&self, mission_dir: &Path, config: &RunnerConfig) -> String {
        let base = self.expand_runner_command(mission_dir, &config.working_dir);
        match &config.model {
            Some(model) => format!("{} --model {}", base, model),
            None => base,
        }
    }
}

impl Runner for ClaudeCodeAgent {
    fn run(&self, mission_dir: &Path, config: &RunnerConfig) -> Result<RunnerOutcome> {
        let cmd = self.build_runner_command(mission_dir, config);
        run_cli_runner(&cmd, mission_dir, config, true)
    }

    fn name(&self) -> &'static str {
        "claude"
    }

    fn planned_invocation(
        &self,
        mission_dir: &Path,
        config: &RunnerConfig,
    ) -> Option<RunnerInvocation> {
        let cmd = self.build_runner_command(mission_dir, config);
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }
        let program = parts[0].to_string();
        let args = parts[1..].iter().map(|s| s.to_string()).collect();
        let mission_dir_abs =
            std::fs::canonicalize(mission_dir).unwrap_or_else(|_| mission_dir.to_path_buf());
        Some(RunnerInvocation {
            program,
            args,
            working_dir: config.working_dir.display().to_string(),
            env: vec![(
                "TESAKI_MISSION_DIR".to_string(),
                mission_dir_abs.display().to_string(),
            )],
        })
    }
}

impl ChatPlanner for ClaudeCodeAgent {
    fn plan_turn(&self, input: &ChatTurnInput) -> Result<ChatPlan> {
        self.planner.plan_turn(input)
    }

    fn name(&self) -> &'static str {
        "claude"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_code_agent_expand() {
        let agent = ClaudeCodeAgent::new(
            Some("echo {mission_dir}/MISSION.md".to_string()),
            None,
            PathBuf::from("/workspace"),
        )
        .unwrap();
        let expanded =
            agent.expand_runner_command(Path::new("/test/mission"), Path::new("/workspace"));
        assert_eq!(expanded, "echo /test/mission/MISSION.md");
    }
}
