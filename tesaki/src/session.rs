//! Session state for the interactive REPL.

use serde::{Deserialize, Serialize};
use servling::SessionTokenStats;

use crate::chat_planner::MissionProposal;
use crate::stage::Stage;
use crate::surface_policy::{SurfaceLock, SurfacePolicy};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionIntent {
    pub stage: Option<Stage>,
    pub surface_overrides: Option<SurfacePolicy>,
    pub focus: Option<String>,
}

impl SessionIntent {
    pub fn apply_user_message(&mut self, message: &str) -> bool {
        let mut updated = false;
        let msg = message.to_ascii_lowercase();

        let mut stage_updated = false;

        if msg.contains("refine") {
            self.stage = Some(Stage::RefineSpec);
            updated = true;
            stage_updated = true;
        } else if msg.contains("structure") {
            self.stage = Some(Stage::StructureSpec);
            updated = true;
            stage_updated = true;
        } else if msg.contains("sut only") || msg.contains("implement sut") || msg.contains("sut") {
            self.stage = Some(Stage::ImplementSut);
            updated = true;
            stage_updated = true;
        } else if msg.contains("tests") || msg.contains("bindings") {
            self.stage = Some(Stage::ImplementTests);
            updated = true;
            stage_updated = true;
        } else if msg.contains("finalize") || msg.contains("finish") {
            self.stage = Some(Stage::Finalize);
            updated = true;
            stage_updated = true;
        }

        let mut overrides = if stage_updated {
            self.stage
                .map(|s| s.default_surface_policy())
                .unwrap_or_else(SurfacePolicy::for_finalize)
        } else {
            self.surface_overrides.clone().unwrap_or_else(|| {
                self.stage
                    .map(|s| s.default_surface_policy())
                    .unwrap_or_else(SurfacePolicy::for_finalize)
            })
        };

        if msg.contains("lock spec") {
            overrides.spec = SurfaceLock::Locked;
            updated = true;
        }
        if msg.contains("unlock spec") {
            overrides.spec = SurfaceLock::Unlocked;
            updated = true;
        }
        if msg.contains("lock tests") || msg.contains("lock bindings") {
            overrides.tests_bindings = SurfaceLock::Locked;
            updated = true;
        }
        if msg.contains("do not touch tests") {
            overrides.tests_bindings = SurfaceLock::Locked;
            updated = true;
        }
        if msg.contains("unlock tests") || msg.contains("unlock bindings") {
            overrides.tests_bindings = SurfaceLock::Unlocked;
            updated = true;
        }
        if msg.contains("lock sut") {
            overrides.sut = SurfaceLock::Locked;
            updated = true;
        }
        if msg.contains("unlock sut") {
            overrides.sut = SurfaceLock::Unlocked;
            updated = true;
        }

        if msg.contains("lock everything else") {
            overrides.tests_bindings = SurfaceLock::Locked;
            overrides.sut = SurfaceLock::Locked;
            updated = true;
        }

        if updated {
            self.surface_overrides = Some(overrides);
        }

        if let Some(tag) = extract_scenario_tag(message) {
            self.focus = Some(tag);
            updated = true;
        }

        updated
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingMission {
    pub proposal: MissionProposal,
    pub approved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureRecord {
    pub mission_type: String,
    pub target: Option<String>,
    pub stop_reason: String,
    pub violated_files: Vec<String>,
    pub violated_surface: Option<String>,
    pub attempted_approach: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionState {
    pub intent: SessionIntent,
    pub last_packets_fingerprint: Option<String>,
    pub pending_mission: Option<PendingMission>,
    pub recent_missions: Vec<String>,
    pub chat_summary: Option<String>,
    pub last_repo_state_summary: Option<String>,
    /// Token usage statistics for the current session.
    #[serde(default)]
    pub token_stats: SessionTokenStats,
    /// Initial issue count at session start (for summary calculation).
    #[serde(default)]
    pub initial_issue_count: usize,
    /// Captured gate-failure details from the last mission (injected into next mission context).
    /// Written by the headless loop; consumed by run_run via .tesaki/last_failure.json.
    #[serde(default)]
    pub last_gate_failure: Option<crate::prompts::PreviousFailureContext>,
    /// Number of regressions detected this session.
    #[serde(default)]
    pub regression_count: u32,
    /// Number of policy violations detected this session.
    #[serde(default)]
    pub policy_violation_count: u32,
    /// History of failures this session.
    #[serde(default)]
    pub failure_history: Vec<FailureRecord>,
    /// How many times current target has failed.
    #[serde(default)]
    pub current_target_failures: u32,
    /// Number of issues resolved this session.
    #[serde(default)]
    pub issues_resolved: u32,
}

fn extract_scenario_tag(message: &str) -> Option<String> {
    let start = message.find("@Scenario(")?;
    let end = message[start..].find(')')?;
    Some(message[start..start + end + 1].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_detects_stage() {
        let mut intent = SessionIntent::default();
        intent.apply_user_message("let's focus on tests");
        assert_eq!(intent.stage, Some(Stage::ImplementTests));
    }

    #[test]
    fn intent_detects_locking() {
        let mut intent = SessionIntent::default();
        intent.apply_user_message("unlock spec and lock sut");
        let overrides = intent.surface_overrides.unwrap();
        assert_eq!(overrides.spec, SurfaceLock::Unlocked);
        assert_eq!(overrides.sut, SurfaceLock::Locked);
    }

    #[test]
    fn intent_locks_everything_else() {
        let mut intent = SessionIntent::default();
        intent.apply_user_message("unlock spec and lock everything else");
        let overrides = intent.surface_overrides.unwrap();
        assert_eq!(overrides.spec, SurfaceLock::Unlocked);
        assert_eq!(overrides.tests_bindings, SurfaceLock::Locked);
        assert_eq!(overrides.sut, SurfaceLock::Locked);
    }

    #[test]
    fn intent_prefers_sut_stage() {
        let mut intent = SessionIntent::default();
        intent.apply_user_message("implement sut only, do not touch tests");
        assert_eq!(intent.stage, Some(Stage::ImplementSut));
        let overrides = intent.surface_overrides.unwrap();
        assert_eq!(overrides.tests_bindings, SurfaceLock::Locked);
    }

    #[test]
    fn stage_change_resets_overrides() {
        let mut intent = SessionIntent::default();
        intent.apply_user_message("unlock spec");
        let overrides = intent.surface_overrides.clone().unwrap();
        assert_eq!(overrides.spec, SurfaceLock::Unlocked);

        intent.apply_user_message("implement sut only");
        let overrides = intent.surface_overrides.unwrap();
        assert_eq!(overrides.spec, SurfaceLock::Locked);
        assert_eq!(intent.stage, Some(Stage::ImplementSut));
    }

    #[test]
    fn intent_extracts_scenario_tag() {
        let mut intent = SessionIntent::default();
        intent.apply_user_message("check @Scenario(03) failures");
        assert_eq!(intent.focus, Some("@Scenario(03)".to_string()));
    }
}
