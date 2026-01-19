//! `namako review` command implementation.
//!
//! This command generates a deterministic "work backlog packet" that converts
//! the `.feature` corpus into an AI-executable backlog.
//!
//! Per TODO.md §2, this enables Tesaki to prioritize scenario promotion.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use clap::Args;
use gherkin::{Feature, GherkinEnv, Rule, Scenario};
use serde::Serialize;
use walkdir::WalkDir;

use namako::engine::ResolutionEngine;
use namako::npap::{SemanticStepRegistry, HASH_CONTRACT_VERSION};

/// Arguments for the review command.
#[derive(Args, Debug)]
pub struct ReviewArgs {
    /// Path to the specs directory containing features/.
    #[arg(short, long, default_value = ".")]
    pub specs_dir: PathBuf,

    /// Adapter command to fetch manifest.
    #[arg(short = 'a', long)]
    pub adapter_cmd: String,

    /// Output path for review JSON file (required).
    #[arg(long)]
    pub out: PathBuf,

    /// Maximum number of promotion candidates to include.
    #[arg(long, default_value = "25")]
    pub top: usize,

    /// Include deferred items in output.
    #[arg(long, default_value = "true")]
    pub include_deferred: bool,

    /// Print verbose output.
    #[arg(short, long, default_value = "false")]
    pub verbose: bool,
}

/// Review output schema per TODO.md §2.2
#[derive(Debug, Clone, Serialize)]
pub struct ReviewOutput {
    /// Schema version
    pub version: u32,
    /// Spec root path
    pub spec_root: String,
    /// Current identity
    pub identity_current: IdentityCurrent,
    /// Features sorted by feature_path
    pub features: Vec<FeatureReview>,
    /// Coverage summary
    pub coverage_summary: CoverageSummary,
    /// Promotion candidates (ranked, deterministic)
    pub promotion_candidates: Vec<PromotionCandidate>,
    /// Missing bindings for top candidates
    pub missing_bindings_for_top_candidates: Vec<MissingBindingInfo>,
    /// Suggested binding bundle per TODO.md §3.1
    /// When all candidates require new bindings (reuse_score=0), this tells exactly what to implement
    pub suggested_binding_bundle: SuggestedBindingBundle,
}

/// Current identity fields
#[derive(Debug, Clone, Serialize)]
pub struct IdentityCurrent {
    pub hash_contract_version: String,
    pub feature_fingerprint_hash: String,
    pub step_registry_hash: String,
    pub resolved_plan_hash: String,
}

/// Feature review info
#[derive(Debug, Clone, Serialize)]
pub struct FeatureReview {
    pub feature_path: String,
    pub feature_name: String,
    pub rules: Vec<RuleReview>,
}

/// Rule review info
#[derive(Debug, Clone, Serialize)]
pub struct RuleReview {
    pub rule_name: String,
    pub source_span: SourceSpan,
    pub executable_scenarios: Vec<ScenarioReview>,
    pub deferred_items: Vec<DeferredItem>,
}

/// Source span in file
#[derive(Debug, Clone, Serialize)]
pub struct SourceSpan {
    pub start_line: u32,
    pub end_line: u32,
}

/// Scenario review info
#[derive(Debug, Clone, Serialize)]
pub struct ScenarioReview {
    pub name: String,
    pub source_span: SourceSpan,
    pub steps: Vec<StepInfo>,
}

/// Step info
#[derive(Debug, Clone, Serialize)]
pub struct StepInfo {
    pub kind: String,
    pub text: String,
}

/// Blocker classification for deferred scenarios.
///
/// This indicates what type of work is required to unblock a deferred scenario:
/// - `HARNESS_ONLY`: Can be unblocked with test harness changes only (no core changes)
/// - `CORE`: Requires changes to the core Naia codebase
/// - `EXTERNAL`: Requires external dependencies or changes outside the repo
/// - `UNKNOWN`: No blocker annotation found, classification needed
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BlockerType {
    HarnessOnly,
    Core,
    External,
    #[default]
    Unknown,
}

/// Deferred item from DEFERRED TESTS section or @Deferred tag
#[derive(Debug, Clone, Serialize)]
pub struct DeferredItem {
    pub text: String,
    pub source_span: SourceSpan,
    /// Blocker classification (from @Blocker tag or default UNKNOWN)
    pub blocker: BlockerType,
}

/// Coverage summary
#[derive(Debug, Clone, Serialize)]
pub struct CoverageSummary {
    pub rules_total: u32,
    pub rules_with_zero_executable: u32,
    pub executable_scenarios_total: u32,
    pub deferred_items_total: u32,
}

/// Promotion candidate
#[derive(Debug, Clone, Serialize)]
pub struct PromotionCandidate {
    pub feature_path: String,
    pub rule_name: String,
    pub scenario_name: String,
    pub steps: Vec<StepInfo>,
    pub new_step_texts_estimate: u32,
    pub reuse_score: u32,
    /// Blocker classification (from @Blocker tag or default UNKNOWN)
    pub blocker: BlockerType,
}

/// Missing binding info
#[derive(Debug, Clone, Serialize)]
pub struct MissingBindingInfo {
    pub candidate_name: String,
    pub missing_step_texts: Vec<String>,
}

/// Suggested binding bundle per TODO.md §3.1
/// Computed from top N promotion candidates - tells Tesaki exactly what to implement
#[derive(Debug, Clone, Serialize)]
pub struct SuggestedBindingBundle {
    /// List of step bindings to implement, ranked by frequency
    pub steps: Vec<BundleStepInfo>,
    /// Rationale explaining the bundle
    pub rationale: String,
}

/// Step info for the binding bundle
#[derive(Debug, Clone, Serialize)]
pub struct BundleStepInfo {
    /// Step kind: Given, When, Then
    pub kind: String,
    /// Step text (normalized)
    pub text: String,
    /// Number of candidates that need this step
    pub frequency: u32,
}

/// Run the review command.
pub fn run(args: ReviewArgs) -> Result<()> {
    let output = compute_review(&args)?;

    let json = serde_json::to_string_pretty(&output)
        .context("Failed to serialize review output")?;

    // Ensure parent directory exists
    if let Some(parent) = args.out.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    std::fs::write(&args.out, &json)
        .with_context(|| format!("Failed to write review to {}", args.out.display()))?;

    if args.verbose {
        eprintln!("✓ Review written to: {}", args.out.display());
        eprintln!("  Features: {}", output.features.len());
        eprintln!("  Executable scenarios: {}", output.coverage_summary.executable_scenarios_total);
        eprintln!("  Deferred items: {}", output.coverage_summary.deferred_items_total);
        eprintln!("  Promotion candidates: {}", output.promotion_candidates.len());
    }

    Ok(())
}

fn compute_review(args: &ReviewArgs) -> Result<ReviewOutput> {
    let spec_root = args.specs_dir.canonicalize()
        .unwrap_or_else(|_| args.specs_dir.clone())
        .to_string_lossy()
        .to_string();

    // Discover and read feature files
    let features_dir = args.specs_dir.join("features");
    if !features_dir.exists() {
        bail!("Features directory not found: {}", features_dir.display());
    }

    let feature_paths = discover_features(&features_dir)?;
    let features = read_features(&args.specs_dir, &feature_paths)?;

    // Fetch adapter manifest for binding info
    let registry = fetch_adapter_manifest(&args.adapter_cmd)?;

    // Build resolution engine for identity computation
    let engine = ResolutionEngine::new(&registry)
        .map_err(|errs| anyhow::anyhow!("Failed to build engine: {:?}", errs))?;

    let feature_refs: Vec<(&str, &str)> = features
        .iter()
        .map(|(path, content)| (path.as_str(), content.as_str()))
        .collect();

    // Try to resolve to get identity (may fail with missing steps, that's OK)
    let resolution_result = engine.resolve(feature_refs.clone().into_iter());
    let identity_current = if let Some(ref plan) = resolution_result.plan {
        IdentityCurrent {
            hash_contract_version: HASH_CONTRACT_VERSION.to_string(),
            feature_fingerprint_hash: plan.header.feature_fingerprint_hash.clone(),
            step_registry_hash: plan.header.step_registry_hash.clone(),
            resolved_plan_hash: plan.header.resolved_plan_hash.clone(),
        }
    } else {
        // Compute partial identity
        let feature_hash = namako::npap::compute_feature_fingerprint(feature_refs.into_iter());
        IdentityCurrent {
            hash_contract_version: HASH_CONTRACT_VERSION.to_string(),
            feature_fingerprint_hash: feature_hash,
            step_registry_hash: registry.step_registry_hash.clone(),
            resolved_plan_hash: "UNRESOLVED".to_string(),
        }
    };

    // Build set of existing step expressions for reuse detection
    let existing_expressions: BTreeSet<(String, String)> = registry.bindings.iter()
        .map(|b| (b.kind.clone(), b.expression.clone()))
        .collect();

    // Parse all features and extract review info
    let mut feature_reviews = Vec::new();
    let mut all_deferred_items = Vec::new();
    let mut promotion_candidates = Vec::new();

    for (path, source) in &features {
        let env = GherkinEnv::default();
        match Feature::parse(source, env) {
            Ok(feature) => {
                let (review, deferred, candidates) = analyze_feature(
                    path,
                    &feature,
                    source,
                    &existing_expressions,
                    args.include_deferred,
                );
                feature_reviews.push(review);
                all_deferred_items.extend(deferred);
                promotion_candidates.extend(candidates);
            }
            Err(e) => {
                if args.verbose {
                    eprintln!("Warning: Failed to parse {}: {}", path, e);
                }
            }
        }
    }

    // Sort features by path
    feature_reviews.sort_by(|a, b| a.feature_path.cmp(&b.feature_path));

    // Compute coverage summary
    let rules_total: u32 = feature_reviews.iter()
        .map(|f| f.rules.len() as u32)
        .sum();
    let rules_with_zero_executable: u32 = feature_reviews.iter()
        .flat_map(|f| &f.rules)
        .filter(|r| r.executable_scenarios.is_empty())
        .count() as u32;
    let executable_scenarios_total: u32 = feature_reviews.iter()
        .flat_map(|f| &f.rules)
        .map(|r| r.executable_scenarios.len() as u32)
        .sum();
    let deferred_items_total: u32 = all_deferred_items.len() as u32;

    let coverage_summary = CoverageSummary {
        rules_total,
        rules_with_zero_executable,
        executable_scenarios_total,
        deferred_items_total,
    };

    // Sort and limit promotion candidates
    // Ranking: highest reuse_score first, then lowest new_step_texts_estimate
    // Tie-breaker: feature_path, rule_name, scenario_name
    promotion_candidates.sort_by(|a, b| {
        b.reuse_score.cmp(&a.reuse_score)
            .then_with(|| a.new_step_texts_estimate.cmp(&b.new_step_texts_estimate))
            .then_with(|| a.feature_path.cmp(&b.feature_path))
            .then_with(|| a.rule_name.cmp(&b.rule_name))
            .then_with(|| a.scenario_name.cmp(&b.scenario_name))
    });
    promotion_candidates.truncate(args.top);

    // Compute missing bindings for top 5 candidates
    let missing_bindings = compute_missing_bindings(
        &promotion_candidates[..std::cmp::min(5, promotion_candidates.len())],
        &existing_expressions,
    );

    // Compute suggested binding bundle per TODO.md §3.1
    let suggested_binding_bundle = compute_binding_bundle(
        &promotion_candidates[..std::cmp::min(10, promotion_candidates.len())],
        &existing_expressions,
    );

    Ok(ReviewOutput {
        version: 1,
        spec_root,
        identity_current,
        features: feature_reviews,
        coverage_summary,
        promotion_candidates,
        missing_bindings_for_top_candidates: missing_bindings,
        suggested_binding_bundle,
    })
}

fn analyze_feature(
    path: &str,
    feature: &Feature,
    source: &str,
    existing_expressions: &BTreeSet<(String, String)>,
    include_deferred: bool,
) -> (FeatureReview, Vec<DeferredItem>, Vec<PromotionCandidate>) {
    let lines: Vec<&str> = source.lines().collect();
    let mut rules = Vec::new();
    let mut deferred_items = Vec::new();
    let mut promotion_candidates = Vec::new();

    // Check if feature has rules or just scenarios at the top level
    if feature.rules.is_empty() {
        // Top-level scenarios go into a synthetic "default" rule
        let scenarios: Vec<ScenarioReview> = feature.scenarios.iter()
            .filter(|s| !is_deferred_scenario(s, &lines))
            .map(|s| scenario_to_review(s, &lines))
            .collect();

        let deferred = if include_deferred {
            extract_deferred_from_feature(source)
        } else {
            vec![]
        };

        // Create promotion candidates from deferred scenarios
        for scenario in &feature.scenarios {
            if is_deferred_scenario(scenario, &lines) {
                let steps = scenario_steps_to_info(scenario, &lines);
                let (reuse_score, new_count) = compute_reuse_metrics(&steps, existing_expressions);
                let blocker = extract_blocker_type(scenario);
                promotion_candidates.push(PromotionCandidate {
                    feature_path: path.to_string(),
                    rule_name: "default".to_string(),
                    scenario_name: scenario.name.clone(),
                    steps,
                    new_step_texts_estimate: new_count,
                    reuse_score,
                    blocker,
                });
            }
        }

        let feature_line = feature.position.line as u32;
        rules.push(RuleReview {
            rule_name: "default".to_string(),
            source_span: SourceSpan {
                start_line: feature_line,
                end_line: lines.len() as u32,
            },
            executable_scenarios: scenarios,
            deferred_items: deferred.clone(),
        });
        deferred_items.extend(deferred);
    } else {
        // Process each rule
        for rule in &feature.rules {
            let (rule_review, rule_deferred, rule_candidates) = analyze_rule(
                path,
                rule,
                &lines,
                source,
                existing_expressions,
                include_deferred,
            );
            rules.push(rule_review);
            deferred_items.extend(rule_deferred);
            promotion_candidates.extend(rule_candidates);
        }
    }

    let review = FeatureReview {
        feature_path: path.to_string(),
        feature_name: feature.name.clone(),
        rules,
    };

    (review, deferred_items, promotion_candidates)
}

fn analyze_rule(
    path: &str,
    rule: &Rule,
    lines: &[&str],
    source: &str,
    existing_expressions: &BTreeSet<(String, String)>,
    include_deferred: bool,
) -> (RuleReview, Vec<DeferredItem>, Vec<PromotionCandidate>) {
    let scenarios: Vec<ScenarioReview> = rule.scenarios.iter()
        .filter(|s| !is_deferred_scenario(s, lines))
        .map(|s| scenario_to_review(s, lines))
        .collect();

    let deferred = if include_deferred {
        extract_deferred_from_rule(rule, source)
    } else {
        vec![]
    };

    let mut promotion_candidates = Vec::new();
    for scenario in &rule.scenarios {
        if is_deferred_scenario(scenario, lines) {
            let steps = scenario_steps_to_info(scenario, lines);
            let (reuse_score, new_count) = compute_reuse_metrics(&steps, existing_expressions);
            let blocker = extract_blocker_type(scenario);
            promotion_candidates.push(PromotionCandidate {
                feature_path: path.to_string(),
                rule_name: rule.name.clone(),
                scenario_name: scenario.name.clone(),
                steps,
                new_step_texts_estimate: new_count,
                reuse_score,
                blocker,
            });
        }
    }

    let rule_line = rule.position.line as u32;
    let end_line = estimate_rule_end(rule, lines);

    let rule_review = RuleReview {
        rule_name: rule.name.clone(),
        source_span: SourceSpan {
            start_line: rule_line,
            end_line,
        },
        executable_scenarios: scenarios,
        deferred_items: deferred.clone(),
    };

    (rule_review, deferred, promotion_candidates)
}

fn is_deferred_scenario(scenario: &Scenario, lines: &[&str]) -> bool {
    // Check for @deferred tag or scenario inside DEFERRED section
    for tag in &scenario.tags {
        if tag.to_lowercase() == "deferred" || tag.to_lowercase() == "@deferred" {
            return true;
        }
    }

    // Check if there's a "# DEFERRED" comment above
    let line_idx = scenario.position.line.saturating_sub(2);
    if line_idx < lines.len() {
        let prev_line = lines[line_idx].trim().to_uppercase();
        if prev_line.contains("DEFERRED") {
            return true;
        }
    }

    false
}

/// Extract blocker type from scenario tags.
///
/// Looks for tags like `@Blocker(CORE)`, `@Blocker(HARNESS_ONLY)`, or `@Blocker(EXTERNAL)`.
/// Returns `Unknown` if no blocker tag is found.
fn extract_blocker_type(scenario: &Scenario) -> BlockerType {
    for tag in &scenario.tags {
        let tag_lower = tag.to_lowercase();
        // Handle both @Blocker(TYPE) and Blocker(TYPE) formats
        if tag_lower.starts_with("blocker(") || tag_lower.starts_with("@blocker(") {
            let start = tag_lower.find('(').unwrap_or(0) + 1;
            let end = tag_lower.find(')').unwrap_or(tag_lower.len());
            let blocker_value = tag_lower[start..end].trim();
            return match blocker_value {
                "harness_only" | "harness-only" => BlockerType::HarnessOnly,
                "core" => BlockerType::Core,
                "external" => BlockerType::External,
                _ => BlockerType::Unknown,
            };
        }
    }
    BlockerType::Unknown
}

fn scenario_to_review(scenario: &Scenario, lines: &[&str]) -> ScenarioReview {
    let start_line = scenario.position.line as u32;
    let end_line = estimate_scenario_end(scenario, lines);

    ScenarioReview {
        name: scenario.name.clone(),
        source_span: SourceSpan { start_line, end_line },
        steps: scenario_steps_to_info(scenario, lines),
    }
}

fn scenario_steps_to_info(scenario: &Scenario, _lines: &[&str]) -> Vec<StepInfo> {
    let mut last_keyword = "Given";
    scenario.steps.iter().map(|step| {
        let kind = normalize_step_kind(&step.keyword, &mut last_keyword);
        StepInfo {
            kind: kind.to_string(),
            text: step.value.clone(),
        }
    }).collect()
}

fn normalize_step_kind<'a>(keyword: &str, last_keyword: &mut &'a str) -> &'a str {
    let kw = keyword.trim();
    match kw {
        "Given" => { *last_keyword = "Given"; "Given" }
        "When" => { *last_keyword = "When"; "When" }
        "Then" => { *last_keyword = "Then"; "Then" }
        "And" | "But" | "*" => *last_keyword,
        _ => *last_keyword,
    }
}

fn estimate_scenario_end(scenario: &Scenario, lines: &[&str]) -> u32 {
    if scenario.steps.is_empty() {
        return scenario.position.line as u32;
    }

    let last_step = scenario.steps.last().unwrap();
    last_step.position.line as u32
}

fn estimate_rule_end(rule: &Rule, lines: &[&str]) -> u32 {
    if let Some(last_scenario) = rule.scenarios.last() {
        estimate_scenario_end(last_scenario, lines)
    } else {
        rule.position.line as u32
    }
}

fn extract_deferred_from_feature(source: &str) -> Vec<DeferredItem> {
    extract_deferred_section(source)
}

fn extract_deferred_from_rule(_rule: &Rule, source: &str) -> Vec<DeferredItem> {
    // For now, extract from the whole source - in practice you'd limit to rule span
    extract_deferred_section(source)
}

fn extract_deferred_section(source: &str) -> Vec<DeferredItem> {
    let mut items = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut in_deferred = false;
    let mut current_item_start = 0u32;
    let mut current_item_lines = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let line_num = (i + 1) as u32;

        // Check for DEFERRED TESTS section header
        if trimmed.to_uppercase().contains("DEFERRED TESTS")
            || trimmed.to_uppercase().contains("# DEFERRED") {
            in_deferred = true;
            continue;
        }

        // Check for end of deferred section (next rule, next scenario, etc.)
        if in_deferred && (trimmed.starts_with("Rule:")
            || trimmed.starts_with("Scenario:")
            || trimmed.starts_with("Feature:")
            || trimmed.starts_with("@")) {
            // Flush current item
            if !current_item_lines.is_empty() {
                items.push(DeferredItem {
                    text: current_item_lines.join("\n"),
                    source_span: SourceSpan {
                        start_line: current_item_start,
                        end_line: line_num - 1,
                    },
                    blocker: BlockerType::Unknown,
                });
                current_item_lines.clear();
            }
            in_deferred = false;
            continue;
        }

        if in_deferred && !trimmed.is_empty() && !trimmed.starts_with('#') {
            // This is a deferred item line
            if current_item_lines.is_empty() {
                current_item_start = line_num;
            }
            current_item_lines.push(trimmed.to_string());
        } else if in_deferred && trimmed.is_empty() && !current_item_lines.is_empty() {
            // End of current item
            items.push(DeferredItem {
                text: current_item_lines.join("\n"),
                source_span: SourceSpan {
                    start_line: current_item_start,
                    end_line: line_num - 1,
                },
                blocker: BlockerType::Unknown,
            });
            current_item_lines.clear();
        }
    }

    // Flush final item
    if !current_item_lines.is_empty() {
        items.push(DeferredItem {
            text: current_item_lines.join("\n"),
            source_span: SourceSpan {
                start_line: current_item_start,
                end_line: lines.len() as u32,
            },
            blocker: BlockerType::Unknown,
        });
    }

    items
}

fn compute_reuse_metrics(
    steps: &[StepInfo],
    existing_expressions: &BTreeSet<(String, String)>,
) -> (u32, u32) {
    let mut reuse_score = 0u32;
    let mut new_count = 0u32;

    for step in steps {
        // Check if this step text matches any existing expression exactly
        // (simplified check - just look for exact text match)
        let matches = existing_expressions.iter().any(|(kind, expr)| {
            kind == &step.kind && (expr == &step.text || expr.contains(&step.text) || step.text.contains(expr))
        });

        if matches {
            reuse_score += 1;
        } else {
            new_count += 1;
        }
    }

    (reuse_score, new_count)
}

fn compute_missing_bindings(
    candidates: &[PromotionCandidate],
    existing_expressions: &BTreeSet<(String, String)>,
) -> Vec<MissingBindingInfo> {
    candidates.iter().map(|candidate| {
        let missing: Vec<String> = candidate.steps.iter()
            .filter(|step| {
                !existing_expressions.iter().any(|(kind, expr)| {
                    kind == &step.kind && (expr == &step.text || expr.contains(&step.text))
                })
            })
            .map(|step| format!("{} {}", step.kind, step.text))
            .collect();

        MissingBindingInfo {
            candidate_name: candidate.scenario_name.clone(),
            missing_step_texts: missing,
        }
    }).collect()
}

/// Compute a suggested binding bundle from top N promotion candidates.
/// Per TODO.md §3.1, this ensures Tesaki never stops on reuse_score=0.
fn compute_binding_bundle(
    candidates: &[PromotionCandidate],
    existing_expressions: &BTreeSet<(String, String)>,
) -> SuggestedBindingBundle {
    // Count frequency of each missing step across all candidates
    let mut step_frequency: BTreeMap<(String, String), u32> = BTreeMap::new();

    for candidate in candidates {
        for step in &candidate.steps {
            let is_missing = !existing_expressions.iter().any(|(kind, expr)| {
                kind == &step.kind && (expr == &step.text || expr.contains(&step.text))
            });

            if is_missing {
                let key = (step.kind.clone(), step.text.clone());
                *step_frequency.entry(key).or_insert(0) += 1;
            }
        }
    }

    // Sort by frequency (descending), then by kind (Given < When < Then), then by text
    let mut steps: Vec<_> = step_frequency.into_iter().collect();
    steps.sort_by(|a, b| {
        b.1.cmp(&a.1) // frequency desc
            .then_with(|| {
                let kind_order = |k: &str| match k {
                    "Given" => 0,
                    "When" => 1,
                    "Then" => 2,
                    _ => 3,
                };
                kind_order(&a.0.0).cmp(&kind_order(&b.0.0))
            })
            .then_with(|| a.0.1.cmp(&b.0.1)) // text asc
    });

    let total_steps = steps.len();
    let top_candidates_count = candidates.len();

    let bundle_steps: Vec<BundleStepInfo> = steps
        .into_iter()
        .take(15) // Limit to top 15 most common missing steps
        .map(|((kind, text), frequency)| BundleStepInfo { kind, text, frequency })
        .collect();

    // Generate rationale
    let rationale = if bundle_steps.is_empty() {
        "All steps in top promotion candidates have existing bindings.".to_string()
    } else {
        format!(
            "To promote the top {} scenarios, implement these {} step bindings (most common first). \
             Implementing the highest-frequency steps will unblock the most scenarios.",
            top_candidates_count,
            total_steps
        )
    };

    SuggestedBindingBundle {
        steps: bundle_steps,
        rationale,
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
    fn test_normalize_step_kind() {
        let mut last = "Given";
        assert_eq!(normalize_step_kind("Given", &mut last), "Given");
        assert_eq!(normalize_step_kind("And", &mut last), "Given");
        assert_eq!(normalize_step_kind("When", &mut last), "When");
        assert_eq!(normalize_step_kind("And", &mut last), "When");
        assert_eq!(normalize_step_kind("Then", &mut last), "Then");
        assert_eq!(normalize_step_kind("But", &mut last), "Then");
    }

    #[test]
    fn test_extract_deferred_section() {
        let source = r#"
Feature: Test
  Rule: Some rule
    Scenario: Active
      Given something

    # DEFERRED TESTS
    - Test item 1
    - Test item 2
"#;
        let items = extract_deferred_section(source);
        assert!(!items.is_empty());
    }
}
