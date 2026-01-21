//! Packet parsers for Namako JSON outputs.
//!
//! These structs mirror the JSON schema emitted by namako CLI commands.
//! Parsers are intentionally strict to catch format drift early.

use anyhow::{Context, Result};
use serde::Deserialize;

// =============================================================================
// Status Packet
// =============================================================================

/// Parsed output from `namako status --json`.
#[derive(Debug, Clone, Deserialize)]
pub struct StatusPacket {
    pub version: u32,
    pub spec_root: String,
    pub recommended_next_action: String,
    pub lint_status: StatusValue,
    pub run_status: StatusValue,
    pub verify_status: StatusValue,
    #[serde(default)]
    pub drift: Option<DriftInfo>,
    #[serde(default)]
    pub last_run_failures: Vec<FailureRecord>,
    pub identity: IdentitySection,
    #[serde(default)]
    pub metadata: Option<StatusMetadata>,
    #[serde(default)]
    pub gates: Option<LegacyGateStatus>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StatusValue {
    Pass,
    Fail,
    Stale,
    NotRun,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IdentitySection {
    pub current: IdentityFields,
    #[serde(default)]
    pub baseline: Option<IdentityFields>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IdentityFields {
    pub hash_contract_version: String,
    pub feature_fingerprint_hash: String,
    pub step_registry_hash: String,
    pub resolved_plan_hash: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StatusMetadata {
    pub timestamp: String,
    pub namako_version: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FailureRecord {
    pub scenario_key: String,
    pub scenario_name: String,
    pub failure_kind: String,
    pub summary: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DriftInfo {
    pub kind: String,
    pub details: Vec<DriftDetail>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DriftDetail {
    pub field: String,
    pub baseline: String,
    pub current: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LegacyGateStatus {
    pub lint: LegacyGateResult,
    pub run: LegacyGateResult,
    pub verify: LegacyGateResult,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LegacyGateResult {
    pub ok: bool,
    pub code: i32,
    #[serde(default)]
    pub summary: Option<String>,
}

// =============================================================================
// Review Packet
// =============================================================================

/// Parsed output from `namako review --json`.
#[derive(Debug, Clone, Deserialize)]
pub struct ReviewPacket {
    pub version: u32,
    pub spec_root: String,
    pub identity_current: IdentityFields,
    pub features: Vec<FeatureReview>,
    pub coverage_summary: CoverageSummary,
    #[serde(default)]
    pub deferred_items: Vec<DeferredScenarioItem>,
    #[serde(default)]
    pub promotion_candidates: Vec<PromotionCandidate>,
    #[serde(default)]
    pub missing_bindings_for_top_candidates: Vec<MissingBindingInfo>,
    #[serde(default)]
    pub harness_gaps: Vec<HarnessGap>,
    #[serde(default)]
    pub suggested_binding_bundle: Option<SuggestedBindingBundle>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeatureReview {
    pub feature_path: String,
    pub feature_name: String,
    pub rules: Vec<RuleReview>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuleReview {
    pub rule_name: String,
    pub source_span: SourceSpan,
    pub executable_scenarios: Vec<ScenarioReview>,
    #[serde(default)]
    pub deferred_items: Vec<DeferredItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SourceSpan {
    pub start_line: u32,
    pub end_line: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScenarioReview {
    pub name: String,
    pub source_span: SourceSpan,
    pub steps: Vec<StepInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StepInfo {
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CoverageSummary {
    pub rules_total: u32,
    pub rules_with_zero_executable: u32,
    pub executable_scenarios_total: u32,
    pub deferred_items_total: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeferredScenarioItem {
    pub scenario_key: String,
    pub scenario_name: String,
    pub feature_path: String,
    pub rule_name: String,
    pub blocker: BlockerType,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeferredItem {
    pub text: String,
    pub source_span: SourceSpan,
    pub blocker: BlockerType,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PromotionCandidate {
    pub feature_path: String,
    pub rule_name: String,
    pub scenario_name: String,
    pub steps: Vec<StepInfo>,
    pub new_step_texts_estimate: u32,
    pub reuse_score: u32,
    pub blocker: BlockerType,
    #[serde(default)]
    pub is_stub: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MissingBindingInfo {
    pub candidate_name: String,
    pub missing_step_texts: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HarnessGap {
    pub capability: String,
    pub blocked_count: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SuggestedBindingBundle {
    pub steps: Vec<BundleStepInfo>,
    pub rationale: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BundleStepInfo {
    pub kind: String,
    pub text: String,
    pub frequency: u32,
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BlockerType {
    HarnessOnly,
    Core,
    External,
    #[default]
    Unknown,
}

// =============================================================================
// Gate Packet
// =============================================================================

/// Parsed output from `namako gate --json`.
#[derive(Debug, Clone, Deserialize)]
pub struct GatePacket {
    pub lint: GatePhase,
    pub run: GatePhase,
    pub verify: GatePhase,
    #[serde(default)]
    pub determinism: Option<GateDeterminism>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GatePhase {
    pub status: GatePhaseStatus,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GatePhaseStatus {
    Pass,
    Fail,
    Skipped,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GateDeterminism {
    pub status: GatePhaseStatus,
    #[serde(default)]
    pub reason: Option<String>,
}

// =============================================================================
// Explain Packet
// =============================================================================

/// Parsed output from `namako explain --json`.
#[derive(Debug, Clone, Deserialize)]
pub struct ExplainPacket {
    pub version: u32,
    pub scenario_key: String,
    pub scenario_name: String,
    pub feature_path: String,
    #[serde(default)]
    pub rule_name: Option<String>,
    #[serde(default)]
    pub rule_description: Option<String>,
    pub steps: Vec<ExplainStep>,
    #[serde(default)]
    pub related_tags: Vec<String>,
    pub contract_excerpt: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExplainStep {
    pub step_kind: String,
    pub step_text: String,
    pub binding_id: String,
    pub binding_expression: String,
    pub impl_hash: String,
    pub source_location: String,
}

// =============================================================================
// Parser Functions
// =============================================================================

pub fn parse_status_json(content: &str) -> Result<StatusPacket> {
    serde_json::from_str::<StatusPacket>(content)
        .context("Failed to parse status.json")
}

pub fn parse_review_json(content: &str) -> Result<ReviewPacket> {
    serde_json::from_str::<ReviewPacket>(content)
        .context("Failed to parse review.json")
}

pub fn parse_gate_json(content: &str) -> Result<GatePacket> {
    serde_json::from_str::<GatePacket>(content)
        .context("Failed to parse gate.json")
}

pub fn parse_explain_json(content: &str) -> Result<ExplainPacket> {
    serde_json::from_str::<ExplainPacket>(content)
        .context("Failed to parse explain.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_status_json_minimal() {
        let json = r#"{
            "version": 1,
            "spec_root": "/repo/specs",
            "recommended_next_action": "DONE",
            "lint_status": "pass",
            "run_status": "pass",
            "verify_status": "pass",
            "drift": { "kind": "NONE", "details": [] },
            "last_run_failures": [],
            "identity": {
                "current": {
                    "hash_contract_version": "namako-v1-json+blake3-256",
                    "feature_fingerprint_hash": "a",
                    "step_registry_hash": "b",
                    "resolved_plan_hash": "c"
                },
                "baseline": null
            },
            "metadata": { "timestamp": "2026-01-21T00:00:00Z", "namako_version": "1.0" }
        }"#;

        let parsed = parse_status_json(json).unwrap();
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.recommended_next_action, "DONE");
        assert_eq!(parsed.lint_status, StatusValue::Pass);
        assert!(parsed.last_run_failures.is_empty());
    }

    #[test]
    fn parse_status_json_with_failures() {
        let json = r#"{
            "version": 1,
            "spec_root": "/repo/specs",
            "recommended_next_action": "FIX_RUN",
            "lint_status": "pass",
            "run_status": "fail",
            "verify_status": "not_run",
            "drift": { "kind": "NONE", "details": [] },
            "last_run_failures": [
                {
                    "scenario_key": "feature:Rule(01):Scenario(02)",
                    "scenario_name": "fails",
                    "failure_kind": "assertion",
                    "summary": "expected 1"
                }
            ],
            "identity": {
                "current": {
                    "hash_contract_version": "namako-v1-json+blake3-256",
                    "feature_fingerprint_hash": "a",
                    "step_registry_hash": "b",
                    "resolved_plan_hash": "c"
                },
                "baseline": {
                    "hash_contract_version": "namako-v1-json+blake3-256",
                    "feature_fingerprint_hash": "x",
                    "step_registry_hash": "y",
                    "resolved_plan_hash": "z"
                }
            }
        }"#;

        let parsed = parse_status_json(json).unwrap();
        assert_eq!(parsed.last_run_failures.len(), 1);
        assert_eq!(parsed.run_status, StatusValue::Fail);
        assert!(parsed.identity.baseline.is_some());
    }

    #[test]
    fn parse_status_json_missing_optional_sections() {
        let json = r#"{
            "version": 1,
            "spec_root": "/repo/specs",
            "recommended_next_action": "RUN_LINT",
            "lint_status": "stale",
            "run_status": "not_run",
            "verify_status": "not_run",
            "identity": {
                "current": {
                    "hash_contract_version": "namako-v1-json+blake3-256",
                    "feature_fingerprint_hash": "a",
                    "step_registry_hash": "b",
                    "resolved_plan_hash": "c"
                }
            },
            "last_run_failures": []
        }"#;

        let parsed = parse_status_json(json).unwrap();
        assert_eq!(parsed.lint_status, StatusValue::Stale);
        assert!(parsed.drift.is_none());
    }

    #[test]
    fn parse_review_json_minimal() {
        let json = r#"{
            "version": 1,
            "spec_root": "/repo/specs",
            "identity_current": {
                "hash_contract_version": "namako-v1-json+blake3-256",
                "feature_fingerprint_hash": "a",
                "step_registry_hash": "b",
                "resolved_plan_hash": "c"
            },
            "features": [],
            "coverage_summary": {
                "rules_total": 0,
                "rules_with_zero_executable": 0,
                "executable_scenarios_total": 0,
                "deferred_items_total": 0
            },
            "deferred_items": [],
            "promotion_candidates": [],
            "missing_bindings_for_top_candidates": [],
            "harness_gaps": [],
            "suggested_binding_bundle": null
        }"#;

        let parsed = parse_review_json(json).unwrap();
        assert_eq!(parsed.coverage_summary.rules_total, 0);
        assert!(parsed.features.is_empty());
    }

    #[test]
    fn parse_review_json_with_candidates() {
        let json = r#"{
            "version": 1,
            "spec_root": "/repo/specs",
            "identity_current": {
                "hash_contract_version": "namako-v1-json+blake3-256",
                "feature_fingerprint_hash": "a",
                "step_registry_hash": "b",
                "resolved_plan_hash": "c"
            },
            "features": [],
            "coverage_summary": {
                "rules_total": 1,
                "rules_with_zero_executable": 0,
                "executable_scenarios_total": 1,
                "deferred_items_total": 0
            },
            "deferred_items": [],
            "promotion_candidates": [
                {
                    "feature_path": "features/a.feature",
                    "rule_name": "Rule(01)",
                    "scenario_name": "Scenario A",
                    "steps": [],
                    "new_step_texts_estimate": 2,
                    "reuse_score": 1,
                    "blocker": "UNKNOWN",
                    "is_stub": false
                }
            ],
            "missing_bindings_for_top_candidates": [
                { "candidate_name": "Scenario A", "missing_step_texts": ["Given a"] }
            ],
            "harness_gaps": [],
            "suggested_binding_bundle": null
        }"#;

        let parsed = parse_review_json(json).unwrap();
        assert_eq!(parsed.promotion_candidates.len(), 1);
        assert_eq!(parsed.missing_bindings_for_top_candidates.len(), 1);
    }

    #[test]
    fn parse_review_json_with_features() {
        let json = r#"{
            "version": 1,
            "spec_root": "/repo/specs",
            "identity_current": {
                "hash_contract_version": "namako-v1-json+blake3-256",
                "feature_fingerprint_hash": "a",
                "step_registry_hash": "b",
                "resolved_plan_hash": "c"
            },
            "features": [
                {
                    "feature_path": "features/a.feature",
                    "feature_name": "A",
                    "rules": [
                        {
                            "rule_name": "Rule(01)",
                            "source_span": { "start_line": 1, "end_line": 10 },
                            "executable_scenarios": [],
                            "deferred_items": []
                        }
                    ]
                }
            ],
            "coverage_summary": {
                "rules_total": 1,
                "rules_with_zero_executable": 1,
                "executable_scenarios_total": 0,
                "deferred_items_total": 0
            },
            "deferred_items": [],
            "promotion_candidates": [],
            "missing_bindings_for_top_candidates": [],
            "harness_gaps": [],
            "suggested_binding_bundle": null
        }"#;

        let parsed = parse_review_json(json).unwrap();
        assert_eq!(parsed.features.len(), 1);
        assert_eq!(parsed.features[0].rules.len(), 1);
    }

    #[test]
    fn parse_gate_json_pass() {
        let json = r#"{
            "lint": { "status": "pass" },
            "run": { "status": "pass" },
            "verify": { "status": "pass" }
        }"#;

        let parsed = parse_gate_json(json).unwrap();
        assert_eq!(parsed.lint.status, GatePhaseStatus::Pass);
        assert_eq!(parsed.verify.status, GatePhaseStatus::Pass);
    }

    #[test]
    fn parse_gate_json_with_reason() {
        let json = r#"{
            "lint": { "status": "fail", "reason": "lint error" },
            "run": { "status": "skipped" },
            "verify": { "status": "skipped" }
        }"#;

        let parsed = parse_gate_json(json).unwrap();
        assert_eq!(parsed.lint.reason.as_deref(), Some("lint error"));
        assert_eq!(parsed.run.status, GatePhaseStatus::Skipped);
    }

    #[test]
    fn parse_gate_json_with_determinism() {
        let json = r#"{
            "lint": { "status": "pass" },
            "run": { "status": "pass" },
            "verify": { "status": "pass" },
            "determinism": { "status": "pass" }
        }"#;

        let parsed = parse_gate_json(json).unwrap();
        assert!(parsed.determinism.is_some());
    }

    #[test]
    fn parse_explain_json_minimal() {
        let json = r#"{
            "version": 1,
            "scenario_key": "feature:Rule(01):Scenario(01)",
            "scenario_name": "Example",
            "feature_path": "features/a.feature",
            "rule_name": null,
            "rule_description": null,
            "steps": [],
            "related_tags": [],
            "contract_excerpt": "Rule: Example"
        }"#;

        let parsed = parse_explain_json(json).unwrap();
        assert_eq!(parsed.scenario_name, "Example");
        assert!(parsed.steps.is_empty());
    }

    #[test]
    fn parse_explain_json_with_steps() {
        let json = r#"{
            "version": 1,
            "scenario_key": "feature:Rule(01):Scenario(01)",
            "scenario_name": "Example",
            "feature_path": "features/a.feature",
            "rule_name": "Rule(01)",
            "rule_description": "desc",
            "steps": [
                {
                    "step_kind": "Given",
                    "step_text": "a thing",
                    "binding_id": "abc",
                    "binding_expression": "a thing",
                    "impl_hash": "hash",
                    "source_location": "tests.rs:10"
                }
            ],
            "related_tags": ["@Scenario(01)"],
            "contract_excerpt": "Rule: Example"
        }"#;

        let parsed = parse_explain_json(json).unwrap();
        assert_eq!(parsed.steps.len(), 1);
        assert_eq!(parsed.steps[0].step_kind, "Given");
    }

    #[test]
    fn parse_explain_json_with_tags() {
        let json = r#"{
            "version": 1,
            "scenario_key": "feature:Rule(01):Scenario(01)",
            "scenario_name": "Example",
            "feature_path": "features/a.feature",
            "rule_name": null,
            "rule_description": null,
            "steps": [],
            "related_tags": ["@Feature(foo)", "@Scenario(01)"],
            "contract_excerpt": "Rule: Example"
        }"#;

        let parsed = parse_explain_json(json).unwrap();
        assert_eq!(parsed.related_tags.len(), 2);
    }
}
