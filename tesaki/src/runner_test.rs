//! Test runners and test suite for runner backends.

use crate::runner::{Runner, RunnerConfig, RunnerOutcome, OutcomeClassification};
use anyhow::Context;
use std::path::Path;
use std::time::Duration;

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
    fn run(&self, mission_dir: &Path, _config: &RunnerConfig) -> anyhow::Result<RunnerOutcome> {
        // Simulate execution time
        std::thread::sleep(Duration::from_secs_f64(self.simulated_time));

        // Write attempt report if configured
        if self.write_attempt_report {
            let report_path = mission_dir.join("RUNNER_OUTPUT/attempt_report.md");
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
    fn test_mock_runner_success() {
        let temp_dir = TempDir::new().unwrap();
        let mission_dir = temp_dir.path().join("mission");
        std::fs::create_dir_all(mission_dir.join("RUNNER_OUTPUT")).unwrap();

        let runner = MockRunner::success();
        let config = RunnerConfig {
            max_runtime_seconds: 60,
            working_dir: temp_dir.path().to_path_buf(),
            mode: "BOOTSTRAP".to_string(),
        };

        let outcome = runner.run(&mission_dir, &config).unwrap();
        assert_eq!(outcome.classification, OutcomeClassification::Ok);
        assert_eq!(outcome.exit_code, Some(0));
        assert!(mission_dir.join("RUNNER_OUTPUT/attempt_report.md").exists());
    }

    #[test]
    fn test_mock_runner_failure() {
        let temp_dir = TempDir::new().unwrap();
        let mission_dir = temp_dir.path().join("mission");
        std::fs::create_dir_all(mission_dir.join("RUNNER_OUTPUT")).unwrap();

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
