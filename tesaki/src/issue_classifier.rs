//! Issue classification for RepoState computation.

use crate::packet_parser::{ReviewPacket, StatusPacket, BlockerType};
use crate::repo_state::{
    BindingIssue, BindingIssueKind, Blocker, BlockerKind, SpecIssue, SpecIssueKind,
    StructureIssue, StructureIssueKind, SutIssue,
};

/// Classify spec issues from the review packet.
pub fn classify_spec_issues(review: &ReviewPacket) -> Vec<SpecIssue> {
    let mut issues = Vec::new();

    for feature in &review.features {
        let executable_count: usize = feature
            .rules
            .iter()
            .map(|rule| rule.executable_scenarios.len())
            .sum();

        if executable_count == 0 {
            issues.push(SpecIssue {
                kind: SpecIssueKind::MissingCoverage,
                feature_path: feature.feature_path.clone(),
                description: "Feature has zero executable scenarios.".to_string(),
                rule_name: None,
            });
        }

        for rule in &feature.rules {
            if rule.executable_scenarios.is_empty() {
                issues.push(SpecIssue {
                    kind: SpecIssueKind::MissingCoverage,
                    feature_path: feature.feature_path.clone(),
                    description: format!("Rule '{}' has zero executable scenarios.", rule.rule_name),
                    rule_name: Some(rule.rule_name.clone()),
                });
            }
        }
    }

    issues
}

/// Classify structure issues from status and review packets.
pub fn classify_structure_issues(status: &StatusPacket, _review: &ReviewPacket) -> Vec<StructureIssue> {
    let mut issues = Vec::new();

    if matches!(status.lint_status, crate::packet_parser::StatusValue::Fail) {
        issues.push(StructureIssue {
            kind: StructureIssueKind::ParseError,
            location: "unknown".to_string(),
            description: "Lint failed; check feature structure and identity tags.".to_string(),
        });
    }

    issues
}

/// Classify binding issues from review and status packets.
pub fn classify_binding_issues(review: &ReviewPacket, status: &StatusPacket) -> Vec<BindingIssue> {
    let mut issues = Vec::new();

    // Parse MissingStep errors from gates.lint.summary
    if let Some(gates) = &status.gates {
        if let Some(summary) = &gates.lint.summary {
            issues.extend(parse_missing_steps_from_summary(summary));
        }
    }

    // Also include missing_bindings_for_top_candidates from review
    for missing in &review.missing_bindings_for_top_candidates {
        if missing.missing_step_texts.is_empty() {
            continue;
        }
        for step in &missing.missing_step_texts {
            // Avoid duplicates
            if issues.iter().any(|i| i.step_text.as_deref() == Some(step.as_str())) {
                continue;
            }
            issues.push(BindingIssue {
                kind: BindingIssueKind::MissingBinding,
                scenario_key: Some(missing.candidate_name.clone()),
                step_text: Some(step.clone()),
                description: format!(
                    "Missing binding for step '{}' in candidate '{}'.",
                    step, missing.candidate_name
                ),
            });
        }
    }

    issues
}

/// Parse MissingStep entries from lint summary (debug format).
fn parse_missing_steps_from_summary(summary: &str) -> Vec<BindingIssue> {
    use regex::Regex;
    
    let mut issues = Vec::new();
    
    // Pattern: MissingStep { step_text: "...", step_kind: "...", feature_path: "...", line: N }
    let re = Regex::new(
        r#"MissingStep \{ step_text: "([^"]+)", step_kind: "([^"]+)", feature_path: "([^"]+)", line: (\d+) \}"#
    ).unwrap();
    
    for cap in re.captures_iter(summary) {
        let step_text = cap.get(1).map(|m| m.as_str().to_string());
        let step_kind = cap.get(2).map(|m| m.as_str());
        let feature_path = cap.get(3).map(|m| m.as_str().to_string());
        let line = cap.get(4).and_then(|m| m.as_str().parse::<u32>().ok());
        
        issues.push(BindingIssue {
            kind: BindingIssueKind::MissingBinding,
            scenario_key: None,
            step_text: step_text.clone(),
            description: format!(
                "Missing {} binding for \"{}\" at {}:{}",
                step_kind.unwrap_or("?"),
                step_text.as_deref().unwrap_or("?"),
                feature_path.as_deref().unwrap_or("?"),
                line.unwrap_or(0)
            ),
        });
    }
    
    issues
}

/// Classify SUT issues from status packet.
pub fn classify_sut_issues(status: &StatusPacket) -> Vec<SutIssue> {
    status
        .last_run_failures
        .iter()
        .map(|f| SutIssue {
            scenario_key: f.scenario_key.clone(),
            scenario_name: f.scenario_name.clone(),
            failure_kind: f.failure_kind.clone(),
            error_message: Some(f.summary.clone()),
        })
        .collect()
}

/// Classify global blockers from review packet.
pub fn classify_blockers(review: &ReviewPacket) -> Vec<Blocker> {
    let mut blockers = Vec::new();

    for deferred in &review.deferred_items {
        if deferred.blocker == BlockerType::External {
            blockers.push(Blocker {
                kind: BlockerKind::External,
                description: format!(
                    "Deferred scenario '{}' blocked by external dependency.",
                    deferred.scenario_key
                ),
            });
        }
    }

    blockers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet_parser::{CoverageSummary, FeatureReview, IdentityFields, MissingBindingInfo, ReviewPacket, RuleReview, SourceSpan, StatusPacket, StatusValue};

    fn review_stub() -> ReviewPacket {
        ReviewPacket {
            version: 1,
            spec_root: "/repo/specs".to_string(),
            identity_current: IdentityFields {
                hash_contract_version: "namako-v1-json+blake3-256".to_string(),
                feature_fingerprint_hash: "a".to_string(),
                step_registry_hash: "b".to_string(),
                resolved_plan_hash: "c".to_string(),
            },
            features: vec![FeatureReview {
                feature_path: "features/a.feature".to_string(),
                feature_name: "A".to_string(),
                rules: vec![RuleReview {
                    rule_name: "Rule(01)".to_string(),
                    source_span: SourceSpan { start_line: 1, end_line: 10 },
                    executable_scenarios: vec![],
                    deferred_items: vec![],
                }],
            }],
            coverage_summary: CoverageSummary {
                rules_total: 1,
                rules_with_zero_executable: 1,
                executable_scenarios_total: 0,
                deferred_items_total: 0,
            },
            deferred_items: vec![],
            promotion_candidates: vec![],
            missing_bindings_for_top_candidates: vec![],
            harness_gaps: vec![],
            suggested_binding_bundle: None,
        }
    }

    #[test]
    fn classify_spec_issues_detects_zero_coverage() {
        let review = review_stub();
        let issues = classify_spec_issues(&review);
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].kind, SpecIssueKind::MissingCoverage);
    }

    fn status_stub() -> StatusPacket {
        StatusPacket {
            version: 1,
            spec_root: "/repo/specs".to_string(),
            recommended_next_action: "".to_string(),
            lint_status: StatusValue::Pass,
            run_status: StatusValue::NotRun,
            verify_status: StatusValue::NotRun,
            drift: None,
            last_run_failures: vec![],
            identity: crate::packet_parser::IdentitySection {
                current: review_stub().identity_current.clone(),
                baseline: None,
            },
            metadata: None,
            gates: None,
        }
    }

    #[test]
    fn classify_binding_issues_from_missing_steps() {
        let mut review = review_stub();
        review.missing_bindings_for_top_candidates = vec![MissingBindingInfo {
            candidate_name: "Scenario A".to_string(),
            missing_step_texts: vec!["Given a user".to_string()],
        }];

        let status = status_stub();
        let issues = classify_binding_issues(&review, &status);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, BindingIssueKind::MissingBinding);
    }

    #[test]
    fn classify_structure_issues_on_lint_fail() {
        let review = review_stub();
        let status = StatusPacket {
            version: 1,
            spec_root: "/repo/specs".to_string(),
            recommended_next_action: "FIX_LINT".to_string(),
            lint_status: StatusValue::Fail,
            run_status: StatusValue::NotRun,
            verify_status: StatusValue::NotRun,
            drift: None,
            last_run_failures: vec![],
            identity: crate::packet_parser::IdentitySection {
                current: review.identity_current.clone(),
                baseline: None,
            },
            metadata: None,
            gates: None,
        };

        let issues = classify_structure_issues(&status, &review);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, StructureIssueKind::ParseError);
    }

    #[test]
    fn classify_sut_issues_from_failures() {
        let mut status = StatusPacket {
            version: 1,
            spec_root: "/repo/specs".to_string(),
            recommended_next_action: "FIX_RUN".to_string(),
            lint_status: StatusValue::Pass,
            run_status: StatusValue::Fail,
            verify_status: StatusValue::NotRun,
            drift: None,
            last_run_failures: vec![],
            identity: crate::packet_parser::IdentitySection {
                current: review_stub().identity_current.clone(),
                baseline: None,
            },
            metadata: None,
            gates: None,
        };
        status.last_run_failures.push(crate::packet_parser::FailureRecord {
            scenario_key: "feature:Rule(01):Scenario(01)".to_string(),
            scenario_name: "Example".to_string(),
            failure_kind: "assertion".to_string(),
            summary: "expected 1".to_string(),
        });

        let issues = classify_sut_issues(&status);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].scenario_name, "Example");
    }

    #[test]
    fn classify_blockers_external_only() {
        let mut review = review_stub();
        review.deferred_items.push(crate::packet_parser::DeferredScenarioItem {
            scenario_key: "feature:Rule(01):Scenario(02)".to_string(),
            scenario_name: "Blocked".to_string(),
            feature_path: "features/a.feature".to_string(),
            rule_name: "Rule(01)".to_string(),
            blocker: BlockerType::External,
        });

        let blockers = classify_blockers(&review);
        assert_eq!(blockers.len(), 1);
        assert_eq!(blockers[0].kind, BlockerKind::External);
    }
}
