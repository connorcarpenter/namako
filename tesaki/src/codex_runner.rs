//! Codex CLI runner backend for v1.7 Runner Integration.

use crate::runner::{OutcomeClassification, Runner, RunnerConfig, RunnerOutcome};
use anyhow::{bail, Result};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Runner backend for Codex CLI.
///
/// This executes Codex CLI with the mission directory as input,
/// allowing autonomous coding agents to work on the specs repository.
pub struct CodexRunner {
    /// Optional custom command template (use {mission_dir} placeholder).
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
}

impl Runner for CodexRunner {
    fn run(&self, mission_dir: &Path, config: &RunnerConfig) -> Result<RunnerOutcome> {
        let expanded = self.expand_command(mission_dir, &config.working_dir);
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
        let mission_dir_abs =
            std::fs::canonicalize(mission_dir).unwrap_or_else(|_| mission_dir.to_path_buf());

        // Read the MISSION.md file to provide as stdin prompt
        let mission_path = mission_dir.join("MISSION.md");
        let prompt = match std::fs::read_to_string(&mission_path) {
            Ok(content) => content,
            Err(e) => {
                return Ok(RunnerOutcome {
                    exit_code: None,
                    classification: OutcomeClassification::EnvironmentError,
                    elapsed_seconds: start.elapsed().as_secs_f64(),
                    stdout_path: None,
                    stderr_path: None,
                    error_message: Some(format!(
                        "Failed to read MISSION.md from {}: {}",
                        mission_path.display(),
                        e
                    )),
                });
            }
        };

        let mut cmd = Command::new(program);
        cmd.args(&args)
            .current_dir(&config.working_dir)
            .env("TESAKI_MISSION_DIR", &mission_dir_abs)
            .env("TESAKI_MODE", &config.mode)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
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

        // Write the prompt to stdin
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            if let Err(e) = stdin.write_all(prompt.as_bytes()) {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(RunnerOutcome {
                    exit_code: None,
                    classification: OutcomeClassification::EnvironmentError,
                    elapsed_seconds: start.elapsed().as_secs_f64(),
                    stdout_path: None,
                    stderr_path: None,
                    error_message: Some(format!("Failed to write prompt to stdin: {}", e)),
                });
            }
            // stdin is dropped here, closing the pipe
        }

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
        "codex"
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
                let stdout = child
                    .stdout
                    .take()
                    .map(|mut s| {
                        let mut buf = Vec::new();
                        let _ = s.read_to_end(&mut buf);
                        buf
                    })
                    .unwrap_or_default();

                let stderr = child
                    .stderr
                    .take()
                    .map(|mut s| {
                        let mut buf = Vec::new();
                        let _ = s.read_to_end(&mut buf);
                        buf
                    })
                    .unwrap_or_default();

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
