//! Mission type definitions and helpers.

use serde::{Deserialize, Serialize};

use crate::repo_state::{FailureInfo, RepoState};
use crate::surface_policy::SurfacePolicy;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MissionTypeCategory {
    Spec,
    Structure,
    Tests,
    Sut,
    Finalize,
    Meta,
}

/// Mission type templates for Tesaki v1.8.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MissionType {
    // Core (Priority 1)
    CreateMissingBindings {
        scenario_key: String,
        missing_steps: Vec<String>,
    },
    ImplementBehaviorForScenario {
        scenario_key: String,
        scenario_name: String,
        failure_info: Option<FailureInfo>,
    },
    FixRegressionFromGateFailure {
        failure: FailureInfo,
    },

    // Spec (Priority 2)
    RefineFeatureIntent {
        feature_path: String,
    },
    AddOrClarifyScenario {
        feature_path: String,
        rule_name: Option<String>,
    },
    NormalizeIdentityTags {
        feature_path: String,
        missing_tags: Vec<String>,
    },

    // Tests (Priority 2)
    StrengthenThenAssertions {
        scenario_key: String,
        weak_steps: Vec<String>,
    },
    RefactorBindingsForClarity {
        binding_ids: Vec<String>,
    },

    // Finalize
    SummarizeAndClose,
    CleanupAfterSuccess,

    // Meta (no runner)
    ExplainState,
    TriageFailures,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum EvidenceChange {
    BindingCountIncreases,
    FailingScenarioDecreases,
    CoverageIncreases,
    GatePasses,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MissionBrief {
    pub mission_type: MissionType,
    pub title: String,
    pub objective: String,
    pub context: String,
    pub validation_criteria: Vec<String>,
}

impl MissionType {
    pub fn name(&self) -> &str {
        match self {
            Self::CreateMissingBindings { .. } => "CreateMissingBindings",
            Self::ImplementBehaviorForScenario { .. } => "ImplementBehaviorForScenario",
            Self::FixRegressionFromGateFailure { .. } => "FixRegressionFromGateFailure",
            Self::RefineFeatureIntent { .. } => "RefineFeatureIntent",
            Self::AddOrClarifyScenario { .. } => "AddOrClarifyScenario",
            Self::NormalizeIdentityTags { .. } => "NormalizeIdentityTags",
            Self::StrengthenThenAssertions { .. } => "StrengthenThenAssertions",
            Self::RefactorBindingsForClarity { .. } => "RefactorBindingsForClarity",
            Self::SummarizeAndClose => "SummarizeAndClose",
            Self::CleanupAfterSuccess => "CleanupAfterSuccess",
            Self::ExplainState => "ExplainState",
            Self::TriageFailures => "TriageFailures",
        }
    }

    pub fn category(&self) -> MissionTypeCategory {
        match self {
            Self::CreateMissingBindings { .. }
            | Self::StrengthenThenAssertions { .. }
            | Self::RefactorBindingsForClarity { .. } => MissionTypeCategory::Tests,
            Self::ImplementBehaviorForScenario { .. }
            | Self::FixRegressionFromGateFailure { .. } => MissionTypeCategory::Sut,
            Self::RefineFeatureIntent { .. } | Self::AddOrClarifyScenario { .. } => MissionTypeCategory::Spec,
            Self::NormalizeIdentityTags { .. } => MissionTypeCategory::Structure,
            Self::SummarizeAndClose | Self::CleanupAfterSuccess => MissionTypeCategory::Finalize,
            Self::ExplainState | Self::TriageFailures => MissionTypeCategory::Meta,
        }
    }

    pub fn default_surface_policy(&self) -> SurfacePolicy {
        match self {
            Self::CreateMissingBindings { .. }
            | Self::StrengthenThenAssertions { .. }
            | Self::RefactorBindingsForClarity { .. } => SurfacePolicy::for_implement_tests(),
            Self::ImplementBehaviorForScenario { .. }
            | Self::FixRegressionFromGateFailure { .. } => SurfacePolicy::for_implement_sut(),
            Self::RefineFeatureIntent { .. } | Self::AddOrClarifyScenario { .. } => SurfacePolicy::for_refine_spec(),
            Self::NormalizeIdentityTags { .. } => SurfacePolicy::for_structure_spec(),
            Self::SummarizeAndClose | Self::CleanupAfterSuccess => SurfacePolicy::for_finalize(),
            Self::ExplainState | Self::TriageFailures => SurfacePolicy::for_finalize(),
        }
    }

    #[allow(dead_code)]
    pub fn requires_runner(&self) -> bool {
        !matches!(self, Self::ExplainState | Self::TriageFailures)
    }

    #[allow(dead_code)]
    pub fn expected_evidence_change(&self) -> Vec<EvidenceChange> {
        match self {
            Self::CreateMissingBindings { .. } => vec![EvidenceChange::BindingCountIncreases],
            Self::ImplementBehaviorForScenario { .. } | Self::FixRegressionFromGateFailure { .. } => {
                vec![EvidenceChange::FailingScenarioDecreases]
            }
            Self::RefineFeatureIntent { .. } | Self::AddOrClarifyScenario { .. } => {
                vec![EvidenceChange::CoverageIncreases]
            }
            Self::NormalizeIdentityTags { .. } => vec![EvidenceChange::CoverageIncreases],
            Self::StrengthenThenAssertions { .. } => vec![EvidenceChange::GatePasses],
            Self::RefactorBindingsForClarity { .. } => vec![EvidenceChange::GatePasses],
            Self::SummarizeAndClose | Self::CleanupAfterSuccess => vec![EvidenceChange::GatePasses],
            Self::ExplainState | Self::TriageFailures => vec![],
        }
    }

    pub fn target_label(&self) -> Option<String> {
        match self {
            Self::CreateMissingBindings { scenario_key, .. } => Some(scenario_key.clone()),
            Self::ImplementBehaviorForScenario { scenario_key, .. } => Some(scenario_key.clone()),
            Self::FixRegressionFromGateFailure { failure } => Some(failure.scenario_key.clone()),
            Self::RefineFeatureIntent { feature_path } => Some(feature_path.clone()),
            Self::AddOrClarifyScenario { feature_path, .. } => Some(feature_path.clone()),
            Self::NormalizeIdentityTags { feature_path, .. } => Some(feature_path.clone()),
            Self::StrengthenThenAssertions { scenario_key, .. } => Some(scenario_key.clone()),
            Self::RefactorBindingsForClarity { .. } => None,
            Self::SummarizeAndClose | Self::CleanupAfterSuccess => None,
            Self::ExplainState | Self::TriageFailures => None,
        }
    }

    pub fn generate_brief(&self, state: &RepoState) -> MissionBrief {
        match self {
            Self::CreateMissingBindings { scenario_key, missing_steps: _ } => {
                // Collect ALL missing steps for comprehensive context (batching)
                let all_missing: Vec<String> = state.binding_issues
                    .iter()
                    .filter(|b| matches!(b.kind, crate::repo_state::BindingIssueKind::MissingBinding))
                    .filter_map(|b| b.step_text.clone())
                    .collect();
                
                // Deduplicate (same step text may appear multiple times)
                let mut unique_steps: Vec<String> = all_missing.clone();
                unique_steps.sort();
                unique_steps.dedup();
                
                let step_list = if unique_steps.is_empty() {
                    "No missing steps listed - check namako lint output.".to_string()
                } else {
                    unique_steps
                        .iter()
                        .take(30)  // Show up to 30 unique steps
                        .enumerate()
                        .map(|(i, s)| format!("{}. `{}`", i + 1, s))
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                
                let total = state.binding_issues.len();

                MissionBrief {
                    mission_type: self.clone(),
                    title: format!("Create missing bindings ({} total)", total),
                    objective: format!(
                        "Create step bindings for as many missing steps as possible. {} bindings needed.",
                        total
                    ),
                    context: format!(
                        "Missing step bindings ({} unique patterns, showing up to 30):\n{}\n\n\
                        Create bindings in test/tests/steps/. Use #[given], #[when], #[then] macros.",
                        unique_steps.len(),
                        step_list
                    ),
                    validation_criteria: vec![
                        "New bindings created for missing steps".to_string(),
                        "namako lint shows fewer unresolved steps".to_string(),
                    ],
                }
            }
            Self::ImplementBehaviorForScenario { scenario_key, scenario_name, .. } => MissionBrief {
                mission_type: self.clone(),
                title: format!("Implement behavior for {}", scenario_key),
                objective: format!("Implement SUT behavior so scenario '{}' passes.", scenario_name),
                context: format!("Scenario '{}' is failing in the last run.", scenario_name),
                validation_criteria: vec![
                    format!("Scenario '{}' passes", scenario_name),
                    "namako gate --json shows run passes".to_string(),
                ],
            },
            Self::FixRegressionFromGateFailure { failure } => MissionBrief {
                mission_type: self.clone(),
                title: format!("Fix regression for {}", failure.scenario_key),
                objective: format!(
                    "Fix regression causing '{}' to fail.",
                    failure.scenario_name
                ),
                context: format!(
                    "Failure kind: {}. Scenario key: {}.",
                    failure.failure_kind, failure.scenario_key
                ),
                validation_criteria: vec![
                    format!("Scenario '{}' passes", failure.scenario_name),
                    "namako gate --json passes".to_string(),
                ],
            },
            Self::RefineFeatureIntent { feature_path } => MissionBrief {
                mission_type: self.clone(),
                title: format!("Refine intent for {}", feature_path),
                objective: "Clarify feature intent and scope in the spec.".to_string(),
                context: format!("Feature {} is underspecified.", feature_path),
                validation_criteria: vec!["Spec intent clarifications added".to_string()],
            },
            Self::AddOrClarifyScenario { feature_path, rule_name } => MissionBrief {
                mission_type: self.clone(),
                title: format!("Add or clarify scenario in {}", feature_path),
                objective: "Add or clarify scenarios to improve coverage.".to_string(),
                context: format!(
                    "Coverage gaps detected in {}{}.",
                    feature_path,
                    rule_name
                        .as_ref()
                        .map(|r| format!(" ({})", r))
                        .unwrap_or_default()
                ),
                validation_criteria: vec!["New scenario(s) added".to_string()],
            },
            Self::NormalizeIdentityTags { feature_path, missing_tags } => MissionBrief {
                mission_type: self.clone(),
                title: format!("Normalize identity tags in {}", feature_path),
                objective: "Ensure required identity tags are present and correct.".to_string(),
                context: format!(
                    "Missing tags: {}",
                    if missing_tags.is_empty() {
                        "unknown".to_string()
                    } else {
                        missing_tags.join(", ")
                    }
                ),
                validation_criteria: vec!["Lint passes with identity tags fixed".to_string()],
            },
            Self::StrengthenThenAssertions { scenario_key, .. } => MissionBrief {
                mission_type: self.clone(),
                title: format!("Strengthen Then assertions for {}", scenario_key),
                objective: "Improve Then assertions to be specific and stable.".to_string(),
                context: format!("Then steps in {} need stronger assertions.", scenario_key),
                validation_criteria: vec!["Then assertions improved".to_string()],
            },
            Self::RefactorBindingsForClarity { binding_ids } => MissionBrief {
                mission_type: self.clone(),
                title: "Refactor bindings for clarity".to_string(),
                objective: "Improve binding clarity and reuse without changing behavior.".to_string(),
                context: format!(
                    "Refactor {} binding(s).",
                    binding_ids.len()
                ),
                validation_criteria: vec!["Bindings remain green under gate".to_string()],
            },
            Self::SummarizeAndClose => MissionBrief {
                mission_type: self.clone(),
                title: "Summarize and close".to_string(),
                objective: "Summarize changes and confirm clean state.".to_string(),
                context: format!("Repo state: {}", state.summary()),
                validation_criteria: vec!["Summary produced".to_string()],
            },
            Self::CleanupAfterSuccess => MissionBrief {
                mission_type: self.clone(),
                title: "Cleanup after success".to_string(),
                objective: "Ensure repo is clean and artifacts are tidy.".to_string(),
                context: "All gates pass; finalize cleanup.".to_string(),
                validation_criteria: vec!["No leftover artifacts".to_string()],
            },
            Self::ExplainState => MissionBrief {
                mission_type: self.clone(),
                title: "Explain state".to_string(),
                objective: "Summarize current state from packets.".to_string(),
                context: state.summary(),
                validation_criteria: vec![],
            },
            Self::TriageFailures => MissionBrief {
                mission_type: self.clone(),
                title: "Triage failures".to_string(),
                objective: "Cluster failures and identify likely causes.".to_string(),
                context: format!("Failures: {}", state.last_run_failures.len()),
                validation_criteria: vec![],
            },
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mission_type_names_are_stable() {
        let m = MissionType::CreateMissingBindings {
            scenario_key: "s".into(),
            missing_steps: vec![],
        };
        assert_eq!(m.name(), "CreateMissingBindings");
    }

    #[test]
    fn mission_type_requires_runner() {
        assert!(!MissionType::ExplainState.requires_runner());
        assert!(MissionType::SummarizeAndClose.requires_runner());
    }

    #[test]
    fn mission_type_default_policy() {
        let m = MissionType::ImplementBehaviorForScenario {
            scenario_key: "s".into(),
            scenario_name: "n".into(),
            failure_info: None,
        };
        let policy = m.default_surface_policy();
        assert_eq!(policy.sut, crate::surface_policy::SurfaceLock::Unlocked);
    }

    #[test]
    fn generate_brief_with_real_repo_state() {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let packets_dir = manifest_dir.join("../../naia/test/specs/target/namako_artifacts/tesaki");

        let status_path = packets_dir.join("status.json");
        let review_path = packets_dir.join("review.json");

        if !status_path.exists() || !review_path.exists() {
            return;
        }

        let status_json = std::fs::read_to_string(&status_path).unwrap();
        let review_json = std::fs::read_to_string(&review_path).unwrap();

        let status = crate::packet_parser::parse_status_json(&status_json).unwrap();
        let review = crate::packet_parser::parse_review_json(&review_json).unwrap();
        let gate = crate::packet_parser::GatePacket {
            lint: crate::packet_parser::GatePhase {
                status: crate::packet_parser::GatePhaseStatus::Pass,
                reason: None,
            },
            run: crate::packet_parser::GatePhase {
                status: crate::packet_parser::GatePhaseStatus::Pass,
                reason: None,
            },
            verify: crate::packet_parser::GatePhase {
                status: crate::packet_parser::GatePhaseStatus::Pass,
                reason: None,
            },
            determinism: None,
        };

        let state = RepoState::compute(&status, &review, &gate, None).unwrap();
        let mission = MissionType::SummarizeAndClose;
        let brief = mission.generate_brief(&state);
        assert!(brief.title.contains("Summarize"));
    }
}
