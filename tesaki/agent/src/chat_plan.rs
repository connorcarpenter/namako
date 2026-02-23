//! Chat plan JSON structures used by the interactive REPL.

use serde::{Deserialize, Serialize};

/// A single allowlisted command request from the chat planner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowedCommand {
    /// Must be "namako". Enforced by Tesaki allowlist.
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

/// Surface policy shape used in chat plans (spec/tests/sut).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfacePolicy {
    pub spec: SurfaceLock,
    pub tests: SurfaceLock,
    pub sut: SurfaceLock,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SurfaceLock {
    Locked,
    Unlocked,
}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    pub tool: String,
    pub args: Vec<String>,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}
