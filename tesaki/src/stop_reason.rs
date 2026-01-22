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

    /// Rate limited by AI provider - no retry, wait for limit reset.
    RateLimited,
}

impl StopReason {
    /// Returns true if this is a success condition.
    #[allow(dead_code)]
    pub fn is_success(&self) -> bool {
        matches!(self, StopReason::Done)
    }

    /// Returns true if this requires human intervention.
    #[allow(dead_code)]
    pub fn requires_human(&self) -> bool {
        matches!(self, StopReason::HumanRequired | StopReason::Blocked)
    }

    /// Returns true if this failure is retryable.
    ///
    /// Per TODO.md §B1, retries are allowed ONLY for:
    /// - RunnerFailed: non-zero exit, timeout
    /// - NoProgress: runner produced zero file changes
    /// - GateFailed: post-run gate failed
    ///
    /// NOT retryable:
    /// - HumanRequired: requires human intervention
    /// - EnvironmentError: toolchain/setup issues
    /// - Budget: limits reached (retrying would violate budget)
    /// - RateLimited: would just hit rate limit again
    /// - Done/Blocked: not failures
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            StopReason::RunnerFailed | StopReason::NoProgress | StopReason::GateFailed
        )
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
            StopReason::RateLimited => "Rate limited by AI provider",
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

    /// Per TODO.md §B1: test is_retryable for all stop reasons.
    #[test]
    fn test_is_retryable() {
        // Retryable failures
        assert!(StopReason::RunnerFailed.is_retryable());
        assert!(StopReason::NoProgress.is_retryable());
        assert!(StopReason::GateFailed.is_retryable());

        // NOT retryable
        assert!(!StopReason::Done.is_retryable());
        assert!(!StopReason::Blocked.is_retryable());
        assert!(!StopReason::HumanRequired.is_retryable());
        assert!(!StopReason::EnvironmentError.is_retryable());
        assert!(!StopReason::Budget.is_retryable());
    }

    // =========================================================================
    // Tests for retry logic (per TODO.md §B3)
    // =========================================================================

    /// Per TODO.md §B3: simulate runner fails once then succeeds
    #[test]
    fn test_retry_logic_runner_fails_once_succeeds() {
        let outcomes = vec![StopReason::Done, StopReason::RunnerFailed];
        let mut iter = outcomes.into_iter().rev();
        let max_retries = 2u32;
        let mut attempts = 0u32;

        // Simulate retry loop
        loop {
            if let Some(outcome) = iter.next() {
                attempts += 1;
                if outcome.is_success() {
                    // Success - break
                    break;
                } else if outcome.is_retryable() && attempts < max_retries {
                    // Retry
                    continue;
                } else {
                    // Not retryable or exhausted
                    panic!("Should have succeeded");
                }
            } else {
                break;
            }
        }
        assert_eq!(attempts, 2); // Failed once, succeeded on retry
    }

    /// Per TODO.md §B3: simulate NO_PROGRESS once then progress
    #[test]
    fn test_retry_logic_no_progress_then_progress() {
        let outcomes = vec![StopReason::Done, StopReason::NoProgress];
        let mut iter = outcomes.into_iter().rev();
        let max_retries = 3u32;
        let mut attempts = 0u32;

        loop {
            if let Some(outcome) = iter.next() {
                attempts += 1;
                if outcome.is_success() {
                    break;
                } else if outcome.is_retryable() && attempts < max_retries {
                    continue;
                } else {
                    panic!("Should have succeeded");
                }
            } else {
                break;
            }
        }
        assert_eq!(attempts, 2);
        assert!(StopReason::NoProgress.is_retryable());
    }

    /// Per TODO.md §B3: simulate GATE_FAILED once then pass
    #[test]
    fn test_retry_logic_gate_failed_then_pass() {
        let outcomes = vec![StopReason::Done, StopReason::GateFailed];
        let mut iter = outcomes.into_iter().rev();
        let max_retries = 2u32;
        let mut attempts = 0u32;

        loop {
            if let Some(outcome) = iter.next() {
                attempts += 1;
                if outcome.is_success() {
                    break;
                } else if outcome.is_retryable() && attempts < max_retries {
                    continue;
                } else {
                    panic!("Should have succeeded");
                }
            } else {
                break;
            }
        }
        assert_eq!(attempts, 2);
    }

    /// Per TODO.md §B3: HUMAN_REQUIRED → NO retry
    #[test]
    fn test_retry_logic_human_required_no_retry() {
        let outcome = StopReason::HumanRequired;
        let max_retries = 5u32;
        let attempts = 1u32;

        // HUMAN_REQUIRED is NOT retryable regardless of budget
        let should_retry = outcome.is_retryable() && attempts < max_retries;
        assert!(!should_retry);
    }

    /// Per TODO.md §B3: ENVIRONMENT_ERROR → NO retry
    #[test]
    fn test_retry_logic_environment_error_no_retry() {
        let outcome = StopReason::EnvironmentError;
        assert!(!outcome.is_retryable());
    }

    /// Per TODO.md §B3: BUDGET → NO retry (retrying would violate the budget)
    #[test]
    fn test_retry_logic_budget_no_retry() {
        let outcome = StopReason::Budget;
        assert!(!outcome.is_retryable());
    }

    /// Per TODO.md §B3: max_retries=0 means no retries allowed
    #[test]
    fn test_retry_logic_zero_budget() {
        let outcome = StopReason::RunnerFailed;
        let max_retries = 0u32;
        let attempts = 1u32;

        // Even for retryable outcome, if max_retries=0, no retry
        let should_retry = outcome.is_retryable() && attempts < max_retries;
        assert!(!should_retry);
    }

    /// Per TODO.md §B3: exhausted retries stops
    #[test]
    fn test_retry_logic_retries_exhausted() {
        let outcome = StopReason::RunnerFailed;
        let max_retries = 3u32;
        let attempts = 3u32; // Already at max

        let should_retry = outcome.is_retryable() && attempts < max_retries;
        assert!(!should_retry);
    }
}
