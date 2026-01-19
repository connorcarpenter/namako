//! Tesaki - AI-friendly task orchestrator for Namako spec-driven development
//!
//! Tesaki is a deterministic task generator that:
//! - Consumes Namako status and review packets
//! - Generates NEXT_TASK.md with specific, actionable instructions
//! - Never modifies source files (only writes to artifact directories)
//! - May run update-cert ONLY if ALLOW_UPDATE_CERT token is present (per TODO.md §1.1)

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Token file name for autonomous baseline update approval
const ALLOW_UPDATE_CERT_TOKEN: &str = "ALLOW_UPDATE_CERT.json";

/// Schema for the ALLOW_UPDATE_CERT token per TODO.md §1.1
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateCertToken {
    /// Schema version (must be 1)
    version: u32,
    /// Remaining number of autonomous updates allowed
    max_updates: u32,
}

impl UpdateCertToken {
    /// Read token from file path, returns None if file doesn't exist or is invalid
    fn read(path: &PathBuf) -> Option<Self> {
        let content = fs::read_to_string(path).ok()?;
        let token: Self = serde_json::from_str(&content).ok()?;
        // Validate version
        if token.version != 1 {
            eprintln!("  WARNING: Token version {} not supported (expected 1)", token.version);
            return None;
        }
        Some(token)
    }

    /// Write token to file path
    fn write(&self, path: &PathBuf) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json).context("Failed to write token file")?;
        Ok(())
    }

    /// Decrement max_updates and return whether update is still allowed
    /// If max_updates reaches 0, returns false and token should be deleted
    fn decrement(&mut self) -> bool {
        if self.max_updates > 0 {
            self.max_updates -= 1;
            true
        } else {
            false
        }
    }
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

#[derive(Debug, Deserialize)]
struct PromotionCandidate {
    scenario_name: String,
    feature_path: String,
    rule_name: String,
    reuse_score: f32,
    new_step_texts_estimate: u32,
}

#[derive(Debug, Deserialize)]
struct MissingBindings {
    candidate_name: String,
    #[serde(default)]
    missing_step_texts: Vec<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Next {
            spec_root,
            adapter,
            out,
            namako_cli,
        } => run_next(&spec_root, &adapter, out, namako_cli),
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
) -> Result<()> {
    // Canonicalize spec_root to get absolute path
    let spec_root = fs::canonicalize(spec_root)
        .context("Failed to canonicalize spec_root path")?;

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
    let token_path = out_dir.join(ALLOW_UPDATE_CERT_TOKEN);

    eprintln!("=== Tesaki v1 ===");
    eprintln!("Spec root: {}", spec_root.display());
    eprintln!("Output dir: {}", out_dir.display());
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

    // Step 3: Handle NEEDS_UPDATE_CERT_APPROVAL with token-based autonomy (per TODO.md §1.1)
    let mut update_cert_message: Option<String> = None;
    if action == "NEEDS_UPDATE_CERT_APPROVAL" {
        eprintln!("[3/4] Checking ALLOW_UPDATE_CERT token...");
        if token_path.exists() {
            eprintln!("  Token found at: {}", token_path.display());
        } else {
            eprintln!("  No token found - manual approval required");
        }

        if run_report_path.exists() {
            let result = try_autonomous_update_cert(
                &namako,
                &adapter,
                &spec_root,
                &run_report_path,
                &cert_path,
                &token_path,
            );

            match result {
                UpdateCertResult::Updated { remaining } => {
                    eprintln!("  ✓ Baseline updated autonomously ({} updates remaining)", remaining);
                    update_cert_message = Some(format!(
                        "Baseline updated autonomously using ALLOW_UPDATE_CERT token. {} updates remaining.",
                        remaining
                    ));
                    action = "DONE".to_string(); // Re-run will show DONE
                }
                UpdateCertResult::TokenExhausted => {
                    eprintln!("  ✓ Baseline updated (token exhausted and deleted)");
                    update_cert_message = Some(
                        "Baseline updated autonomously. Token exhausted (max_updates reached 0), token file deleted.".to_string()
                    );
                    action = "DONE".to_string();
                }
                UpdateCertResult::NoToken => {
                    eprintln!("  No valid token found - manual approval required");
                    update_cert_message = Some(
                        "No ALLOW_UPDATE_CERT token found. Create token file to enable autonomous updates.".to_string()
                    );
                }
                UpdateCertResult::Failed(err) => {
                    eprintln!("  ✗ Update-cert failed: {}", err);
                    update_cert_message = Some(format!("Update-cert failed: {}", err));
                }
            }
        } else {
            update_cert_message = Some(
                "Cannot attempt autonomous update: run_report.json not found. Run `namako run` first.".to_string()
            );
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
        &token_path,
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
/// Per TODO.md §1.1, this should only be called when ALLOW_UPDATE_CERT token is present.
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

/// Result of attempting autonomous baseline update
#[derive(Debug)]
enum UpdateCertResult {
    /// Token was consumed, baseline updated, N updates remaining
    Updated { remaining: u32 },
    /// Token exhausted (max_updates reached 0), file deleted
    TokenExhausted,
    /// No token present, approval required
    NoToken,
    /// Update-cert command failed
    Failed(String),
}

/// Attempt autonomous baseline update using ALLOW_UPDATE_CERT token.
/// Per TODO.md §1.1:
/// - If token missing: return NoToken (caller should STOP)
/// - If token present with max_updates > 0: run update-cert, decrement, rewrite
/// - If max_updates reaches 0: delete token
fn try_autonomous_update_cert(
    namako: &str,
    adapter: &str,
    spec_root: &PathBuf,
    run_report_path: &PathBuf,
    cert_output_path: &PathBuf,
    token_path: &PathBuf,
) -> UpdateCertResult {
    // Check if token exists
    let mut token = match UpdateCertToken::read(token_path) {
        Some(t) => t,
        None => return UpdateCertResult::NoToken,
    };

    // Check if we have updates remaining
    if token.max_updates == 0 {
        // Token exists but exhausted - delete it
        let _ = fs::remove_file(token_path);
        return UpdateCertResult::TokenExhausted;
    }

    // Attempt update-cert
    if let Err(e) = run_namako_update_cert(namako, adapter, spec_root, run_report_path, cert_output_path) {
        return UpdateCertResult::Failed(e.to_string());
    }

    // Update-cert succeeded, decrement token
    token.decrement();

    if token.max_updates == 0 {
        // Token exhausted, delete it
        let _ = fs::remove_file(token_path);
        return UpdateCertResult::TokenExhausted;
    }

    // Rewrite token with decremented count
    if let Err(e) = token.write(token_path) {
        eprintln!("  WARNING: Failed to update token file: {}", e);
    }

    UpdateCertResult::Updated { remaining: token.max_updates }
}

fn generate_next_task(
    path: &PathBuf,
    action: &str,
    status: &StatusJson,
    review: &ReviewJson,
    explain_path: Option<&PathBuf>,
    out_dir: &PathBuf,
    update_cert_message: Option<&String>,
    token_path: &PathBuf,
) -> Result<()> {
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
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
        "| Promotion Candidates | {} |\n",
        review.promotion_candidates.len()
    ));
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

            content.push_str("### Recommended Next Steps\n\n");

            if !review.promotion_candidates.is_empty() {
                content.push_str("Consider promoting the top 3 scenarios from Deferred → Executable:\n\n");
                for (i, candidate) in review.promotion_candidates.iter().take(3).enumerate() {
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

            // Show update-cert message if available (token status)
            if let Some(msg) = update_cert_message {
                content.push_str("### Token Status\n\n");
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
            content.push_str(&format!("3. To enable autonomous updates, create `{}`:\n", token_path.display()));
            content.push_str("   ```json\n");
            content.push_str("   { \"version\": 1, \"max_updates\": 3 }\n");
            content.push_str("   ```\n");
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
    content.push_str("*Generated by tesaki — Tesaki v1 (autonomous update-cert with token)*\n");

    fs::write(path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Test token decrement behavior (per TODO.md §1.1 acceptance criteria)
    #[test]
    fn test_token_decrement() {
        let mut token = UpdateCertToken {
            version: 1,
            max_updates: 3,
        };

        // First decrement: 3 -> 2, returns true
        assert!(token.decrement());
        assert_eq!(token.max_updates, 2);

        // Second decrement: 2 -> 1, returns true
        assert!(token.decrement());
        assert_eq!(token.max_updates, 1);

        // Third decrement: 1 -> 0, returns true
        assert!(token.decrement());
        assert_eq!(token.max_updates, 0);

        // Fourth decrement: already 0, returns false
        assert!(!token.decrement());
        assert_eq!(token.max_updates, 0);
    }

    /// Test token read/write roundtrip
    #[test]
    fn test_token_read_write() {
        let temp_dir = TempDir::new().unwrap();
        let token_path = temp_dir.path().join("ALLOW_UPDATE_CERT.json");

        // Write token
        let token = UpdateCertToken {
            version: 1,
            max_updates: 5,
        };
        token.write(&token_path).unwrap();

        // Read token back
        let read_token = UpdateCertToken::read(&token_path).unwrap();
        assert_eq!(read_token.version, 1);
        assert_eq!(read_token.max_updates, 5);
    }

    /// Test token read returns None for missing file
    #[test]
    fn test_token_read_missing() {
        let temp_dir = TempDir::new().unwrap();
        let token_path = temp_dir.path().join("nonexistent.json");

        assert!(UpdateCertToken::read(&token_path).is_none());
    }

    /// Test token read returns None for invalid JSON
    #[test]
    fn test_token_read_invalid_json() {
        let temp_dir = TempDir::new().unwrap();
        let token_path = temp_dir.path().join("invalid.json");

        fs::write(&token_path, "not valid json").unwrap();
        assert!(UpdateCertToken::read(&token_path).is_none());
    }

    /// Test token read returns None for wrong version
    #[test]
    fn test_token_read_wrong_version() {
        let temp_dir = TempDir::new().unwrap();
        let token_path = temp_dir.path().join("wrong_version.json");

        fs::write(&token_path, r#"{ "version": 2, "max_updates": 3 }"#).unwrap();
        assert!(UpdateCertToken::read(&token_path).is_none());
    }
}
