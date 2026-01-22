use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OutcomeClassification {
    Ok,
    Failed,
    Timeout,
    EnvironmentError,
    /// Rate limited by the AI provider (Claude, Codex, etc.)
    RateLimited,
}

/// Mission execution interface (unchanged in spirit).
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
