//! Stage definitions and detection for Tesaki v1.8.

use crate::mission_type::MissionTypeCategory;
use crate::repo_state::{GateStatus, RepoState};
use crate::surface_policy::SurfacePolicy;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Stage {
    RefineSpec,
    StructureSpec,
    ImplementTests,
    ImplementSut,
    Finalize,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct StageConstraint {
    pub stage: Option<Stage>,
    pub surface_overrides: Option<SurfacePolicy>,
}

impl Stage {
    pub fn name(&self) -> &str {
        match self {
            Self::RefineSpec => "Refine Spec",
            Self::StructureSpec => "Structure Spec",
            Self::ImplementTests => "Implement Tests",
            Self::ImplementSut => "Implement SUT",
            Self::Finalize => "Finalize",
        }
    }

    pub fn default_surface_policy(&self) -> SurfacePolicy {
        match self {
            Self::RefineSpec => SurfacePolicy::for_refine_spec(),
            Self::StructureSpec => SurfacePolicy::for_structure_spec(),
            Self::ImplementTests => SurfacePolicy::for_implement_tests(),
            Self::ImplementSut => SurfacePolicy::for_implement_sut(),
            Self::Finalize => SurfacePolicy::for_finalize(),
        }
    }

    pub fn applicable_mission_types(&self) -> Vec<MissionTypeCategory> {
        match self {
            Self::RefineSpec => vec![MissionTypeCategory::Spec],
            Self::StructureSpec => vec![MissionTypeCategory::Structure],
            Self::ImplementTests => vec![MissionTypeCategory::Tests],
            Self::ImplementSut => vec![MissionTypeCategory::Sut],
            Self::Finalize => vec![MissionTypeCategory::Finalize],
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "refine" => Some(Self::RefineSpec),
            "structure" => Some(Self::StructureSpec),
            "tests" => Some(Self::ImplementTests),
            "sut" => Some(Self::ImplementSut),
            "finalize" => Some(Self::Finalize),
            _ => None,
        }
    }
}

pub fn detect_stage(state: &RepoState) -> Stage {
    if state.lint_status == GateStatus::Pass
        && state.run_status == GateStatus::Pass
        && state.verify_status == GateStatus::Pass
        && state.sut_issues.is_empty()
        && state.binding_issues.is_empty()
        && state.structure_issues.is_empty()
        && state.spec_issues.is_empty()
    {
        return Stage::Finalize;
    }

    if !state.sut_issues.is_empty() {
        return Stage::ImplementSut;
    }

    if !state.binding_issues.is_empty() {
        return Stage::ImplementTests;
    }

    if !state.structure_issues.is_empty() {
        return Stage::StructureSpec;
    }

    if !state.spec_issues.is_empty() {
        return Stage::RefineSpec;
    }

    Stage::Finalize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo_state::{SpecIssue, SpecIssueKind};

    #[test]
    fn stage_names() {
        assert_eq!(Stage::RefineSpec.name(), "Refine Spec");
        assert_eq!(Stage::Finalize.name(), "Finalize");
    }

    #[test]
    fn detect_stage_refine_spec() {
        let state = RepoState {
            spec_issues: vec![SpecIssue {
                kind: SpecIssueKind::MissingCoverage,
                feature_path: "features/a.feature".to_string(),
                description: "Missing".to_string(),
                rule_name: None,
            }],
            ..Default::default()
        };
        assert_eq!(detect_stage(&state), Stage::RefineSpec);
    }

    #[test]
    fn detect_stage_finalize_when_clean() {
        let mut state = RepoState::new();
        state.lint_status = GateStatus::Pass;
        state.run_status = GateStatus::Pass;
        state.verify_status = GateStatus::Pass;
        assert_eq!(detect_stage(&state), Stage::Finalize);
    }

    #[test]
    fn stage_from_str() {
        assert_eq!(Stage::from_str("tests"), Some(Stage::ImplementTests));
        assert_eq!(Stage::from_str("unknown"), None);
    }

    #[test]
    fn applicable_mission_types() {
        let types = Stage::ImplementSut.applicable_mission_types();
        assert!(types.contains(&MissionTypeCategory::Sut));
    }
}
