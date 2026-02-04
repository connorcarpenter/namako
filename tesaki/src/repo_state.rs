//! RepoState: Computed repository state from Namako packets.
//!
//! Per GOLD_PLAN.md §10.8.3 and TODO.md Phase 1, this module defines the core
//! types that represent Tesaki's view of the repository state. The RepoState
//! is computed fresh from Namako packets on every cycle.
//!
//! Tesaki never relies on "chat memory" to decide what's next — it recomputes.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::issue_classifier::{
    classify_binding_issues, classify_blockers, classify_spec_issues, classify_structure_issues,
    classify_sut_issues,
};
use crate::packet_parser::{
    ExplainPacket, GatePacket, GatePhaseStatus, ReviewPacket, StatusPacket,
    StatusValue as PacketStatusValue,
};

// =============================================================================
// Gate Status
// =============================================================================

/// Status of a single gate phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    /// Phase passed successfully.
    Pass,
    /// Phase failed.
    Fail,
    /// Phase was not run (e.g., stale artifacts).
    Stale,
    /// Phase has never been run.
    NotRun,
}

impl Default for GateStatus {
    fn default() -> Self {
        Self::NotRun
    }
}

// =============================================================================
// Drift Detection
// =============================================================================

/// Kind of identity drift detected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftKind {
    /// Feature fingerprint hash changed.
    FeatureFingerprint,
    /// Step registry hash changed.
    StepRegistry,
    /// Resolved plan hash changed.
    ResolvedPlan,
    /// Multiple hashes changed.
    Multiple,
}

/// Detail about a specific drift.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftDetail {
    /// Which hash field drifted.
    pub field: String,
    /// Expected (baseline) value.
    pub expected: String,
    /// Actual (current) value.
    pub actual: String,
}

/// Information about identity drift from baseline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftInfo {
    /// Kind of drift detected.
    pub kind: DriftKind,
    /// Detailed information about each drift.
    pub details: Vec<DriftDetail>,
}

// =============================================================================
// Failure Information
// =============================================================================

/// Information about a test failure from the last run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureInfo {
    /// Scenario key (e.g., "feature:Rule(01):Scenario(03)").
    pub scenario_key: String,
    /// Human-readable scenario name.
    pub scenario_name: String,
    /// Kind of failure (e.g., "assertion", "panic", "timeout").
    pub failure_kind: String,
    /// Optional error message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

// =============================================================================
// Issue Categories (per GOLD_PLAN §10.8.3)
// =============================================================================

/// Kind of spec issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecIssueKind {
    /// Requirement is ambiguous or underspecified.
    Ambiguous,
    /// Missing test coverage for a feature or rule.
    MissingCoverage,
    /// Feature intent is unclear.
    UnclearIntent,
}

/// Issue with the spec (feature files).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecIssue {
    /// Kind of spec issue.
    pub kind: SpecIssueKind,
    /// Path to the feature file.
    pub feature_path: String,
    /// Human-readable description.
    pub description: String,
    /// Optional rule name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_name: Option<String>,
}

/// Kind of structure issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructureIssueKind {
    /// Missing explicit identity tag (@Feature, @Rule, @Scenario).
    MissingIdentityTag,
    /// Parse error in feature file.
    ParseError,
    /// Invalid reference (e.g., broken scenario key).
    InvalidReference,
}

/// Issue with spec structure (identity tags, parse errors).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructureIssue {
    /// Kind of structure issue.
    pub kind: StructureIssueKind,
    /// Location of the issue (file:line or scenario key).
    pub location: String,
    /// Human-readable description.
    pub description: String,
}

/// Kind of binding issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingIssueKind {
    /// Step has no matching binding.
    MissingBinding,
    /// Then step has weak/missing assertions.
    WeakAssertion,
    /// Binding exists but is not used by any scenario.
    Orphan,
}

/// Issue with test bindings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingIssue {
    /// Kind of binding issue.
    pub kind: BindingIssueKind,
    /// Associated scenario key (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scenario_key: Option<String>,
    /// Step text (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_text: Option<String>,
    /// Human-readable description.
    pub description: String,
}

/// Issue with the system under test (failing tests).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SutIssue {
    /// Scenario key of the failing test.
    pub scenario_key: String,
    /// Human-readable scenario name.
    pub scenario_name: String,
    /// Kind of failure.
    pub failure_kind: String,
    /// Optional error message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

impl From<FailureInfo> for SutIssue {
    fn from(f: FailureInfo) -> Self {
        SutIssue {
            scenario_key: f.scenario_key,
            scenario_name: f.scenario_name,
            failure_kind: f.failure_kind,
            error_message: f.error_message,
        }
    }
}

impl From<SutIssue> for FailureInfo {
    fn from(f: SutIssue) -> Self {
        FailureInfo {
            scenario_key: f.scenario_key,
            scenario_name: f.scenario_name,
            failure_kind: f.failure_kind,
            error_message: f.error_message,
        }
    }
}

/// Kind of global blocker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockerKind {
    /// Build system broken.
    Build,
    /// Environment/tooling issue.
    Environment,
    /// External dependency problem.
    External,
}

/// Global blocker preventing progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blocker {
    /// Kind of blocker.
    pub kind: BlockerKind,
    /// Human-readable description.
    pub description: String,
}

// =============================================================================
// Candidate Tasks
// =============================================================================

/// Priority level for candidate tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    /// Priority 1: SUT issues (failing tests).
    Critical = 1,
    /// Priority 2: Binding issues (missing steps).
    High = 2,
    /// Priority 3: Structure issues (identity tags).
    Medium = 3,
    /// Priority 4: Spec issues (coverage gaps).
    Low = 4,
}

/// Category of candidate task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskCategory {
    /// Fix failing tests (SUT issues).
    FixSut,
    /// Create missing bindings.
    CreateBindings,
    /// Fix structure issues.
    FixStructure,
    /// Improve spec coverage.
    ImproveSpec,
}

/// A candidate task derived from issues.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateTask {
    /// Task category.
    pub category: TaskCategory,
    /// Priority level.
    pub priority: TaskPriority,
    /// Human-readable task name.
    pub name: String,
    /// Detailed description.
    pub description: String,
    /// Target scenario key (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scenario_key: Option<String>,
    /// Target feature path (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature_path: Option<String>,
}

// =============================================================================
// Identity
// =============================================================================

/// Identity hashes from namako status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    /// Hash contract version (e.g., "namako-v1-json+blake3-256").
    pub hash_contract_version: String,
    /// Feature fingerprint hash.
    pub feature_fingerprint_hash: String,
    /// Step registry hash.
    pub step_registry_hash: String,
    /// Resolved plan hash.
    pub resolved_plan_hash: String,
}

// =============================================================================
// RepoState
// =============================================================================

/// Computed repository state from Namako packets.
///
/// This is the central model for Tesaki v1.8. It is computed fresh from
/// Namako packets on every cycle. Tesaki uses this to select the next
/// mission and determine what work is needed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoState {
    // -------------------------------------------------------------------------
    // Gate status
    // -------------------------------------------------------------------------
    /// Status of the lint phase.
    pub lint_status: GateStatus,
    /// Status of the run phase.
    pub run_status: GateStatus,
    /// Status of the verify phase.
    pub verify_status: GateStatus,

    // -------------------------------------------------------------------------
    // Drift detection
    // -------------------------------------------------------------------------
    /// Identity drift from baseline (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drift: Option<DriftInfo>,

    // -------------------------------------------------------------------------
    // Failures from last run
    // -------------------------------------------------------------------------
    /// Test failures from the most recent run.
    pub last_run_failures: Vec<FailureInfo>,

    // -------------------------------------------------------------------------
    // Issue categories (per GOLD_PLAN §10.8.3)
    // -------------------------------------------------------------------------
    /// Issues with specs (ambiguity, missing coverage).
    pub spec_issues: Vec<SpecIssue>,
    /// Issues with spec structure (identity tags, parse errors).
    pub structure_issues: Vec<StructureIssue>,
    /// Issues with bindings (missing, weak assertions).
    pub binding_issues: Vec<BindingIssue>,
    /// Issues with SUT (failing tests).
    pub sut_issues: Vec<SutIssue>,
    /// Global blockers preventing progress.
    pub global_blockers: Vec<Blocker>,

    // -------------------------------------------------------------------------
    // Feature stats (from feature files)
    // -------------------------------------------------------------------------
    /// Total executable scenario counts per feature (excluding @Deferred).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub scenarios_per_feature: HashMap<String, usize>,
    /// Scenario counts per rule (keyed by "feature_path::rule_name").
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub scenarios_per_rule: HashMap<String, usize>,
    /// Rule counts per feature (from feature files).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub rules_per_feature: HashMap<String, usize>,
    /// Coverage ambiguity indicators (rules with 1 or >4 executable scenarios).
    #[serde(default, skip_serializing_if = "CoverageAmbiguity::is_empty")]
    pub coverage_ambiguity: CoverageAmbiguity,
    /// Latest LLM coverage assessment, if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage_assessment: Option<CoverageAssessment>,

    /// Promotion candidates from review (summarized for mission guidance).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub promotion_candidates: Vec<PromotionCandidateSummary>,

    // -------------------------------------------------------------------------
    // Candidate tasks (derived)
    // -------------------------------------------------------------------------
    /// Candidate tasks derived from issues, sorted by priority.
    pub candidate_tasks: Vec<CandidateTask>,

    // -------------------------------------------------------------------------
    // Identity
    // -------------------------------------------------------------------------
    /// Current identity hashes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_identity: Option<Identity>,
    /// Baseline (certified) identity hashes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_identity: Option<Identity>,
}

impl RepoState {
    /// Create a new empty RepoState.
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Compute RepoState from Namako packets.
    pub fn compute(
        status: &StatusPacket,
        review: &ReviewPacket,
        gate: &GatePacket,
        _explain: Option<&ExplainPacket>,
    ) -> anyhow::Result<Self> {
        let lint_status = gate_status_from_packet(&status.lint_status, gate.lint.status == GatePhaseStatus::Pass);
        let run_status = gate_status_from_packet(&status.run_status, gate.run.status == GatePhaseStatus::Pass);
        let verify_status = gate_status_from_packet(&status.verify_status, gate.verify.status == GatePhaseStatus::Pass);

        let drift = status.drift.as_ref().and_then(map_drift_info);
        let last_run_failures = status
            .last_run_failures
            .iter()
            .map(|f| FailureInfo {
                scenario_key: f.scenario_key.clone(),
                scenario_name: f.scenario_name.clone(),
                failure_kind: f.failure_kind.clone(),
                error_message: Some(f.summary.clone()),
            })
            .collect::<Vec<_>>();

        let mut spec_issues = classify_spec_issues(review);
        let structure_issues = classify_structure_issues(status, review);
        let binding_issues = classify_binding_issues(review, status);
        let sut_issues = classify_sut_issues(status);
        let global_blockers = classify_blockers(review);

        let candidate_tasks = derive_candidate_tasks(
            &spec_issues,
            &structure_issues,
            &binding_issues,
            &sut_issues,
        );

        let scenario_counts = collect_scenario_counts(review);
        let promotion_candidates = review
            .promotion_candidates
            .iter()
            .map(|candidate| PromotionCandidateSummary {
                feature_path: candidate.feature_path.clone(),
                rule_name: candidate.rule_name.clone(),
                scenario_name: candidate.scenario_name.clone(),
                new_step_texts_estimate: candidate.new_step_texts_estimate,
                reuse_score: candidate.reuse_score,
                is_stub: candidate.is_stub,
            })
            .collect::<Vec<_>>();
        let coverage_ambiguity = compute_coverage_ambiguity(review);
        let coverage_assessment = load_coverage_assessment(&review.spec_root);
        if let Some(assessment) = &coverage_assessment {
            let verdict = assessment.verdict.trim().to_ascii_uppercase();
            if verdict == "ADEQUATE" {
                spec_issues.retain(|issue| issue.kind != SpecIssueKind::Ambiguous);
            } else if verdict == "INADEQUATE" {
                for issue in spec_issues.iter_mut() {
                    if issue.kind == SpecIssueKind::Ambiguous {
                        issue.kind = SpecIssueKind::MissingCoverage;
                        issue.description = format!(
                            "Coverage judge: INADEQUATE. {}",
                            issue.description
                        );
                    }
                }
            }
        }

        let current_identity = Some(Identity {
            hash_contract_version: status.identity.current.hash_contract_version.clone(),
            feature_fingerprint_hash: status.identity.current.feature_fingerprint_hash.clone(),
            step_registry_hash: status.identity.current.step_registry_hash.clone(),
            resolved_plan_hash: status.identity.current.resolved_plan_hash.clone(),
        });
        let baseline_identity = status.identity.baseline.as_ref().map(|id| Identity {
            hash_contract_version: id.hash_contract_version.clone(),
            feature_fingerprint_hash: id.feature_fingerprint_hash.clone(),
            step_registry_hash: id.step_registry_hash.clone(),
            resolved_plan_hash: id.resolved_plan_hash.clone(),
        });

        Ok(Self {
            lint_status,
            run_status,
            verify_status,
            drift,
            last_run_failures,
            spec_issues,
            structure_issues,
            binding_issues,
            sut_issues,
            global_blockers,
            scenarios_per_feature: scenario_counts.scenarios_per_feature,
            scenarios_per_rule: scenario_counts.scenarios_per_rule,
            rules_per_feature: scenario_counts.rules_per_feature,
            coverage_ambiguity,
            coverage_assessment,
            promotion_candidates,
            candidate_tasks,
            current_identity,
            baseline_identity,
        })
    }

    /// Generate a human-readable summary string.
    ///
    /// Format: "Spec: N issues • Structure: N • Bindings: N missing • SUT: N failing"
    pub fn summary(&self) -> String {
        let spec_count = self.spec_issues.len();
        let structure_count = self.structure_issues.len();
        let binding_count = self.binding_issues.len();
        let sut_count = self.sut_issues.len();

        format!(
            "Spec: {} issue{} • Structure: {} • Bindings: {} missing • SUT: {} failing",
            spec_count,
            if spec_count == 1 { "" } else { "s" },
            structure_count,
            binding_count,
            sut_count
        )
    }

    /// Check if all gates pass.
    pub fn all_gates_pass(&self) -> bool {
        self.lint_status == GateStatus::Pass
            && self.run_status == GateStatus::Pass
            && self.verify_status == GateStatus::Pass
    }

    /// Check if there are any issues.
    pub fn has_issues(&self) -> bool {
        !self.spec_issues.is_empty()
            || !self.structure_issues.is_empty()
            || !self.binding_issues.is_empty()
            || !self.sut_issues.is_empty()
            || !self.global_blockers.is_empty()
    }

    /// Check if there is any work to do.
    pub fn has_work(&self) -> bool {
        self.has_issues() || !self.all_gates_pass()
    }

    /// Get the highest priority candidate task, if any.
    pub fn top_candidate(&self) -> Option<&CandidateTask> {
        self.candidate_tasks.first()
    }

    /// Count total issues across all categories.
    pub fn total_issue_count(&self) -> usize {
        self.spec_issues.len()
            + self.structure_issues.len()
            + self.binding_issues.len()
            + self.sut_issues.len()
    }

    pub fn propagation_summary(&self) -> PropagationSummary {
        PropagationSummary {
            spec: !self.spec_issues.is_empty(),
            structure: !self.structure_issues.is_empty(),
            tests: !self.binding_issues.is_empty(),
            sut: !self.sut_issues.is_empty(),
            finalize: self.total_issue_count() == 0,
        }
    }

    /// Lookup total scenario count for a feature, if available.
    pub fn scenario_count_for_feature(&self, feature_path: &str) -> Option<usize> {
        self.scenarios_per_feature.get(feature_path).copied()
    }

    /// Lookup scenario count for a specific rule in a feature, if available.
    pub fn scenario_count_for_rule(&self, feature_path: &str, rule_name: &str) -> Option<usize> {
        self.scenarios_per_rule.get(&rule_key(feature_path, rule_name)).copied()
    }

    /// Lookup rule count for a feature, if available.
    pub fn rule_count_for_feature(&self, feature_path: &str) -> Option<usize> {
        self.rules_per_feature.get(feature_path).copied()
    }

    pub fn coverage_is_ambiguous(&self) -> bool {
        !self.coverage_ambiguity.is_empty()
    }
}

#[derive(Debug, Default)]
struct ScenarioCountSummary {
    scenarios_per_feature: HashMap<String, usize>,
    scenarios_per_rule: HashMap<String, usize>,
    rules_per_feature: HashMap<String, usize>,
}

#[derive(Debug, Default)]
struct FeatureScenarioCounts {
    rule_count: usize,
    scenario_total: usize,
    scenarios_per_rule: HashMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleCoverageInfo {
    pub feature_path: String,
    pub rule_name: String,
    pub executable_scenarios: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CoverageAmbiguity {
    pub rules_with_one_scenario: Vec<RuleCoverageInfo>,
    pub rules_with_many_scenarios: Vec<RuleCoverageInfo>,
}

impl CoverageAmbiguity {
    fn is_empty(&self) -> bool {
        self.rules_with_one_scenario.is_empty() && self.rules_with_many_scenarios.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageAssessment {
    pub verdict: String,
    pub score: f32,
    #[serde(default)]
    pub gaps: Vec<String>,
    #[serde(default)]
    pub judges: Vec<CoverageJudgeScore>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionCandidateSummary {
    pub feature_path: String,
    pub rule_name: String,
    pub scenario_name: String,
    pub new_step_texts_estimate: u32,
    pub reuse_score: u32,
    pub is_stub: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageJudgeScore {
    pub judge_id: String,
    pub score: f32,
    pub verdict: String,
}

fn collect_scenario_counts(review: &ReviewPacket) -> ScenarioCountSummary {
    let mut summary = ScenarioCountSummary::default();

    for feature in &review.features {
        let feature_path = resolve_feature_path(&review.spec_root, &feature.feature_path);
        let Some(counts) = parse_feature_scenario_counts(&feature_path) else {
            continue;
        };

        summary
            .scenarios_per_feature
            .insert(feature.feature_path.clone(), counts.scenario_total);
        summary
            .rules_per_feature
            .insert(feature.feature_path.clone(), counts.rule_count);

        for (rule_name, count) in counts.scenarios_per_rule {
            summary
                .scenarios_per_rule
                .insert(rule_key(&feature.feature_path, &rule_name), count);
        }
    }

    summary
}

fn resolve_feature_path(spec_root: &str, feature_path: &str) -> PathBuf {
    let path = PathBuf::from(feature_path);
    if path.is_absolute() {
        return path;
    }
    Path::new(spec_root).join(feature_path)
}

fn parse_feature_scenario_counts(path: &Path) -> Option<FeatureScenarioCounts> {
    if !path.is_file() {
        return None;
    }
    let content = fs::read_to_string(path).ok()?;
    let mut counts = FeatureScenarioCounts::default();
    let mut current_rule: Option<String> = None;
    let mut pending_deferred = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with('@') {
            if trimmed
                .split_whitespace()
                .any(|tag| tag.eq_ignore_ascii_case("@Deferred"))
            {
                pending_deferred = true;
            }
            continue;
        }

        if trimmed.starts_with("Feature:") || trimmed.starts_with("Background:") {
            pending_deferred = false;
            continue;
        }

        if let Some(rule_name) = trimmed.strip_prefix("Rule:") {
            let rule_name = rule_name.trim();
            counts.rule_count += 1;
            current_rule = Some(rule_name.to_string());
            counts
                .scenarios_per_rule
                .entry(rule_name.to_string())
                .or_insert(0);
            pending_deferred = false;
            continue;
        }

        if trimmed.starts_with("Scenario:") || trimmed.starts_with("Scenario Outline:") {
            if !pending_deferred {
                counts.scenario_total += 1;
                if let Some(rule_name) = current_rule.as_ref() {
                    let entry = counts
                        .scenarios_per_rule
                        .entry(rule_name.clone())
                        .or_insert(0);
                    *entry += 1;
                }
            }
            pending_deferred = false;
        }
    }

    Some(counts)
}

fn compute_coverage_ambiguity(review: &ReviewPacket) -> CoverageAmbiguity {
    let mut ambiguity = CoverageAmbiguity::default();

    for feature in &review.features {
        for rule in &feature.rules {
            let count = rule.executable_scenarios.len();
            if count == 1 {
                ambiguity.rules_with_one_scenario.push(RuleCoverageInfo {
                    feature_path: feature.feature_path.clone(),
                    rule_name: rule.rule_name.clone(),
                    executable_scenarios: count,
                });
            } else if count > 4 {
                ambiguity.rules_with_many_scenarios.push(RuleCoverageInfo {
                    feature_path: feature.feature_path.clone(),
                    rule_name: rule.rule_name.clone(),
                    executable_scenarios: count,
                });
            }
        }
    }

    ambiguity
}

fn load_coverage_assessment(spec_root: &str) -> Option<CoverageAssessment> {
    let path = Path::new(spec_root).join(".tesaki/coverage_assessment.json");
    if !path.is_file() {
        return None;
    }
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn rule_key(feature_path: &str, rule_name: &str) -> String {
    format!("{}::{}", feature_path, rule_name)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PropagationSummary {
    pub spec: bool,
    pub structure: bool,
    pub tests: bool,
    pub sut: bool,
    pub finalize: bool,
}

impl PropagationSummary {
    pub fn to_line(&self) -> String {
        let mut items = Vec::new();
        if self.spec {
            items.push("Spec");
        }
        if self.structure {
            items.push("Structure");
        }
        if self.tests {
            items.push("Tests");
        }
        if self.sut {
            items.push("SUT");
        }
        if self.finalize {
            items.push("Finalize");
        }
        if items.is_empty() {
            "None".to_string()
        } else {
            items.join(" -> ")
        }
    }
}

fn gate_status_from_packet(status: &PacketStatusValue, gate_pass: bool) -> GateStatus {
    match status {
        PacketStatusValue::Pass => GateStatus::Pass,
        PacketStatusValue::Fail => GateStatus::Fail,
        PacketStatusValue::Stale => GateStatus::Stale,
        PacketStatusValue::NotRun => {
            if gate_pass {
                GateStatus::Pass
            } else {
                GateStatus::NotRun
            }
        }
    }
}

fn map_drift_info(drift: &crate::packet_parser::DriftInfo) -> Option<DriftInfo> {
    if drift.kind == "NONE" || drift.kind == "NO_BASELINE" {
        return None;
    }

    let kind = match drift.kind.as_str() {
        "FEATURE" => DriftKind::FeatureFingerprint,
        "STEP_REGISTRY" => DriftKind::StepRegistry,
        "RESOLVED_PLAN" => DriftKind::ResolvedPlan,
        "MULTIPLE" => DriftKind::Multiple,
        _ => DriftKind::Multiple,
    };

    let details = drift
        .details
        .iter()
        .map(|d| DriftDetail {
            field: d.field.clone(),
            expected: d.baseline.clone(),
            actual: d.current.clone(),
        })
        .collect();

    Some(DriftInfo { kind, details })
}

fn derive_candidate_tasks(
    spec_issues: &[SpecIssue],
    structure_issues: &[StructureIssue],
    binding_issues: &[BindingIssue],
    sut_issues: &[SutIssue],
) -> Vec<CandidateTask> {
    let mut tasks = Vec::new();

    for issue in sut_issues {
        tasks.push(CandidateTask {
            category: TaskCategory::FixSut,
            priority: TaskPriority::Critical,
            name: format!("Fix failing scenario {}", issue.scenario_key),
            description: format!(
                "Fix scenario '{}' failing with {}.",
                issue.scenario_name, issue.failure_kind
            ),
            scenario_key: Some(issue.scenario_key.clone()),
            feature_path: None,
        });
    }

    for issue in binding_issues {
        tasks.push(CandidateTask {
            category: TaskCategory::CreateBindings,
            priority: TaskPriority::High,
            name: "Create missing binding".to_string(),
            description: issue.description.clone(),
            scenario_key: issue.scenario_key.clone(),
            feature_path: None,
        });
    }

    for issue in structure_issues {
        tasks.push(CandidateTask {
            category: TaskCategory::FixStructure,
            priority: TaskPriority::Medium,
            name: "Fix spec structure".to_string(),
            description: issue.description.clone(),
            scenario_key: None,
            feature_path: None,
        });
    }

    for issue in spec_issues {
        tasks.push(CandidateTask {
            category: TaskCategory::ImproveSpec,
            priority: TaskPriority::Low,
            name: "Improve spec coverage".to_string(),
            description: issue.description.clone(),
            scenario_key: None,
            feature_path: Some(issue.feature_path.clone()),
        });
    }

    tasks.sort_by_key(|t| t.priority);
    tasks
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_gate_status_default() {
        let status: GateStatus = Default::default();
        assert_eq!(status, GateStatus::NotRun);
    }

    #[test]
    fn test_gate_status_serialization() {
        let status = GateStatus::Pass;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"pass\"");

        let parsed: GateStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, GateStatus::Pass);
    }

    #[test]
    fn test_drift_kind_serialization() {
        let kind = DriftKind::FeatureFingerprint;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, "\"feature_fingerprint\"");
    }

    #[test]
    fn test_repo_state_summary_singular() {
        let state = RepoState {
            spec_issues: vec![SpecIssue {
                kind: SpecIssueKind::Ambiguous,
                feature_path: "test.feature".into(),
                description: "Ambiguous requirement".into(),
                rule_name: None,
            }],
            ..Default::default()
        };

        assert_eq!(
            state.summary(),
            "Spec: 1 issue • Structure: 0 • Bindings: 0 missing • SUT: 0 failing"
        );
    }

    #[test]
    fn test_repo_state_summary_plural() {
        let state = RepoState {
            spec_issues: vec![
                SpecIssue {
                    kind: SpecIssueKind::Ambiguous,
                    feature_path: "a.feature".into(),
                    description: "Issue 1".into(),
                    rule_name: None,
                },
                SpecIssue {
                    kind: SpecIssueKind::MissingCoverage,
                    feature_path: "b.feature".into(),
                    description: "Issue 2".into(),
                    rule_name: None,
                },
            ],
            binding_issues: vec![BindingIssue {
                kind: BindingIssueKind::MissingBinding,
                scenario_key: Some("test:Rule(01):Scenario(01)".into()),
                step_text: Some("Given a test".into()),
                description: "No binding".into(),
            }],
            sut_issues: vec![
                SutIssue {
                    scenario_key: "test:Rule(01):Scenario(02)".into(),
                    scenario_name: "Test scenario".into(),
                    failure_kind: "assertion".into(),
                    error_message: None,
                },
                SutIssue {
                    scenario_key: "test:Rule(01):Scenario(03)".into(),
                    scenario_name: "Another scenario".into(),
                    failure_kind: "panic".into(),
                    error_message: Some("test panic".into()),
                },
            ],
            ..Default::default()
        };

        assert_eq!(
            state.summary(),
            "Spec: 2 issues • Structure: 0 • Bindings: 1 missing • SUT: 2 failing"
        );
    }

    #[test]
    fn test_repo_state_all_gates_pass() {
        let mut state = RepoState::new();
        assert!(!state.all_gates_pass());

        state.lint_status = GateStatus::Pass;
        state.run_status = GateStatus::Pass;
        state.verify_status = GateStatus::Pass;
        assert!(state.all_gates_pass());

        state.verify_status = GateStatus::Fail;
        assert!(!state.all_gates_pass());
    }

    #[test]
    fn test_repo_state_has_issues() {
        let mut state = RepoState::new();
        assert!(!state.has_issues());

        state.spec_issues.push(SpecIssue {
            kind: SpecIssueKind::Ambiguous,
            feature_path: "test.feature".into(),
            description: "Test".into(),
            rule_name: None,
        });
        assert!(state.has_issues());
    }

    #[test]
    fn test_repo_state_has_work() {
        let mut state = RepoState::new();
        // Gates not passing → has work
        assert!(state.has_work());

        state.lint_status = GateStatus::Pass;
        state.run_status = GateStatus::Pass;
        state.verify_status = GateStatus::Pass;
        // All pass, no issues → no work
        assert!(!state.has_work());

        state.sut_issues.push(SutIssue {
            scenario_key: "test:Rule(01):Scenario(01)".into(),
            scenario_name: "Test".into(),
            failure_kind: "assertion".into(),
            error_message: None,
        });
        // Has issues → has work
        assert!(state.has_work());
    }

    #[test]
    fn test_candidate_task_priority_ordering() {
        assert!(TaskPriority::Critical < TaskPriority::High);
        assert!(TaskPriority::High < TaskPriority::Medium);
        assert!(TaskPriority::Medium < TaskPriority::Low);
    }

    #[test]
    fn test_failure_info_to_sut_issue() {
        let failure = FailureInfo {
            scenario_key: "test:Rule(01):Scenario(01)".into(),
            scenario_name: "Test scenario".into(),
            failure_kind: "assertion".into(),
            error_message: Some("Expected 1, got 2".into()),
        };

        let sut_issue: SutIssue = failure.into();
        assert_eq!(sut_issue.scenario_key, "test:Rule(01):Scenario(01)");
        assert_eq!(sut_issue.scenario_name, "Test scenario");
        assert_eq!(sut_issue.failure_kind, "assertion");
        assert_eq!(sut_issue.error_message, Some("Expected 1, got 2".into()));
    }

    #[test]
    fn test_repo_state_top_candidate() {
        let mut state = RepoState::new();
        assert!(state.top_candidate().is_none());

        state.candidate_tasks.push(CandidateTask {
            category: TaskCategory::FixSut,
            priority: TaskPriority::Critical,
            name: "Fix failing test".into(),
            description: "Test is failing".into(),
            scenario_key: Some("test:Rule(01):Scenario(01)".into()),
            feature_path: None,
        });

        let top = state.top_candidate().unwrap();
        assert_eq!(top.priority, TaskPriority::Critical);
        assert_eq!(top.name, "Fix failing test");
    }

    #[test]
    fn test_repo_state_serialization() {
        let state = RepoState {
            lint_status: GateStatus::Pass,
            run_status: GateStatus::Pass,
            verify_status: GateStatus::Fail,
            drift: Some(DriftInfo {
                kind: DriftKind::FeatureFingerprint,
                details: vec![DriftDetail {
                    field: "feature_fingerprint_hash".into(),
                    expected: "abc123".into(),
                    actual: "def456".into(),
                }],
            }),
            current_identity: Some(Identity {
                hash_contract_version: "namako-v1-json+blake3-256".into(),
                feature_fingerprint_hash: "def456".into(),
                step_registry_hash: "789abc".into(),
                resolved_plan_hash: "fedcba".into(),
            }),
            ..Default::default()
        };

        let json = serde_json::to_string_pretty(&state).unwrap();
        assert!(json.contains("\"lint_status\": \"pass\""));
        assert!(json.contains("\"verify_status\": \"fail\""));
        assert!(json.contains("\"feature_fingerprint\""));

        // Round-trip
        let parsed: RepoState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.lint_status, GateStatus::Pass);
        assert_eq!(parsed.verify_status, GateStatus::Fail);
        assert!(parsed.drift.is_some());
    }

    #[test]
    fn test_total_issue_count() {
        let state = RepoState {
            spec_issues: vec![SpecIssue {
                kind: SpecIssueKind::Ambiguous,
                feature_path: "test.feature".into(),
                description: "Issue".into(),
                rule_name: None,
            }],
            structure_issues: vec![
                StructureIssue {
                    kind: StructureIssueKind::MissingIdentityTag,
                    location: "test.feature:10".into(),
                    description: "Missing @Scenario".into(),
                },
                StructureIssue {
                    kind: StructureIssueKind::ParseError,
                    location: "test.feature:20".into(),
                    description: "Parse error".into(),
                },
            ],
            binding_issues: vec![BindingIssue {
                kind: BindingIssueKind::MissingBinding,
                scenario_key: None,
                step_text: Some("Given something".into()),
                description: "No binding".into(),
            }],
            sut_issues: vec![],
            ..Default::default()
        };

        assert_eq!(state.total_issue_count(), 4);
    }

    #[test]
    fn test_propagation_summary_line() {
        let state = RepoState {
            binding_issues: vec![BindingIssue {
                kind: BindingIssueKind::MissingBinding,
                scenario_key: None,
                step_text: Some("Given a step".into()),
                description: "Missing".into(),
            }],
            ..Default::default()
        };

        let summary = state.propagation_summary();
        assert!(summary.tests);
        assert!(summary.to_line().contains("Tests"));
    }

    fn status_stub() -> StatusPacket {
        StatusPacket {
            version: 1,
            spec_root: "/repo/specs".to_string(),
            recommended_next_action: "DONE".to_string(),
            lint_status: PacketStatusValue::Pass,
            run_status: PacketStatusValue::Pass,
            verify_status: PacketStatusValue::Pass,
            drift: Some(crate::packet_parser::DriftInfo {
                kind: "NONE".to_string(),
                details: vec![],
            }),
            last_run_failures: vec![],
            identity: crate::packet_parser::IdentitySection {
                current: crate::packet_parser::IdentityFields {
                    hash_contract_version: "namako-v1-json+blake3-256".to_string(),
                    feature_fingerprint_hash: "a".to_string(),
                    step_registry_hash: "b".to_string(),
                    resolved_plan_hash: "c".to_string(),
                },
                baseline: None,
            },
            metadata: None,
            gates: None,
        }
    }

    fn review_stub() -> ReviewPacket {
        ReviewPacket {
            version: 1,
            spec_root: "/repo/specs".to_string(),
            identity_current: crate::packet_parser::IdentityFields {
                hash_contract_version: "namako-v1-json+blake3-256".to_string(),
                feature_fingerprint_hash: "a".to_string(),
                step_registry_hash: "b".to_string(),
                resolved_plan_hash: "c".to_string(),
            },
            features: vec![crate::packet_parser::FeatureReview {
                feature_path: "features/a.feature".to_string(),
                feature_name: "A".to_string(),
                rules: vec![crate::packet_parser::RuleReview {
                    rule_name: "Rule(01)".to_string(),
                    source_span: crate::packet_parser::SourceSpan { start_line: 1, end_line: 10 },
                    executable_scenarios: vec![],
                    deferred_items: vec![],
                }],
            }],
            coverage_summary: crate::packet_parser::CoverageSummary {
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

    fn gate_stub() -> GatePacket {
        GatePacket {
            lint: crate::packet_parser::GatePhase { status: GatePhaseStatus::Pass, reason: None },
            run: crate::packet_parser::GatePhase { status: GatePhaseStatus::Pass, reason: None },
            verify: crate::packet_parser::GatePhase { status: GatePhaseStatus::Pass, reason: None },
            determinism: None,
        }
    }

    #[test]
    fn test_compute_populates_candidate_tasks() {
        let state = RepoState::compute(&status_stub(), &review_stub(), &gate_stub(), None).unwrap();
        assert!(!state.candidate_tasks.is_empty());
    }

    #[test]
    fn test_compute_all_gates_pass() {
        let state = RepoState::compute(&status_stub(), &review_stub(), &gate_stub(), None).unwrap();
        assert!(state.all_gates_pass());
    }

    #[test]
    fn test_compute_identity_fields() {
        let state = RepoState::compute(&status_stub(), &review_stub(), &gate_stub(), None).unwrap();
        let identity = state.current_identity.unwrap();
        assert_eq!(identity.hash_contract_version, "namako-v1-json+blake3-256");
    }

    #[test]
    fn test_compute_scenario_counts_from_features() {
        let temp = tempdir().unwrap();
        let spec_root = temp.path();
        let features_dir = spec_root.join("features");
        fs::create_dir_all(&features_dir).unwrap();

        let feature_path = features_dir.join("a.feature");
        fs::write(
            &feature_path,
            r#"
Feature: Example

  @Rule(01)
  Rule: First rule

    @Scenario(01)
    Scenario: First scenario
      Given a precondition
      When an action happens
      Then an outcome occurs

  @Rule(02)
  Rule: Second rule

    @Scenario(01)
    Scenario: Another scenario
      Given a precondition
      When an action happens
      Then an outcome occurs

    @Scenario(02)
    Scenario: Third scenario
      Given a precondition
      When an action happens
      Then an outcome occurs
"#,
        )
        .unwrap();

        let review = ReviewPacket {
            version: 1,
            spec_root: spec_root.to_string_lossy().to_string(),
            identity_current: crate::packet_parser::IdentityFields {
                hash_contract_version: "namako-v1-json+blake3-256".to_string(),
                feature_fingerprint_hash: "a".to_string(),
                step_registry_hash: "b".to_string(),
                resolved_plan_hash: "c".to_string(),
            },
            features: vec![crate::packet_parser::FeatureReview {
                feature_path: "features/a.feature".to_string(),
                feature_name: "Example".to_string(),
                rules: vec![
                    crate::packet_parser::RuleReview {
                        rule_name: "First rule".to_string(),
                        source_span: crate::packet_parser::SourceSpan { start_line: 1, end_line: 10 },
                        executable_scenarios: vec![],
                        deferred_items: vec![],
                    },
                    crate::packet_parser::RuleReview {
                        rule_name: "Second rule".to_string(),
                        source_span: crate::packet_parser::SourceSpan { start_line: 1, end_line: 10 },
                        executable_scenarios: vec![],
                        deferred_items: vec![],
                    },
                ],
            }],
            coverage_summary: crate::packet_parser::CoverageSummary {
                rules_total: 2,
                rules_with_zero_executable: 2,
                executable_scenarios_total: 0,
                deferred_items_total: 0,
            },
            deferred_items: vec![],
            promotion_candidates: vec![],
            missing_bindings_for_top_candidates: vec![],
            harness_gaps: vec![],
            suggested_binding_bundle: None,
        };

        let status = StatusPacket {
            version: 1,
            spec_root: spec_root.to_string_lossy().to_string(),
            recommended_next_action: "DONE".to_string(),
            lint_status: PacketStatusValue::Pass,
            run_status: PacketStatusValue::Pass,
            verify_status: PacketStatusValue::Pass,
            drift: None,
            last_run_failures: vec![],
            identity: crate::packet_parser::IdentitySection {
                current: review.identity_current.clone(),
                baseline: None,
            },
            metadata: None,
            gates: None,
        };

        let state = RepoState::compute(&status, &review, &gate_stub(), None).unwrap();
        assert_eq!(state.scenario_count_for_feature("features/a.feature"), Some(3));
        assert_eq!(state.rule_count_for_feature("features/a.feature"), Some(2));

        let first_rule_key = super::rule_key("features/a.feature", "First rule");
        let second_rule_key = super::rule_key("features/a.feature", "Second rule");
        assert_eq!(state.scenarios_per_rule.get(&first_rule_key), Some(&1));
        assert_eq!(state.scenarios_per_rule.get(&second_rule_key), Some(&2));
    }

    #[test]
    fn test_compute_scenario_counts_excludes_deferred() {
        let temp = tempdir().unwrap();
        let spec_root = temp.path();
        let features_dir = spec_root.join("features");
        fs::create_dir_all(&features_dir).unwrap();

        let feature_path = features_dir.join("b.feature");
        fs::write(
            &feature_path,
            r#"
Feature: Example

  @Rule(01)
  Rule: First rule

    @Scenario(01)
    Scenario: Active scenario
      Given a precondition
      When an action happens
      Then an outcome occurs

    @Deferred
    @Scenario(02)
    Scenario: Deferred scenario
      Given a precondition
      When an action happens
      Then an outcome occurs

    @Scenario(03)
    Scenario: Another active scenario
      Given a precondition
      When an action happens
      Then an outcome occurs
"#,
        )
        .unwrap();

        let review = ReviewPacket {
            version: 1,
            spec_root: spec_root.to_string_lossy().to_string(),
            identity_current: crate::packet_parser::IdentityFields {
                hash_contract_version: "namako-v1-json+blake3-256".to_string(),
                feature_fingerprint_hash: "a".to_string(),
                step_registry_hash: "b".to_string(),
                resolved_plan_hash: "c".to_string(),
            },
            features: vec![crate::packet_parser::FeatureReview {
                feature_path: "features/b.feature".to_string(),
                feature_name: "Example".to_string(),
                rules: vec![crate::packet_parser::RuleReview {
                    rule_name: "First rule".to_string(),
                    source_span: crate::packet_parser::SourceSpan { start_line: 1, end_line: 10 },
                    executable_scenarios: vec![],
                    deferred_items: vec![],
                }],
            }],
            coverage_summary: crate::packet_parser::CoverageSummary {
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
        };

        let status = StatusPacket {
            version: 1,
            spec_root: spec_root.to_string_lossy().to_string(),
            recommended_next_action: "DONE".to_string(),
            lint_status: PacketStatusValue::Pass,
            run_status: PacketStatusValue::Pass,
            verify_status: PacketStatusValue::Pass,
            drift: None,
            last_run_failures: vec![],
            identity: crate::packet_parser::IdentitySection {
                current: review.identity_current.clone(),
                baseline: None,
            },
            metadata: None,
            gates: None,
        };

        let state = RepoState::compute(&status, &review, &gate_stub(), None).unwrap();
        assert_eq!(state.scenario_count_for_feature("features/b.feature"), Some(2));

        let first_rule_key = super::rule_key("features/b.feature", "First rule");
        assert_eq!(state.scenarios_per_rule.get(&first_rule_key), Some(&2));
    }

    #[test]
    fn test_rule_count_increases_across_versions() {
        let temp = tempdir().unwrap();
        let spec_root = temp.path();
        let features_dir = spec_root.join("features");
        fs::create_dir_all(&features_dir).unwrap();

        let feature_path = features_dir.join("a.feature");
        fs::write(
            &feature_path,
            r#"
Feature: Example

  @Rule(01)
  Rule: First rule

    @Scenario(01)
    Scenario: First scenario
      Given a precondition
      When an action happens
      Then an outcome occurs
"#,
        )
        .unwrap();

        let review = ReviewPacket {
            version: 1,
            spec_root: spec_root.to_string_lossy().to_string(),
            identity_current: crate::packet_parser::IdentityFields {
                hash_contract_version: "namako-v1-json+blake3-256".to_string(),
                feature_fingerprint_hash: "a".to_string(),
                step_registry_hash: "b".to_string(),
                resolved_plan_hash: "c".to_string(),
            },
            features: vec![crate::packet_parser::FeatureReview {
                feature_path: "features/a.feature".to_string(),
                feature_name: "Example".to_string(),
                rules: vec![crate::packet_parser::RuleReview {
                    rule_name: "First rule".to_string(),
                    source_span: crate::packet_parser::SourceSpan { start_line: 1, end_line: 10 },
                    executable_scenarios: vec![],
                    deferred_items: vec![],
                }],
            }],
            coverage_summary: crate::packet_parser::CoverageSummary {
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
        };

        let status = StatusPacket {
            version: 1,
            spec_root: spec_root.to_string_lossy().to_string(),
            recommended_next_action: "DONE".to_string(),
            lint_status: PacketStatusValue::Pass,
            run_status: PacketStatusValue::Pass,
            verify_status: PacketStatusValue::Pass,
            drift: None,
            last_run_failures: vec![],
            identity: crate::packet_parser::IdentitySection {
                current: review.identity_current.clone(),
                baseline: None,
            },
            metadata: None,
            gates: None,
        };

        let state_before = RepoState::compute(&status, &review, &gate_stub(), None).unwrap();
        assert_eq!(state_before.rule_count_for_feature("features/a.feature"), Some(1));

        fs::write(
            &feature_path,
            r#"
Feature: Example

  @Rule(01)
  Rule: First rule

    @Scenario(01)
    Scenario: First scenario
      Given a precondition
      When an action happens
      Then an outcome occurs

  @Rule(02)
  Rule: Second rule

    @Scenario(01)
    Scenario: Another scenario
      Given a precondition
      When an action happens
      Then an outcome occurs
"#,
        )
        .unwrap();

        let state_after = RepoState::compute(&status, &review, &gate_stub(), None).unwrap();
        assert_eq!(state_after.rule_count_for_feature("features/a.feature"), Some(2));
    }

    #[test]
    fn test_coverage_ambiguity_detects_one_scenario() {
        let mut review = review_stub();
        if let Some(rule) = review.features.first_mut().and_then(|f| f.rules.first_mut()) {
            rule.executable_scenarios.push(crate::packet_parser::ScenarioReview {
                name: "Scenario A".to_string(),
                source_span: crate::packet_parser::SourceSpan { start_line: 1, end_line: 5 },
                steps: vec![],
            });
        }

        let state = RepoState::compute(&status_stub(), &review, &gate_stub(), None).unwrap();
        assert_eq!(state.coverage_ambiguity.rules_with_one_scenario.len(), 1);
    }

    #[test]
    fn test_integration_with_naia_packets() {
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

        let state = RepoState::compute(&status, &review, &gate_stub(), None).unwrap();
        assert!(state.current_identity.is_some());
    }
}
