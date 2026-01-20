//! `namako explain` command implementation.
//!
//! This command generates a deterministic "scenario fidelity packet" that provides
//! detailed context about a specific scenario for LLM-based review.
//!
//! Per TODO.md §3, this enables Tesaki to review the "spirit of spec".

use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use clap::Args;
use gherkin::{Feature, GherkinEnv};
use serde::Serialize;
use walkdir::WalkDir;

use namako::engine::ResolutionEngine;
use namako::npap::{ResolvedPlan, SemanticStepRegistry, HASH_CONTRACT_VERSION};

/// Arguments for the explain command.
#[derive(Args, Debug)]
pub struct ExplainArgs {
    /// Path to the specs directory containing features/.
    #[arg(short, long, default_value = ".")]
    pub specs_dir: PathBuf,

    /// Adapter command to fetch manifest.
    #[arg(short = 'a', long)]
    pub adapter_cmd: String,

    /// Scenario key selector in format "path:L<line>" (e.g., "features/smoke.feature:L15").
    #[arg(long)]
    pub scenario_key: String,

    /// Output path for explain JSON file (required).
    #[arg(long)]
    pub out: PathBuf,

    /// Print verbose output.
    #[arg(short, long, default_value = "false")]
    pub verbose: bool,
}

/// Explain output schema per GOLD_PLAN §10.5.4
#[derive(Debug, Clone, Serialize)]
pub struct ExplainOutput {
    /// Schema version
    pub version: u32,
    /// Scenario key (v1.5 format: feature_id:Rule_nn:Scenario_nn)
    pub scenario_key: String,
    /// Scenario name
    pub scenario_name: String,
    /// Feature file path
    pub feature_path: String,
    /// Rule name (null for feature-level scenarios)
    pub rule_name: Option<String>,
    /// Rule description (from Rule: line)
    pub rule_description: Option<String>,
    /// Steps with binding metadata
    pub steps: Vec<ExplainStep>,
    /// Combined tags from feature, rule, and scenario
    pub related_tags: Vec<String>,
    /// Contract excerpt (rule header + description + scenario header)
    pub contract_excerpt: String,
    /// Notes and limitations (for backward compat)
    pub notes: Notes,
}

/// Step with binding metadata per GOLD_PLAN §10.5.4
#[derive(Debug, Clone, Serialize)]
pub struct ExplainStep {
    /// Step kind: Given, When, Then
    pub step_kind: String,
    /// Actual step text from feature
    pub step_text: String,
    /// Binding ID
    pub binding_id: String,
    /// Binding expression (cucumber expression)
    pub binding_expression: String,
    /// Implementation hash
    pub impl_hash: String,
    /// Source location as "path/to/file.rs:123"
    pub source_location: String,
}

/// Scenario info (kept for internal use)
#[derive(Debug, Clone, Serialize)]
pub struct ScenarioInfo {
    pub scenario_key: String,
    pub feature_path: String,
    pub feature_name: String,
    pub rule_name: Option<String>,
    pub scenario_name: String,
    pub source_span: SourceSpan,
    pub steps: Vec<StepInfo>,
}

/// Source span
#[derive(Debug, Clone, Serialize)]
pub struct SourceSpan {
    pub start_line: u32,
    pub end_line: u32,
}

/// Step info
#[derive(Debug, Clone, Serialize)]
pub struct StepInfo {
    pub kind: String,
    pub text: String,
}

/// Contract context
#[derive(Debug, Clone, Serialize)]
pub struct ContractContext {
    pub normative_excerpt: Vec<NormativeExcerpt>,
}

/// Normative excerpt
#[derive(Debug, Clone, Serialize)]
pub struct NormativeExcerpt {
    pub heading: String,
    pub text: String,
}

/// Resolution info
#[derive(Debug, Clone, Serialize)]
pub struct ResolutionInfo {
    pub binding_resolutions: Vec<BindingResolution>,
}

/// Binding resolution info
#[derive(Debug, Clone, Serialize)]
pub struct BindingResolution {
    pub kind: String,
    pub step_text: String,
    pub binding_id: String,
    pub impl_hash: String,
    pub binding_source: BindingSource,
}

/// Binding source location
#[derive(Debug, Clone, Serialize)]
pub struct BindingSource {
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
}

/// Notes and limitations
#[derive(Debug, Clone, Serialize)]
pub struct Notes {
    pub limitations: Vec<String>,
}

/// Run the explain command.
pub fn run(args: ExplainArgs) -> Result<()> {
    let output = compute_explain(&args)?;

    let json = serde_json::to_string_pretty(&output)
        .context("Failed to serialize explain output")?;

    // Ensure parent directory exists
    if let Some(parent) = args.out.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    std::fs::write(&args.out, &json)
        .with_context(|| format!("Failed to write explain to {}", args.out.display()))?;

    if args.verbose {
        eprintln!("✓ Explain written to: {}", args.out.display());
        eprintln!("  Scenario: {}", output.scenario_name);
        eprintln!("  Steps: {}", output.steps.len());
    }

    Ok(())
}

fn compute_explain(args: &ExplainArgs) -> Result<ExplainOutput> {
    // Parse scenario_key to get feature path and line
    let (feature_rel_path, target_line) = parse_scenario_key(&args.scenario_key)?;

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

    // Resolve all features
    let feature_refs: Vec<(&str, &str)> = features
        .iter()
        .map(|(path, content)| (path.as_str(), content.as_str()))
        .collect();
    let result = engine.resolve(feature_refs.into_iter());

    if result.plan.is_none() {
        bail!("Resolution failed: {:?}", result.errors);
    }

    let plan = result.plan.unwrap();

    // Find the matching scenario
    let resolved_scenario = plan.scenarios.iter()
        .find(|s| s.scenario_key == args.scenario_key)
        .ok_or_else(|| anyhow::anyhow!(
            "Scenario key '{}' not found in resolved plan. Available keys: {}",
            args.scenario_key,
            plan.scenarios.iter().map(|s| s.scenario_key.as_str()).collect::<Vec<_>>().join(", ")
        ))?;

    // Find the feature file content
    let (_, feature_source) = features.iter()
        .find(|(path, _)| path == &feature_rel_path)
        .ok_or_else(|| anyhow::anyhow!("Feature file not found: {}", feature_rel_path))?;

    // Parse the feature to get additional context
    let env = GherkinEnv::default();
    let feature = Feature::parse(feature_source, env)
        .map_err(|e| anyhow::anyhow!("Failed to parse feature: {:?}", e))?;

    // Extract scenario info from parsed feature
    let scenario_info = extract_scenario_info(
        &feature,
        &feature_rel_path,
        target_line,
        &args.scenario_key,
    )?;

    // Build ExplainStep list with binding metadata
    let explain_steps = build_explain_steps(resolved_scenario, &registry)?;

    // Extract tags from feature, rule, and scenario
    let related_tags = extract_related_tags(&feature, &scenario_info);

    // Build contract excerpt (Rule header + description + Scenario header)
    let contract_excerpt = build_contract_excerpt(feature_source, &scenario_info);

    Ok(ExplainOutput {
        version: 1,
        scenario_key: args.scenario_key.clone(),
        scenario_name: scenario_info.scenario_name.clone(),
        feature_path: feature_rel_path.clone(),
        rule_name: scenario_info.rule_name.clone(),
        rule_description: extract_rule_description(&feature, &scenario_info.rule_name),
        steps: explain_steps,
        related_tags,
        contract_excerpt,
        notes: Notes {
            limitations: vec![
                "Call graph not computed in v1".to_string(),
                "Binding source lines are best-effort estimates".to_string(),
            ],
        },
    })
}

fn parse_scenario_key(key: &str) -> Result<(String, u32)> {
    // Format: "features/path.feature:L<line>"
    let parts: Vec<&str> = key.rsplitn(2, ":L").collect();
    if parts.len() != 2 {
        bail!("Invalid scenario key format: '{}'. Expected 'path:L<line>'", key);
    }

    let line: u32 = parts[0].parse()
        .with_context(|| format!("Invalid line number in scenario key: {}", parts[0]))?;
    let path = parts[1].to_string();

    Ok((path, line))
}

fn extract_scenario_info(
    feature: &Feature,
    feature_path: &str,
    target_line: u32,
    scenario_key: &str,
) -> Result<ScenarioInfo> {
    // Search in top-level scenarios
    for scenario in &feature.scenarios {
        if scenario.position.line as u32 == target_line {
            let steps: Vec<StepInfo> = scenario.steps.iter().map(|s| {
                StepInfo {
                    kind: normalize_step_keyword(&s.keyword),
                    text: s.value.clone(),
                }
            }).collect();

            let end_line = if let Some(last) = scenario.steps.last() {
                last.position.line as u32
            } else {
                target_line
            };

            return Ok(ScenarioInfo {
                scenario_key: scenario_key.to_string(),
                feature_path: feature_path.to_string(),
                feature_name: feature.name.clone(),
                rule_name: None,
                scenario_name: scenario.name.clone(),
                source_span: SourceSpan {
                    start_line: target_line,
                    end_line,
                },
                steps,
            });
        }
    }

    // Search in rules
    for rule in &feature.rules {
        for scenario in &rule.scenarios {
            if scenario.position.line as u32 == target_line {
                let steps: Vec<StepInfo> = scenario.steps.iter().map(|s| {
                    StepInfo {
                        kind: normalize_step_keyword(&s.keyword),
                        text: s.value.clone(),
                    }
                }).collect();

                let end_line = if let Some(last) = scenario.steps.last() {
                    last.position.line as u32
                } else {
                    target_line
                };

                return Ok(ScenarioInfo {
                    scenario_key: scenario_key.to_string(),
                    feature_path: feature_path.to_string(),
                    feature_name: feature.name.clone(),
                    rule_name: Some(rule.name.clone()),
                    scenario_name: scenario.name.clone(),
                    source_span: SourceSpan {
                        start_line: target_line,
                        end_line,
                    },
                    steps,
                });
            }
        }
    }

    bail!("Scenario not found at line {} in {}", target_line, feature_path);
}

fn normalize_step_keyword(keyword: &str) -> String {
    let kw = keyword.trim();
    match kw {
        "Given" | "When" | "Then" => kw.to_string(),
        "And" | "But" | "*" => kw.to_string(), // Keep original for context
        _ => kw.to_string(),
    }
}

fn extract_contract_context(source: &str) -> ContractContext {
    let mut excerpts = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    let mut current_heading = String::new();
    let mut current_text = Vec::new();
    let mut in_contract_section = false;

    for line in &lines {
        let trimmed = line.trim();

        // Look for NORMATIVE CONTRACT MIRROR or similar headings
        if trimmed.contains("NORMATIVE CONTRACT")
            || trimmed.contains("CONTRACT MIRROR")
            || trimmed.starts_with("### ")
            || trimmed.starts_with("## ") {

            // Save previous section if exists
            if !current_heading.is_empty() && !current_text.is_empty() {
                excerpts.push(NormativeExcerpt {
                    heading: current_heading.clone(),
                    text: current_text.join("\n"),
                });
            }

            current_heading = trimmed.trim_start_matches('#').trim().to_string();
            current_text.clear();
            in_contract_section = trimmed.contains("NORMATIVE") || trimmed.contains("CONTRACT");
            continue;
        }

        // Collect text in contract sections
        if in_contract_section && !trimmed.is_empty()
            && !trimmed.starts_with("Feature:")
            && !trimmed.starts_with("Scenario:")
            && !trimmed.starts_with("Rule:")
            && !trimmed.starts_with("Given ")
            && !trimmed.starts_with("When ")
            && !trimmed.starts_with("Then ")
            && !trimmed.starts_with("And ")
            && !trimmed.starts_with("But ") {
            current_text.push(trimmed.to_string());
        }

        // Stop at next major section
        if trimmed.starts_with("Rule:") || trimmed.starts_with("Scenario:") {
            in_contract_section = false;
        }
    }

    // Save final section
    if !current_heading.is_empty() && !current_text.is_empty() {
        excerpts.push(NormativeExcerpt {
            heading: current_heading,
            text: current_text.join("\n"),
        });
    }

    // If no excerpts found, include a summary
    if excerpts.is_empty() {
        // Extract feature description as fallback
        let desc_lines: Vec<&str> = lines.iter()
            .skip_while(|l| !l.trim().starts_with("Feature:"))
            .skip(1)
            .take_while(|l| !l.trim().starts_with("Rule:") && !l.trim().starts_with("Scenario:"))
            .filter(|l| !l.trim().is_empty() && !l.trim().starts_with("#") && !l.trim().starts_with("@"))
            .copied()
            .collect();

        if !desc_lines.is_empty() {
            excerpts.push(NormativeExcerpt {
                heading: "Feature Description".to_string(),
                text: desc_lines.join("\n").trim().to_string(),
            });
        }
    }

    ContractContext {
        normative_excerpt: excerpts,
    }
}

fn build_resolution_info(
    scenario: &namako::npap::ResolvedScenario,
    registry: &SemanticStepRegistry,
) -> Result<ResolutionInfo> {
    let mut binding_resolutions = Vec::new();

    for step in &scenario.steps {
        // Find the binding in registry
        let binding = registry.bindings.iter()
            .find(|b| b.binding_id == step.binding_id);

        let (impl_hash, binding_source) = if let Some(b) = binding {
            (
                b.impl_hash.clone(),
                BindingSource {
                    path: format!("naia/test/tests/src/steps/*.rs"),  // Best effort
                    start_line: 0,  // Would need source mapping
                    end_line: 0,
                },
            )
        } else {
            (
                "UNKNOWN".to_string(),
                BindingSource {
                    path: "UNKNOWN".to_string(),
                    start_line: 0,
                    end_line: 0,
                },
            )
        };

        binding_resolutions.push(BindingResolution {
            kind: step.effective_kind.clone(),
            step_text: step.step_text.clone(),
            binding_id: step.binding_id.clone(),
            impl_hash,
            binding_source,
        });
    }

    Ok(ResolutionInfo { binding_resolutions })
}

/// Build ExplainStep list with binding metadata per GOLD_PLAN §10.5.4.
fn build_explain_steps(
    scenario: &namako::npap::ResolvedScenario,
    registry: &SemanticStepRegistry,
) -> Result<Vec<ExplainStep>> {
    let mut steps = Vec::new();

    for step in &scenario.steps {
        // Find the binding in the registry
        let binding = registry.bindings.iter()
            .find(|b| b.binding_id == step.binding_id);

        let (expression, impl_hash, source_symbol) = if let Some(b) = binding {
            (
                b.expression.clone(),
                b.impl_hash.clone(),
                b.source_symbol.clone(),
            )
        } else {
            ("UNKNOWN".to_string(), "UNKNOWN".to_string(), None)
        };

        // Source location: use source_symbol if available (per TODO.md §3),
        // otherwise fall back to binding_id prefix
        let source_location = source_symbol
            .unwrap_or_else(|| format!("binding:{}:0", &step.binding_id[..16.min(step.binding_id.len())]));

        steps.push(ExplainStep {
            step_kind: step.effective_kind.clone(),
            step_text: step.step_text.clone(),
            binding_id: step.binding_id.clone(),
            binding_expression: expression,
            impl_hash,
            source_location,
        });
    }

    Ok(steps)
}

/// Extract related tags from feature, rule, and scenario.
fn extract_related_tags(feature: &Feature, scenario_info: &ScenarioInfo) -> Vec<String> {
    let mut tags = Vec::new();

    // Add feature tags (gherkin 0.15 uses String for tags)
    for tag in &feature.tags {
        if !tag.starts_with('@') {
            tags.push(format!("@{}", tag));
        } else {
            tags.push(tag.clone());
        }
    }

    // Add rule tags if present
    if let Some(rule_name) = &scenario_info.rule_name {
        for rule in &feature.rules {
            if &rule.name == rule_name {
                for tag in &rule.tags {
                    if !tag.starts_with('@') {
                        tags.push(format!("@{}", tag));
                    } else {
                        tags.push(tag.clone());
                    }
                }
                // Also find scenario in this rule
                for scenario in &rule.scenarios {
                    if scenario.name == scenario_info.scenario_name {
                        for tag in &scenario.tags {
                            if !tag.starts_with('@') {
                                tags.push(format!("@{}", tag));
                            } else {
                                tags.push(tag.clone());
                            }
                        }
                        break;
                    }
                }
                break;
            }
        }
    } else {
        // Feature-level scenario
        for scenario in &feature.scenarios {
            if scenario.name == scenario_info.scenario_name {
                for tag in &scenario.tags {
                    if !tag.starts_with('@') {
                        tags.push(format!("@{}", tag));
                    } else {
                        tags.push(tag.clone());
                    }
                }
                break;
            }
        }
    }

    // Sort for determinism
    tags.sort();
    tags.dedup();
    tags
}

/// Extract rule description from feature.
fn extract_rule_description(feature: &Feature, rule_name: &Option<String>) -> Option<String> {
    if let Some(name) = rule_name {
        for rule in &feature.rules {
            if &rule.name == name {
                // Rule description is the rule name in gherkin
                // For additional description, we'd need to parse comments
                return Some(rule.name.clone());
            }
        }
    }
    None
}

/// Build contract excerpt: Rule header + description + Scenario header.
fn build_contract_excerpt(source: &str, scenario_info: &ScenarioInfo) -> String {
    let lines: Vec<&str> = source.lines().collect();

    // Find the scenario start line
    let scenario_start = scenario_info.source_span.start_line as usize;
    if scenario_start == 0 || scenario_start > lines.len() {
        return String::new();
    }

    let mut excerpt_lines = Vec::new();

    // Find rule header if present
    if scenario_info.rule_name.is_some() {
        // Search backwards for Rule: line
        for i in (0..scenario_start).rev() {
            let line = lines.get(i).unwrap_or(&"");
            if line.trim().starts_with("Rule:") {
                // Include rule line and up to 3 lines after (description)
                for j in i..(i + 4).min(scenario_start) {
                    if let Some(l) = lines.get(j) {
                        excerpt_lines.push(*l);
                    }
                }
                excerpt_lines.push(""); // blank line
                break;
            }
        }
    }

    // Include scenario header (up to 5 lines starting from scenario)
    let scenario_end = (scenario_start + 5).min(lines.len());
    for i in (scenario_start - 1)..scenario_end {
        if let Some(l) = lines.get(i) {
            excerpt_lines.push(*l);
        }
    }

    excerpt_lines.join("\n")
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
    fn test_parse_scenario_key() {
        let (path, line) = parse_scenario_key("features/smoke.feature:L15").unwrap();
        assert_eq!(path, "features/smoke.feature");
        assert_eq!(line, 15);
    }

    #[test]
    fn test_parse_scenario_key_with_subdirs() {
        let (path, line) = parse_scenario_key("features/sub/dir/test.feature:L100").unwrap();
        assert_eq!(path, "features/sub/dir/test.feature");
        assert_eq!(line, 100);
    }

    #[test]
    fn test_parse_scenario_key_invalid() {
        assert!(parse_scenario_key("invalid").is_err());
        assert!(parse_scenario_key("features/test.feature").is_err());
        assert!(parse_scenario_key("features/test.feature:abc").is_err());
    }
}
