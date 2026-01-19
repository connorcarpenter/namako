//! Tesaki - AI-friendly task orchestrator for Namako spec-driven development
//!
//! Tesaki is a deterministic task generator that:
//! - Consumes Namako status and review packets
//! - Generates NEXT_TASK.md with specific, actionable instructions
//! - Never modifies source files (only writes to artifact directories)
//! - May run update-cert up to --max-cert-updates times per run (governed by CLI flag)

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Log file name for update-cert audit trail
const UPDATE_CERT_LOG: &str = "update_cert_log.jsonl";

/// Log entry for each update-cert operation (append-only audit log)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateCertLogEntry {
    timestamp_utc: String,
    old_identity: Option<IdentitySnapshot>,
    new_identity: IdentitySnapshot,
    reason: String,
    updates_this_run: u32,
    max_updates_allowed: u32,
}

/// Snapshot of identity hashes for logging
#[derive(Debug, Clone, Serialize, Deserialize)]
struct IdentitySnapshot {
    feature_fingerprint_hash: String,
    step_registry_hash: String,
    resolved_plan_hash: String,
}

/// Tesaki - AI-friendly task orchestrator for Namako
#[derive(Parser)]
#[command(name = "tesaki")]
#[command(about = "AI-friendly task orchestrator for Namako spec-driven development")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate the next task based on current Namako state
    Next {
        /// Path to the specs root directory
        #[arg(short = 's', long)]
        spec_root: PathBuf,

        /// Adapter command (e.g., "cargo run --manifest-path ../npa/Cargo.toml --")
        #[arg(short = 'a', long)]
        adapter: String,

        /// Output directory for artifacts (default: <spec_root>/target/namako_artifacts/tesaki/)
        #[arg(short = 'o', long)]
        out: Option<PathBuf>,

        /// Path to the namako CLI (default: searches in parent workspace)
        #[arg(long)]
        namako_cli: Option<String>,

        /// Maximum number of autonomous update-cert operations per run (0 to disable)
        #[arg(long, default_value = "3", value_parser = clap::value_parser!(u32).range(0..=999))]
        max_cert_updates: u32,

        /// Path to CURRENT_STATUS.md for mode-aware filtering (optional)
        /// If provided, CORE blockers will be filtered in BOOTSTRAP mode.
        #[arg(long)]
        current_status: Option<PathBuf>,
    },
}

/// Status JSON structure from `namako status --json`
#[derive(Debug, Deserialize)]
struct StatusJson {
    recommended_next_action: String,
    #[serde(default)]
    drift: Option<DriftInfo>,
    #[serde(default)]
    last_run_failures: Vec<FailureInfo>,
}

#[derive(Debug, Deserialize, Default)]
struct DriftInfo {
    kind: String,
    #[serde(default)]
    details: Vec<DriftDetail>,
}

#[derive(Debug, Deserialize)]
struct DriftDetail {
    field: String,
    baseline: String,
    current: String,
}

#[derive(Debug, Deserialize, Clone)]
struct FailureInfo {
    scenario_key: String,
    scenario_name: String,
    failure_kind: String,
}

/// Review JSON structure from `namako review`
#[derive(Debug, Deserialize)]
struct ReviewJson {
    coverage_summary: CoverageSummary,
    #[serde(default)]
    promotion_candidates: Vec<PromotionCandidate>,
    #[serde(default)]
    missing_bindings_for_top_candidates: Vec<MissingBindings>,
}

#[derive(Debug, Deserialize)]
struct CoverageSummary {
    executable_scenarios_total: u32,
    deferred_items_total: u32,
}

/// Blocker classification matching namako review output.
/// - HARNESS_ONLY: Can be unblocked with test harness changes only
/// - CORE: Requires changes to the core codebase (blocked in BOOTSTRAP mode)
/// - EXTERNAL: Requires external dependencies
/// - UNKNOWN: No blocker annotation found
#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum BlockerType {
    HarnessOnly,
    Core,
    External,
    Unknown,
}

impl Default for BlockerType {
    fn default() -> Self {
        BlockerType::Unknown
    }
}

#[derive(Debug, Deserialize)]
struct PromotionCandidate {
    scenario_name: String,
    feature_path: String,
    rule_name: String,
    reuse_score: f32,
    new_step_texts_estimate: u32,
    #[serde(default)]
    blocker: BlockerType,
}

#[derive(Debug, Deserialize)]
struct MissingBindings {
    candidate_name: String,
    #[serde(default)]
    missing_step_texts: Vec<String>,
}

/// Operating mode from CURRENT_STATUS.md
#[derive(Debug, Clone, PartialEq)]
enum OperatingMode {
    Bootstrap,
    Consumption,
    Unknown,
}

/// Read the operating mode from CURRENT_STATUS.md.
/// Looks for "MODE: BOOTSTRAP" or "MODE: CONSUMPTION" in the file.
fn read_current_status_mode(status_path: &PathBuf) -> OperatingMode {
    let content = match fs::read_to_string(status_path) {
        Ok(c) => c,
        Err(_) => return OperatingMode::Unknown,
    };

    // Look for MODE: ... in the content
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("MODE:") || trimmed.starts_with("**MODE:**") {
            let mode_str = trimmed
                .trim_start_matches("MODE:")
                .trim_start_matches("**MODE:**")
                .trim();
            return match mode_str.to_uppercase().as_str() {
                "BOOTSTRAP" => OperatingMode::Bootstrap,
                "CONSUMPTION" => OperatingMode::Consumption,
                _ => OperatingMode::Unknown,
            };
        }
    }

    OperatingMode::Unknown
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Next {
            spec_root,
            adapter,
            out,
            namako_cli,
            max_cert_updates,
            current_status,
        } => run_next(&spec_root, &adapter, out, namako_cli, max_cert_updates, current_status),
    }
}

/// Canonicalize any --manifest-path arguments in the adapter command
fn canonicalize_adapter_cmd(adapter: &str) -> Result<String> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let parts: Vec<&str> = adapter.split_whitespace().collect();
    let mut result = Vec::new();
    let mut i = 0;
    while i < parts.len() {
        if parts[i] == "--manifest-path" && i + 1 < parts.len() {
            result.push(parts[i].to_string());
            i += 1;
            let path = PathBuf::from(parts[i]);
            let abs_path = if path.is_absolute() {
                path
            } else {
                cwd.join(&path)
            };
            // Try to canonicalize, or use the absolute path if it doesn't exist yet
            let final_path = fs::canonicalize(&abs_path).unwrap_or(abs_path);
            result.push(final_path.display().to_string());
        } else if parts[i].starts_with("--manifest-path=") {
            let path_str = parts[i].strip_prefix("--manifest-path=").unwrap_or("");
            let path = PathBuf::from(path_str);
            let abs_path = if path.is_absolute() {
                path
            } else {
                cwd.join(&path)
            };
            let final_path = fs::canonicalize(&abs_path).unwrap_or(abs_path);
            result.push(format!("--manifest-path={}", final_path.display()));
        } else {
            result.push(parts[i].to_string());
        }
        i += 1;
    }
    Ok(result.join(" "))
}

fn run_next(
    spec_root: &PathBuf,
    adapter: &str,
    out: Option<PathBuf>,
    namako_cli: Option<String>,
    max_cert_updates: u32,
    current_status: Option<PathBuf>,
) -> Result<()> {
    // Canonicalize spec_root to get absolute path
    let spec_root = fs::canonicalize(spec_root)
        .context("Failed to canonicalize spec_root path")?;

    // Read operating mode from CURRENT_STATUS.md if provided
    let mode = current_status
        .as_ref()
        .map(|p| read_current_status_mode(p))
        .unwrap_or(OperatingMode::Unknown);

    // Canonicalize adapter command paths
    let adapter = canonicalize_adapter_cmd(adapter)?;

    // Determine output directory
    let out_dir = out
        .map(|p| {
            if p.is_absolute() {
                p
            } else {
                std::env::current_dir().unwrap_or_default().join(p)
            }
        })
        .unwrap_or_else(|| spec_root.join("target/namako_artifacts/tesaki"));
    fs::create_dir_all(&out_dir).context("Failed to create output directory")?;

    // Determine namako CLI command
    let namako = namako_cli.unwrap_or_else(|| {
        // Try to find namako-cli in parent workspace
        // spec_root is like .../naia/test/specs, so go up 3 levels and find namako
        let namako_root = spec_root
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .map(|p| p.join("namako"))
            .unwrap_or_else(|| PathBuf::from("/home/ccarpenter/Personal/specops/namako"));
        format!(
            "cargo run -p namako-cli --manifest-path {}/Cargo.toml -q --",
            namako_root.display()
        )
    });

    // Define artifact paths used throughout
    let artifacts_dir = spec_root.join("target/namako_artifacts");
    let run_report_path = artifacts_dir.join("run_report.json");
    let cert_path = spec_root.join("certification.json");
    let log_path = out_dir.join(UPDATE_CERT_LOG);

    // Track update-cert operations for this run
    let mut updates_this_run: u32 = 0;

    eprintln!("=== Tesaki v2 ===");
    eprintln!("Spec root: {}", spec_root.display());
    eprintln!("Output dir: {}", out_dir.display());
    eprintln!("Max cert updates: {}", max_cert_updates);
    eprintln!();

    // Step 1: Run namako status (auto-passes --run-report if file exists per TODO.md §2.1)
    eprintln!("[1/4] Running namako status...");
    let status_path = out_dir.join("status.json");
    let run_report_opt = if run_report_path.exists() {
        Some(&run_report_path)
    } else {
        None
    };
    run_namako_status(&namako, &adapter, &spec_root, &status_path, run_report_opt)?;

    // Parse status
    let status_content = fs::read_to_string(&status_path).context("Failed to read status.json")?;
    let status: StatusJson =
        serde_json::from_str(&status_content).context("Failed to parse status.json")?;

    let mut action = status.recommended_next_action.clone();
    eprintln!("  Action: {}", action);

    // Step 2: Run namako review
    eprintln!("[2/4] Running namako review...");
    let review_path = out_dir.join("review.json");
    run_namako_review(&namako, &adapter, &spec_root, &review_path)?;

    // Parse review
    let review_content = fs::read_to_string(&review_path).context("Failed to read review.json")?;
    let review: ReviewJson =
        serde_json::from_str(&review_content).context("Failed to parse review.json")?;

    eprintln!(
        "  Executable: {} | Deferred: {} | Promotable: {}",
        review.coverage_summary.executable_scenarios_total,
        review.coverage_summary.deferred_items_total,
        review.promotion_candidates.len()
    );

    // Check for CORE blockers in BOOTSTRAP mode
    if mode == OperatingMode::Bootstrap {
        let core_blockers: Vec<_> = review.promotion_candidates.iter()
            .filter(|c| c.blocker == BlockerType::Core)
            .collect();
        if !core_blockers.is_empty() {
            eprintln!(
                "  ⚠️  {} CORE blocker(s) skipped in BOOTSTRAP mode:",
                core_blockers.len()
            );
            for c in &core_blockers {
                eprintln!("      - {} (blocked; wait for CONSUMPTION mode)", c.scenario_name);
            }
        }
    }

    // Step 3: Handle NEEDS_UPDATE_CERT_APPROVAL with --max-cert-updates governance
    let mut update_cert_message: Option<String> = None;
    if action == "NEEDS_UPDATE_CERT_APPROVAL" {
        eprintln!("[3/4] Checking update-cert governance...");

        let remaining = max_cert_updates.saturating_sub(updates_this_run);
        if remaining > 0 {
            eprintln!("  {} updates remaining this run (max: {})", remaining, max_cert_updates);
        } else {
            eprintln!("  No updates remaining this run (max: {})", max_cert_updates);
        }

        if !run_report_path.exists() {
            update_cert_message = Some(
                "Cannot attempt autonomous update: run_report.json not found. Run `namako run` first.".to_string()
            );
        } else if max_cert_updates == 0 {
            eprintln!("  Autonomous updates disabled (--max-cert-updates=0)");
            update_cert_message = Some(
                "Autonomous updates disabled. Use --max-cert-updates=N (N>0) to enable.".to_string()
            );
        } else if updates_this_run >= max_cert_updates {
            eprintln!("  Update limit reached for this run");
            update_cert_message = Some(format!(
                "Update limit reached ({}/{} used this run). Run tesaki again for more updates.",
                updates_this_run, max_cert_updates
            ));
        } else {
            // Read old certification for logging (if exists)
            let old_identity = read_certification_identity(&cert_path);

            // Attempt update-cert
            let result = run_namako_update_cert(
                &namako,
                &adapter,
                &spec_root,
                &run_report_path,
                &cert_path,
            );

            match result {
                Ok(()) => {
                    updates_this_run += 1;
                    eprintln!("  ✓ Baseline updated ({}/{} used this run)", updates_this_run, max_cert_updates);

                    // Read new certification for logging
                    let new_identity = read_certification_identity(&cert_path);

                    // Log the update
                    let drift_reason = status.drift.as_ref()
                        .map(|d| d.kind.clone())
                        .unwrap_or_else(|| "UNKNOWN".to_string());

                    if let Some(new_id) = new_identity {
                        let log_entry = UpdateCertLogEntry {
                            timestamp_utc: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                            old_identity,
                            new_identity: new_id,
                            reason: drift_reason,
                            updates_this_run,
                            max_updates_allowed: max_cert_updates,
                        };
                        append_to_log(&log_path, &log_entry);
                    }

                    update_cert_message = Some(format!(
                        "Baseline updated autonomously. {}/{} updates used this run.",
                        updates_this_run, max_cert_updates
                    ));
                    action = "DONE".to_string();
                }
                Err(err) => {
                    eprintln!("  ✗ Update-cert failed: {}", err);
                    update_cert_message = Some(format!("Update-cert failed: {}", err));
                }
            }
        }
    } else {
        eprintln!("[3/4] Skipped (no baseline update needed)");
    }

    // Step 4: Generate NEXT_TASK.md
    eprintln!("[4/4] Generating NEXT_TASK.md...");

    // If FIX_RUN and we have failures, generate explain
    let explain_path = if action == "FIX_RUN" && !status.last_run_failures.is_empty() {
        let first_failure = &status.last_run_failures[0];
        eprintln!(
            "  Generating explain for: {}",
            first_failure.scenario_key
        );
        let explain_file = out_dir.join("explain_failure.json");
        let _ = run_namako_explain(
            &namako,
            &adapter,
            &spec_root,
            &first_failure.scenario_key,
            &explain_file,
        );
        Some(explain_file)
    } else {
        None
    };

    let next_task_path = out_dir.join("NEXT_TASK.md");
    generate_next_task(
        &next_task_path,
        &action,
        &status,
        &review,
        explain_path.as_ref(),
        &out_dir,
        update_cert_message.as_ref(),
        max_cert_updates,
        &mode,
    )?;

    eprintln!();
    eprintln!("Generated: {}", next_task_path.display());
    eprintln!();

    // Print the generated task
    let task_content = fs::read_to_string(&next_task_path)?;
    println!("{}", task_content);

    Ok(())
}

fn run_namako_status(
    namako: &str,
    adapter: &str,
    spec_root: &PathBuf,
    out_path: &PathBuf,
    run_report_path: Option<&PathBuf>,
) -> Result<()> {
    let args: Vec<&str> = namako.split_whitespace().collect();
    let (program, namako_args) = args.split_first().context("Empty namako command")?;

    let mut cmd = Command::new(program);
    cmd.args(namako_args)
        .arg("status")
        .arg("-a")
        .arg(adapter)
        .arg("--json")
        .arg("--out")
        .arg(out_path);

    // Pass --run-report automatically if the file exists (per TODO.md §2.1)
    if let Some(run_report) = run_report_path {
        if run_report.exists() {
            cmd.arg("--run-report").arg(run_report);
        }
    }

    let output = cmd
        .current_dir(spec_root)
        .output()
        .context("Failed to run namako status")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("namako status failed: {}", stderr);
    }

    Ok(())
}

fn run_namako_review(
    namako: &str,
    adapter: &str,
    spec_root: &PathBuf,
    out_path: &PathBuf,
) -> Result<()> {
    let args: Vec<&str> = namako.split_whitespace().collect();
    let (program, namako_args) = args.split_first().context("Empty namako command")?;

    let output = Command::new(program)
        .args(namako_args)
        .arg("review")
        .arg("-a")
        .arg(adapter)
        .arg("--out")
        .arg(out_path)
        .current_dir(spec_root)
        .output()
        .context("Failed to run namako review")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("namako review failed: {}", stderr);
    }

    Ok(())
}

fn run_namako_explain(
    namako: &str,
    adapter: &str,
    spec_root: &PathBuf,
    scenario_key: &str,
    out_path: &PathBuf,
) -> Result<()> {
    let args: Vec<&str> = namako.split_whitespace().collect();
    let (program, namako_args) = args.split_first().context("Empty namako command")?;

    let output = Command::new(program)
        .args(namako_args)
        .arg("explain")
        .arg("-a")
        .arg(adapter)
        .arg("--scenario-key")
        .arg(scenario_key)
        .arg("--out")
        .arg(out_path)
        .current_dir(spec_root)
        .output()
        .context("Failed to run namako explain")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("namako explain failed: {}", stderr);
    }

    Ok(())
}

/// Run namako update-cert to update the baseline certification.
fn run_namako_update_cert(
    namako: &str,
    adapter: &str,
    spec_root: &PathBuf,
    run_report_path: &PathBuf,
    cert_output_path: &PathBuf,
) -> Result<()> {
    let args: Vec<&str> = namako.split_whitespace().collect();
    let (program, namako_args) = args.split_first().context("Empty namako command")?;

    let output = Command::new(program)
        .args(namako_args)
        .arg("update-cert")
        .arg("-a")
        .arg(adapter)
        .arg("--run-report")
        .arg(run_report_path)
        .arg("--output")
        .arg(cert_output_path)
        .current_dir(spec_root)
        .output()
        .context("Failed to run namako update-cert")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("namako update-cert failed: {}", stderr);
    }

    Ok(())
}

/// Read certification.json and extract identity fields for logging
fn read_certification_identity(cert_path: &PathBuf) -> Option<IdentitySnapshot> {
    let content = fs::read_to_string(cert_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    let identity = json.get("identity")?;
    Some(IdentitySnapshot {
        feature_fingerprint_hash: identity.get("feature_fingerprint_hash")?.as_str()?.to_string(),
        step_registry_hash: identity.get("step_registry_hash")?.as_str()?.to_string(),
        resolved_plan_hash: identity.get("resolved_plan_hash")?.as_str()?.to_string(),
    })
}

/// Append an update-cert log entry to the audit log (JSONL format)
fn append_to_log(log_path: &PathBuf, entry: &UpdateCertLogEntry) {
    use std::io::Write;
    let json_line = match serde_json::to_string(entry) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("  WARNING: Failed to serialize log entry: {}", e);
            return;
        }
    };
    let mut file = match fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("  WARNING: Failed to open log file: {}", e);
            return;
        }
    };
    if let Err(e) = writeln!(file, "{}", json_line) {
        eprintln!("  WARNING: Failed to write log entry: {}", e);
    }
}

fn generate_next_task(
    path: &PathBuf,
    action: &str,
    status: &StatusJson,
    review: &ReviewJson,
    explain_path: Option<&PathBuf>,
    out_dir: &PathBuf,
    update_cert_message: Option<&String>,
    max_cert_updates: u32,
    mode: &OperatingMode,
) -> Result<()> {
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // Filter out CORE blockers in BOOTSTRAP mode
    let (eligible_candidates, blocked_candidates): (Vec<_>, Vec<_>) =
        review.promotion_candidates.iter().partition(|c| {
            // In BOOTSTRAP mode, CORE blockers are not eligible
            if *mode == OperatingMode::Bootstrap && c.blocker == BlockerType::Core {
                false
            } else {
                true
            }
        });
    let drift_kind = status
        .drift
        .as_ref()
        .map(|d| d.kind.as_str())
        .unwrap_or("NONE");

    let mut content = String::new();

    // Header
    content.push_str("# NEXT_TASK.md — Tesaki Generated Task\n\n");
    content.push_str(&format!("**Generated:** {}\n", timestamp));
    content.push_str(&format!("**Action:** `{}`\n\n", action));
    content.push_str("---\n\n");

    // Current Status
    content.push_str("## Current Status\n\n");
    content.push_str("| Metric | Value |\n");
    content.push_str("|--------|-------|\n");
    content.push_str(&format!(
        "| Executable Scenarios | {} |\n",
        review.coverage_summary.executable_scenarios_total
    ));
    content.push_str(&format!(
        "| Deferred Items | {} |\n",
        review.coverage_summary.deferred_items_total
    ));
    content.push_str(&format!(
        "| Promotion Candidates | {} (total), {} (eligible) |\n",
        review.promotion_candidates.len(),
        eligible_candidates.len()
    ));
    if !blocked_candidates.is_empty() {
        content.push_str(&format!(
            "| Blocked (CORE in BOOTSTRAP) | {} |\n",
            blocked_candidates.len()
        ));
    }
    content.push_str(&format!("| Drift Status | {} |\n\n", drift_kind));
    content.push_str("---\n\n");

    // Action-specific content
    match action {
        "DONE" => {
            content.push_str("## Task: Propose Micro-Milestone Batch\n\n");
            content.push_str("All gates are green. The system is stable.\n\n");

            // Show update-cert message if an autonomous update just occurred
            if let Some(msg) = update_cert_message {
                content.push_str("### Baseline Update\n\n");
                content.push_str(&format!("{}\n\n", msg));
            }

            // Show blocked candidates warning
            if !blocked_candidates.is_empty() {
                content.push_str("### ⚠️ Blocked Scenarios (CORE in BOOTSTRAP mode)\n\n");
                content.push_str("The following scenarios require CORE changes and are blocked in BOOTSTRAP mode:\n\n");
                for candidate in &blocked_candidates {
                    content.push_str(&format!(
                        "  - **{}** — Blocked on CORE; wait until CONSUMPTION mode to address.\n",
                        candidate.scenario_name
                    ));
                }
                content.push_str("\n");
            }

            content.push_str("### Recommended Next Steps\n\n");

            if !eligible_candidates.is_empty() {
                content.push_str("Consider promoting the top 3 scenarios from Deferred → Executable:\n\n");
                for (i, candidate) in eligible_candidates.iter().take(3).enumerate() {
                    content.push_str(&format!(
                        "  {}. **{}**\n     - Feature: `{}`\n     - Rule: {}\n     - Reuse score: {:.1}, New steps: {}\n\n",
                        i + 1,
                        candidate.scenario_name,
                        candidate.feature_path,
                        candidate.rule_name,
                        candidate.reuse_score,
                        candidate.new_step_texts_estimate
                    ));
                }
            } else {
                content.push_str("No promotion candidates available. Consider:\n");
                content.push_str("- Adding new @Deferred scenarios to feature files\n");
                content.push_str("- Expanding to new feature files\n\n");
            }

            content.push_str("### Missing Bindings to Implement\n\n");
            if !review.missing_bindings_for_top_candidates.is_empty() {
                for mb in review.missing_bindings_for_top_candidates.iter().take(3) {
                    let missing = if mb.missing_step_texts.is_empty() {
                        "(all steps covered)".to_string()
                    } else {
                        mb.missing_step_texts.join(", ")
                    };
                    content.push_str(&format!("  - **{}**: {}\n", mb.candidate_name, missing));
                }
            } else {
                content.push_str("  (none)\n");
            }
            content.push_str("\n");

            content.push_str("### Instructions\n\n");
            content.push_str("1. Uncomment/enable the top candidate scenarios in their `.feature` files\n");
            content.push_str("2. Implement missing step bindings in `naia/test/tests/src/steps/`\n");
            content.push_str("3. Run `bash scripts/namako_ci.sh` until green\n");
            content.push_str("4. Run `bash scripts/determinism_check.sh` to verify determinism\n");
            content.push_str("5. If baseline drift is detected, request `update-cert` approval\n\n");
        }

        "FIX_LINT" => {
            content.push_str("## Task: Fix Lint Errors\n\n");
            content.push_str("Lint failed. Missing step bindings or spec errors detected.\n\n");

            content.push_str("### Top Candidates with Missing Bindings\n\n");
            if !review.missing_bindings_for_top_candidates.is_empty() {
                for mb in review.missing_bindings_for_top_candidates.iter().take(3) {
                    let missing = if mb.missing_step_texts.is_empty() {
                        "(all steps covered)".to_string()
                    } else {
                        mb.missing_step_texts.join(", ")
                    };
                    content.push_str(&format!("  - **{}**: {}\n", mb.candidate_name, missing));
                }
            } else {
                content.push_str("  (none)\n");
            }
            content.push_str("\n");

            content.push_str("### Instructions\n\n");
            content.push_str("1. Review the lint errors\n");
            content.push_str("2. Implement missing step bindings in `naia/test/tests/src/steps/`\n");
            content.push_str("3. Run `bash scripts/namako_ci.sh` to verify fix\n");
            content.push_str("4. Repeat until lint passes\n\n");
        }

        "FIX_RUN" => {
            content.push_str("## Task: Fix Failing Scenarios\n\n");
            content.push_str("Test execution failed. Debug and fix step implementations.\n\n");

            content.push_str("### Failing Scenarios\n\n");
            if !status.last_run_failures.is_empty() {
                for failure in &status.last_run_failures {
                    content.push_str(&format!(
                        "  - {}: {} [{}]\n",
                        failure.scenario_key, failure.scenario_name, failure.failure_kind
                    ));
                }
            } else {
                content.push_str("  (no failure details available)\n");
            }
            content.push_str("\n");

            content.push_str("### Explain Packet\n\n");
            if let Some(explain) = explain_path {
                content.push_str(&format!("See: `{}`\n\n", explain.display()));
            } else {
                content.push_str("(No explain packet generated — failure details may not be machine-readable yet)\n\n");
            }

            content.push_str("### Fix Categories\n\n");
            content.push_str("- **Binding Bug:** Step implementation is incorrect\n");
            content.push_str("- **Harness Gap:** Test harness missing capability\n");
            content.push_str("- **SUT Behavior Mismatch:** System under test behaves differently than specified\n\n");

            content.push_str("### Instructions\n\n");
            content.push_str("1. Identify the root cause from the failure output\n");
            content.push_str("2. Fix the binding, harness, or investigate SUT behavior\n");
            content.push_str("3. Run `bash scripts/namako_ci.sh` to verify fix\n");
            content.push_str("4. If behavior differs from spec, file a clarification request\n\n");
        }

        "NEEDS_UPDATE_CERT_APPROVAL" => {
            content.push_str("## STOP: Approval Required\n\n");
            content.push_str("Drift detected between current state and baseline certification.\n\n");

            // Show update-cert message if available
            if let Some(msg) = update_cert_message {
                content.push_str("### Update Status\n\n");
                content.push_str(&format!("{}\n\n", msg));
            }

            content.push_str("### Drift Details\n\n");
            if let Some(drift) = &status.drift {
                for detail in &drift.details {
                    content.push_str(&format!(
                        "  - {}: {} → {}\n",
                        detail.field, detail.baseline, detail.current
                    ));
                }
            } else {
                content.push_str("  (unable to parse drift details)\n");
            }
            content.push_str("\n");

            content.push_str("### DO NOT PROCEED WITHOUT EXPLICIT APPROVAL\n\n");
            content.push_str("The baseline certification must be updated, but this requires Connor's explicit approval.\n\n");

            content.push_str("### What Changed\n\n");
            content.push_str("Review the drift details above. Common causes:\n");
            content.push_str("- Feature file content changed (intentional spec update)\n");
            content.push_str("- Step bindings changed (implementation fix)\n");
            content.push_str("- Step registry changed (new/modified bindings)\n\n");

            content.push_str("### Instructions\n\n");
            content.push_str("1. Review the drift details carefully\n");
            content.push_str("2. **STOP AND WAIT** for Connor's approval\n");
            if max_cert_updates > 0 {
                content.push_str(&format!(
                    "3. Autonomous updates are enabled (--max-cert-updates={}). Run tesaki again to auto-update.\n",
                    max_cert_updates
                ));
            } else {
                content.push_str("3. Autonomous updates are disabled (--max-cert-updates=0). Use --max-cert-updates=N to enable.\n");
            }
            content.push_str("4. Or manually: run `namako update-cert`\n\n");
        }

        "RUN_LINT" | "RUN" | "RUN_VERIFY" => {
            content.push_str(&format!("## Task: Run Gate `{}`\n\n", action));
            content.push_str("The pipeline needs to be executed.\n\n");

            content.push_str("### Instructions\n\n");
            content.push_str("1. Run: `bash scripts/namako_ci.sh`\n");
            content.push_str("2. This will execute lint → run → verify pipeline\n");
            content.push_str("3. If any step fails, follow the appropriate fix instructions\n\n");
        }

        _ => {
            content.push_str(&format!("## Task: Unknown State `{}`\n\n", action));
            content.push_str("The recommended action is not recognized.\n\n");

            content.push_str("### Instructions\n\n");
            content.push_str("1. Check the status.json for more details\n");
            content.push_str("2. Manually investigate the state\n");
            content.push_str("3. Run: `bash scripts/namako_ci.sh` to attempt recovery\n\n");
        }
    }

    // Artifacts section
    content.push_str("---\n\n");
    content.push_str("## Artifacts\n\n");
    content.push_str("| Artifact | Path |\n");
    content.push_str("|----------|------|\n");
    content.push_str(&format!("| Status | `{}/status.json` |\n", out_dir.display()));
    content.push_str(&format!("| Review | `{}/review.json` |\n", out_dir.display()));
    if let Some(explain) = explain_path {
        content.push_str(&format!("| Explain (Failure) | `{}` |\n", explain.display()));
    }
    content.push_str("\n---\n\n");
    content.push_str("*Generated by tesaki — Tesaki v2 (--max-cert-updates governance)*\n");

    fs::write(path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Test log entry serialization
    #[test]
    fn test_log_entry_serialization() {
        let entry = UpdateCertLogEntry {
            timestamp_utc: "2026-01-19T12:00:00Z".to_string(),
            old_identity: Some(IdentitySnapshot {
                feature_fingerprint_hash: "aaa".to_string(),
                step_registry_hash: "bbb".to_string(),
                resolved_plan_hash: "ccc".to_string(),
            }),
            new_identity: IdentitySnapshot {
                feature_fingerprint_hash: "ddd".to_string(),
                step_registry_hash: "eee".to_string(),
                resolved_plan_hash: "fff".to_string(),
            },
            reason: "FEATURE_DRIFT".to_string(),
            updates_this_run: 1,
            max_updates_allowed: 3,
        };

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("timestamp_utc"));
        assert!(json.contains("old_identity"));
        assert!(json.contains("new_identity"));
        assert!(json.contains("reason"));

        // Verify it can be deserialized back
        let parsed: UpdateCertLogEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.reason, "FEATURE_DRIFT");
        assert_eq!(parsed.updates_this_run, 1);
    }

    /// Test append_to_log writes valid JSONL
    #[test]
    fn test_append_to_log() {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("update_cert_log.jsonl");

        let entry1 = UpdateCertLogEntry {
            timestamp_utc: "2026-01-19T12:00:00Z".to_string(),
            old_identity: None,
            new_identity: IdentitySnapshot {
                feature_fingerprint_hash: "aaa".to_string(),
                step_registry_hash: "bbb".to_string(),
                resolved_plan_hash: "ccc".to_string(),
            },
            reason: "INITIAL".to_string(),
            updates_this_run: 1,
            max_updates_allowed: 3,
        };

        let entry2 = UpdateCertLogEntry {
            timestamp_utc: "2026-01-19T12:01:00Z".to_string(),
            old_identity: Some(IdentitySnapshot {
                feature_fingerprint_hash: "aaa".to_string(),
                step_registry_hash: "bbb".to_string(),
                resolved_plan_hash: "ccc".to_string(),
            }),
            new_identity: IdentitySnapshot {
                feature_fingerprint_hash: "ddd".to_string(),
                step_registry_hash: "eee".to_string(),
                resolved_plan_hash: "fff".to_string(),
            },
            reason: "REGISTRY_DRIFT".to_string(),
            updates_this_run: 2,
            max_updates_allowed: 3,
        };

        append_to_log(&log_path, &entry1);
        append_to_log(&log_path, &entry2);

        let content = fs::read_to_string(&log_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);

        // Each line should be valid JSON
        let _: UpdateCertLogEntry = serde_json::from_str(lines[0]).unwrap();
        let _: UpdateCertLogEntry = serde_json::from_str(lines[1]).unwrap();
    }

    /// Test read_certification_identity with valid cert
    #[test]
    fn test_read_certification_identity() {
        let temp_dir = TempDir::new().unwrap();
        let cert_path = temp_dir.path().join("certification.json");

        let cert_content = r#"{
            "identity": {
                "feature_fingerprint_hash": "abc123",
                "step_registry_hash": "def456",
                "resolved_plan_hash": "ghi789"
            },
            "metadata": {}
        }"#;

        fs::write(&cert_path, cert_content).unwrap();

        let identity = read_certification_identity(&cert_path).unwrap();
        assert_eq!(identity.feature_fingerprint_hash, "abc123");
        assert_eq!(identity.step_registry_hash, "def456");
        assert_eq!(identity.resolved_plan_hash, "ghi789");
    }

    /// Test read_certification_identity returns None for missing file
    #[test]
    fn test_read_certification_identity_missing() {
        let temp_dir = TempDir::new().unwrap();
        let cert_path = temp_dir.path().join("nonexistent.json");

        assert!(read_certification_identity(&cert_path).is_none());
    }
}
