//! `namako gate` command implementation.
//!
//! This command provides a single Rust-native entrypoint that replaces the bash
//! scripts (namako_ci.sh, determinism_check.sh) with a unified gate command.
//!
//! Per TODO.md, the command:
//! - Runs: `lint` → `run` → `verify` in sequence
//! - Exits non-zero on first failure
//! - With `--determinism`: runs twice and compares evidence bundles

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use clap::Args;
use serde::Serialize;
use serde_json::Value;
use tempfile::TempDir;

use namako::engine::ResolutionEngine;
use namako::npap::{
    Certification, CertificationIdentity, RunReport, ScenarioStatus,
    SemanticStepRegistry, HASH_CONTRACT_VERSION,
};

use crate::status::{self, StatusArgs};
use crate::review::{self, ReviewArgs};

/// Arguments for the gate command.
#[derive(Args, Debug)]
pub struct GateArgs {
    /// Path to the specs directory containing features/.
    #[arg(short, long, default_value = ".")]
    pub specs_dir: PathBuf,

    /// Adapter command to invoke.
    /// Will be invoked as: `<adapter_cmd> manifest` and `<adapter_cmd> run ...`
    #[arg(short, long)]
    pub adapter_cmd: String,

    /// Path to the certification.json baseline.
    #[arg(short, long, default_value = "certification.json")]
    pub certification: PathBuf,

    /// Enable determinism check: run the gate twice and compare evidence bundles.
    #[arg(long, default_value = "false")]
    pub determinism: bool,

    /// Number of runs for determinism check (default: 2 when --determinism, 1 otherwise).
    #[arg(long)]
    pub runs: Option<usize>,

    /// Output as JSON (machine-readable summary of each phase).
    #[arg(long, default_value = "false")]
    pub json: bool,

    /// Print verbose output.
    #[arg(short, long, default_value = "false")]
    pub verbose: bool,
}

/// Gate output schema for --json mode
#[derive(Debug, Clone, Serialize)]
pub struct GateOutput {
    pub lint: PhaseResult,
    pub run: PhaseResult,
    pub verify: PhaseResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinism: Option<DeterminismResult>,
}

/// Result of a single phase
#[derive(Debug, Clone, Serialize)]
pub struct PhaseResult {
    pub status: PhaseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Phase status enum
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PhaseStatus {
    Pass,
    Fail,
    Skipped,
}

/// Determinism check result
#[derive(Debug, Clone, Serialize)]
pub struct DeterminismResult {
    pub status: PhaseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mismatches: Vec<EvidenceMismatch>,
}

/// Evidence mismatch detail
#[derive(Debug, Clone, Serialize)]
pub struct EvidenceMismatch {
    pub file: String,
    pub summary: String,
}

/// Evidence bundle for determinism comparison
#[derive(Debug, Clone)]
struct EvidenceBundle {
    /// Canonicalized JSON content by filename
    contents: BTreeMap<String, String>,
}

/// Ephemeral fields to strip from JSON before comparison
const EPHEMERAL_FIELD_PATTERNS: &[&str] = &[
    "timestamp",
    "timestamp_utc",
    "generated_at",
    "metadata.timestamp",
];

/// Run the gate command.
pub fn run(args: GateArgs) -> Result<()> {
    let num_runs = args.runs.unwrap_or(if args.determinism { 2 } else { 1 });

    if args.determinism && num_runs < 2 {
        bail!("Determinism check requires at least 2 runs (got {})", num_runs);
    }

    if args.determinism {
        run_with_determinism(&args, num_runs)
    } else {
        run_single(&args, None)
    }
}

/// Run a single gate pass (lint → run → verify)
fn run_single(args: &GateArgs, artifact_root: Option<&PathBuf>) -> Result<()> {
    let temp_dir;
    let artifacts_dir = match artifact_root {
        Some(dir) => dir.clone(),
        None => {
            temp_dir = TempDir::new().context("Failed to create temp directory")?;
            temp_dir.path().to_path_buf()
        }
    };

    std::fs::create_dir_all(&artifacts_dir)
        .with_context(|| format!("Failed to create artifacts dir: {}", artifacts_dir.display()))?;

    let resolved_plan_path = artifacts_dir.join("resolved_plan.json");
    let run_report_path = artifacts_dir.join("run_report.json");

    let mut output = GateOutput {
        lint: PhaseResult { status: PhaseStatus::Skipped, reason: None },
        run: PhaseResult { status: PhaseStatus::Skipped, reason: None },
        verify: PhaseResult { status: PhaseStatus::Skipped, reason: None },
        determinism: None,
    };

    // Phase 1: Lint
    if !args.json {
        eprintln!("[1/3] Running lint...");
    }

    match run_lint(args, &resolved_plan_path) {
        Ok(()) => {
            output.lint = PhaseResult { status: PhaseStatus::Pass, reason: None };
            if !args.json {
                eprintln!("  lint: PASS");
            }
        }
        Err(e) => {
            output.lint = PhaseResult {
                status: PhaseStatus::Fail,
                reason: Some(format!("{}", e)),
            };
            if args.json {
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                eprintln!("  lint: FAIL - {}", e);
            }
            bail!("Gate failed at lint phase");
        }
    }

    // Phase 2: Run (invoke adapter)
    if !args.json {
        eprintln!("[2/3] Running adapter execution...");
    }

    match run_adapter(args, &resolved_plan_path, &run_report_path) {
        Ok(()) => {
            output.run = PhaseResult { status: PhaseStatus::Pass, reason: None };
            if !args.json {
                eprintln!("  run: PASS");
            }
        }
        Err(e) => {
            output.run = PhaseResult {
                status: PhaseStatus::Fail,
                reason: Some(format!("{}", e)),
            };
            if args.json {
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                eprintln!("  run: FAIL - {}", e);
            }
            bail!("Gate failed at run phase");
        }
    }

    // Phase 3: Verify
    if !args.json {
        eprintln!("[3/3] Running verify...");
    }

    match run_verify(args, &run_report_path) {
        Ok(()) => {
            output.verify = PhaseResult { status: PhaseStatus::Pass, reason: None };
            if !args.json {
                eprintln!("  verify: PASS");
            }
        }
        Err(e) => {
            output.verify = PhaseResult {
                status: PhaseStatus::Fail,
                reason: Some(format!("{}", e)),
            };
            if args.json {
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                eprintln!("  verify: FAIL - {}", e);
            }
            bail!("Gate failed at verify phase");
        }
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        eprintln!("\n=== All gate checks passed ===");
    }

    Ok(())
}

/// Run the gate with determinism checking
fn run_with_determinism(args: &GateArgs, num_runs: usize) -> Result<()> {
    let temp_root = TempDir::new().context("Failed to create temp directory for determinism")?;

    let mut bundles: Vec<EvidenceBundle> = Vec::with_capacity(num_runs);

    for run_num in 1..=num_runs {
        if !args.json {
            eprintln!("\n=== Determinism run {}/{} ===", run_num, num_runs);
        }

        let run_dir = temp_root.path().join(format!("run_{}", run_num));
        std::fs::create_dir_all(&run_dir)?;

        // Run the full gate for this iteration
        run_single_for_determinism(args, &run_dir)?;

        // Collect evidence bundle
        let bundle = collect_evidence_bundle(&run_dir)?;
        bundles.push(bundle);
    }

    // Compare bundles
    if !args.json {
        eprintln!("\n=== Comparing evidence bundles ===");
    }

    let mismatches = compare_bundles(&bundles)?;

    let determinism_result = if mismatches.is_empty() {
        if !args.json {
            eprintln!("  determinism: PASS");
        }
        DeterminismResult {
            status: PhaseStatus::Pass,
            reason: None,
            mismatches: vec![],
        }
    } else {
        if !args.json {
            eprintln!("  determinism: FAIL");
            eprintln!("\nMismatched files:");
            for m in &mismatches {
                eprintln!("  - {}: {}", m.file, m.summary);
            }
        }
        DeterminismResult {
            status: PhaseStatus::Fail,
            reason: Some(format!("{} file(s) differ between runs", mismatches.len())),
            mismatches: mismatches.clone(),
        }
    };

    if args.json {
        let output = GateOutput {
            lint: PhaseResult { status: PhaseStatus::Pass, reason: None },
            run: PhaseResult { status: PhaseStatus::Pass, reason: None },
            verify: PhaseResult { status: PhaseStatus::Pass, reason: None },
            determinism: Some(determinism_result.clone()),
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
    }

    if determinism_result.status == PhaseStatus::Fail {
        bail!("Determinism check failed");
    }

    if !args.json {
        eprintln!("\n=== All gate checks passed (including determinism) ===");
    }

    Ok(())
}

/// Run a single gate pass for determinism checking (doesn't print final summary)
fn run_single_for_determinism(args: &GateArgs, artifact_root: &PathBuf) -> Result<()> {
    let resolved_plan_path = artifact_root.join("resolved_plan.json");
    let run_report_path = artifact_root.join("run_report.json");
    let status_path = artifact_root.join("status.json");
    let review_path = artifact_root.join("review.json");

    // Phase 1: Lint
    if !args.json {
        eprintln!("[1/3] Running lint...");
    }
    run_lint(args, &resolved_plan_path)?;
    if !args.json {
        eprintln!("  lint: PASS");
    }

    // Phase 2: Run
    if !args.json {
        eprintln!("[2/3] Running adapter execution...");
    }
    run_adapter(args, &resolved_plan_path, &run_report_path)?;
    if !args.json {
        eprintln!("  run: PASS");
    }

    // Phase 3: Verify
    if !args.json {
        eprintln!("[3/3] Running verify...");
    }
    run_verify(args, &run_report_path)?;
    if !args.json {
        eprintln!("  verify: PASS");
    }

    // Generate status.json for determinism evidence
    generate_status(args, &run_report_path, &status_path)?;

    // Generate review.json for determinism evidence
    generate_review(args, &review_path)?;

    Ok(())
}

// ============================================================================
// Phase implementations (call existing logic or invoke adapter process)
// ============================================================================

/// Run the lint phase
fn run_lint(args: &GateArgs, output_path: &PathBuf) -> Result<()> {
    // Discover feature files
    let features_dir = args.specs_dir.join("features");
    if !features_dir.exists() {
        bail!("Features directory not found: {}", features_dir.display());
    }

    let feature_paths = discover_features(&features_dir)?;
    if feature_paths.is_empty() {
        bail!("No .feature files found in {}", features_dir.display());
    }

    // Read all feature files
    let features = read_features(&args.specs_dir, &feature_paths)?;

    // Fetch adapter manifest
    let registry = fetch_adapter_manifest(&args.adapter_cmd)?;

    // Build resolution engine
    let engine = ResolutionEngine::new(&registry)
        .map_err(|errs| anyhow::anyhow!("Failed to build resolution engine: {:?}", errs))?;

    // Resolve features
    let feature_refs: Vec<(&str, &str)> = features
        .iter()
        .map(|(path, content)| (path.as_str(), content.as_str()))
        .collect();
    let result = engine.resolve(feature_refs.into_iter());

    // Check for errors
    if !result.errors.is_empty() {
        bail!("Resolution failed with {} error(s)", result.errors.len());
    }

    // Check for orphan bindings (v1.5 hard error)
    if !result.orphan_bindings.is_empty() {
        bail!("{} orphan binding(s) detected", result.orphan_bindings.len());
    }

    let plan = result.plan.expect("No errors but no plan - this is a bug");

    // Write resolved_plan.json
    let json = serde_json::to_string_pretty(&plan)
        .context("Failed to serialize resolved plan")?;
    std::fs::write(output_path, &json)
        .with_context(|| format!("Failed to write {}", output_path.display()))?;

    Ok(())
}

/// Run the adapter (execute resolved plan)
fn run_adapter(args: &GateArgs, plan_path: &PathBuf, output_path: &PathBuf) -> Result<()> {
    // Split command into program and args
    let parts: Vec<&str> = args.adapter_cmd.split_whitespace().collect();
    if parts.is_empty() {
        bail!("Empty adapter command");
    }

    let program = parts[0];
    let cmd_args: Vec<&str> = parts[1..].to_vec();

    // Execute: adapter_cmd run --plan <path> --output <path>
    let output = Command::new(program)
        .args(&cmd_args)
        .arg("run")
        .arg("--plan")
        .arg(plan_path)
        .arg("--output")
        .arg(output_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("Failed to execute adapter: {}", args.adapter_cmd))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Adapter run failed (exit {}): {}", output.status.code().unwrap_or(-1), stderr);
    }

    // Check for failed scenarios in run_report
    let report_json = std::fs::read_to_string(output_path)
        .with_context(|| format!("Failed to read run report: {}", output_path.display()))?;
    let report: RunReport = serde_json::from_str(&report_json)
        .context("Failed to parse run report JSON")?;

    let failed_count = report.scenarios.iter()
        .filter(|s| s.status != ScenarioStatus::Passed)
        .count();

    if failed_count > 0 {
        bail!("{} scenario(s) failed", failed_count);
    }

    Ok(())
}

/// Run the verify phase
fn run_verify(args: &GateArgs, run_report_path: &PathBuf) -> Result<()> {
    // Read run report
    let run_report_json = std::fs::read_to_string(run_report_path)
        .with_context(|| format!("Failed to read run report: {}", run_report_path.display()))?;
    let run_report: RunReport = serde_json::from_str(&run_report_json)
        .context("Failed to parse run report JSON")?;

    // Read certification baseline
    let cert_path = args.specs_dir.join(&args.certification);
    let cert_json = std::fs::read_to_string(&cert_path)
        .with_context(|| format!("Failed to read certification: {}", cert_path.display()))?;
    let certification: Certification = serde_json::from_str(&cert_json)
        .context("Failed to parse certification JSON")?;

    // Recompute identity from current sources
    let recomputed = recompute_identity(args)?;

    // Compare hash_contract_version
    if run_report.header.hash_contract_version != HASH_CONTRACT_VERSION {
        bail!("hash_contract_version mismatch");
    }

    // Compare against run report (freshness check)
    if run_report.header.feature_fingerprint_hash != recomputed.feature_fingerprint_hash {
        bail!("Stale artifact: feature_fingerprint_hash mismatch");
    }
    if run_report.header.step_registry_hash != recomputed.step_registry_hash {
        bail!("Stale artifact: step_registry_hash mismatch");
    }
    if run_report.header.resolved_plan_hash != recomputed.resolved_plan_hash {
        bail!("Stale artifact: resolved_plan_hash mismatch");
    }

    // Compare against certification baseline
    if certification.identity.feature_fingerprint_hash != recomputed.feature_fingerprint_hash {
        bail!("Baseline drift: feature_fingerprint_hash");
    }
    if certification.identity.step_registry_hash != recomputed.step_registry_hash {
        bail!("Baseline drift: step_registry_hash");
    }
    if certification.identity.resolved_plan_hash != recomputed.resolved_plan_hash {
        bail!("Baseline drift: resolved_plan_hash");
    }

    Ok(())
}

/// Generate status.json for determinism evidence
fn generate_status(args: &GateArgs, run_report_path: &PathBuf, output_path: &PathBuf) -> Result<()> {
    let status_args = StatusArgs {
        specs_dir: args.specs_dir.clone(),
        adapter_cmd: args.adapter_cmd.clone(),
        certification: args.certification.clone(),
        run_report: run_report_path.clone(),
        out: Some(output_path.clone()),
        json: true,
        verbose: false,
    };
    status::run(status_args)
}

/// Generate review.json for determinism evidence
fn generate_review(args: &GateArgs, output_path: &PathBuf) -> Result<()> {
    let review_args = ReviewArgs {
        specs_dir: args.specs_dir.clone(),
        adapter_cmd: args.adapter_cmd.clone(),
        out: output_path.clone(),
        top: 25,
        include_deferred: true,
        verbose: false,
    };
    review::run(review_args)
}

// ============================================================================
// Evidence collection and comparison for determinism
// ============================================================================

/// Collect evidence bundle from a run directory
fn collect_evidence_bundle(run_dir: &PathBuf) -> Result<EvidenceBundle> {
    let mut contents = BTreeMap::new();

    // Evidence files to collect
    let evidence_files = ["resolved_plan.json", "run_report.json", "status.json", "review.json"];

    for filename in &evidence_files {
        let path = run_dir.join(filename);
        if path.exists() {
            let raw = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read evidence file: {}", path.display()))?;

            // Canonicalize JSON (strip ephemeral fields, sort keys)
            let canonical = canonicalize_json(&raw)?;
            contents.insert(filename.to_string(), canonical);
        }
    }

    Ok(EvidenceBundle { contents })
}

/// Canonicalize JSON: parse, strip ephemeral fields, re-serialize with sorted keys
fn canonicalize_json(raw: &str) -> Result<String> {
    let mut value: Value = serde_json::from_str(raw)
        .context("Failed to parse JSON for canonicalization")?;

    strip_ephemeral_fields(&mut value);

    // Serialize with sorted keys (serde_json sorts keys for BTreeMap-backed values,
    // but Value uses arbitrary ordering - we need to convert)
    let canonical = serialize_canonical(&value)?;

    Ok(canonical)
}

/// Strip ephemeral fields from a JSON value recursively
fn strip_ephemeral_fields(value: &mut Value) {
    match value {
        Value::Object(map) => {
            // Remove ephemeral fields
            let keys_to_remove: Vec<String> = map.keys()
                .filter(|k| is_ephemeral_key(k))
                .cloned()
                .collect();

            for key in keys_to_remove {
                map.remove(&key);
            }

            // Recurse into remaining values
            for (_, v) in map.iter_mut() {
                strip_ephemeral_fields(v);
            }
        }
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                strip_ephemeral_fields(item);
            }
        }
        _ => {}
    }
}

/// Check if a key is ephemeral (should be stripped for comparison)
fn is_ephemeral_key(key: &str) -> bool {
    EPHEMERAL_FIELD_PATTERNS.iter().any(|pattern| {
        // Simple matching: exact match or suffix match for nested patterns
        key == *pattern || key.ends_with(pattern)
    })
}

/// Serialize JSON value with sorted keys for deterministic comparison
fn serialize_canonical(value: &Value) -> Result<String> {
    // Convert Value to a BTreeMap-based structure for sorted keys
    let sorted = sort_value(value);
    serde_json::to_string_pretty(&sorted).context("Failed to serialize canonical JSON")
}

/// Recursively convert a Value to use sorted keys
fn sort_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted: BTreeMap<String, Value> = map.iter()
                .map(|(k, v)| (k.clone(), sort_value(v)))
                .collect();
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(arr) => {
            Value::Array(arr.iter().map(sort_value).collect())
        }
        other => other.clone(),
    }
}

/// Compare multiple evidence bundles
fn compare_bundles(bundles: &[EvidenceBundle]) -> Result<Vec<EvidenceMismatch>> {
    if bundles.len() < 2 {
        return Ok(vec![]);
    }

    let reference = &bundles[0];
    let mut mismatches = Vec::new();

    for (run_idx, bundle) in bundles.iter().enumerate().skip(1) {
        // Check files in reference
        for (filename, ref_content) in &reference.contents {
            match bundle.contents.get(filename) {
                Some(other_content) => {
                    if ref_content != other_content {
                        // Find diff summary
                        let summary = summarize_json_diff(ref_content, other_content);
                        mismatches.push(EvidenceMismatch {
                            file: format!("{} (run 1 vs run {})", filename, run_idx + 1),
                            summary,
                        });
                    }
                }
                None => {
                    mismatches.push(EvidenceMismatch {
                        file: filename.clone(),
                        summary: format!("Missing in run {}", run_idx + 1),
                    });
                }
            }
        }

        // Check for extra files in other bundle
        for filename in bundle.contents.keys() {
            if !reference.contents.contains_key(filename) {
                mismatches.push(EvidenceMismatch {
                    file: filename.clone(),
                    summary: format!("Extra file in run {}", run_idx + 1),
                });
            }
        }
    }

    Ok(mismatches)
}

/// Summarize differences between two JSON strings
fn summarize_json_diff(a: &str, b: &str) -> String {
    let a_val: Result<Value, _> = serde_json::from_str(a);
    let b_val: Result<Value, _> = serde_json::from_str(b);

    match (a_val, b_val) {
        (Ok(Value::Object(a_map)), Ok(Value::Object(b_map))) => {
            let mut diff_keys = Vec::new();

            // Find keys that differ
            for (key, a_v) in &a_map {
                match b_map.get(key) {
                    Some(b_v) if a_v != b_v => diff_keys.push(key.clone()),
                    None => diff_keys.push(format!("-{}", key)),
                    _ => {}
                }
            }

            for key in b_map.keys() {
                if !a_map.contains_key(key) {
                    diff_keys.push(format!("+{}", key));
                }
            }

            if diff_keys.is_empty() {
                "Content differs (nested)".to_string()
            } else {
                format!("Differing keys: {}", diff_keys.join(", "))
            }
        }
        _ => "Content differs".to_string(),
    }
}

// ============================================================================
// Helper functions (duplicated from lint.rs/verify.rs for self-containment)
// ============================================================================

fn discover_features(dir: &std::path::Path) -> Result<Vec<PathBuf>> {
    use walkdir::WalkDir;

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

fn read_features(specs_dir: &std::path::Path, paths: &[PathBuf]) -> Result<Vec<(String, String)>> {
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
        bail!("Adapter manifest failed: {}", stderr);
    }

    let stdout = String::from_utf8(output.stdout)
        .context("Adapter output is not valid UTF-8")?;

    serde_json::from_str(&stdout).context("Failed to parse adapter manifest JSON")
}

fn recompute_identity(args: &GateArgs) -> Result<CertificationIdentity> {
    let features_dir = args.specs_dir.join("features");
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
        bail!("Resolution failed during verify: {:?}", result.errors);
    }

    let plan = result.plan.expect("No errors but no plan");

    Ok(CertificationIdentity {
        hash_contract_version: HASH_CONTRACT_VERSION.to_string(),
        feature_fingerprint_hash: plan.header.feature_fingerprint_hash.clone(),
        step_registry_hash: plan.header.step_registry_hash.clone(),
        resolved_plan_hash: plan.header.resolved_plan_hash.clone(),
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_ephemeral_key() {
        assert!(is_ephemeral_key("timestamp"));
        assert!(is_ephemeral_key("timestamp_utc"));
        assert!(is_ephemeral_key("generated_at"));
        assert!(is_ephemeral_key("metadata.timestamp"));
        assert!(!is_ephemeral_key("name"));
        assert!(!is_ephemeral_key("status"));
        assert!(!is_ephemeral_key("hash"));
    }

    #[test]
    fn test_strip_ephemeral_fields() {
        let mut value: Value = serde_json::from_str(r#"{
            "name": "test",
            "timestamp": "2026-01-19T00:00:00Z",
            "nested": {
                "generated_at": "2026-01-19T00:00:00Z",
                "data": "value"
            }
        }"#).unwrap();

        strip_ephemeral_fields(&mut value);

        let expected: Value = serde_json::from_str(r#"{
            "name": "test",
            "nested": {
                "data": "value"
            }
        }"#).unwrap();

        // Compare after sorting
        assert_eq!(sort_value(&value), sort_value(&expected));
    }

    #[test]
    fn test_canonicalize_json_ignores_timestamps() {
        let json1 = r#"{"name": "test", "timestamp": "2026-01-19T00:00:00Z"}"#;
        let json2 = r#"{"name": "test", "timestamp": "2026-01-20T12:34:56Z"}"#;

        let canonical1 = canonicalize_json(json1).unwrap();
        let canonical2 = canonicalize_json(json2).unwrap();

        assert_eq!(canonical1, canonical2);
    }

    #[test]
    fn test_canonicalize_json_detects_semantic_diff() {
        let json1 = r#"{"name": "test", "value": 1}"#;
        let json2 = r#"{"name": "test", "value": 2}"#;

        let canonical1 = canonicalize_json(json1).unwrap();
        let canonical2 = canonicalize_json(json2).unwrap();

        assert_ne!(canonical1, canonical2);
    }

    #[test]
    fn test_json_key_ordering_is_stable() {
        // Keys in different order should produce same canonical form
        let json1 = r#"{"b": 2, "a": 1, "c": 3}"#;
        let json2 = r#"{"a": 1, "c": 3, "b": 2}"#;

        let canonical1 = canonicalize_json(json1).unwrap();
        let canonical2 = canonicalize_json(json2).unwrap();

        assert_eq!(canonical1, canonical2);
    }

    #[test]
    fn test_compare_bundles_identical() {
        let bundle1 = EvidenceBundle {
            contents: [("test.json".to_string(), r#"{"a":1}"#.to_string())].into_iter().collect(),
        };
        let bundle2 = EvidenceBundle {
            contents: [("test.json".to_string(), r#"{"a":1}"#.to_string())].into_iter().collect(),
        };

        let mismatches = compare_bundles(&[bundle1, bundle2]).unwrap();
        assert!(mismatches.is_empty());
    }

    #[test]
    fn test_compare_bundles_different() {
        let bundle1 = EvidenceBundle {
            contents: [("test.json".to_string(), r#"{"a":1}"#.to_string())].into_iter().collect(),
        };
        let bundle2 = EvidenceBundle {
            contents: [("test.json".to_string(), r#"{"a":2}"#.to_string())].into_iter().collect(),
        };

        let mismatches = compare_bundles(&[bundle1, bundle2]).unwrap();
        assert_eq!(mismatches.len(), 1);
        assert!(mismatches[0].file.contains("test.json"));
    }

    #[test]
    fn test_evidence_bundle_includes_all_required_files() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let run_dir = temp_dir.path().to_path_buf();

        // Create all evidence files
        fs::write(run_dir.join("resolved_plan.json"), r#"{"test": 1}"#).unwrap();
        fs::write(run_dir.join("run_report.json"), r#"{"test": 2}"#).unwrap();
        fs::write(run_dir.join("status.json"), r#"{"test": 3}"#).unwrap();
        fs::write(run_dir.join("review.json"), r#"{"test": 4}"#).unwrap();

        let bundle = collect_evidence_bundle(&run_dir).unwrap();

        // Verify all four files are included
        assert!(bundle.contents.contains_key("resolved_plan.json"));
        assert!(bundle.contents.contains_key("run_report.json"));
        assert!(bundle.contents.contains_key("status.json"));
        assert!(bundle.contents.contains_key("review.json"));
        assert_eq!(bundle.contents.len(), 4);
    }
}
