//! `namako update-cert` command implementation.
//!
//! This command updates the certification baseline.
//!
//! Per GOLD_PLAN §7.5, update-cert REFUSES unless:
//! - lint passes (all steps resolve)
//! - run completes with all scenarios Passed

use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use clap::Args;
use walkdir::WalkDir;

use namako::engine::ResolutionEngine;
use namako::npap::{
    Certification, CertificationIdentity, CertificationMetadata,
    RunReport, SemanticStepRegistry, ScenarioStatus,
    HASH_CONTRACT_VERSION, NPAP_VERSION,
};

/// Arguments for the update-cert command.
#[derive(Args, Debug)]
pub struct UpdateCertArgs {
    /// Path to the specs directory containing features/.
    #[arg(short, long, default_value = ".")]
    pub specs_dir: PathBuf,

    /// Adapter command to fetch manifest.
    #[arg(short, long)]
    pub adapter_cmd: String,

    /// Path to the run_report.json file.
    #[arg(short, long, default_value = "run_report.json")]
    pub run_report: PathBuf,

    /// Output path for certification.json.
    #[arg(short, long, default_value = "certification.json")]
    pub output: PathBuf,

    /// Print verbose output.
    #[arg(short, long, default_value = "false")]
    pub verbose: bool,
}

/// Run the update-cert command.
pub fn run(args: UpdateCertArgs) -> Result<()> {
    if args.verbose {
        eprintln!("Namako update-cert: starting...");
    }

    // Step 1: Read the run report
    let run_report_json = std::fs::read_to_string(&args.run_report)
        .with_context(|| format!("Failed to read run report: {}", args.run_report.display()))?;
    let run_report: RunReport = serde_json::from_str(&run_report_json)
        .context("Failed to parse run report JSON")?;

    // Step 2: Check that all scenarios passed
    let failed_scenarios: Vec<_> = run_report.scenarios.iter()
        .filter(|s| s.status != ScenarioStatus::Passed)
        .map(|s| &s.scenario_key)
        .collect();

    if !failed_scenarios.is_empty() {
        bail!(
            "REFUSED: Cannot update certification with failing scenarios:\n  {}",
            failed_scenarios.iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n  ")
        );
    }

    // Step 3: Recompute identity from current sources (verify it matches run_report)
    let recomputed = recompute_identity(&args)?;
    if args.verbose {
        eprintln!("Recomputed identity from current sources");
    }

    // Step 4: Verify run_report matches current sources
    if run_report.header.feature_fingerprint_hash != recomputed.feature_fingerprint_hash {
        bail!(
            "REFUSED: Run report feature_fingerprint_hash does not match current sources.\n\
             Run report: {}\n\
             Current:    {}\n\
             Please re-run `namako lint` and `<adapter> run` with current sources.",
            run_report.header.feature_fingerprint_hash,
            recomputed.feature_fingerprint_hash
        );
    }

    if run_report.header.step_registry_hash != recomputed.step_registry_hash {
        bail!(
            "REFUSED: Run report step_registry_hash does not match current adapter.\n\
             Run report: {}\n\
             Current:    {}\n\
             Please re-run `namako lint` and `<adapter> run` with current adapter.",
            run_report.header.step_registry_hash,
            recomputed.step_registry_hash
        );
    }

    if run_report.header.resolved_plan_hash != recomputed.resolved_plan_hash {
        bail!(
            "REFUSED: Run report resolved_plan_hash does not match current resolution.\n\
             Run report: {}\n\
             Current:    {}\n\
             Please re-run `namako lint` and `<adapter> run` with current sources.",
            run_report.header.resolved_plan_hash,
            recomputed.resolved_plan_hash
        );
    }

    // Step 5: Create certification
    let timestamp = chrono::Utc::now().to_rfc3339();
    let certification = Certification {
        identity: CertificationIdentity {
            hash_contract_version: HASH_CONTRACT_VERSION.to_string(),
            feature_fingerprint_hash: recomputed.feature_fingerprint_hash,
            step_registry_hash: recomputed.step_registry_hash,
            resolved_plan_hash: recomputed.resolved_plan_hash,
        },
        metadata: CertificationMetadata {
            timestamp,
            namako_version: env!("CARGO_PKG_VERSION").to_string(),
            npap_version: NPAP_VERSION,
            run_report_hash: run_report.header.run_report_hash.clone(),
        },
    };

    // Step 6: Write certification
    let json = serde_json::to_string_pretty(&certification)?;
    std::fs::write(&args.output, &json)
        .with_context(|| format!("Failed to write {}", args.output.display()))?;

    eprintln!("✓ Certification updated: {}", args.output.display());
    Ok(())
}

/// Recompute certification identity from current sources.
fn recompute_identity(args: &UpdateCertArgs) -> Result<CertificationIdentity> {
    let features_dir = args.specs_dir.join("features");
    if !features_dir.exists() {
        bail!("Features directory not found: {}", features_dir.display());
    }

    let feature_paths = discover_features(&features_dir)?;
    let features = read_features(&args.specs_dir, &feature_paths)?;
    let registry = fetch_adapter_manifest(&args.adapter_cmd)?;

    let engine = ResolutionEngine::new(&registry)
        .map_err(|errs| anyhow::anyhow!("Lint failed: {:?}", errs))?;

    let feature_refs: Vec<(&str, &str)> = features
        .iter()
        .map(|(path, content)| (path.as_str(), content.as_str()))
        .collect();
    let result = engine.resolve(feature_refs.into_iter());

    if !result.errors.is_empty() {
        bail!("REFUSED: Lint failed with errors: {:?}", result.errors);
    }

    let plan = result.plan.expect("No errors but no plan");

    Ok(CertificationIdentity {
        hash_contract_version: HASH_CONTRACT_VERSION.to_string(),
        feature_fingerprint_hash: plan.header.feature_fingerprint_hash,
        step_registry_hash: plan.header.step_registry_hash,
        resolved_plan_hash: plan.header.resolved_plan_hash,
    })
}

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
        bail!("Adapter command failed: {}", stderr);
    }
    let stdout = String::from_utf8(output.stdout).context("Adapter output not UTF-8")?;
    serde_json::from_str(&stdout).context("Failed to parse adapter manifest")
}
