//! `namako lint` command implementation.
//!
//! This command:
//! 1. Discovers all `.feature` files in the spec directory
//! 2. Fetches the adapter manifest (semantic step registry)
//! 3. Resolves each step against the registry
//! 4. Outputs `resolved_plan.json`

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use clap::Args;
use walkdir::WalkDir;

use namako_engine::engine::ResolutionEngine;
use namako_engine::npap::SemanticStepRegistry;

/// Arguments for the lint command.
#[derive(Args, Debug)]
pub struct LintArgs {
    /// Path to the specs directory containing features/.
    /// Defaults to current directory.
    #[arg(short, long, default_value = ".")]
    pub specs_dir: PathBuf,

    /// Adapter command to fetch manifest.
    /// Will be invoked as: `<adapter_cmd> manifest`
    #[arg(short, long)]
    pub adapter_cmd: String,

    /// Output path for resolved_plan.json.
    /// Defaults to ./resolved_plan.json
    #[arg(short, long, default_value = "resolved_plan.json")]
    pub output: PathBuf,

    /// Show orphan bindings (bindings not used by any scenario).
    /// Orphans are always a hard error; this flag provides extra detail.
    #[arg(long, default_value = "false")]
    pub show_orphans: bool,

    /// Print verbose output.
    #[arg(short, long, default_value = "false")]
    pub verbose: bool,
}

/// Run the lint command.
pub fn run(args: LintArgs) -> Result<()> {
    if args.verbose {
        eprintln!("Namako lint: starting...");
    }

    // Step 1: Discover feature files
    let features_dir = args.specs_dir.join("features");
    if !features_dir.exists() {
        bail!(
            "Features directory not found: {}. Expected specs_dir/features/",
            features_dir.display()
        );
    }

    let feature_paths = discover_features(&features_dir)?;
    if args.verbose {
        eprintln!("Found {} feature file(s)", feature_paths.len());
    }

    if feature_paths.is_empty() {
        bail!("No .feature files found in {}", features_dir.display());
    }

    // Step 2: Read all feature files (path, content pairs)
    let features = read_features(&args.specs_dir, &feature_paths)?;
    if args.verbose {
        eprintln!("Read {} feature file(s) successfully", features.len());
    }

    // Step 3: Fetch adapter manifest
    let registry = fetch_adapter_manifest(&args.adapter_cmd)?;
    if args.verbose {
        eprintln!(
            "Fetched adapter manifest with {} binding(s)",
            registry.bindings.len()
        );
    }

    // Step 4: Build resolution engine
    let engine = ResolutionEngine::new(&registry)
        .map_err(|errs| anyhow::anyhow!("Failed to build resolution engine: {:?}", errs))?;

    // Step 5: Resolve features - engine expects (relative_path, source) pairs
    let feature_refs: Vec<(&str, &str)> = features
        .iter()
        .map(|(path, content)| (path.as_str(), content.as_str()))
        .collect();
    let result = engine.resolve(feature_refs.into_iter());

    // Step 6: Handle result
    if !result.errors.is_empty() {
        eprintln!("✗ Lint failed with {} error(s):", result.errors.len());
        for (i, err) in result.errors.iter().enumerate() {
            eprintln!("\n  {}. {}", i + 1, err);
        }
        bail!("Resolution failed");
    }

    let plan = result.plan.expect("No errors but no plan - this is a bug");

    // Step 6.5: Check for orphan bindings (v1.5 hard error per GOLD_PLAN §10.5.2)
    if !result.orphan_bindings.is_empty() {
        eprintln!(
            "✗ Lint failed: {} orphan binding(s) detected.",
            result.orphan_bindings.len()
        );
        eprintln!("\nOrphan bindings are bindings in the registry not used by any scenario.");
        eprintln!("This is a hard error in v1.5 per GOLD_PLAN §10.5.2.\n");

        eprintln!("Orphan bindings (sorted by binding_id):");
        let mut orphans: Vec<_> = result.orphan_bindings.iter().collect();
        orphans.sort_by(|a, b| a.binding_id.cmp(&b.binding_id));

        for orphan in &orphans {
            eprintln!(
                "  - {} \"{}\" (id: {}...)",
                orphan.kind,
                orphan.expression,
                &orphan.binding_id[..16.min(orphan.binding_id.len())]
            );
        }

        if args.show_orphans {
            eprintln!("\nFull orphan binding details:");
            for orphan in &orphans {
                eprintln!("  binding_id: {}", orphan.binding_id);
                eprintln!("      kind: {}", orphan.kind);
                eprintln!("      expression: \"{}\"", orphan.expression);
                eprintln!();
            }
        }

        eprintln!("\nTo fix:");
        eprintln!("  1. Use `namako stub --all` to generate placeholder scenarios for orphans");
        eprintln!("  2. Or use `namako stub --binding <id> --feature <path>` for a single orphan");
        eprintln!("  3. Or delete the unused step bindings from your test code");

        bail!("Orphan bindings detected");
    }

    // Write resolved_plan.json
    let json = serde_json::to_string_pretty(&plan).context("Failed to serialize resolved plan")?;
    std::fs::write(&args.output, &json)
        .with_context(|| format!("Failed to write {}", args.output.display()))?;

    eprintln!(
        "✓ Lint passed. Resolved {} scenario(s), {} step(s).",
        plan.scenarios.len(),
        plan.scenarios.iter().map(|s| s.steps.len()).sum::<usize>()
    );
    eprintln!("  Output: {}", args.output.display());

    if !result.warnings.is_empty() {
        eprintln!("\nWarnings:");
        for warning in &result.warnings {
            eprintln!("  - {}", warning);
        }
    }

    Ok(())
}

/// Discover all `.feature` files under the given directory.
fn discover_features(dir: &Path) -> Result<Vec<PathBuf>> {
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

    // Sort for deterministic ordering
    paths.sort();
    Ok(paths)
}

/// Read feature files and return (relative_path, content) pairs.
/// The relative path is from specs_dir for use as scenario keys.
fn read_features(specs_dir: &Path, paths: &[PathBuf]) -> Result<Vec<(String, String)>> {
    let mut features = Vec::with_capacity(paths.len());

    for path in paths {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        // Compute relative path from specs_dir for scenario_key derivation
        let relative_path = path
            .strip_prefix(specs_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/"); // Normalize path separators

        features.push((relative_path, content));
    }

    Ok(features)
}

/// Fetch the semantic step registry from the adapter.
fn fetch_adapter_manifest(adapter_cmd: &str) -> Result<SemanticStepRegistry> {
    // Split command into program and args
    let parts: Vec<&str> = adapter_cmd.split_whitespace().collect();
    if parts.is_empty() {
        bail!("Empty adapter command");
    }

    let program = parts[0];
    let args: Vec<&str> = parts[1..].to_vec();

    // Execute: adapter_cmd manifest
    let output = Command::new(program)
        .args(&args)
        .arg("manifest")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("Failed to execute adapter command: {}", adapter_cmd))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Adapter command failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            stderr
        );
    }

    let stdout = String::from_utf8(output.stdout).context("Adapter output is not valid UTF-8")?;

    serde_json::from_str(&stdout).context("Failed to parse adapter manifest JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_discover_features() {
        let dir = TempDir::new().unwrap();
        let features_dir = dir.path().join("features");
        fs::create_dir_all(&features_dir).unwrap();

        // Create test feature files
        fs::write(features_dir.join("test.feature"), "Feature: Test").unwrap();
        fs::write(features_dir.join("other.feature"), "Feature: Other").unwrap();
        fs::write(features_dir.join("not_a_feature.txt"), "Hello").unwrap();

        let nested = features_dir.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("nested.feature"), "Feature: Nested").unwrap();

        let paths = discover_features(&features_dir).unwrap();
        assert_eq!(paths.len(), 3);

        // Should be sorted
        let names: Vec<_> = paths
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap())
            .collect();
        assert!(names.contains(&"test.feature"));
        assert!(names.contains(&"other.feature"));
        assert!(names.contains(&"nested.feature"));
    }
}
