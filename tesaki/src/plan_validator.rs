//! Plan validation module for pre-flight surface policy checks.
//!
//! This module validates proposed file changes against surface policy
//! BEFORE they are actually written to disk, providing early feedback
//! to the AI agent about policy violations.

use crate::base_runner::matches_any_pattern;

/// A proposed plan from the runner containing files to modify.
#[derive(Debug, Clone)]
pub struct ProposedPlan {
    pub files_to_modify: Vec<String>,
}

/// Result of validating a proposed plan against surface policy.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub valid: bool,
    pub violations: Vec<String>,
    pub guidance: String,
}

/// Validate a proposed plan against surface policy.
///
/// This function checks if the proposed file changes would violate
/// the current surface policy BEFORE any changes are made.
pub fn validate_plan(
    plan: &ProposedPlan,
    spec_patterns: &[String],
    tests_patterns: &[String],
    sut_patterns: &[String],
    spec_locked: bool,
    tests_locked: bool,
    sut_locked: bool,
) -> ValidationResult {
    let mut violations = Vec::new();

    for file in &plan.files_to_modify {
        let in_spec = matches_any_pattern(file, spec_patterns);
        let in_tests = matches_any_pattern(file, tests_patterns);
        let in_sut = matches_any_pattern(file, sut_patterns);

        if in_spec && spec_locked {
            violations.push(format!("{} (spec surface is LOCKED)", file));
        } else if in_tests && tests_locked {
            violations.push(format!("{} (tests surface is LOCKED)", file));
        } else if in_sut && sut_locked {
            violations.push(format!("{} (sut surface is LOCKED)", file));
        }
    }

    let valid = violations.is_empty();
    let guidance = if valid {
        "All proposed file changes are within allowed surfaces.".to_string()
    } else {
        format!(
            "⚠️  The following files are in LOCKED surfaces and cannot be modified:\n{}\n\n\
            RECOMMENDATION: Focus only on files in UNLOCKED surfaces, or request that the \
            appropriate surface be unlocked if this change is necessary.",
            violations.iter().map(|v| format!("  - {}", v)).collect::<Vec<_>>().join("\n")
        )
    };

    ValidationResult {
        valid,
        violations,
        guidance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_plan_all_allowed() {
        let plan = ProposedPlan {
            files_to_modify: vec!["test/step_definitions.rs".to_string()],
        };

        let result = validate_plan(
            &plan,
            &["specs/**/*.feature".to_string()],
            &["test/**".to_string()],
            &["src/**".to_string()],
            true,  // spec locked
            false, // tests unlocked
            true,  // sut locked
        );

        assert!(result.valid);
        assert!(result.violations.is_empty());
    }

    #[test]
    fn test_validate_plan_with_violation() {
        let plan = ProposedPlan {
            files_to_modify: vec![
                "test/step_definitions.rs".to_string(),
                "specs/feature.feature".to_string(),
            ],
        };

        let result = validate_plan(
            &plan,
            &["specs/**/*.feature".to_string()],
            &["test/**".to_string()],
            &["src/**".to_string()],
            true,  // spec locked
            false, // tests unlocked
            true,  // sut locked
        );

        assert!(!result.valid);
        assert_eq!(result.violations.len(), 1);
        assert!(result.violations[0].contains("specs/feature.feature"));
        assert!(result.violations[0].contains("spec surface is LOCKED"));
    }

    #[test]
    fn test_glob_matching() {
        assert!(matches_any_pattern("test/steps.rs", &["test/**".to_string()]));
        assert!(matches_any_pattern("specs/feature.feature", &["specs/**/*.feature".to_string()]));
        assert!(!matches_any_pattern("src/main.rs", &["test/**".to_string()]));
    }
}
