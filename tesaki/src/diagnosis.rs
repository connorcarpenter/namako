//! Stall diagnosis - analyzing why the autonomous loop stopped.
//!
//! This module generates detailed diagnostic reports when the loop stalls,
//! explaining what was attempted, why it failed, and what to try next.

use crate::repo_state::RepoState;
use crate::session::{FailureRecord, SessionState};
use crate::stop_reason::StopReason;
use std::collections::HashSet;

/// Comprehensive diagnosis of why the loop stalled.
#[derive(Debug, Clone)]
pub struct StallDiagnosis {
    /// The stop reason that triggered the stall
    pub stop_reason: StopReason,
    /// Last mission type that was attempted
    pub mission_type: String,
    /// Target that was being worked on (if applicable)
    pub target: Option<String>,
    /// Number of attempts made
    pub attempts_made: u32,
    /// Issue count at start of session
    pub issues_at_start: usize,
    /// Issue count at end of session
    pub issues_at_end: usize,
    /// Distinct approaches that were tried
    pub approaches_tried: Vec<String>,
    /// Factors that blocked progress
    pub blocking_factors: Vec<String>,
    /// Recommended actions to take
    pub recommended_actions: Vec<String>,
}

impl StallDiagnosis {
    /// Generate a diagnosis from session state and stop reason.
    pub fn diagnose(
        session: &SessionState,
        state: &RepoState,
        last_stop: &StopReason,
        last_mission_type: Option<&str>,
        last_target: Option<&str>,
    ) -> Self {
        let mission_type = last_mission_type.unwrap_or("Unknown").to_string();
        let target = last_target.map(|s| s.to_string());

        // Count failures related to current target
        let target_failures: Vec<&FailureRecord> = if let Some(ref t) = target {
            session
                .failure_history
                .iter()
                .filter(|f| f.target.as_deref() == Some(t))
                .collect()
        } else {
            vec![]
        };

        let attempts_made = target_failures.len() as u32 + 1;

        // Extract unique approaches
        let mut approaches = HashSet::new();
        for failure in &target_failures {
            if let Some(approach) = &failure.attempted_approach {
                approaches.insert(approach.clone());
            }
        }
        let approaches_tried: Vec<String> = approaches.into_iter().collect();

        // Determine blocking factors
        let mut blocking_factors = Vec::new();

        // Check for policy violations
        let policy_violations = target_failures
            .iter()
            .filter(|f| f.stop_reason.contains("POLICY_VIOLATION"))
            .count();
        if policy_violations > 0 {
            // Identify which surfaces are locked
            let mut locked_surfaces = Vec::new();
            for failure in &target_failures {
                if let Some(surface) = &failure.violated_surface {
                    if !locked_surfaces.contains(surface) {
                        locked_surfaces.push(surface.clone());
                    }
                }
            }
            if !locked_surfaces.is_empty() {
                blocking_factors.push(format!(
                    "Surface policy: {} surface(s) locked ({})",
                    locked_surfaces.len(),
                    locked_surfaces.join(", ")
                ));
            }
        }

        // Check for repeated failures
        if attempts_made > 2 {
            blocking_factors.push(format!(
                "Repeated failures: {} attempts with no progress",
                attempts_made
            ));
        }

        // Check for zero progress
        let issues_delta = state.total_issue_count() as i32 - session.initial_issue_count as i32;
        if issues_delta >= 0 {
            blocking_factors.push("No issues resolved this session".to_string());
        }

        // Generate recommendations
        let recommended_actions = Self::generate_recommendations(
            last_stop,
            &blocking_factors,
            policy_violations > 0,
            session,
        );

        Self {
            stop_reason: last_stop.clone(),
            mission_type,
            target,
            attempts_made,
            issues_at_start: session.initial_issue_count,
            issues_at_end: state.total_issue_count(),
            approaches_tried,
            blocking_factors,
            recommended_actions,
        }
    }

    fn generate_recommendations(
        stop_reason: &StopReason,
        blocking_factors: &[String],
        has_policy_violations: bool,
        session: &SessionState,
    ) -> Vec<String> {
        let mut actions = Vec::new();

        match stop_reason {
            StopReason::PolicyViolation => {
                actions.push("Review surface policy: Check which surfaces are locked and if they need to be unlocked".to_string());
                actions.push(
                    "Command: Consider unlocking the necessary surface with 'unlock <surface>'"
                        .to_string(),
                );

                // Identify which surface needs unlocking
                if let Some(last_failure) = session.failure_history.last() {
                    if let Some(surface) = &last_failure.violated_surface {
                        actions.push(format!(
                            "Specific action: Run with surface override to unlock {} (e.g., 'tesaki --unlock-{}')",
                            surface, surface.to_lowercase()
                        ));
                    }
                }
            }
            StopReason::NoProgress if has_policy_violations => {
                actions.push("Surface constraints are blocking progress".to_string());
                actions.push("Options: 1) Unlock blocked surfaces, 2) Skip this issue, 3) Provide manual hint".to_string());
            }
            StopReason::NoProgress => {
                actions.push(
                    "Analyze the issue manually to determine why automated fix is failing"
                        .to_string(),
                );
                actions.push(
                    "Consider: Is the issue specification clear? Are dependencies missing?"
                        .to_string(),
                );
                actions.push(
                    "Try: Provide a manual hint or skip this issue to work on others".to_string(),
                );
            }
            StopReason::GateFailed => {
                actions.push(
                    "Last change introduced a regression - review the diff carefully".to_string(),
                );
                actions.push("Command: git diff HEAD~1 to see what changed".to_string());
                actions.push(
                    "Consider: Is the gate too strict? Does the spec need adjustment?".to_string(),
                );
            }
            StopReason::RunnerFailed => {
                actions.push(
                    "Mission failed repeatedly - this issue may require human intervention"
                        .to_string(),
                );
                actions.push("Review logs to understand what's failing".to_string());
                actions.push(
                    "Consider marking this issue as known-hard and skipping for now".to_string(),
                );
            }
            _ => {
                actions.push("Review session logs for more context".to_string());
                actions
                    .push("Check .tesaki/ directory for detailed failure information".to_string());
            }
        }

        // Add general recommendations based on blocking factors
        if blocking_factors
            .iter()
            .any(|f| f.contains("Repeated failures"))
        {
            actions.push("This issue may be fundamentally blocked - consider changing approach or getting human input".to_string());
        }

        if blocking_factors
            .iter()
            .any(|f| f.contains("No issues resolved"))
        {
            actions.push(
                "No progress made this session - review issue selection strategy".to_string(),
            );
            actions.push("Tip: Start with simpler issues to build momentum".to_string());
        }

        actions
    }

    /// Format diagnosis as a human-readable report.
    pub fn format_report(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();

        writeln!(
            out,
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        )
        .unwrap();
        writeln!(out, "STALL DIAGNOSIS").unwrap();
        writeln!(
            out,
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        )
        .unwrap();
        writeln!(out).unwrap();

        writeln!(out, "## What Happened").unwrap();
        writeln!(out).unwrap();
        writeln!(out, "**Stop Reason:** {:?}", self.stop_reason).unwrap();
        writeln!(out, "**Last Mission:** {}", self.mission_type).unwrap();
        if let Some(ref target) = self.target {
            writeln!(out, "**Target:** {}", target).unwrap();
        }
        writeln!(out, "**Attempts:** {}", self.attempts_made).unwrap();
        writeln!(
            out,
            "**Issues:** {} → {} ({})",
            self.issues_at_start,
            self.issues_at_end,
            if self.issues_at_end < self.issues_at_start {
                format!("-{}", self.issues_at_start - self.issues_at_end)
            } else if self.issues_at_end > self.issues_at_start {
                format!("+{}", self.issues_at_end - self.issues_at_start)
            } else {
                "no change".to_string()
            }
        )
        .unwrap();
        writeln!(out).unwrap();

        if !self.approaches_tried.is_empty() {
            writeln!(out, "**Approaches Tried:**").unwrap();
            for (i, approach) in self.approaches_tried.iter().enumerate() {
                writeln!(out, "{}. {}", i + 1, approach).unwrap();
            }
            writeln!(out).unwrap();
        }

        writeln!(out, "## Why It Stalled").unwrap();
        writeln!(out).unwrap();
        if self.blocking_factors.is_empty() {
            writeln!(out, "No specific blocking factors identified.").unwrap();
        } else {
            for factor in &self.blocking_factors {
                writeln!(out, "- {}", factor).unwrap();
            }
        }
        writeln!(out).unwrap();

        writeln!(out, "## What To Try").unwrap();
        writeln!(out).unwrap();
        for (i, action) in self.recommended_actions.iter().enumerate() {
            writeln!(out, "{}. {}", i + 1, action).unwrap();
        }
        writeln!(out).unwrap();

        writeln!(out, "---").unwrap();
        writeln!(
            out,
            "*This diagnosis has been saved to .tesaki/last_stall_diagnosis.md*"
        )
        .unwrap();

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_diagnosis_policy_violation() {
        let mut session = SessionState::default();
        session.initial_issue_count = 10;
        session.failure_history.push(FailureRecord {
            mission_type: "FixRegressionFromGateFailure".to_string(),
            target: Some("feature:auth:login".to_string()),
            stop_reason: "POLICY_VIOLATION".to_string(),
            violated_files: vec!["specs/auth.feature".to_string()],
            violated_surface: Some("spec".to_string()),
            attempted_approach: Some("Tried to edit spec file".to_string()),
            timestamp: "2026-02-03T10:00:00Z".to_string(),
        });

        let state = RepoState::default();

        let diagnosis = StallDiagnosis::diagnose(
            &session,
            &state,
            &StopReason::PolicyViolation,
            Some("FixRegressionFromGateFailure"),
            Some("feature:auth:login"),
        );

        assert_eq!(diagnosis.mission_type, "FixRegressionFromGateFailure");
        assert_eq!(diagnosis.target, Some("feature:auth:login".to_string()));
        assert!(diagnosis
            .blocking_factors
            .iter()
            .any(|f| f.contains("spec")));
        assert!(!diagnosis.recommended_actions.is_empty());
    }

    #[test]
    fn test_diagnosis_repeated_failure() {
        let mut session = SessionState::default();
        session.initial_issue_count = 10;

        // Add multiple failures for same target
        for i in 0..3 {
            session.failure_history.push(FailureRecord {
                mission_type: "ImplementBehavior".to_string(),
                target: Some("feature:auth:login".to_string()),
                stop_reason: "NO_PROGRESS".to_string(),
                violated_files: vec![],
                violated_surface: None,
                attempted_approach: Some(format!("Approach {}", i + 1)),
                timestamp: format!("2026-02-03T10:{}:00Z", i),
            });
        }

        let state = RepoState::default();

        let diagnosis = StallDiagnosis::diagnose(
            &session,
            &state,
            &StopReason::NoProgress,
            Some("ImplementBehavior"),
            Some("feature:auth:login"),
        );

        assert_eq!(diagnosis.attempts_made, 4); // 3 in history + 1 current
        assert_eq!(diagnosis.approaches_tried.len(), 3);
        assert!(diagnosis
            .blocking_factors
            .iter()
            .any(|f| f.contains("Repeated failures")));
    }

    #[test]
    fn test_format_report() {
        let diagnosis = StallDiagnosis {
            stop_reason: StopReason::PolicyViolation,
            mission_type: "FixRegressionFromGateFailure".to_string(),
            target: Some("feature:auth:login".to_string()),
            attempts_made: 2,
            issues_at_start: 10,
            issues_at_end: 10,
            approaches_tried: vec!["Edit spec directly".to_string()],
            blocking_factors: vec!["Surface policy: spec surface locked".to_string()],
            recommended_actions: vec!["Unlock spec surface".to_string()],
        };

        let report = diagnosis.format_report();
        assert!(report.contains("STALL DIAGNOSIS"));
        assert!(report.contains("What Happened"));
        assert!(report.contains("Why It Stalled"));
        assert!(report.contains("What To Try"));
        assert!(report.contains("PolicyViolation"));
        assert!(report.contains("spec surface locked"));
    }
}
