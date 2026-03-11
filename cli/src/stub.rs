//! `namako stub` command implementation.
//!
//! This command generates placeholder scenarios for orphan bindings.
//! Per GOLD_PLAN §10.5.2, this is the mitigation tool for orphan binding hard errors.
//!
//! Usage:
//! - `namako stub --all`: Generate _orphan_stubs.feature with stubs for all orphans
//! - `namako stub --binding <id> --feature <path>`: Generate stub for a single binding

use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use clap::Args;
use walkdir::WalkDir;

use namako_engine::engine::{OrphanBinding, ResolutionEngine};
use namako_engine::npap::SemanticStepRegistry;

/// Arguments for the stub command.
#[derive(Args, Debug)]
pub struct StubArgs {
    /// Path to the specs directory containing features/.
    #[arg(short, long, default_value = ".")]
    pub specs_dir: PathBuf,

    /// Adapter command to fetch manifest.
    #[arg(short = 'a', long)]
    pub adapter_cmd: String,

    /// Generate stubs for all orphan bindings into _orphan_stubs.feature.
    #[arg(long, conflicts_with = "binding")]
    pub all: bool,

    /// Binding ID to generate a stub for (use with --feature).
    #[arg(long, requires = "feature")]
    pub binding: Option<String>,

    /// Feature file to append the stub to (use with --binding).
    #[arg(long, requires = "binding")]
    pub feature: Option<PathBuf>,

    /// Print verbose output.
    #[arg(short, long, default_value = "false")]
    pub verbose: bool,
}

/// Run the stub command.
pub fn run(args: StubArgs) -> Result<()> {
    if !args.all && args.binding.is_none() {
        bail!("Either --all or --binding must be specified");
    }

    // Discover and read feature files
    let features_dir = args.specs_dir.join("features");
    if !features_dir.exists() {
        bail!("Features directory not found: {}", features_dir.display());
    }

    let feature_paths = discover_features(&features_dir)?;
    let features = read_features(&args.specs_dir, &feature_paths)?;

    // Fetch adapter manifest
    let registry = fetch_adapter_manifest(&args.adapter_cmd)?;

    // Build resolution engine
    let engine = ResolutionEngine::new(&registry)
        .map_err(|errs| anyhow::anyhow!("Failed to build engine: {:?}", errs))?;

    // Resolve to get orphan bindings
    let feature_refs: Vec<(&str, &str)> = features
        .iter()
        .map(|(path, content)| (path.as_str(), content.as_str()))
        .collect();
    let result = engine.resolve(feature_refs.into_iter());

    let orphans = result.orphan_bindings;

    if args.all {
        generate_all_stubs(&args, &orphans, &features_dir)?;
    } else if let Some(ref binding_id) = args.binding {
        generate_single_stub(&args, binding_id, &orphans)?;
    }

    Ok(())
}

/// Generate stubs for all orphan bindings.
fn generate_all_stubs(
    args: &StubArgs,
    orphans: &[OrphanBinding],
    features_dir: &PathBuf,
) -> Result<()> {
    if orphans.is_empty() {
        eprintln!("✓ No orphan bindings found. Nothing to stub.");
        return Ok(());
    }

    // Generate the stub feature file
    let stub_path = features_dir.join("_orphan_stubs.feature");
    let content = generate_stub_feature(orphans);

    std::fs::write(&stub_path, &content)
        .with_context(|| format!("Failed to write {}", stub_path.display()))?;

    eprintln!(
        "✓ Generated {} stub scenario(s) in {}",
        orphans.len(),
        stub_path.display()
    );

    if args.verbose {
        eprintln!("\nStubbed bindings:");
        for orphan in orphans {
            eprintln!("  - {} \"{}\"", orphan.kind, orphan.expression);
        }
    }

    Ok(())
}

/// Generate a stub for a single binding.
fn generate_single_stub(
    args: &StubArgs,
    binding_id: &str,
    orphans: &[OrphanBinding],
) -> Result<()> {
    // Find the orphan binding
    let orphan = orphans
        .iter()
        .find(|o| o.binding_id == binding_id || o.binding_id.starts_with(binding_id))
        .ok_or_else(|| {
            let available: Vec<_> = orphans
                .iter()
                .map(|o| &o.binding_id[..16.min(o.binding_id.len())])
                .collect();
            anyhow::anyhow!(
                "Binding ID '{}' not found in orphans. Available orphans: {:?}",
                binding_id,
                available
            )
        })?;

    let feature_path = args.feature.as_ref().unwrap();

    // Read existing feature file
    let existing = std::fs::read_to_string(feature_path)
        .with_context(|| format!("Failed to read {}", feature_path.display()))?;

    // Generate stub scenario (ensuring a Rule section)
    let stub = generate_single_stub_scenario(orphan);

    // Append to feature file
    let new_content = format!("{}\n\n{}", existing.trim_end(), stub);
    std::fs::write(feature_path, new_content)
        .with_context(|| format!("Failed to write {}", feature_path.display()))?;

    eprintln!(
        "✓ Appended stub scenario for {} \"{}\" to {}",
        orphan.kind,
        orphan.expression,
        feature_path.display()
    );

    Ok(())
}

/// Generate a complete stub feature file for all orphans.
fn generate_stub_feature(orphans: &[OrphanBinding]) -> String {
    let mut lines = Vec::new();

    lines.push("# Auto-generated by `namako stub --all`".to_string());
    lines.push(
        "# Delete this file after implementing real scenarios for these bindings.".to_string(),
    );
    lines.push("#".to_string());
    lines.push("# Per GOLD_PLAN §10.5.2, orphan bindings are a hard error.".to_string());
    lines.push("# These stubs allow lint to pass while you work on proper scenarios.".to_string());
    lines.push(String::new());
    lines.push("@Feature(orphan_stubs)".to_string());
    lines.push("Feature: Orphan Binding Stubs".to_string());
    lines.push(
        "  Placeholder scenarios for bindings not yet used by real specifications.".to_string(),
    );
    lines.push(String::new());
    // Sort orphans for deterministic output
    let mut sorted_orphans: Vec<_> = orphans.iter().collect();
    sorted_orphans.sort_by(|a, b| a.binding_id.cmp(&b.binding_id));

    for (i, orphan) in sorted_orphans.iter().enumerate() {
        let scenario_id = format!("@Scenario({:02})", i + 1);
        let step_text = expression_to_step_text(&orphan.expression);
        let short_id = &orphan.binding_id[..16.min(orphan.binding_id.len())];

        lines.push(format!("    @Deferred @Stub"));
        lines.push(format!("    {}", scenario_id));
        lines.push(format!(
            "    Scenario: Stub for orphan binding {}...",
            short_id
        ));
        lines.push(format!("      # Expression: \"{}\"", orphan.expression));
        lines.push(format!("      {} {}", orphan.kind, step_text));
        lines.push(String::new());
    }

    lines.join("\n")
}

/// Generate a single stub scenario for appending to an existing feature.
fn generate_single_stub_scenario(orphan: &OrphanBinding) -> String {
    let step_text = expression_to_step_text(&orphan.expression);
    let short_id = &orphan.binding_id[..16.min(orphan.binding_id.len())];

    format!(
        r#"    # Auto-generated stub for orphan binding
    @Deferred @Stub
    Scenario: Stub for orphan binding {}...
      # Expression: "{}"
      {} {}"#,
        short_id, orphan.expression, orphan.kind, step_text
    )
}

/// Convert a cucumber expression to a concrete step text.
///
/// Per TODO.md Sprint 2.B:
/// - `{string}` -> `"stub"`
/// - `{int}` -> `0`
/// - `{float}` -> `0.0`
/// - `{word}` -> `stub`
fn expression_to_step_text(expression: &str) -> String {
    let mut result = expression.to_string();

    // Replace parameter placeholders with concrete values
    // Order matters: more specific patterns first

    // {string} -> "stub"
    result = result.replace("{string}", "\"stub\"");

    // {int} -> 0
    result = result.replace("{int}", "0");

    // {float} -> 0.0
    result = result.replace("{float}", "0.0");

    // {word} -> stub
    result = result.replace("{word}", "stub");

    // {bigdecimal} -> 0
    result = result.replace("{bigdecimal}", "0");

    // {double} -> 0.0
    result = result.replace("{double}", "0.0");

    // {byte} -> 0
    result = result.replace("{byte}", "0");

    // {short} -> 0
    result = result.replace("{short}", "0");

    // {long} -> 0
    result = result.replace("{long}", "0");

    // Handle optional groups like (s)? by picking the first option
    // For now, just remove the optional marker
    // This is a simplified approach; complex expressions may need manual intervention

    result
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

    let stdout = String::from_utf8(output.stdout).context("Adapter output is not valid UTF-8")?;

    serde_json::from_str(&stdout).context("Failed to parse adapter manifest JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expression_to_step_text_simple() {
        assert_eq!(
            expression_to_step_text("a server is running"),
            "a server is running"
        );
    }

    #[test]
    fn test_expression_to_step_text_string() {
        assert_eq!(
            expression_to_step_text("a user named {string}"),
            "a user named \"stub\""
        );
    }

    #[test]
    fn test_expression_to_step_text_int() {
        assert_eq!(
            expression_to_step_text("I have {int} apples"),
            "I have 0 apples"
        );
    }

    #[test]
    fn test_expression_to_step_text_multiple() {
        assert_eq!(
            expression_to_step_text("{string} has {int} items"),
            "\"stub\" has 0 items"
        );
    }

    #[test]
    fn test_expression_to_step_text_float() {
        assert_eq!(
            expression_to_step_text("the value is {float}"),
            "the value is 0.0"
        );
    }
}
