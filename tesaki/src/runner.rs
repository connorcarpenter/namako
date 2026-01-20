//! Runner abstraction for v1.7 Runner Integration.
//!
//! Per GOLD_PLAN.md §10.7.4, the runner is an internal Tesaki abstraction.
//! This module defines the core trait and types for runner backends.
//!
//! # Important: Runner Scope
//!
//! The runner operates on the **specs repository only**. It executes missions
//! that modify project files according to the configured edit surfaces.
//! The runner NEVER edits Namako/Tesaki toolchain code.

use serde::{Deserialize, Serialize};
use std::path::Path;
use anyhow::Result;

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
