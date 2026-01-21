//! Claude Code runner backend for v1.7 Runner Integration.

use crate::runner::{Runner, RunnerConfig, RunnerOutcome, OutcomeClassification};
use anyhow::{Result, bail};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use std::io::Read;

/// Runner backend for Claude Code.
///
/// This executes Claude CLI with the mission directory as input,
/// allowing autonomous coding agents to work on the specs repository.
pub struct ClaudeCodeRunner {
    /// Optional custom command template (use {mission_dir} placeholder).
    command_template: String,
}

impl ClaudeCodeRunner {
    /// Create a new ClaudeCodeRunner.
    ///
    /// If `command` is None, uses a default command.
    pub fn new(command: Option<String>) -> Result<Self> {
        let cmd = command.unwrap_or_else(|| {
            // Default Claude Code command
            // The runner will read MISSION.md from the mission directory
            "claude --print --dangerously-skip-permissions --input-file {mission_dir}/MISSION.md".to_string()
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

    /// Expand the command template with the mission directory.
    fn expand_command(&self, mission_dir: &Path) -> String {
        self.command_template
            .replace("{mission_dir}", &mission_dir.display().to_string())
    }
}

impl Runner for ClaudeCodeRunner {
    fn run(&self, mission_dir: &Path, config: &RunnerConfig) -> Result<RunnerOutcome> {
        let expanded = self.expand_command(mission_dir);
        let parts: Vec<&str> = expanded.split_whitespace().collect();

        if parts.is_empty() {
            return Ok(RunnerOutcome {
                exit_code: None,
                classification: OutcomeClassification::EnvironmentError,
                elapsed_seconds: 0.0,
                stdout_path: None,
                stderr_path: None,
                error_message: Some("Empty command".to_string()),
            });
        }

        let program = parts[0];
        let args: Vec<&str> = parts[1..].to_vec();

        let start = Instant::now();
        let timeout = Duration::from_secs(config.max_runtime_seconds as u64);

        // Set up environment variables
        let mission_dir_abs = std::fs::canonicalize(mission_dir)
            .unwrap_or_else(|_| mission_dir.to_path_buf());

        let mut cmd = Command::new(program);
        cmd.args(&args)
            .current_dir(&config.working_dir)
            .env("TESAKI_MISSION_DIR", &mission_dir_abs)
            .env("TESAKI_MODE", &config.mode)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return Ok(RunnerOutcome {
                    exit_code: None,
                    classification: OutcomeClassification::EnvironmentError,
                    elapsed_seconds: start.elapsed().as_secs_f64(),
                    stdout_path: None,
                    stderr_path: None,
                    error_message: Some(format!("Failed to start runner: {}", e)),
                });
            }
        };

        // Wait with timeout
        let output = match wait_with_timeout(child, timeout) {
            WaitResult::Completed(output) => output,
            WaitResult::Timeout => {
                return Ok(RunnerOutcome {
                    exit_code: None,
                    classification: OutcomeClassification::Timeout,
                    elapsed_seconds: start.elapsed().as_secs_f64(),
                    stdout_path: None,
                    stderr_path: None,
                    error_message: Some(format!(
                        "Runner exceeded timeout of {} seconds",
                        config.max_runtime_seconds
                    )),
                });
            }
            WaitResult::Error(e) => {
                return Ok(RunnerOutcome {
                    exit_code: None,
                    classification: OutcomeClassification::EnvironmentError,
                    elapsed_seconds: start.elapsed().as_secs_f64(),
                    stdout_path: None,
                    stderr_path: None,
                    error_message: Some(format!("Error waiting for runner: {}", e)),
                });
            }
        };

        let elapsed = start.elapsed().as_secs_f64();
        let exit_code = output.status.code();
        let classification = if output.status.success() {
            OutcomeClassification::Ok
        } else {
            OutcomeClassification::Failed
        };

        // Write stdout/stderr to mission RUNNER_OUTPUT/ if non-empty
        let output_dir = mission_dir.join("RUNNER_OUTPUT");
        let _ = std::fs::create_dir_all(&output_dir);
        let stdout_path = if !output.stdout.is_empty() {
            let path = output_dir.join("runner_stdout.txt");
            let _ = std::fs::write(&path, &output.stdout);
            Some(path.display().to_string())
        } else {
            None
        };

        let stderr_path = if !output.stderr.is_empty() {
            let path = output_dir.join("runner_stderr.txt");
            let _ = std::fs::write(&path, &output.stderr);
            Some(path.display().to_string())
        } else {
            None
        };

        Ok(RunnerOutcome {
            exit_code,
            classification,
            elapsed_seconds: elapsed,
            stdout_path,
            stderr_path,
            error_message: None,
        })
    }

    fn name(&self) -> &'static str {
        "claude"
    }
}

/// Result of waiting for a child process.
enum WaitResult {
    Completed(std::process::Output),
    Timeout,
    Error(std::io::Error),
}

/// Wait for a child process with a timeout.
fn wait_with_timeout(mut child: std::process::Child, timeout: Duration) -> WaitResult {
    // Simple polling approach for timeout
    let start = Instant::now();
    let poll_interval = Duration::from_millis(100);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // Process exited
                let stdout = child.stdout.take().map(|mut s| {
                    let mut buf = Vec::new();
                    let _ = s.read_to_end(&mut buf);
                    buf
                }).unwrap_or_default();

                let stderr = child.stderr.take().map(|mut s| {
                    let mut buf = Vec::new();
                    let _ = s.read_to_end(&mut buf);
                    buf
                }).unwrap_or_default();

                return WaitResult::Completed(std::process::Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            Ok(None) => {
                // Still running
                if start.elapsed() > timeout {
                    // Kill the process
                    let _ = child.kill();
                    let _ = child.wait(); // Reap the zombie
                    return WaitResult::Timeout;
                }
                std::thread::sleep(poll_interval);
            }
            Err(e) => return WaitResult::Error(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_code_runner_expand() {
        let runner = ClaudeCodeRunner::new(Some("echo {mission_dir}/MISSION.md".to_string())).unwrap();
        let expanded = runner.expand_command(Path::new("/test/mission"));
        assert_eq!(expanded, "echo /test/mission/MISSION.md");
    }
}
