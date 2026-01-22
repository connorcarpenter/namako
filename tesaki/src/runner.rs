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

/// A single allowlisted command request from the chat planner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowedCommand {
    /// Must be "namako" or "tesaki". Enforced by Tesaki allowlist.
    pub tool: String,
    /// Args only (no shell). Enforced by Tesaki allowlist.
    pub args: Vec<String>,
    /// Optional explanation for UX.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Optional mission proposal emitted by the chat planner.
/// Tesaki still validates/normalizes this into a real mission bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionProposal {
    pub mission_type: String,   // e.g. "CreateMissingBindings"
    pub stage: String,          // e.g. "Implement Tests & Bindings"
    pub target: String,         // e.g. "@Scenario(03)"
    pub surfaces: SurfacePolicy,
    pub objective: String,
    pub validation: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfacePolicy {
    pub spec: SurfaceLock,
    pub tests: SurfaceLock,
    pub sut: SurfaceLock,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SurfaceLock { Locked, Unlocked }

/// The plan-only result for one REPL “turn step”.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatPlan {
    pub say: String,
    #[serde(default)]
    pub run: Vec<AllowedCommand>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mission_proposal: Option<MissionProposal>,
    pub done: bool,
}

/// Input to the chat planner (small, structured; no repo access).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatTurnInput {
    pub user_message: String,
    pub session_state_json: serde_json::Value,
    pub recent_command_results: Vec<CommandResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planner_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    pub tool: String,
    pub args: Vec<String>,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Plan-only chat interface. Implement this for ClaudeCodeRunner and CodexRunner.
pub trait ChatPlanner: Send + Sync {
    fn plan_turn(&self, input: &ChatTurnInput) -> Result<ChatPlan>;
    #[allow(dead_code)]
    fn name(&self) -> &'static str;
}
