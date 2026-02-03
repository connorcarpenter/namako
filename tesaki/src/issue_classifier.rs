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
            let executable_count = rule.executable_scenarios.len();
            if executable_count < 2 {
                let mut description = format!(
                    "Rule '{}' has {} executable scenario(s); minimum is 2.",
                    rule.rule_name, executable_count
                );
                if executable_count == 0 && !rule.deferred_items.is_empty() {
                    description = format!(
                        "Rule '{}' has 0 executable scenarios but {} deferred. Promote a scenario by removing @Deferred to make it executable.",
                        rule.rule_name,
                        rule.deferred_items.len()
                    );
                }
                issues.push(SpecIssue {
                    kind: SpecIssueKind::MissingCoverage,
                    feature_path: feature.feature_path.clone(),
                    description,
                    rule_name: Some(rule.rule_name.clone()),
                });
            } else {
                let unique_then_sets = unique_then_sets_excluding_safety(rule);
                let unique_condition_sets = unique_condition_sets(rule);
                if unique_then_sets < 2 && unique_condition_sets < 2 {
                    issues.push(SpecIssue {
                        kind: SpecIssueKind::Ambiguous,
                        feature_path: feature.feature_path.clone(),
                        description: format!(
                            "Rule '{}' has {} scenarios but lacks distinct outcomes or input conditions. Coverage may be sufficient or may need a new scenario with a different outcome (beyond 'no panic occurs') or distinct Given/When conditions.",
                            rule.rule_name, executable_count
                        ),
                        rule_name: Some(rule.rule_name.clone()),
                    });
                }
            }
        }
    }

    issues
}

fn unique_then_sets_excluding_safety(rule: &crate::packet_parser::RuleReview) -> usize {
    use std::collections::HashSet;

    let mut unique_sets: HashSet<String> = HashSet::new();
    for scenario in &rule.executable_scenarios {
        let mut thens: Vec<String> = extract_steps_by_kind(&scenario.steps, "then")
            .into_iter()
            .filter(|text| text.as_str() != "no panic occurs")
            .collect();

        thens.sort();
        thens.dedup();
        unique_sets.insert(thens.join(" | "));
    }

    unique_sets.len()
}

fn unique_condition_sets(rule: &crate::packet_parser::RuleReview) -> usize {
    use std::collections::HashSet;

    let mut unique_sets: HashSet<String> = HashSet::new();
    for scenario in &rule.executable_scenarios {
        let mut conditions = Vec::new();
        conditions.extend(extract_steps_by_kind(&scenario.steps, "given"));
        conditions.extend(extract_steps_by_kind(&scenario.steps, "when"));
        conditions.sort();
        conditions.dedup();
        unique_sets.insert(conditions.join(" | "));
    }

    unique_sets.len()
}

fn extract_steps_by_kind(steps: &[crate::packet_parser::StepInfo], kind: &str) -> Vec<String> {
    let mut current_kind: Option<String> = None;
    let mut collected = Vec::new();
    for step in steps {
        let step_kind = step.kind.trim().to_ascii_lowercase();
        let normalized = if step_kind == "and" || step_kind == "but" {
            current_kind.clone().unwrap_or_default()
        } else {
            step_kind.clone()
        };
        if !step_kind.is_empty() && step_kind != "and" && step_kind != "but" {
            current_kind = Some(normalized.clone());
        }
        if normalized == kind {
            collected.push(step.text.trim().to_ascii_lowercase());
        }
    }
    collected
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
        
        let scenario_key = match (&feature_path, line) {
            (Some(path), Some(line)) => Some(format!("{}:{}", path, line)),
            (Some(path), None) => Some(path.clone()),
            _ => None,
        };

        issues.push(BindingIssue {
            kind: BindingIssueKind::MissingBinding,
            scenario_key,
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
    use crate::packet_parser::{CoverageSummary, FeatureReview, IdentityFields, MissingBindingInfo, ReviewPacket, RuleReview, ScenarioReview, SourceSpan, StatusPacket, StatusValue, StepInfo};

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

    #[test]
    fn classify_spec_issues_detects_insufficient_coverage() {
        let mut review = review_stub();
        if let Some(rule) = review.features.first_mut().and_then(|f| f.rules.first_mut()) {
            rule.executable_scenarios = vec![ScenarioReview {
                name: "Scenario A".to_string(),
                source_span: SourceSpan { start_line: 1, end_line: 5 },
                steps: vec![],
            }];
        }

        let issues = classify_spec_issues(&review);
        assert!(issues.iter().any(|i| i.rule_name.as_deref() == Some("Rule(01)")));
    }

    #[test]
    fn classify_spec_issues_detects_duplicate_then_sets() {
        let mut review = review_stub();
        if let Some(rule) = review.features.first_mut().and_then(|f| f.rules.first_mut()) {
            rule.executable_scenarios = vec![
                ScenarioReview {
                    name: "Scenario A".to_string(),
                    source_span: SourceSpan { start_line: 1, end_line: 5 },
                    steps: vec![
                        StepInfo { kind: "Then".to_string(), text: "the operation returns an Err result".to_string() },
                    ],
                },
                ScenarioReview {
                    name: "Scenario B".to_string(),
                    source_span: SourceSpan { start_line: 6, end_line: 10 },
                    steps: vec![
                        StepInfo { kind: "Then".to_string(), text: "the operation returns an Err result".to_string() },
                        StepInfo { kind: "Then".to_string(), text: "no panic occurs".to_string() },
                    ],
                },
            ];
        }

        let issues = classify_spec_issues(&review);
        assert!(issues.iter().any(|i| i.rule_name.as_deref() == Some("Rule(01)")));
    }

    #[test]
    fn classify_spec_issues_allows_distinct_then_sets() {
        let mut review = review_stub();
        if let Some(rule) = review.features.first_mut().and_then(|f| f.rules.first_mut()) {
            rule.executable_scenarios = vec![
                ScenarioReview {
                    name: "Scenario A".to_string(),
                    source_span: SourceSpan { start_line: 1, end_line: 5 },
                    steps: vec![
                        StepInfo { kind: "Then".to_string(), text: "the operation returns an Err result".to_string() },
                    ],
                },
                ScenarioReview {
                    name: "Scenario B".to_string(),
                    source_span: SourceSpan { start_line: 6, end_line: 10 },
                    steps: vec![
                        StepInfo { kind: "Then".to_string(), text: "the client receives the response".to_string() },
                    ],
                },
            ];
        }

        let issues = classify_spec_issues(&review);
        assert!(!issues.iter().any(|i| i.rule_name.as_deref() == Some("Rule(01)")));
    }

    #[test]
    fn classify_spec_issues_allows_distinct_conditions() {
        let mut review = review_stub();
        if let Some(rule) = review.features.first_mut().and_then(|f| f.rules.first_mut()) {
            rule.executable_scenarios = vec![
                ScenarioReview {
                    name: "Scenario A".to_string(),
                    source_span: SourceSpan { start_line: 1, end_line: 5 },
                    steps: vec![
                        StepInfo { kind: "Given".to_string(), text: "the client sends an oversize packet".to_string() },
                        StepInfo { kind: "When".to_string(), text: "the server receives the packet".to_string() },
                        StepInfo { kind: "Then".to_string(), text: "the packet is dropped".to_string() },
                    ],
                },
                ScenarioReview {
                    name: "Scenario B".to_string(),
                    source_span: SourceSpan { start_line: 6, end_line: 10 },
                    steps: vec![
                        StepInfo { kind: "Given".to_string(), text: "the client sends a malformed packet".to_string() },
                        StepInfo { kind: "When".to_string(), text: "the server receives the packet".to_string() },
                        StepInfo { kind: "Then".to_string(), text: "the packet is dropped".to_string() },
                    ],
                },
            ];
        }

        let issues = classify_spec_issues(&review);
        assert!(!issues.iter().any(|i| i.rule_name.as_deref() == Some("Rule(01)")));
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
