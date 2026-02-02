//! GitHub Copilot CLI agent backend (runner + chat planner).

use crate::base_runner::run_cli_runner;
use crate::chat_plan::{ChatPlan, ChatTurnInput};
use crate::chat_planner::{BaseChatPlanner, ChatPlanner};
use crate::runner::{Runner, RunnerConfig, RunnerInvocation, RunnerOutcome};
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// Agent backend for GitHub Copilot CLI.
///
/// Runs missions via `Runner` and provides chat planning via `ChatPlanner`.
pub struct CopilotAgent {
    runner_command_template: String,
    planner: BaseChatPlanner,
}

impl CopilotAgent {
    /// Create a new CopilotAgent.
    ///
    /// If `runner_command` is None, uses a default `copilot -p` command.
    /// If `planner_command` is None, uses a default `copilot -p` command.
    pub fn new(
        runner_command: Option<String>,
        planner_command: Option<String>,
        planner_working_dir: PathBuf,
    ) -> Result<Self> {
        Self::new_with_timeout(runner_command, planner_command, planner_working_dir, None)
    }

    pub fn new_with_timeout(
        runner_command: Option<String>,
        planner_command: Option<String>,
        planner_working_dir: PathBuf,
        planner_timeout: Option<Duration>,
    ) -> Result<Self> {
        Self::new_with_timeout_and_stream(
            runner_command,
            planner_command,
            planner_working_dir,
            planner_timeout,
            false,
        )
    }

    pub fn new_with_timeout_and_stream(
        runner_command: Option<String>,
        planner_command: Option<String>,
        planner_working_dir: PathBuf,
        planner_timeout: Option<Duration>,
        stream_output: bool,
    ) -> Result<Self> {
        let runner_cmd = runner_command.unwrap_or_else(|| {
            // Default Copilot CLI command using -p for non-interactive mode.
            // Uses --allow-all for autonomous execution.
            // The prompt is passed via -p flag.
            "copilot -p {prompt_file} --allow-all --cwd {working_dir}".to_string()
        });
        let planner_cmd = planner_command.unwrap_or_else(|| {
            // Default Copilot planner command.
            // Uses xargs to read prompt from stdin and pass to -p flag.
            // --silent outputs only the agent response.
            // --allow-all for autonomous execution.
            "xargs -0 -I{} copilot -p {} --silent --allow-all".to_string()
        });

        Ok(Self {
            runner_command_template: runner_cmd,
            planner: BaseChatPlanner::new_with_timeout_and_stream(
                planner_cmd,
                planner_working_dir,
                planner_timeout,
                stream_output,
            ),
        })
    }

    /// Check if the Copilot CLI is available.
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

impl Runner for CopilotAgent {
    fn run(&self, mission_dir: &Path, config: &RunnerConfig) -> Result<RunnerOutcome> {
        let cmd = self.build_runner_command(mission_dir, config);
        run_cli_runner(&cmd, mission_dir, config, false)
    }

    fn name(&self) -> &'static str {
        "copilot"
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

impl ChatPlanner for CopilotAgent {
    fn plan_turn(&self, input: &ChatTurnInput) -> Result<ChatPlan> {
        self.planner.plan_turn(input)
    }

    fn name(&self) -> &'static str {
        "copilot"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_copilot_agent_expand_default() {
        let agent = CopilotAgent::new(None, None, PathBuf::from("/workspace")).unwrap();
        let expanded =
            agent.expand_runner_command(Path::new("/test/mission"), Path::new("/workspace"));
        assert!(expanded.contains("copilot"));
        assert!(expanded.contains("/workspace"));
    }

    #[test]
    fn test_copilot_agent_name() {
        let agent = CopilotAgent::new(None, None, PathBuf::from("/workspace")).unwrap();
        assert_eq!(crate::runner::Runner::name(&agent), "copilot");
    }
}
