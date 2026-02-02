//! Mission type definitions and helpers.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::binding_extractor::extract_binding_exemplars;
use crate::scenario_extractor::extract_example_scenario;
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

/// Format binding exemplars as a markdown section for mission context.
fn format_binding_exemplars(exemplars: &[crate::prompts::BindingExemplar]) -> String {
    if exemplars.is_empty() {
        return String::new();
    }
    
    let mut output = String::from("## Similar Bindings (from this repository)\n\n");
    output.push_str("Use these patterns from existing bindings:\n\n");
    
    for (i, ex) in exemplars.iter().enumerate() {
        output.push_str(&format!("### Example {}: `{}`\n", i + 1, ex.step_text));
        output.push_str(&format!("**File:** `{}`\n", ex.file_path));
        output.push_str("```rust\n");
        output.push_str(&ex.binding_code);
        output.push_str("\n```\n\n");
    }
    
    output
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

    /// Returns the recommended model tier for this mission type.
    ///
    /// Model tiers (from most capable to least):
    /// - "opus": High intelligence tasks (spec interpretation, debugging, complex reasoning)
    /// - "sonnet": Structured work with patterns (bindings, implementation, refactoring)
    /// - "haiku": Trivial tasks (normalization, cleanup, summaries)
    pub fn recommended_model(&self) -> &'static str {
        match self {
            // High intelligence required - spec interpretation, debugging
            Self::AddOrClarifyScenario { .. } => "opus",
            Self::RefineFeatureIntent { .. } => "opus",
            Self::FixRegressionFromGateFailure { .. } => "opus",

            // Structured work with patterns
            Self::CreateMissingBindings { .. } => "sonnet",
            Self::ImplementBehaviorForScenario { .. } => "sonnet",
            Self::StrengthenThenAssertions { .. } => "sonnet",
            Self::RefactorBindingsForClarity { .. } => "sonnet",
            Self::TriageFailures => "sonnet",

            // Trivial tasks
            Self::NormalizeIdentityTags { .. } => "haiku",
            Self::SummarizeAndClose => "haiku",
            Self::CleanupAfterSuccess => "haiku",
            Self::ExplainState => "haiku",
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
            Self::ImplementBehaviorForScenario { scenario_key, scenario_name, failure_info } => {
                // Build context with failure details if available
                let mut context = format!("Scenario '{}' is failing in the last run.", scenario_name);
                
                if let Some(ref failure) = failure_info {
                    context.push_str(&format!("\n\n**Failure Kind:** {}", failure.failure_kind));
                    if let Some(ref err_msg) = failure.error_message {
                        // Include error message, truncated if very long
                        let truncated_msg = if err_msg.len() > 2000 {
                            format!("{}...\n(truncated)", &err_msg[..2000])
                        } else {
                            err_msg.clone()
                        };
                        context.push_str(&format!("\n\n**Error Message:**\n```\n{}\n```", truncated_msg));
                    }
                }
                
                MissionBrief {
                    mission_type: self.clone(),
                    title: format!("Implement behavior for {}", scenario_key),
                    objective: format!("Implement SUT behavior so scenario '{}' passes.", scenario_name),
                    context,
                    validation_criteria: vec![
                        format!("Scenario '{}' passes", scenario_name),
                        "namako gate --json shows run passes".to_string(),
                    ],
                }
            },
            Self::FixRegressionFromGateFailure { failure } => {
                let mut context = format!(
                    "**Failure kind:** {}\n**Scenario key:** {}",
                    failure.failure_kind, failure.scenario_key
                );
                
                if let Some(ref err_msg) = failure.error_message {
                    let truncated_msg = if err_msg.len() > 2000 {
                        format!("{}...\n(truncated)", &err_msg[..2000])
                    } else {
                        err_msg.clone()
                    };
                    context.push_str(&format!("\n\n**Error Message:**\n```\n{}\n```", truncated_msg));
                }
                
                MissionBrief {
                    mission_type: self.clone(),
                    title: format!("Fix regression for {}", failure.scenario_key),
                    objective: format!(
                        "Fix regression causing '{}' to fail.",
                        failure.scenario_name
                    ),
                    context,
                    validation_criteria: vec![
                        format!("Scenario '{}' passes", failure.scenario_name),
                        "namako gate --json passes".to_string(),
                    ],
                }
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

    /// Generate a brief with dynamic binding exemplars from the codebase.
    ///
    /// This enhanced version extracts similar bindings from the steps directory
    /// and example scenarios from feature files to provide high-quality, 
    /// repository-specific examples instead of generic patterns.
    ///
    /// The `steps_dir` parameter should point to the test steps directory
    /// (e.g., `test/tests/src/steps`), and `specs_dir` should point to the
    /// specs features directory (e.g., `test/specs/features`).
    pub fn generate_brief_with_exemplars(
        &self, 
        state: &RepoState, 
        steps_dir: Option<&Path>,
        specs_dir: Option<&Path>,
    ) -> MissionBrief {
        let mut brief = self.generate_brief(state);
        
        // Enhance CreateMissingBindings missions with binding exemplars
        if let Self::CreateMissingBindings { missing_steps, .. } = self {
            if let Some(dir) = steps_dir {
                if dir.is_dir() {
                    if let Ok(exemplars) = extract_binding_exemplars(dir, missing_steps, 3) {
                        if !exemplars.is_empty() {
                            // Append exemplars to context
                            let exemplar_section = format_binding_exemplars(&exemplars);
                            brief.context = format!("{}\n\n{}", brief.context, exemplar_section);
                        }
                    }
                }
            }
        }
        
        // Enhance AddOrClarifyScenario missions with example scenarios
        if let Self::AddOrClarifyScenario { feature_path, .. } = self {
            if let Some(dir) = specs_dir {
                // Construct path to the feature file
                let feature_file = dir.join(feature_path);
                if let Ok(Some(example)) = extract_example_scenario(&feature_file) {
                    brief.context = format!(
                        "{}\n\n## Example Scenario (from this feature)\n\n```gherkin\n{}\n```",
                        brief.context,
                        example
                    );
                }
            }
        }
        
        brief
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
    fn recommended_model_opus_for_high_intelligence_tasks() {
        assert_eq!(MissionType::AddOrClarifyScenario { feature_path: "f".into(), rule_name: None }.recommended_model(), "opus");
        assert_eq!(MissionType::RefineFeatureIntent { feature_path: "f".into() }.recommended_model(), "opus");
        assert_eq!(MissionType::FixRegressionFromGateFailure {
            failure: crate::repo_state::FailureInfo {
                scenario_key: "s".into(),
                scenario_name: "n".into(),
                failure_kind: "test".into(),
                error_message: None,
            }
        }.recommended_model(), "opus");
    }

    #[test]
    fn recommended_model_sonnet_for_structured_work() {
        assert_eq!(MissionType::CreateMissingBindings { scenario_key: "s".into(), missing_steps: vec![] }.recommended_model(), "sonnet");
        assert_eq!(MissionType::ImplementBehaviorForScenario { scenario_key: "s".into(), scenario_name: "n".into(), failure_info: None }.recommended_model(), "sonnet");
        assert_eq!(MissionType::StrengthenThenAssertions { scenario_key: "s".into(), weak_steps: vec![] }.recommended_model(), "sonnet");
        assert_eq!(MissionType::RefactorBindingsForClarity { binding_ids: vec![] }.recommended_model(), "sonnet");
        assert_eq!(MissionType::TriageFailures.recommended_model(), "sonnet");
    }

    #[test]
    fn recommended_model_haiku_for_trivial_tasks() {
        assert_eq!(MissionType::NormalizeIdentityTags { feature_path: "f".into(), missing_tags: vec![] }.recommended_model(), "haiku");
        assert_eq!(MissionType::SummarizeAndClose.recommended_model(), "haiku");
        assert_eq!(MissionType::CleanupAfterSuccess.recommended_model(), "haiku");
        assert_eq!(MissionType::ExplainState.recommended_model(), "haiku");
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
