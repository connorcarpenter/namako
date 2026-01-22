//! `namako status --json` command implementation.
//!
//! This command provides a deterministic, single-shot JSON describing the current
//! FSM state and identities. Tesaki can decide next action without parsing logs.
//!
//! Per TODO.md §1, this is the highest-leverage, lowest-risk command for v2.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use clap::Args;
use serde::Serialize;
use walkdir::WalkDir;

use namako_engine::engine::ResolutionEngine;
use namako_engine::npap::{
    Certification, CertificationIdentity, RunReport, ScenarioStatus, SemanticStepRegistry,
    HASH_CONTRACT_VERSION,
};

/// Arguments for the status command.
#[derive(Args, Debug)]
pub struct StatusArgs {
    /// Path to the specs directory containing features/.
    #[arg(short, long, default_value = ".")]
    pub specs_dir: PathBuf,

    /// Adapter command to fetch manifest.
    #[arg(short, long)]
    pub adapter_cmd: String,

    /// Path to the certification.json baseline.
    #[arg(short, long, default_value = "certification.json")]
    pub certification: PathBuf,

    /// Path to the run_report.json from last adapter execution.
    #[arg(short, long, default_value = "run_report.json")]
    pub run_report: PathBuf,

    /// Output as JSON (default: human-readable).
    #[arg(long)]
    pub json: bool,

    /// Output path for status JSON file. If omitted, prints to stdout.
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// Print verbose output.
    #[arg(short, long, default_value = "false")]
    pub verbose: bool,
}

/// Status output schema per GOLD_PLAN §10.5.5
#[derive(Debug, Clone, Serialize)]
pub struct StatusOutput {
    /// Schema version (starts at 1)
    pub version: u32,
    /// Absolute or normalized relative path to spec root
    pub spec_root: String,
    /// Recommended next action for Tesaki
    pub recommended_next_action: RecommendedAction,
    /// Lint status: pass | fail | stale
    pub lint_status: StatusValue,
    /// Run status: pass | fail | stale | not_run
    pub run_status: StatusValue,
    /// Verify status: pass | fail | stale | not_run
    pub verify_status: StatusValue,
    /// Drift detection results
    pub drift: DriftInfo,
    /// Failures from last run report
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub last_run_failures: Vec<FailureRecord>,
    /// Identity (current and baseline)
    pub identity: IdentitySection,
    /// Metadata (timestamp, version)
    pub metadata: StatusMetadata,
    /// Legacy gate status (deprecated, kept for compat)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gates: Option<GateStatus>,
}

/// Status value enum per GOLD_PLAN §10.5.5
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusValue {
    Pass,
    Fail,
    Stale,
    NotRun,
}

/// Identity section per GOLD_PLAN §10.5.5
#[derive(Debug, Clone, Serialize)]
pub struct IdentitySection {
    pub current: IdentityFields,
    pub baseline: Option<IdentityFields>,
}

/// Metadata section per GOLD_PLAN §10.5.5
#[derive(Debug, Clone, Serialize)]
pub struct StatusMetadata {
    pub timestamp: String,
    pub namako_version: String,
}

/// Record of a single failed scenario for failure targeting (per TODO.md §4.2).
#[derive(Debug, Clone, Serialize)]
pub struct FailureRecord {
    /// Scenario key from run report
    pub scenario_key: String,
    /// Human-readable scenario name (extracted from scenario_key)
    pub scenario_name: String,
    /// Type of failure: "assertion", "panic", "missing_binding", "timeout", "unknown"
    pub failure_kind: String,
    /// Short error summary from first failing step
    pub summary: String,
}

/// Gate status for each pipeline step
#[derive(Debug, Clone, Serialize)]
pub struct GateStatus {
    pub lint: GateResult,
    pub run: GateResult,
    pub verify: GateResult,
}

/// Individual gate result
#[derive(Debug, Clone, Serialize)]
pub struct GateResult {
    pub ok: bool,
    pub code: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// Identity fields
#[derive(Debug, Clone, Serialize)]
pub struct IdentityFields {
    pub hash_contract_version: String,
    pub feature_fingerprint_hash: String,
    pub step_registry_hash: String,
    pub resolved_plan_hash: String,
}

impl From<CertificationIdentity> for IdentityFields {
    fn from(ci: CertificationIdentity) -> Self {
        Self {
            hash_contract_version: ci.hash_contract_version,
            feature_fingerprint_hash: ci.feature_fingerprint_hash,
            step_registry_hash: ci.step_registry_hash,
            resolved_plan_hash: ci.resolved_plan_hash,
        }
    }
}

/// Drift detection info
#[derive(Debug, Clone, Serialize)]
pub struct DriftInfo {
    pub kind: DriftKind,
    pub details: Vec<DriftDetail>,
}

/// Drift kind enumeration
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DriftKind {
    None,
    Feature,
    StepRegistry,
    ResolvedPlan,
    Multiple,
    NoBaseline,
    Integrity,
}

/// Individual drift detail
#[derive(Debug, Clone, Serialize)]
pub struct DriftDetail {
    pub field: String,
    pub baseline: String,
    pub current: String,
}

/// Recommended next action enumeration
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RecommendedAction {
    FixLint,
    Run,
    FixRun,
    NeedsUpdateCertApproval,
    Done,
}

/// Run the status command.
pub fn run(args: StatusArgs) -> Result<()> {
    let output = compute_status(&args)?;

    let json = serde_json::to_string_pretty(&output)
        .context("Failed to serialize status output")?;

    if let Some(ref path) = args.out {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }
        std::fs::write(path, &json)
            .with_context(|| format!("Failed to write status to {}", path.display()))?;
        if !args.json {
            eprintln!("✓ Status written to: {}", path.display());
        }
    } else if args.json {
        println!("{}", json);
    } else {
        // Human-readable summary
        print_human_readable(&output);
    }

    Ok(())
}

fn compute_status(args: &StatusArgs) -> Result<StatusOutput> {
    let spec_root = args.specs_dir.canonicalize()
        .unwrap_or_else(|_| args.specs_dir.clone())
        .to_string_lossy()
        .to_string();

    let timestamp_utc = chrono::Utc::now().to_rfc3339();

    // Attempt to compute identity from sources
    let (identity_result, _lint_ok, lint_summary) = compute_identity_with_diagnostics(args);

    // Load baseline certification if it exists
    let baseline_result = load_baseline(&args.certification);
    let identity_baseline = baseline_result.as_ref().ok().map(|c| IdentityFields::from(c.identity.clone()));

    // Determine gates status
    let (gates, drift, base_action) = match &identity_result {
        Ok(identity) => {
            // Lint passed - we have a valid identity
            let lint = GateResult { ok: true, code: 0, summary: lint_summary };

            // For run and verify, we check against baseline
            let (run, verify, drift, action) = match &baseline_result {
                Ok(baseline) => {
                    // Compare identity to baseline
                    let drift_info = compute_drift(identity, &baseline.identity);

                    if matches!(drift_info.kind, DriftKind::None) {
                        // Everything matches
                        (
                            GateResult { ok: true, code: 0, summary: Some("All scenarios up-to-date".to_string()) },
                            GateResult { ok: true, code: 0, summary: Some("Baseline matches current".to_string()) },
                            drift_info,
                            RecommendedAction::Done,
                        )
                    } else {
                        // Drift detected - need update-cert approval
                        (
                            GateResult { ok: true, code: 0, summary: Some("Run required after changes".to_string()) },
                            GateResult { ok: false, code: 1, summary: Some("Drift detected".to_string()) },
                            drift_info,
                            RecommendedAction::NeedsUpdateCertApproval,
                        )
                    }
                }
                Err(_) => {
                    // No baseline - need to create one
                    let drift_info = DriftInfo {
                        kind: DriftKind::NoBaseline,
                        details: vec![],
                    };
                    (
                        GateResult { ok: true, code: 0, summary: Some("Ready to run".to_string()) },
                        GateResult { ok: false, code: 1, summary: Some("No baseline certification found".to_string()) },
                        drift_info,
                        RecommendedAction::Run,
                    )
                }
            };

            let gates = GateStatus { lint, run, verify };
            (gates, drift, action)
        }
        Err(err) => {
            // Lint failed
            let lint = GateResult {
                ok: false,
                code: 1,
                summary: Some(format!("Resolution failed: {}", err))
            };
            let run = GateResult { ok: false, code: 1, summary: Some("Cannot run - lint failed".to_string()) };
            let verify = GateResult { ok: false, code: 1, summary: Some("Cannot verify - lint failed".to_string()) };

            let drift = DriftInfo {
                kind: DriftKind::Integrity,
                details: vec![],
            };

            (GateStatus { lint, run, verify }, drift, RecommendedAction::FixLint)
        }
    };

    let identity_current = identity_result.unwrap_or_else(|_| IdentityFields {
        hash_contract_version: HASH_CONTRACT_VERSION.to_string(),
        feature_fingerprint_hash: "UNKNOWN".to_string(),
        step_registry_hash: "UNKNOWN".to_string(),
        resolved_plan_hash: "UNKNOWN".to_string(),
    });

    // Load run report failures for failure targeting.
    let last_run_failures = load_run_failures(&args.run_report);

    // Convert gates to status values
    let lint_status = if gates.lint.ok { StatusValue::Pass } else { StatusValue::Fail };
    let run_status = if gates.run.ok { StatusValue::Pass } else if gates.run.summary.as_ref().map(|s| s.contains("Cannot run")).unwrap_or(false) { StatusValue::NotRun } else { StatusValue::Fail };
    let verify_status = if gates.verify.ok {
        StatusValue::Pass
    } else if gates.verify.summary.as_ref().map(|s| s.contains("Drift detected")).unwrap_or(false) {
        StatusValue::Stale
    } else if gates.verify.summary.as_ref().map(|s| s.contains("Cannot verify") || s.contains("No baseline")).unwrap_or(false) {
        StatusValue::NotRun
    } else {
        StatusValue::Fail
    };

    let recommended_action = if matches!(base_action, RecommendedAction::Done) && !last_run_failures.is_empty() {
        RecommendedAction::FixRun
    } else {
        base_action
    };

    Ok(StatusOutput {
        version: 1,
        spec_root,
        recommended_next_action: recommended_action,
        lint_status,
        run_status,
        verify_status,
        drift,
        last_run_failures,
        identity: IdentitySection {
            current: identity_current,
            baseline: identity_baseline,
        },
        metadata: StatusMetadata {
            timestamp: timestamp_utc,
            namako_version: env!("CARGO_PKG_VERSION").to_string(),
        },
        gates: Some(gates),
    })
}

fn compute_identity_with_diagnostics(args: &StatusArgs) -> (Result<IdentityFields>, bool, Option<String>) {
    match recompute_identity(args) {
        Ok(identity) => (Ok(identity), true, Some("All steps resolved".to_string())),
        Err(e) => (Err(e), false, None),
    }
}

fn recompute_identity(args: &StatusArgs) -> Result<IdentityFields> {
    let features_dir = args.specs_dir.join("features");
    if !features_dir.exists() {
        bail!("Features directory not found: {}", features_dir.display());
    }

    let feature_paths = discover_features(&features_dir)?;
    let features = read_features(&args.specs_dir, &feature_paths)?;
    let registry = fetch_adapter_manifest(&args.adapter_cmd)?;

    let engine = ResolutionEngine::new(&registry)
        .map_err(|errs| anyhow::anyhow!("Failed to build engine: {:?}", errs))?;

    let feature_refs: Vec<(&str, &str)> = features
        .iter()
        .map(|(path, content)| (path.as_str(), content.as_str()))
        .collect();
    let result = engine.resolve(feature_refs.into_iter());

    if !result.errors.is_empty() {
        bail!("Resolution errors: {:?}", result.errors);
    }

    let plan = result.plan.expect("No errors but no plan");

    Ok(IdentityFields {
        hash_contract_version: HASH_CONTRACT_VERSION.to_string(),
        feature_fingerprint_hash: plan.header.feature_fingerprint_hash,
        step_registry_hash: plan.header.step_registry_hash,
        resolved_plan_hash: plan.header.resolved_plan_hash,
    })
}

fn load_baseline(path: &PathBuf) -> Result<Certification> {
    let json = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read baseline: {}", path.display()))?;
    serde_json::from_str(&json).context("Failed to parse certification JSON")
}

/// Load run report and extract failures for failure targeting (per TODO.md §4.2).
/// Returns empty vector if run_report doesn't exist or has no failures.
fn load_run_failures(run_report_path: &PathBuf) -> Vec<FailureRecord> {
    // Attempt to read run report - silently return empty if not found
    let json = match std::fs::read_to_string(run_report_path) {
        Ok(j) => j,
        Err(_) => return Vec::new(),
    };

    // Parse run report
    let run_report: RunReport = match serde_json::from_str(&json) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    // Extract failed scenarios
    let mut failures: Vec<FailureRecord> = run_report
        .scenarios
        .iter()
        .filter(|s| matches!(s.status, ScenarioStatus::Failed))
        .map(|s| {
            // Extract scenario name from scenario_key (e.g., "features/smoke.feature:L8")
            // For now, use the scenario_key as the name
            let scenario_name = extract_scenario_name(&s.scenario_key);

            // Determine failure kind and summary from first failing step
            let (failure_kind, summary) = s
                .steps
                .iter()
                .find(|step| matches!(step.status, namako_engine::npap::StepStatus::Failed))
                .map(|step| {
                    let kind = classify_failure(&step.error_message);
                    let summary = step
                        .error_message
                        .as_ref()
                        .map(|m| truncate_summary(m, 200))
                        .unwrap_or_else(|| "Step failed".to_string());
                    (kind, summary)
                })
                .unwrap_or_else(|| ("unknown".to_string(), "Scenario failed".to_string()));

            FailureRecord {
                scenario_key: s.scenario_key.clone(),
                scenario_name,
                failure_kind,
                summary,
            }
        })
        .collect();

    // Sort by scenario_key for determinism
    failures.sort_by(|a, b| a.scenario_key.cmp(&b.scenario_key));
    failures
}

/// Extract a human-readable scenario name from a scenario_key.
/// Example: "features/smoke.feature:L8" -> "smoke.feature:L8"
fn extract_scenario_name(scenario_key: &str) -> String {
    // Take everything after the last "/" or return the whole key
    scenario_key
        .rsplit('/')
        .next()
        .unwrap_or(scenario_key)
        .to_string()
}

/// Classify failure based on error message content.
fn classify_failure(error_message: &Option<String>) -> String {
    match error_message {
        Some(msg) => {
            let msg_lower = msg.to_lowercase();
            if msg_lower.contains("assertion") || msg_lower.contains("assert") {
                "assertion".to_string()
            } else if msg_lower.contains("panic") || msg_lower.contains("thread") {
                "panic".to_string()
            } else if msg_lower.contains("missing") || msg_lower.contains("not found") {
                "missing_binding".to_string()
            } else if msg_lower.contains("timeout") || msg_lower.contains("timed out") {
                "timeout".to_string()
            } else {
                "unknown".to_string()
            }
        }
        None => "unknown".to_string(),
    }
}

/// Truncate summary to a maximum length, adding "..." if truncated.
fn truncate_summary(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

fn compute_drift(current: &IdentityFields, baseline: &CertificationIdentity) -> DriftInfo {
    let mut details = Vec::new();

    if current.feature_fingerprint_hash != baseline.feature_fingerprint_hash {
        details.push(DriftDetail {
            field: "feature_fingerprint_hash".to_string(),
            baseline: baseline.feature_fingerprint_hash.clone(),
            current: current.feature_fingerprint_hash.clone(),
        });
    }

    if current.step_registry_hash != baseline.step_registry_hash {
        details.push(DriftDetail {
            field: "step_registry_hash".to_string(),
            baseline: baseline.step_registry_hash.clone(),
            current: current.step_registry_hash.clone(),
        });
    }

    if current.resolved_plan_hash != baseline.resolved_plan_hash {
        details.push(DriftDetail {
            field: "resolved_plan_hash".to_string(),
            baseline: baseline.resolved_plan_hash.clone(),
            current: current.resolved_plan_hash.clone(),
        });
    }

    let kind = match details.len() {
        0 => DriftKind::None,
        1 => {
            match details[0].field.as_str() {
                "feature_fingerprint_hash" => DriftKind::Feature,
                "step_registry_hash" => DriftKind::StepRegistry,
                "resolved_plan_hash" => DriftKind::ResolvedPlan,
                _ => DriftKind::Integrity,
            }
        }
        _ => DriftKind::Multiple,
    };

    DriftInfo { kind, details }
}

fn print_human_readable(status: &StatusOutput) {
    // Rich diff output per GOLD_PLAN §10.5.6
    println!("=== Identity Status ===");

    // Identity fields comparison
    if let Some(baseline) = &status.identity.baseline {
        let current = &status.identity.current;

        // feature_fingerprint_hash
        if current.feature_fingerprint_hash == baseline.feature_fingerprint_hash {
            println!("✓ feature_fingerprint_hash: MATCH");
        } else {
            println!("✗ feature_fingerprint_hash: DRIFTED");
            println!("  Baseline: {}", baseline.feature_fingerprint_hash);
            println!("  Current:  {}", current.feature_fingerprint_hash);
        }

        // step_registry_hash
        if current.step_registry_hash == baseline.step_registry_hash {
            println!("✓ step_registry_hash: MATCH");
        } else {
            println!("✗ step_registry_hash: DRIFTED");
            println!("  Baseline: {}", baseline.step_registry_hash);
            println!("  Current:  {}", current.step_registry_hash);
        }

        // resolved_plan_hash
        if current.resolved_plan_hash == baseline.resolved_plan_hash {
            println!("✓ resolved_plan_hash: MATCH");
        } else {
            println!("✗ resolved_plan_hash: DRIFTED");
            println!("  Baseline: {}", baseline.resolved_plan_hash);
            println!("  Current:  {}", current.resolved_plan_hash);
        }
    } else {
        println!("(no baseline certification found)");
        println!("  Current feature_fingerprint: {}", status.identity.current.feature_fingerprint_hash);
        println!("  Current step_registry: {}", status.identity.current.step_registry_hash);
        println!("  Current resolved_plan: {}", status.identity.current.resolved_plan_hash);
    }

    println!();
    println!("=== Recommended Action ===");
    println!("{:?}", status.recommended_next_action);

    if !status.last_run_failures.is_empty() {
        println!();
        println!("=== Recent Failures ===");
        for (i, failure) in status.last_run_failures.iter().enumerate() {
            println!("{}. {} — {}", i + 1, failure.scenario_key, failure.summary);
        }
    }
}

/// Discover all `.feature` files under the given directory.
fn discover_features(dir: &std::path::Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();

    for entry in WalkDir::new(dir).follow_links(true) {
        let entry = entry.with_context(|| format!("Failed to walk {}", dir.display()))?;
        let path = entry.path();

        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "feature" {
                    paths.push(path.to_path_buf());
                }
            }
        }
    }

    paths.sort();
    Ok(paths)
}

/// Read feature files and return (relative_path, content) pairs.
fn read_features(
    specs_dir: &std::path::Path,
    paths: &[PathBuf],
) -> Result<Vec<(String, String)>> {
    let mut features = Vec::with_capacity(paths.len());

    for path in paths {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        let relative_path = path
            .strip_prefix(specs_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        features.push((relative_path, content));
    }

    Ok(features)
}

/// Fetch the semantic step registry from the adapter.
fn fetch_adapter_manifest(adapter_cmd: &str) -> Result<SemanticStepRegistry> {
    let parts: Vec<&str> = adapter_cmd.split_whitespace().collect();
    if parts.is_empty() {
        bail!("Empty adapter command");
    }

    let program = parts[0];
    let args: Vec<&str> = parts[1..].to_vec();

    let output = Command::new(program)
        .args(&args)
        .arg("manifest")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("Failed to execute adapter: {}", adapter_cmd))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Adapter command failed: {}", stderr);
    }

    let stdout = String::from_utf8(output.stdout)
        .context("Adapter output is not valid UTF-8")?;

    serde_json::from_str(&stdout).context("Failed to parse adapter manifest JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drift_kind_none() {
        let current = IdentityFields {
            hash_contract_version: "v1".to_string(),
            feature_fingerprint_hash: "abc".to_string(),
            step_registry_hash: "def".to_string(),
            resolved_plan_hash: "ghi".to_string(),
        };
        let baseline = CertificationIdentity {
            hash_contract_version: "v1".to_string(),
            feature_fingerprint_hash: "abc".to_string(),
            step_registry_hash: "def".to_string(),
            resolved_plan_hash: "ghi".to_string(),
        };
        let drift = compute_drift(&current, &baseline);
        assert!(matches!(drift.kind, DriftKind::None));
        assert!(drift.details.is_empty());
    }

    #[test]
    fn test_drift_kind_feature() {
        let current = IdentityFields {
            hash_contract_version: "v1".to_string(),
            feature_fingerprint_hash: "changed".to_string(),
            step_registry_hash: "def".to_string(),
            resolved_plan_hash: "ghi".to_string(),
        };
        let baseline = CertificationIdentity {
            hash_contract_version: "v1".to_string(),
            feature_fingerprint_hash: "abc".to_string(),
            step_registry_hash: "def".to_string(),
            resolved_plan_hash: "ghi".to_string(),
        };
        let drift = compute_drift(&current, &baseline);
        assert!(matches!(drift.kind, DriftKind::Feature));
        assert_eq!(drift.details.len(), 1);
    }

    #[test]
    fn test_drift_kind_multiple() {
        let current = IdentityFields {
            hash_contract_version: "v1".to_string(),
            feature_fingerprint_hash: "changed1".to_string(),
            step_registry_hash: "changed2".to_string(),
            resolved_plan_hash: "ghi".to_string(),
        };
        let baseline = CertificationIdentity {
            hash_contract_version: "v1".to_string(),
            feature_fingerprint_hash: "abc".to_string(),
            step_registry_hash: "def".to_string(),
            resolved_plan_hash: "ghi".to_string(),
        };
        let drift = compute_drift(&current, &baseline);
        assert!(matches!(drift.kind, DriftKind::Multiple));
        assert_eq!(drift.details.len(), 2);
    }

    #[test]
    fn test_extract_scenario_name() {
        assert_eq!(
            extract_scenario_name("features/smoke.feature:L8"),
            "smoke.feature:L8"
        );
        assert_eq!(
            extract_scenario_name("features/sub/test.feature:L15:E0:R2"),
            "test.feature:L15:E0:R2"
        );
        assert_eq!(
            extract_scenario_name("simple_key"),
            "simple_key"
        );
    }

    #[test]
    fn test_classify_failure() {
        assert_eq!(classify_failure(&Some("assertion failed: expected 1, got 2".to_string())), "assertion");
        assert_eq!(classify_failure(&Some("thread panicked at 'explicit panic'".to_string())), "panic");
        assert_eq!(classify_failure(&Some("binding not found".to_string())), "missing_binding");
        assert_eq!(classify_failure(&Some("operation timed out".to_string())), "timeout");
        assert_eq!(classify_failure(&Some("some other error".to_string())), "unknown");
        assert_eq!(classify_failure(&None), "unknown");
    }

    #[test]
    fn test_truncate_summary() {
        assert_eq!(truncate_summary("short", 100), "short");
        assert_eq!(truncate_summary("0123456789", 10), "0123456789");
        assert_eq!(truncate_summary("01234567890", 10), "0123456...");
    }
}
