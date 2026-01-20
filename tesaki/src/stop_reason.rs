//! Stop conditions for v1.7 Runner Integration.
//!
//! This module defines the structured stop reasons per GOLD_PLAN.md §10.7.7.

use serde::{Deserialize, Serialize};

/// Stop conditions for the Tesaki run loop.
///
/// Per GOLD_PLAN.md §10.7.7, these are the explicit, deterministic stop conditions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StopReason {
    /// No eligible tasks remain (all scenarios passing, no promotion candidates).
    Done,

    /// Only blocked items remain (e.g., all require HARNESS_ONLY or EXTERNAL work).
    Blocked,

    /// Human intervention needed (e.g., baseline update approval required, ambiguous requirements).
    HumanRequired,

    /// Gate invocation failed, adapter crash, filesystem errors.
    EnvironmentError,

    /// Runtime/attempt/file limits reached.
    Budget,

    /// Runner exited non-zero and retries exhausted.
    RunnerFailed,

    /// Runner produced no diff / no meaningful changes and retries exhausted.
    NoProgress,

    /// Gate failed (lint/run/verify issues) and retries exhausted.
    GateFailed,
}

impl StopReason {
    /// Returns true if this is a success condition.
    pub fn is_success(&self) -> bool {
        matches!(self, StopReason::Done)
    }

    /// Returns true if this requires human intervention.
    pub fn requires_human(&self) -> bool {
        matches!(self, StopReason::HumanRequired | StopReason::Blocked)
    }

    /// Returns a human-readable description.
    pub fn description(&self) -> &'static str {
        match self {
            StopReason::Done => "All tasks completed successfully",
            StopReason::Blocked => "Only blocked tasks remain (require external work)",
            StopReason::HumanRequired => "Human intervention required",
            StopReason::EnvironmentError => "Environment or toolchain error",
            StopReason::Budget => "Budget limits exceeded",
            StopReason::RunnerFailed => "Runner failed after retries",
            StopReason::NoProgress => "No progress made after retries",
            StopReason::GateFailed => "Gate failed after retries",
        }
    }
}

impl std::fmt::Display for StopReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description())
    }
}

/// Result of a `tesaki run` invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    /// The stop reason (why we stopped).
    pub reason: StopReason,

    /// Number of missions completed this session.
    pub missions_completed: u32,

    /// Number of cert updates performed this session.
    pub cert_updates_performed: u32,

    /// Optional details about the stop condition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,

    /// Path to the last mission bundle (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_mission_path: Option<String>,
}

impl RunResult {
    /// Create a new RunResult for a successful completion.
    pub fn done(missions_completed: u32, cert_updates: u32) -> Self {
        Self {
            reason: StopReason::Done,
            missions_completed,
            cert_updates_performed: cert_updates,
            details: None,
            last_mission_path: None,
        }
    }

    /// Create a new RunResult for a blocked state.
    pub fn blocked(details: impl Into<String>) -> Self {
        Self {
            reason: StopReason::Blocked,
            missions_completed: 0,
            cert_updates_performed: 0,
            details: Some(details.into()),
            last_mission_path: None,
        }
    }

    /// Create a new RunResult for an error.
    pub fn error(reason: StopReason, details: impl Into<String>) -> Self {
        Self {
            reason,
            missions_completed: 0,
            cert_updates_performed: 0,
            details: Some(details.into()),
            last_mission_path: None,
        }
    }

    /// Set the last mission path.
    pub fn with_mission_path(mut self, path: impl Into<String>) -> Self {
        self.last_mission_path = Some(path.into());
        self
    }

    /// Update mission count.
    pub fn with_missions(mut self, count: u32) -> Self {
        self.missions_completed = count;
        self
    }

    /// Update cert updates count.
    pub fn with_cert_updates(mut self, count: u32) -> Self {
        self.cert_updates_performed = count;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stop_reason_serialization() {
        let reason = StopReason::Done;
        let json = serde_json::to_string(&reason).unwrap();
        assert_eq!(json, "\"DONE\"");

        let reason = StopReason::EnvironmentError;
        let json = serde_json::to_string(&reason).unwrap();
        assert_eq!(json, "\"ENVIRONMENT_ERROR\"");
    }

    #[test]
    fn test_stop_reason_deserialization() {
        let reason: StopReason = serde_json::from_str("\"GATE_FAILED\"").unwrap();
        assert_eq!(reason, StopReason::GateFailed);

        let reason: StopReason = serde_json::from_str("\"HUMAN_REQUIRED\"").unwrap();
        assert_eq!(reason, StopReason::HumanRequired);
    }

    #[test]
    fn test_run_result_done() {
        let result = RunResult::done(3, 1);
        assert!(result.reason.is_success());
        assert_eq!(result.missions_completed, 3);
        assert_eq!(result.cert_updates_performed, 1);
    }

    #[test]
    fn test_run_result_error() {
        let result = RunResult::error(StopReason::GateFailed, "lint phase failed")
            .with_mission_path(".tesaki/failed/001-test");
        assert!(!result.reason.is_success());
        assert_eq!(result.details, Some("lint phase failed".to_string()));
        assert_eq!(result.last_mission_path, Some(".tesaki/failed/001-test".to_string()));
    }
}
