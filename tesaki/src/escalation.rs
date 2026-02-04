//! Escalation handling for when the autonomous loop gets stuck.
//!
//! This module provides intelligent escalation when the loop stalls,
//! offering actionable options to the user instead of just stopping.

use serde::{Deserialize, Serialize};

use crate::session::SessionState;
use crate::stop_reason::StopReason;

/// Types of escalation scenarios.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EscalationType {
    /// Fix requires editing a locked surface
    SurfacePolicyBlocking,
    /// Same issue has failed multiple times with different approaches
    RepeatedFailure,
    /// Multiple attempts made but no progress
    NoProgressMultipleAttempts,
    /// Can't determine why it's stuck
    UnknownBlocker,
}

/// Context for an escalation that requires human intervention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationContext {
    pub escalation_type: EscalationType,
    pub target: Option<String>,
    pub attempts: u32,
    pub tried_approaches: Vec<String>,
    pub blocked_by: Option<String>,
    pub suggested_options: Vec<EscalationOption>,
}

/// A suggested action the user can take to resolve the escalation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationOption {
    pub id: String,
    pub label: String,
    pub description: String,
}

/// Detect if the current situation requires escalation.
///
/// Returns Some(EscalationContext) if human intervention is needed.
pub fn detect_escalation(
    session: &SessionState,
    current_mission_type: &str,
    stop_reason: &StopReason,
) -> Option<EscalationContext> {
    match stop_reason {
        StopReason::PolicyViolation => {
            // Surface policy is blocking progress
            let last_failure = session.last_gate_failure.as_ref()?;
            let violated_surface = last_failure.violated_surface.as_ref()?.clone();

            let mut options = vec![];

            // Suggest unlocking the blocked surface
            if violated_surface.contains("spec") {
                options.push(EscalationOption {
                    id: "unlock_spec".to_string(),
                    label: "Unlock spec surface".to_string(),
                    description: "Allow editing .feature files to fix this issue".to_string(),
                });
            }
            if violated_surface.contains("tests") {
                options.push(EscalationOption {
                    id: "unlock_tests".to_string(),
                    label: "Unlock tests surface".to_string(),
                    description: "Allow editing test/binding files to fix this issue".to_string(),
                });
            }
            if violated_surface.contains("sut") {
                options.push(EscalationOption {
                    id: "unlock_sut".to_string(),
                    label: "Unlock SUT surface".to_string(),
                    description: "Allow editing implementation code to fix this issue".to_string(),
                });
            }

            // Always offer skip option
            options.push(EscalationOption {
                id: "skip".to_string(),
                label: "Skip this issue".to_string(),
                description: "Mark this issue as skipped and move on to other work".to_string(),
            });

            Some(EscalationContext {
                escalation_type: EscalationType::SurfacePolicyBlocking,
                target: last_failure.target.clone(),
                attempts: 1,
                tried_approaches: vec![last_failure.mission_type.clone()],
                blocked_by: Some(format!("{} surface is locked", violated_surface)),
                suggested_options: options,
            })
        }

        StopReason::NoProgress => {
            // Check if same target has failed multiple times
            let failure_count = session.failure_history.len();

            if failure_count >= 2 {
                // Check if recent failures are for the same target
                let recent_targets: Vec<_> = session
                    .failure_history
                    .iter()
                    .rev()
                    .take(3)
                    .filter_map(|f| f.target.as_ref())
                    .collect();

                let all_same = recent_targets.windows(2).all(|w| w[0] == w[1]);

                if all_same && !recent_targets.is_empty() {
                    let target = recent_targets[0].clone();
                    let tried_approaches: Vec<String> = session
                        .failure_history
                        .iter()
                        .rev()
                        .take(3)
                        .map(|f| f.mission_type.clone())
                        .collect();

                    return Some(EscalationContext {
                        escalation_type: EscalationType::RepeatedFailure,
                        target: Some(target),
                        attempts: tried_approaches.len() as u32,
                        tried_approaches,
                        blocked_by: Some("Multiple approaches have failed".to_string()),
                        suggested_options: vec![
                            EscalationOption {
                                id: "hint".to_string(),
                                label: "Provide a hint".to_string(),
                                description: "Give the agent a hint about how to proceed".to_string(),
                            },
                            EscalationOption {
                                id: "skip".to_string(),
                                label: "Skip this issue".to_string(),
                                description: "Move on to other work".to_string(),
                            },
                        ],
                    });
                }
            }

            // Generic no progress scenario
            Some(EscalationContext {
                escalation_type: EscalationType::NoProgressMultipleAttempts,
                target: None,
                attempts: session.failure_history.len() as u32,
                tried_approaches: session
                    .failure_history
                    .iter()
                    .rev()
                    .take(5)
                    .map(|f| f.mission_type.clone())
                    .collect(),
                blocked_by: None,
                suggested_options: vec![
                    EscalationOption {
                        id: "continue".to_string(),
                        label: "Continue anyway".to_string(),
                        description: "Try a few more iterations".to_string(),
                    },
                    EscalationOption {
                        id: "stop".to_string(),
                        label: "Stop here".to_string(),
                        description: "End the autonomous loop".to_string(),
                    },
                ],
            })
        }

        _ => None,
    }
}

/// Generate a human-readable escalation message.
pub fn format_escalation_message(ctx: &EscalationContext) -> String {
    let mut message = String::new();

    message.push_str("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
    message.push_str("🚧 HUMAN INTERVENTION REQUIRED\n");
    message.push_str("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\n");

    // Describe the situation
    match ctx.escalation_type {
        EscalationType::SurfacePolicyBlocking => {
            message.push_str("## Situation\n\n");
            message.push_str("The agent attempted to fix an issue but was blocked by surface policy.\n");
            if let Some(ref blocked_by) = ctx.blocked_by {
                message.push_str(&format!("**Blocker:** {}\n", blocked_by));
            }
            if let Some(ref target) = ctx.target {
                message.push_str(&format!("**Target:** {}\n", target));
            }
        }
        EscalationType::RepeatedFailure => {
            message.push_str("## Situation\n\n");
            message.push_str(&format!(
                "The same issue has failed {} times with different approaches.\n",
                ctx.attempts
            ));
            if let Some(ref target) = ctx.target {
                message.push_str(&format!("**Target:** {}\n", target));
            }
            message.push_str("\n**Approaches tried:**\n");
            for approach in &ctx.tried_approaches {
                message.push_str(&format!("  - {}\n", approach));
            }
        }
        EscalationType::NoProgressMultipleAttempts => {
            message.push_str("## Situation\n\n");
            message.push_str(&format!("Made {} attempts but no progress.\n", ctx.attempts));
            if !ctx.tried_approaches.is_empty() {
                message.push_str("\n**Recent attempts:**\n");
                for approach in &ctx.tried_approaches {
                    message.push_str(&format!("  - {}\n", approach));
                }
            }
        }
        EscalationType::UnknownBlocker => {
            message.push_str("## Situation\n\n");
            message.push_str("The autonomous loop has stalled for an unknown reason.\n");
        }
    }

    // Present options
    message.push_str("\n## What would you like to do?\n\n");
    for (i, option) in ctx.suggested_options.iter().enumerate() {
        message.push_str(&format!(
            "{}. **{}** — {}\n",
            i + 1,
            option.label,
            option.description
        ));
    }

    message.push_str("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompts::PreviousFailureContext;

    #[test]
    fn test_detect_escalation_policy_violation() {
        let mut session = SessionState::default();
        session.last_gate_failure = Some(PreviousFailureContext {
            mission_type: "CreateMissingBindings".to_string(),
            target: Some("feature::rule::scenario".to_string()),
            stop_reason: "POLICY_VIOLATION".to_string(),
            details: Some("Violated spec surface".to_string()),
            violated_files: Some(vec!["specs/feature.feature".to_string()]),
            violated_surface: Some("spec".to_string()),
            attempted_approach: None,
        });

        let escalation = detect_escalation(
            &session,
            "CreateMissingBindings",
            &StopReason::PolicyViolation,
        );

        assert!(escalation.is_some());
        let ctx = escalation.unwrap();
        assert!(matches!(ctx.escalation_type, EscalationType::SurfacePolicyBlocking));
        assert!(ctx.suggested_options.iter().any(|o| o.id == "unlock_spec"));
        assert!(ctx.suggested_options.iter().any(|o| o.id == "skip"));
    }

    #[test]
    fn test_format_escalation_message() {
        let ctx = EscalationContext {
            escalation_type: EscalationType::SurfacePolicyBlocking,
            target: Some("feature::rule::scenario".to_string()),
            attempts: 1,
            tried_approaches: vec!["CreateMissingBindings".to_string()],
            blocked_by: Some("spec surface is locked".to_string()),
            suggested_options: vec![
                EscalationOption {
                    id: "unlock_spec".to_string(),
                    label: "Unlock spec surface".to_string(),
                    description: "Allow editing .feature files".to_string(),
                },
                EscalationOption {
                    id: "skip".to_string(),
                    label: "Skip this issue".to_string(),
                    description: "Move on".to_string(),
                },
            ],
        };

        let message = format_escalation_message(&ctx);

        assert!(message.contains("HUMAN INTERVENTION REQUIRED"));
        assert!(message.contains("spec surface is locked"));
        assert!(message.contains("Unlock spec surface"));
        assert!(message.contains("Skip this issue"));
    }
}
