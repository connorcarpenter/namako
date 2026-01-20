//! Runner abstraction and backends for v1.7 Runner Integration.
//!
//! Per GOLD_PLAN.md §10.7.4, the runner is an internal Tesaki abstraction.
//! Claude Code is the first concrete backend implementation.
//!
//! # Important: Runner Scope
//!
//! The runner operates on the **specs repository only**. It executes missions
//! that modify project files according to the configured edit surfaces.
//! The runner NEVER edits Namako/Tesaki toolchain code.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Configuration for runner execution.
#[derive(Debug, Clone)]
pub struct RunnerConfig {
    /// Maximum runtime in seconds before killing the runner.
    pub max_runtime_seconds: u32,

    /// Working directory for the runner (typically the workspace root).
    pub working_dir: std::path::PathBuf,

    /// Operating mode (BOOTSTRAP or CONSUMPTION).
    pub mode: String,
}

/// Outcome of a runner execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerOutcome {
    /// Exit status of the runner (0 = success).
    pub exit_code: Option<i32>,

    /// Classification of the outcome.
    pub classification: OutcomeClassification,

    /// Elapsed time in seconds.
    pub elapsed_seconds: f64,

    /// Path to captured stdout (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout_path: Option<String>,

    /// Path to captured stderr (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr_path: Option<String>,

    /// Error message (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// Classification of runner outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OutcomeClassification {
    /// Runner completed successfully (exit code 0).
    Ok,
    /// Runner exited with non-zero status.
    Failed,
    /// Runner exceeded time budget.
    Timeout,
    /// Runner could not be started (command not found, etc.).
    EnvironmentError,
}

/// Trait for runner backends.
///
/// Runners execute missions and return outcomes. They are stateless
/// and receive all context via the mission directory.
pub trait Runner: Send + Sync {
    /// Execute the runner against a mission bundle.
    ///
    /// The runner should:
    /// 1. Read NEXT_TASK.md for instructions
    /// 2. Read POLICY.md for constraints
    /// 3. Perform the requested work
    /// 4. Write attempt_report.md to OUTPUT/
    ///
    /// Returns the outcome of the execution.
    fn run(&self, mission_dir: &Path, config: &RunnerConfig) -> Result<RunnerOutcome>;

    /// Return the name of this runner backend.
    fn name(&self) -> &'static str;
}

// ============================================================================
// CommandRunner — Generic command-based runner
// ============================================================================

/// A runner that executes a configured shell command.
///
/// The command template can include `{mission_dir}` which will be replaced
/// with the absolute path to the mission directory.
pub struct CommandRunner {
    /// Command template (e.g., "claude --mission {mission_dir}").
    command_template: String,
}

impl CommandRunner {
    /// Create a new CommandRunner with the given command template.
    pub fn new(command_template: impl Into<String>) -> Self {
        Self {
            command_template: command_template.into(),
        }
    }

    /// Expand the command template with the mission directory.
    fn expand_command(&self, mission_dir: &Path) -> String {
        self.command_template
            .replace("{mission_dir}", &mission_dir.display().to_string())
    }
}

impl Runner for CommandRunner {
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

        // Write stdout/stderr to mission OUTPUT/ if non-empty
        let output_dir = mission_dir.join("OUTPUT");
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
        "command"
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
                    use std::io::Read;
                    let mut buf = Vec::new();
                    let _ = s.read_to_end(&mut buf);
                    buf
                }).unwrap_or_default();

                let stderr = child.stderr.take().map(|mut s| {
                    use std::io::Read;
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

// ============================================================================
// ClaudeCodeRunner — Claude Code specific runner
// ============================================================================

/// Runner backend for Claude Code.
///
/// This is a thin wrapper over CommandRunner with Claude-specific defaults.
pub struct ClaudeCodeRunner {
    inner: CommandRunner,
}

impl ClaudeCodeRunner {
    /// Create a new ClaudeCodeRunner.
    ///
    /// If `command` is None, uses a default command.
    pub fn new(command: Option<String>) -> Result<Self> {
        let cmd = command.unwrap_or_else(|| {
            // Default Claude Code command
            // The runner will read NEXT_TASK.md from the mission directory
            "claude --print --dangerously-skip-permissions --input-file {mission_dir}/NEXT_TASK.md".to_string()
        });

        Ok(Self {
            inner: CommandRunner::new(cmd),
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
}

impl Runner for ClaudeCodeRunner {
    fn run(&self, mission_dir: &Path, config: &RunnerConfig) -> Result<RunnerOutcome> {
        self.inner.run(mission_dir, config)
    }

    fn name(&self) -> &'static str {
        "claude"
    }
}

// ============================================================================
// MockRunner — For testing
// ============================================================================

/// Mock runner for testing.
///
/// Behavior can be configured to simulate various outcomes.
pub struct MockRunner {
    /// Whether to succeed (exit 0) or fail (exit 1).
    pub should_succeed: bool,

    /// Whether to write an attempt report.
    pub write_attempt_report: bool,

    /// Optional file to create in the working directory (simulates edits).
    pub create_file: Option<(String, String)>,

    /// Simulated execution time in seconds.
    pub simulated_time: f64,
}

impl Default for MockRunner {
    fn default() -> Self {
        Self {
            should_succeed: true,
            write_attempt_report: true,
            create_file: None,
            simulated_time: 0.5,
        }
    }
}

impl MockRunner {
    /// Create a mock runner that succeeds.
    pub fn success() -> Self {
        Self::default()
    }

    /// Create a mock runner that fails.
    pub fn failure() -> Self {
        Self {
            should_succeed: false,
            ..Default::default()
        }
    }

    /// Configure to create a file (simulating edits).
    pub fn with_file(mut self, path: impl Into<String>, content: impl Into<String>) -> Self {
        self.create_file = Some((path.into(), content.into()));
        self
    }
}

impl Runner for MockRunner {
    fn run(&self, mission_dir: &Path, _config: &RunnerConfig) -> Result<RunnerOutcome> {
        // Simulate execution time
        std::thread::sleep(Duration::from_secs_f64(self.simulated_time));

        // Write attempt report if configured
        if self.write_attempt_report {
            let report_path = mission_dir.join("OUTPUT/attempt_report.md");
            let content = if self.should_succeed {
                "# Attempt Report\n\nMission completed successfully (mock).\n"
            } else {
                "# Attempt Report\n\nMission failed (mock).\n"
            };
            std::fs::write(&report_path, content)
                .context("Failed to write mock attempt report")?;
        }

        // Create file if configured
        if let Some((path, content)) = &self.create_file {
            std::fs::write(path, content)
                .context("Failed to create mock file")?;
        }

        let (exit_code, classification) = if self.should_succeed {
            (Some(0), OutcomeClassification::Ok)
        } else {
            (Some(1), OutcomeClassification::Failed)
        };

        Ok(RunnerOutcome {
            exit_code,
            classification,
            elapsed_seconds: self.simulated_time,
            stdout_path: None,
            stderr_path: None,
            error_message: None,
        })
    }

    fn name(&self) -> &'static str {
        "mock"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_command_runner_expand() {
        let runner = CommandRunner::new("echo {mission_dir}/NEXT_TASK.md");
        let expanded = runner.expand_command(Path::new("/test/mission"));
        assert_eq!(expanded, "echo /test/mission/NEXT_TASK.md");
    }

    #[test]
    fn test_mock_runner_success() {
        let temp_dir = TempDir::new().unwrap();
        let mission_dir = temp_dir.path().join("mission");
        std::fs::create_dir_all(mission_dir.join("OUTPUT")).unwrap();

        let runner = MockRunner::success();
        let config = RunnerConfig {
            max_runtime_seconds: 60,
            working_dir: temp_dir.path().to_path_buf(),
            mode: "BOOTSTRAP".to_string(),
        };

        let outcome = runner.run(&mission_dir, &config).unwrap();
        assert_eq!(outcome.classification, OutcomeClassification::Ok);
        assert_eq!(outcome.exit_code, Some(0));
        assert!(mission_dir.join("OUTPUT/attempt_report.md").exists());
    }

    #[test]
    fn test_mock_runner_failure() {
        let temp_dir = TempDir::new().unwrap();
        let mission_dir = temp_dir.path().join("mission");
        std::fs::create_dir_all(mission_dir.join("OUTPUT")).unwrap();

        let runner = MockRunner::failure();
        let config = RunnerConfig {
            max_runtime_seconds: 60,
            working_dir: temp_dir.path().to_path_buf(),
            mode: "BOOTSTRAP".to_string(),
        };

        let outcome = runner.run(&mission_dir, &config).unwrap();
        assert_eq!(outcome.classification, OutcomeClassification::Failed);
        assert_eq!(outcome.exit_code, Some(1));
    }

    #[test]
    fn test_outcome_classification_serialization() {
        assert_eq!(
            serde_json::to_string(&OutcomeClassification::Ok).unwrap(),
            "\"OK\""
        );
        assert_eq!(
            serde_json::to_string(&OutcomeClassification::Timeout).unwrap(),
            "\"TIMEOUT\""
        );
    }
}
