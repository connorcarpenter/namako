//! Mission type selection logic for Tesaki v1.8.

use crate::mission_type::{MissionType, MissionTypeCategory};
use crate::repo_state::{BindingIssueKind, RepoState, SpecIssueKind, StructureIssueKind};
use crate::stage::{detect_stage, Stage, StageConstraint};
use crate::surface_policy::SurfacePolicy;

/// Select the next mission type based on RepoState priority ordering.
pub fn select_mission_type(state: &RepoState) -> Option<MissionType> {
    if let Some(issue) = state.sut_issues.first() {
        return Some(MissionType::FixRegressionFromGateFailure {
            failure: crate::repo_state::FailureInfo::from(issue.clone()),
        });
    }

    if let Some(issue) = state.binding_issues.iter().find(|i| {
        matches!(i.kind, BindingIssueKind::MissingBinding) && i.scenario_key.is_some()
    }) {
        return Some(MissionType::CreateMissingBindings {
            scenario_key: issue.scenario_key.clone().unwrap(),
            missing_steps: issue.step_text.clone().into_iter().collect(),
        });
    }

    if let Some(issue) = state.structure_issues.first() {
        if matches!(issue.kind, StructureIssueKind::MissingIdentityTag) {
            let missing_tags = extract_tags_from_description(&issue.description);
            return Some(MissionType::NormalizeIdentityTags {
                feature_path: issue.location.clone(),
                missing_tags,
            });
        }
    }

    let has_missing = state
        .spec_issues
        .iter()
        .any(|issue| issue.kind == SpecIssueKind::MissingCoverage);
    let has_ambiguous = state
        .spec_issues
        .iter()
        .any(|issue| issue.kind == SpecIssueKind::Ambiguous);
    if has_ambiguous && !has_missing && state.coverage_assessment.is_none() {
        return Some(MissionType::AssessSpecCoverage);
    }

    if let Some(issue) = select_spec_issue_for_add_scenario(state) {
        // Check if this is a zero-coverage rule
        let rule_has_zero_scenarios = issue.rule_name.as_ref()
            .map(|r| state.scenario_count_for_rule(&issue.feature_path, r) == Some(0))
            .unwrap_or(false);

        // Check for deferred scenarios that could be promoted
        let deferred = issue.rule_name.as_ref()
            .map(|r| state.deferred_scenarios_for_rule(&issue.feature_path, r))
            .unwrap_or_default();

        if !deferred.is_empty() {
            // Has deferred scenarios — pick PromoteScenariosToExecutable
            return Some(MissionType::PromoteScenariosToExecutable {
                feature_path: issue.feature_path.clone(),
                scenario_name: deferred[0].clone(),
                rule_name: issue.rule_name.clone().unwrap_or_default(),
            });
        } else if rule_has_zero_scenarios {
            // Zero coverage, no deferred — pick DraftSpecScenarios
            return Some(MissionType::DraftSpecScenarios {
                feature_path: issue.feature_path.clone(),
                rule_name: issue.rule_name.clone(),
            });
        } else {
            // Partial coverage — use AddOrClarifyScenario
            return Some(MissionType::AddOrClarifyScenario {
                feature_path: issue.feature_path.clone(),
                rule_name: issue.rule_name.clone(),
            });
        }
    }

    None
}

/// Select a mission type with optional stage constraint.
pub fn select_mission_type_for_stage(
    state: &RepoState,
    stage: Option<Stage>,
) -> Option<MissionType> {
    let stage = stage?;

    if let Some(candidate) = select_mission_type(state) {
        if stage.applicable_mission_types().contains(&candidate.category()) {
            Some(candidate)
        } else {
            select_alternative_for_stage(state, stage)
        }
    } else {
        select_alternative_for_stage(state, stage)
    }
}

fn select_alternative_for_stage(state: &RepoState, stage: Stage) -> Option<MissionType> {
    let category = stage.applicable_mission_types();

    if category.contains(&MissionTypeCategory::Sut) {
        if let Some(issue) = state.sut_issues.first() {
            return Some(MissionType::FixRegressionFromGateFailure {
                failure: crate::repo_state::FailureInfo::from(issue.clone()),
            });
        }
    }

    if category.contains(&MissionTypeCategory::Tests) {
        if let Some(issue) = state.binding_issues.iter().find(|i| {
            matches!(i.kind, BindingIssueKind::MissingBinding) && i.scenario_key.is_some()
        }) {
            return Some(MissionType::CreateMissingBindings {
                scenario_key: issue.scenario_key.clone().unwrap(),
                missing_steps: issue.step_text.clone().into_iter().collect(),
            });
        }
    }

    if category.contains(&MissionTypeCategory::Structure) {
        if let Some(issue) = state.structure_issues.iter().find(|i| {
            matches!(i.kind, StructureIssueKind::MissingIdentityTag)
        }) {
            let missing_tags = extract_tags_from_description(&issue.description);
            return Some(MissionType::NormalizeIdentityTags {
                feature_path: issue.location.clone(),
                missing_tags,
            });
        }
    }

    if category.contains(&MissionTypeCategory::Spec) {
        let has_missing = state
            .spec_issues
            .iter()
            .any(|issue| issue.kind == SpecIssueKind::MissingCoverage);
        let has_ambiguous = state
            .spec_issues
            .iter()
            .any(|issue| issue.kind == SpecIssueKind::Ambiguous);
        if has_ambiguous && !has_missing && state.coverage_assessment.is_none() {
            return Some(MissionType::AssessSpecCoverage);
        }

        if let Some(issue) = select_spec_issue_for_add_scenario(state) {
            // Check if this is a zero-coverage rule
            let rule_has_zero_scenarios = issue.rule_name.as_ref()
                .map(|r| state.scenario_count_for_rule(&issue.feature_path, r) == Some(0))
                .unwrap_or(false);

            // Check for deferred scenarios that could be promoted
            let deferred = issue.rule_name.as_ref()
                .map(|r| state.deferred_scenarios_for_rule(&issue.feature_path, r))
                .unwrap_or_default();

            if !deferred.is_empty() {
                // Has deferred scenarios — pick PromoteScenariosToExecutable
                return Some(MissionType::PromoteScenariosToExecutable {
                    feature_path: issue.feature_path.clone(),
                    scenario_name: deferred[0].clone(),
                    rule_name: issue.rule_name.clone().unwrap_or_default(),
                });
            } else if rule_has_zero_scenarios {
                // Zero coverage, no deferred — pick DraftSpecScenarios
                return Some(MissionType::DraftSpecScenarios {
                    feature_path: issue.feature_path.clone(),
                    rule_name: issue.rule_name.clone(),
                });
            } else {
                // Partial coverage — use AddOrClarifyScenario
                return Some(MissionType::AddOrClarifyScenario {
                    feature_path: issue.feature_path.clone(),
                    rule_name: issue.rule_name.clone(),
                });
            }
        }
    }

    if category.contains(&MissionTypeCategory::Finalize) && !state.has_work() {
        if state.coverage_is_ambiguous() && state.coverage_assessment.is_none() {
            return Some(MissionType::AssessSpecCoverage);
        }
        return Some(MissionType::SummarizeAndClose);
    }

    None
}

fn select_spec_issue_for_add_scenario(state: &RepoState) -> Option<&crate::repo_state::SpecIssue> {
    let mut missing: Vec<&crate::repo_state::SpecIssue> = state
        .spec_issues
        .iter()
        .filter(|issue| issue.kind == SpecIssueKind::MissingCoverage)
        .collect();

    if missing.is_empty() {
        return None;
    }

    if let Some(issue) = missing.iter().find(|issue| issue.rule_name.is_none()) {
        return Some(issue);
    }

    if let Some(issue) = missing.iter().find(|issue| {
        issue
            .rule_name
            .as_ref()
            .map(|rule| state.scenario_count_for_rule(&issue.feature_path, rule) == Some(0))
            .unwrap_or(false)
    }) {
        return Some(issue);
    }

    missing.sort_by_key(|issue| issue.feature_path.clone());
    missing.first().copied()
}

/// Select mission type and surface policy given a StageConstraint.
pub fn select_with_constraints(
    state: &RepoState,
    constraint: &StageConstraint,
) -> Option<(MissionType, Stage, SurfacePolicy)> {
    let detected_stage = detect_stage(state);
    let mut active_stage = constraint.stage.unwrap_or(detected_stage);

    let mission_type = select_mission_type_for_stage(state, Some(active_stage))?;
    if matches!(mission_type, MissionType::AssessSpecCoverage) {
        active_stage = Stage::Finalize;
    }

    let surface_policy = constraint
        .surface_overrides
        .clone()
        .unwrap_or_else(|| mission_type.default_surface_policy());

    Some((mission_type, active_stage, surface_policy))
}

/// Extract @Tag names from a description string.
/// Handles formats like "Missing @Feature tag", "Missing tags: @Feature, @Rule"
fn extract_tags_from_description(description: &str) -> Vec<String> {
    let mut tags = Vec::new();
    for word in description.split(|c: char| c.is_whitespace() || c == ',') {
        let trimmed = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '@');
        if trimmed.starts_with('@') && trimmed.len() > 1 {
            tags.push(trimmed.to_string());
        }
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo_state::{BindingIssue, BindingIssueKind, CoverageAmbiguity, RuleCoverageInfo, SpecIssue, SpecIssueKind, StructureIssueKind};

    #[test]
    fn selects_binding_issue_before_spec_issue() {
        let state = RepoState {
            binding_issues: vec![BindingIssue {
                kind: BindingIssueKind::MissingBinding,
                scenario_key: Some("scenario".to_string()),
                step_text: Some("Given a user".to_string()),
                description: "Missing".to_string(),
            }],
            spec_issues: vec![SpecIssue {
                kind: SpecIssueKind::MissingCoverage,
                feature_path: "features/a.feature".to_string(),
                description: "Missing coverage".to_string(),
                rule_name: None,
            }],
            ..Default::default()
        };

        let mission = select_mission_type(&state).unwrap();
        match mission {
            MissionType::CreateMissingBindings { .. } => {}
            _ => panic!("expected CreateMissingBindings"),
        }
    }

    #[test]
    fn select_with_constraints_respects_stage() {
        let state = RepoState {
            spec_issues: vec![SpecIssue {
                kind: SpecIssueKind::MissingCoverage,
                feature_path: "features/a.feature".to_string(),
                description: "Missing coverage".to_string(),
                rule_name: None,
            }],
            ..Default::default()
        };

        let constraint = StageConstraint {
            stage: Some(Stage::RefineSpec),
            surface_overrides: None,
        };

        let (mission, stage, _) = select_with_constraints(&state, &constraint).unwrap();
        assert_eq!(stage, Stage::RefineSpec);
        assert!(matches!(mission, MissionType::AddOrClarifyScenario { .. }));
    }

    #[test]
    fn selects_assess_spec_coverage_for_ambiguous_only() {
        let state = RepoState {
            spec_issues: vec![SpecIssue {
                kind: SpecIssueKind::Ambiguous,
                feature_path: "features/a.feature".to_string(),
                description: "Ambiguous coverage".to_string(),
                rule_name: Some("Rule(01)".to_string()),
            }],
            ..Default::default()
        };

        let mission = select_mission_type(&state).unwrap();
        assert!(matches!(mission, MissionType::AssessSpecCoverage));
    }

    #[test]
    fn selects_zero_coverage_before_partial_coverage() {
        let state = RepoState {
            spec_issues: vec![
                SpecIssue {
                    kind: SpecIssueKind::MissingCoverage,
                    feature_path: "features/partial.feature".to_string(),
                    description: "Rule has 1 scenario".to_string(),
                    rule_name: Some("Partial Rule".to_string()),
                },
                SpecIssue {
                    kind: SpecIssueKind::MissingCoverage,
                    feature_path: "features/zero.feature".to_string(),
                    description: "Rule has 0 scenarios".to_string(),
                    rule_name: Some("Zero Rule".to_string()),
                },
            ],
            scenarios_per_rule: {
                let mut map = std::collections::HashMap::new();
                map.insert("features/partial.feature::Partial Rule".to_string(), 1);
                map.insert("features/zero.feature::Zero Rule".to_string(), 0);
                map
            },
            ..Default::default()
        };

        let mission = select_mission_type(&state).unwrap();
        match mission {
            MissionType::DraftSpecScenarios { feature_path, .. } => {
                assert_eq!(feature_path, "features/zero.feature");
            }
            _ => panic!("expected DraftSpecScenarios for zero-coverage rule"),
        }
    }

    #[test]
    fn select_finalize_prefers_assess_spec_coverage_when_ambiguous() {
        let state = RepoState {
            lint_status: crate::repo_state::GateStatus::Pass,
            run_status: crate::repo_state::GateStatus::Pass,
            verify_status: crate::repo_state::GateStatus::Pass,
            coverage_ambiguity: CoverageAmbiguity {
                rules_with_one_scenario: vec![RuleCoverageInfo {
                    feature_path: "features/a.feature".to_string(),
                    rule_name: "Rule A".to_string(),
                    executable_scenarios: 1,
                }],
                rules_with_many_scenarios: vec![],
            },
            ..Default::default()
        };

        let constraint = StageConstraint {
            stage: Some(Stage::Finalize),
            surface_overrides: None,
        };

        let (mission, stage, _) = select_with_constraints(&state, &constraint).unwrap();
        assert_eq!(stage, Stage::Finalize);
        assert!(matches!(mission, MissionType::AssessSpecCoverage));
    }

    #[test]
    fn skips_binding_issue_without_scenario_key() {
        let state = RepoState {
            binding_issues: vec![crate::repo_state::BindingIssue {
                kind: BindingIssueKind::MissingBinding,
                scenario_key: None, // no concrete target
                step_text: Some("Given a user".to_string()),
                description: "Missing binding".to_string(),
            }],
            spec_issues: vec![SpecIssue {
                kind: SpecIssueKind::MissingCoverage,
                feature_path: "features/a.feature".to_string(),
                description: "Missing coverage".to_string(),
                rule_name: None,
            }],
            ..Default::default()
        };

        let mission = select_mission_type(&state).unwrap();
        // Should fall through to AddOrClarifyScenario because the binding issue has no target
        assert!(matches!(mission, MissionType::AddOrClarifyScenario { .. }));
    }

    #[test]
    fn extracts_missing_tags_from_description() {
        assert_eq!(
            extract_tags_from_description("Missing @Feature tag"),
            vec!["@Feature"]
        );
        assert_eq!(
            extract_tags_from_description("Missing tags: @Feature, @Rule"),
            vec!["@Feature", "@Rule"]
        );
        assert!(extract_tags_from_description("No tags here").is_empty());
    }

    #[test]
    fn normalize_identity_tags_extracts_evidence() {
        let state = RepoState {
            structure_issues: vec![crate::repo_state::StructureIssue {
                kind: StructureIssueKind::MissingIdentityTag,
                location: "features/x.feature".to_string(),
                description: "Missing @Feature tag".to_string(),
            }],
            ..Default::default()
        };

        let mission = select_mission_type(&state).unwrap();
        match mission {
            MissionType::NormalizeIdentityTags { missing_tags, .. } => {
                assert_eq!(missing_tags, vec!["@Feature"]);
            }
            _ => panic!("expected NormalizeIdentityTags"),
        }
    }

    #[test]
    fn skips_non_identity_structure_issues() {
        // ParseError is not MissingIdentityTag, so should not select NormalizeIdentityTags
        let state = RepoState {
            structure_issues: vec![crate::repo_state::StructureIssue {
                kind: StructureIssueKind::ParseError,
                location: "features/x.feature".to_string(),
                description: "Syntax error at line 5".to_string(),
            }],
            spec_issues: vec![SpecIssue {
                kind: SpecIssueKind::MissingCoverage,
                feature_path: "features/a.feature".to_string(),
                description: "Missing coverage".to_string(),
                rule_name: None,
            }],
            ..Default::default()
        };

        let mission = select_mission_type(&state).unwrap();
        // Falls through structure_issues (ParseError doesn't match) to spec issues
        assert!(matches!(mission, MissionType::AddOrClarifyScenario { .. }));
    }

    #[test]
    fn selects_draft_spec_for_zero_coverage_rule() {
        let state = RepoState {
            spec_issues: vec![SpecIssue {
                kind: SpecIssueKind::MissingCoverage,
                feature_path: "features/a.feature".to_string(),
                description: "Rule has 0 scenarios".to_string(),
                rule_name: Some("Empty Rule".to_string()),
            }],
            scenarios_per_rule: {
                let mut map = std::collections::HashMap::new();
                map.insert("features/a.feature::Empty Rule".to_string(), 0);
                map
            },
            ..Default::default()
        };

        let mission = select_mission_type(&state).unwrap();
        assert!(matches!(mission, MissionType::DraftSpecScenarios { .. }));
    }

    #[test]
    fn selects_promote_when_deferred_exists() {
        use crate::packet_parser::{ReviewPacket, DeferredScenarioItem, BlockerType, IdentityFields, CoverageSummary};

        let state = RepoState {
            spec_issues: vec![SpecIssue {
                kind: SpecIssueKind::MissingCoverage,
                feature_path: "features/a.feature".to_string(),
                description: "Rule has 0 executable scenarios".to_string(),
                rule_name: Some("Deferred Rule".to_string()),
            }],
            review: Some(ReviewPacket {
                version: 1,
                spec_root: "/test".to_string(),
                identity_current: IdentityFields {
                    hash_contract_version: "v1".to_string(),
                    feature_fingerprint_hash: "a".to_string(),
                    step_registry_hash: "b".to_string(),
                    resolved_plan_hash: "c".to_string(),
                },
                features: vec![],
                coverage_summary: CoverageSummary {
                    rules_total: 1,
                    rules_with_zero_executable: 1,
                    executable_scenarios_total: 0,
                    deferred_items_total: 1,
                },
                deferred_items: vec![DeferredScenarioItem {
                    scenario_key: "features/a.feature::Deferred Rule::Deferred scenario".to_string(),
                    scenario_name: "Deferred scenario".to_string(),
                    feature_path: "features/a.feature".to_string(),
                    rule_name: "Deferred Rule".to_string(),
                    blocker: BlockerType::External,
                }],
                promotion_candidates: vec![],
                missing_bindings_for_top_candidates: vec![],
                harness_gaps: vec![],
                suggested_binding_bundle: None,
            }),
            ..Default::default()
        };

        let mission = select_mission_type(&state).unwrap();
        assert!(matches!(mission, MissionType::PromoteScenariosToExecutable { .. }));
    }
}
