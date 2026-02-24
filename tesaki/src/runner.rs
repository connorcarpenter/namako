use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub use servling::OutcomeClassification;
pub use servling::token_usage::TokenUsage;
pub use servling::{Servling, LLMRequest, LLMResponse};

/// Configuration for mission execution.
#[derive(Debug, Clone)]
pub struct RunnerConfig {
    /// Working directory for the runner (typically the workspace root).
    pub working_dir: PathBuf,

    /// Maximum runtime for the runner in seconds.
    pub max_runtime_seconds: u32,

    /// Model to use for the AI runner (e.g., "haiku", "sonnet", "opus").
    pub model: Option<String>,

    /// Stream runner output to terminal in real-time.
    pub stream_output: bool,
}

/// Outcome of a mission execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerOutcome {
    pub exit_code: Option<i32>,
    pub classification: OutcomeClassification,
    pub elapsed_seconds: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Token usage parsed from runner stderr (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<TokenUsage>,
}

/// Mission execution interface.
pub trait Runner: Send + Sync {
    fn run(&self, mission_dir: &Path, config: &RunnerConfig) -> Result<RunnerOutcome>;
    fn name(&self) -> &'static str;
    fn planned_invocation(&self, _mission_dir: &Path, _config: &RunnerConfig) -> Option<RunnerInvocation> {
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerInvocation {
    pub program: String,
    pub args: Vec<String>,
    pub working_dir: String,
    pub env: Vec<(String, String)>,
}

/// Blanket implementation: Every Servling is a Runner.
/// Blanket implementation: Every Servling is a Runner.
impl<T: Servling + ?Sized> Runner for T {
    fn name(&self) -> &'static str {
        self.name()
    }

    fn run(&self, mission_dir: &Path, config: &RunnerConfig) -> Result<RunnerOutcome> {
        let mission_path = mission_dir.join("MISSION.md");
        let prompt = std::fs::read_to_string(&mission_path)
            .with_context(|| format!("Failed to read mission at {}", mission_path.display()))?;

        let request = LLMRequest {
            prompt,
            model: config.model.clone(),
            working_dir: config.working_dir.clone(),
            max_runtime_seconds: config.max_runtime_seconds,
            stream_output: config.stream_output,
            input_file: Some(mission_path),
        };

        let resp = self.execute(&request)?;
        Ok(RunnerOutcome {
            exit_code: resp.exit_code,
            classification: resp.classification,
            elapsed_seconds: resp.elapsed_seconds,
            stdout_path: resp.stdout_path,
            stderr_path: resp.stderr_path,
            error_message: None,
            token_usage: resp.token_usage,
        })
    }

    fn planned_invocation(&self, mission_dir: &Path, config: &RunnerConfig) -> Option<RunnerInvocation> {
        let mission_path = mission_dir.join("MISSION.md");
        let request = LLMRequest {
            prompt: String::new(),
            model: config.model.clone(),
            working_dir: config.working_dir.clone(),
            max_runtime_seconds: config.max_runtime_seconds,
            stream_output: config.stream_output,
            input_file: Some(mission_path),
        };
        
        self.planned_invocation(&request).map(|inv| RunnerInvocation {
            program: inv.program,
            args: inv.args,
            working_dir: inv.working_dir,
            env: inv.env,
        })
    }
}

/// Check whether any files changed by the runner fall outside the allowed surface.
pub fn check_surface_violations(
    changed_files: &[String],
    spec_patterns: &[String],
    tests_patterns: &[String],
    sut_patterns: &[String],
    policy_spec: bool,       // true = unlocked
    policy_tests: bool,      // true = unlocked
    policy_sut: bool,        // true = unlocked
) -> Vec<String> {
    let mut violations = Vec::new();
    for file in changed_files {
        let in_spec = matches_any_pattern(file, spec_patterns);
        let in_tests = matches_any_pattern(file, tests_patterns);
        let in_sut = matches_any_pattern(file, sut_patterns);

        if in_spec && !policy_spec {
            violations.push(format!("{} (spec surface LOCKED)", file));
        } else if in_tests && !policy_tests {
            violations.push(format!("{} (tests surface LOCKED)", file));
        } else if in_sut && !policy_sut {
            violations.push(format!("{} (sut surface LOCKED)", file));
        }
    }
    violations
}

/// Simple glob-style pattern matching.
pub fn matches_any_pattern(path: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pat| glob_match(pat, path))
}

fn glob_match(pattern: &str, path: &str) -> bool {
    let pat = pattern.replace('\\', "/");
    let path = path.replace('\\', "/");
    let pat_parts: Vec<&str> = pat.split('/').collect();
    let path_parts: Vec<&str> = path.split('/').collect();
    glob_match_parts(&pat_parts, &path_parts)
}

fn glob_match_parts(pat: &[&str], path: &[&str]) -> bool {
    match (pat.first(), path.first()) {
        (None, None) => true,
        (Some(&"**"), _) => {
            glob_match_parts(&pat[1..], path)
                || (!path.is_empty() && glob_match_parts(pat, &path[1..]))
        }
        (Some(p), Some(s)) => {
            segment_match(p, s) && glob_match_parts(&pat[1..], &path[1..])
        }
        _ => false,
    }
}

fn segment_match(pattern: &str, segment: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(ext) = pattern.strip_prefix('*') {
        return segment.ends_with(ext);
    }
    pattern == segment
}

/// Mock agent for testing.
pub struct MockAgent {
    pub response_text: String,
}

impl MockAgent {
    pub fn success() -> Self {
        Self {
            response_text: r#"{"say": "Mock success", "done": true}"#.to_string(),
        }
    }
}

impl Servling for MockAgent {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn execute(&self, _request: &LLMRequest) -> Result<LLMResponse> {
        Ok(LLMResponse {
            text: self.response_text.clone(),
            classification: OutcomeClassification::Ok,
            exit_code: Some(0),
            token_usage: None,
            elapsed_seconds: 0.1,
            stdout_path: None,
            stderr_path: None,
        })
    }
}

/// Mock runner for testing.
pub struct MockRunner {
    pub should_succeed: bool,
    pub write_attempt_report: bool,
    pub create_file: Option<(String, String)>,
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
    pub fn success() -> Self {
        Self::default()
    }

    #[allow(dead_code)]
    pub fn failure() -> Self {
        Self {
            should_succeed: false,
            ..Default::default()
        }
    }

    #[allow(dead_code)]
    pub fn with_file(mut self, path: impl Into<String>, content: impl Into<String>) -> Self {
        self.create_file = Some((path.into(), content.into()));
        self
    }
}

impl Runner for MockRunner {
    fn run(&self, mission_dir: &Path, _config: &RunnerConfig) -> Result<RunnerOutcome> {
        std::thread::sleep(Duration::from_secs_f64(self.simulated_time));

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
            token_usage: None,
        })
    }

    fn name(&self) -> &'static str {
        "mock"
    }
}

/// Helper to convert a Result error into a generic EnvironmentError outcome.
pub fn outcome_from_error(err: anyhow::Error) -> RunnerOutcome {
    RunnerOutcome {
        exit_code: None,
        classification: OutcomeClassification::EnvironmentError,
        elapsed_seconds: 0.0,
        stdout_path: None,
        stderr_path: None,
        error_message: Some(err.to_string()),
        token_usage: None,
    }
}

impl From<anyhow::Error> for RunnerOutcome {
    fn from(err: anyhow::Error) -> Self {
        outcome_from_error(err)
    }
}

/// Factory to build a Runner from agent candidates.
pub fn build_runner(candidates: Vec<servling::AgentCandidate>) -> anyhow::Result<Box<dyn Runner>> {
    let agent = servling::build_coding_agent(candidates)?;
    // Use a manual wrap to force dyn Runner coercion
    struct RunnerWrap(Box<dyn Servling>);
    impl Runner for RunnerWrap {
        fn run(&self, mission_dir: &std::path::Path, config: &RunnerConfig) -> anyhow::Result<RunnerOutcome> {
            self.0.run(mission_dir, config)
        }
        fn name(&self) -> &'static str {
            Servling::name(&*self.0)
        }
        fn planned_invocation(&self, mission_dir: &std::path::Path, config: &RunnerConfig) -> Option<RunnerInvocation> {
            // Create a temporary LLMRequest for planned_invocation
            let request = LLMRequest {
                prompt: String::new(),
                model: config.model.clone(),
                working_dir: config.working_dir.clone(),
                max_runtime_seconds: config.max_runtime_seconds,
                stream_output: config.stream_output,
                input_file: Some(mission_dir.join("MISSION.md")),
            };
            Servling::planned_invocation(&*self.0, &request).map(|inv| RunnerInvocation {
                program: inv.program,
                args: inv.args,
                working_dir: inv.working_dir,
                env: inv.env,
            })
        }
    }
    Ok(Box::new(RunnerWrap(agent)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_runner_success() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mission_dir = temp_dir.path().join("mission");
        std::fs::create_dir_all(mission_dir.join("RUNNER_OUTPUT")).unwrap();

        let runner = MockRunner::success();
        let config = RunnerConfig {
            max_runtime_seconds: 60,
            working_dir: temp_dir.path().to_path_buf(),
            model: None,
            stream_output: true,
        };

        let outcome = runner.run(&mission_dir, &config).unwrap();
        assert_eq!(outcome.classification, OutcomeClassification::Ok);
        assert_eq!(outcome.exit_code, Some(0));
        assert!(mission_dir.join("RUNNER_OUTPUT/attempt_report.md").exists());
    }

    #[test]
    fn test_blanket_runner_impl() {
        let agent = MockAgent::success();
        let mission_dir = tempfile::tempdir().unwrap();
        let mission_path = mission_dir.path().join("MISSION.md");
        std::fs::write(&mission_path, "test mission").unwrap();
        
        let config = RunnerConfig {
            working_dir: PathBuf::from("."),
            max_runtime_seconds: 30,
            model: None,
            stream_output: false,
        };
        
        let outcome = Runner::run(&agent, mission_dir.path(), &config).unwrap();
        assert_eq!(outcome.classification, OutcomeClassification::Ok);
    }

    #[test]
    fn test_outcome_classification_serialization() {
        assert_eq!(
            serde_json::to_string(&OutcomeClassification::Ok).unwrap(),
            "\"OK\""
        );
    }

    #[test]
    fn test_surface_check_locked_spec_violated() {
        let changed = vec!["test/specs/features/a.feature".to_string()];
        let spec_pats = vec!["test/specs/**/*.feature".to_string()];
        let tests_pats = vec!["test/tests/**".to_string()];
        let sut_pats = vec!["src/**".to_string()];

        let violations = check_surface_violations(
            &changed, &spec_pats, &tests_pats, &sut_pats,
            false,
            true,
            true,
        );
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("spec surface LOCKED"));
    }

    #[test]
    fn test_glob_match_double_star() {
        assert!(glob_match("test/specs/**/*.feature", "test/specs/features/a.feature"));
        assert!(glob_match("src/**", "src/main.rs"));
        assert!(glob_match("src/**", "src/sub/deep/file.rs"));
        assert!(!glob_match("src/**", "test/main.rs"));
    }
}
