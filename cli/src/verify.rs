//! `namako verify` command implementation.
//!
//! This command verifies that a run report matches the current sources.
//! It recomputes all hashes from current sources (features, adapter manifest)
//! and compares them to the stored certification baseline.
//!
//! Per GOLD_PLAN §7.4, verify DOES NOT trust on-disk artifacts - it recomputes.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use clap::Args;
use walkdir::WalkDir;

use namako_engine::engine::ResolutionEngine;
use namako_engine::npap::{
    Certification, CertificationIdentity, RunReport, SemanticStepRegistry,
    ScenarioStatus, HASH_CONTRACT_VERSION,
};

/// Arguments for the verify command.
#[derive(Args, Debug)]
pub struct VerifyArgs {
    /// Path to the specs directory containing features/.
    #[arg(short, long, default_value = ".")]
    pub specs_dir: PathBuf,

    /// Adapter command to fetch manifest.
    #[arg(short, long)]
    pub adapter_cmd: String,

    /// Path to the run_report.json file.
    #[arg(short, long, default_value = "run_report.json")]
    pub run_report: PathBuf,

    /// Path to the certification.json baseline.
    #[arg(short, long, default_value = "certification.json")]
    pub certification: PathBuf,

    /// Print verbose output.
    #[arg(short, long, default_value = "false")]
    pub verbose: bool,
}
/// Run the verify command.
pub fn run(args: VerifyArgs) -> Result<()> {
    if args.verbose {
        eprintln!("Namako verify: starting...");
    }

    // Collect all failure categories (don't short-circuit)
    let mut scenario_failures = Vec::new();
    let mut identity_mismatches = Vec::new();

    // Step 1: Read the run report
    let run_report_json = std::fs::read_to_string(&args.run_report)
        .with_context(|| format!("Failed to read run report: {}", args.run_report.display()))?;
    let run_report: RunReport = serde_json::from_str(&run_report_json)
        .context("Failed to parse run report JSON")?;

    // Step 1.5: Check that all scenarios passed in the run report
    for scenario in &run_report.scenarios {
        if scenario.status != ScenarioStatus::Passed {
            let mut failure_msg = format!("FAILED: {}", scenario.scenario_key);
            for (i, step) in scenario.steps.iter().enumerate() {
                if step.status != namako_engine::npap::StepStatus::Passed {
                    if let Some(ref msg) = step.error_message {
                        failure_msg.push_str(&format!("\n    Step {}: {}", i + 1, msg));
                    } else {
                        failure_msg.push_str(&format!("\n    Step {}: FAILED (no message)", i + 1));
                    }
                }
            }
            scenario_failures.push(failure_msg);
        }
    }

    // Step 2: Read the certification baseline
    let cert_json = std::fs::read_to_string(&args.certification)
        .with_context(|| format!("Failed to read certification: {}", args.certification.display()))?;
    let certification: Certification = serde_json::from_str(&cert_json)
        .context("Failed to parse certification JSON")?;

    // Step 3: Recompute from current sources
    let recomputed = recompute_identity(&args)?;
    if args.verbose {
        eprintln!("Recomputed identity from current sources");
        eprintln!("  feature_fingerprint_hash: {}", recomputed.feature_fingerprint_hash);
        eprintln!("  step_registry_hash: {}", recomputed.step_registry_hash);
        eprintln!("  resolved_plan_hash: {}", recomputed.resolved_plan_hash);
    }

    // Step 4: Compare run_report headers against recomputed identity
    // Check hash_contract_version
    if run_report.header.hash_contract_version != HASH_CONTRACT_VERSION {
        identity_mismatches.push(format!(
            "hash_contract_version mismatch: run_report has '{}', expected '{}'",
            run_report.header.hash_contract_version, HASH_CONTRACT_VERSION
        ));
    }

    // Check feature_fingerprint_hash
    if run_report.header.feature_fingerprint_hash != recomputed.feature_fingerprint_hash {
        identity_mismatches.push(format!(
            "STALE OR DRIFTED ARTIFACT: feature_fingerprint_hash mismatch\n\
             Run report: {}\n\
             Current:    {}",
            run_report.header.feature_fingerprint_hash, recomputed.feature_fingerprint_hash
        ));
    }

    // Check step_registry_hash
    if run_report.header.step_registry_hash != recomputed.step_registry_hash {
        identity_mismatches.push(format!(
            "STALE OR DRIFTED ARTIFACT: step_registry_hash mismatch\n\
             Run report: {}\n\
             Current:    {}",
            run_report.header.step_registry_hash, recomputed.step_registry_hash
        ));
    }

    // Check resolved_plan_hash
    if run_report.header.resolved_plan_hash != recomputed.resolved_plan_hash {
        identity_mismatches.push(format!(
            "STALE OR DRIFTED ARTIFACT: resolved_plan_hash mismatch\n\
             Run report: {}\n\
             Current:    {}",
            run_report.header.resolved_plan_hash, recomputed.resolved_plan_hash
        ));
    }

    // Step 5: Compare against certification baseline
    if certification.identity.hash_contract_version != recomputed.hash_contract_version {
        identity_mismatches.push(format!(
            "hash_contract_version mismatch with baseline: '{}' vs '{}'",
            certification.identity.hash_contract_version, recomputed.hash_contract_version
        ));
    }

    if certification.identity.feature_fingerprint_hash != recomputed.feature_fingerprint_hash {
        identity_mismatches.push(format!(
            "BASELINE DRIFT: feature_fingerprint_hash\n\
             Baseline: {}\n\
             Current:  {}",
            certification.identity.feature_fingerprint_hash, recomputed.feature_fingerprint_hash
        ));
    }

    if certification.identity.step_registry_hash != recomputed.step_registry_hash {
        identity_mismatches.push(format!(
            "BASELINE DRIFT: step_registry_hash\n\
             Baseline: {}\n\
             Current:  {}",
            certification.identity.step_registry_hash, recomputed.step_registry_hash
        ));
    }

    if certification.identity.resolved_plan_hash != recomputed.resolved_plan_hash {
        identity_mismatches.push(format!(
            "BASELINE DRIFT: resolved_plan_hash\n\
             Baseline: {}\n\
             Current:  {}",
            certification.identity.resolved_plan_hash, recomputed.resolved_plan_hash
        ));
    }

    // Step 6: Report all failures (scenarios AND identity)
    let has_failures = !scenario_failures.is_empty() || !identity_mismatches.is_empty();

    if has_failures {
        if !scenario_failures.is_empty() {
            eprintln!("✗ SCENARIO FAILURES ({}):", scenario_failures.len());
            for failure in &scenario_failures {
                eprintln!("\n  {}", failure);
            }
        }

        if !identity_mismatches.is_empty() {
            eprintln!("\n✗ IDENTITY MISMATCHES ({}):", identity_mismatches.len());
            for (i, mismatch) in identity_mismatches.iter().enumerate() {
                eprintln!("\n  {}. {}", i + 1, mismatch);
            }
        }

        let total = scenario_failures.len() + identity_mismatches.len();
        bail!("Verification failed with {} issue(s)", total);
    }

    eprintln!("✓ Verification passed. All hashes match current sources and baseline.");
    Ok(())
}

/// Recompute certification identity from current sources.
fn recompute_identity(args: &VerifyArgs) -> Result<CertificationIdentity> {
    // Step 1: Discover and read feature files
    let features_dir = args.specs_dir.join("features");
    if !features_dir.exists() {
        bail!("Features directory not found: {}", features_dir.display());
    }

    let feature_paths = discover_features(&features_dir)?;
    let features = read_features(&args.specs_dir, &feature_paths)?;

    // Step 2: Fetch adapter manifest
    let registry = fetch_adapter_manifest(&args.adapter_cmd)?;

    // Step 3: Resolve plan
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

    // Build identity from recomputed values
    Ok(CertificationIdentity {
        hash_contract_version: HASH_CONTRACT_VERSION.to_string(),
        feature_fingerprint_hash: plan.header.feature_fingerprint_hash.clone(),
        step_registry_hash: plan.header.step_registry_hash.clone(),
        resolved_plan_hash: plan.header.resolved_plan_hash.clone(),
    })
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
