//! Mission type selection logic for Tesaki v1.8.

use crate::mission_type::{MissionType, MissionTypeCategory};
use crate::repo_state::{BindingIssueKind, RepoState};
use crate::stage::{detect_stage, Stage, StageConstraint};
use crate::surface_policy::SurfacePolicy;

/// Select the next mission type based on RepoState priority ordering.
pub fn select_mission_type(state: &RepoState) -> Option<MissionType> {
    if let Some(issue) = state.sut_issues.first() {
        return Some(MissionType::FixRegressionFromGateFailure {
            failure: crate::repo_state::FailureInfo::from(issue.clone()),
        });
    }

    if let Some(issue) = state.binding_issues.first() {
        if let BindingIssueKind::MissingBinding = issue.kind {
            return Some(MissionType::CreateMissingBindings {
                scenario_key: issue.scenario_key.clone().unwrap_or_else(|| "unknown".to_string()),
                missing_steps: issue.step_text.clone().into_iter().collect(),
            });
        }
    }

    if let Some(issue) = state.structure_issues.first() {
        return Some(MissionType::NormalizeIdentityTags {
            feature_path: issue.location.clone(),
            missing_tags: vec![],
        });
    }

    if let Some(issue) = state.spec_issues.first() {
        return Some(MissionType::AddOrClarifyScenario {
            feature_path: issue.feature_path.clone(),
            rule_name: issue.rule_name.clone(),
        });
    }

    None
}

/// Select a mission type with optional stage constraint.
pub fn select_mission_type_for_stage(
    state: &RepoState,
    stage: Option<Stage>,
) -> Option<MissionType> {
    let candidate = select_mission_type(state)?;
    let stage = stage?;

    if stage.applicable_mission_types().contains(&candidate.category()) {
        Some(candidate)
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
        if let Some(issue) = state.binding_issues.first() {
            if let BindingIssueKind::MissingBinding = issue.kind {
                return Some(MissionType::CreateMissingBindings {
                    scenario_key: issue.scenario_key.clone().unwrap_or_else(|| "unknown".to_string()),
                    missing_steps: issue.step_text.clone().into_iter().collect(),
                });
            }
        }
    }

    if category.contains(&MissionTypeCategory::Structure) {
        if let Some(issue) = state.structure_issues.first() {
            return Some(MissionType::NormalizeIdentityTags {
                feature_path: issue.location.clone(),
                missing_tags: vec![],
            });
        }
    }

    if category.contains(&MissionTypeCategory::Spec) {
        if let Some(issue) = state.spec_issues.first() {
            return Some(MissionType::AddOrClarifyScenario {
                feature_path: issue.feature_path.clone(),
                rule_name: issue.rule_name.clone(),
            });
        }
    }

    if category.contains(&MissionTypeCategory::Finalize) && !state.has_work() {
        return Some(MissionType::SummarizeAndClose);
    }

    None
}

/// Select mission type and surface policy given a StageConstraint.
pub fn select_with_constraints(
    state: &RepoState,
    constraint: &StageConstraint,
) -> Option<(MissionType, Stage, SurfacePolicy)> {
    let detected_stage = detect_stage(state);
    let active_stage = constraint.stage.unwrap_or(detected_stage);

    let mission_type = select_mission_type_for_stage(state, Some(active_stage))?;

    let surface_policy = constraint
        .surface_overrides
        .clone()
        .unwrap_or_else(|| mission_type.default_surface_policy());

    Some((mission_type, active_stage, surface_policy))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo_state::{BindingIssue, BindingIssueKind, SpecIssue, SpecIssueKind};

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
}
