//! Tesaki - AI-friendly task orchestrator for Namako spec-driven development
//!
//! Tesaki is a deterministic task generator that:
//! - Consumes Namako status and review packets
//! - Generates NEXT_TASK.md with specific, actionable instructions
//! - Never modifies source files (only writes to artifact directories)
//! - May run update-cert up to max-cert-updates times per run (governed by config)
//!
//! # v1.7 Runner Integration
//!
//! With v1.7, Tesaki can orchestrate an autonomous coding agent (runner) via the REPL.
//! The runner operates on the specs repository only - it never edits Namako/Tesaki code.
//!
//! # Configuration Discovery
//!
//! Tesaki searches for `.tesaki/config.toml` in the current directory and parent
//! directories. See the Tesaki README for details.

mod binding_extractor;
mod config;
mod gate;
mod issue_classifier;
mod chat_plan;
mod chat_planner;
mod logging;
mod mission;
mod mission_selector;
mod mission_type;
mod model_tier;
mod packet_parser;
mod prompts;
mod repl;
mod repo_state;
mod runner;
mod base_runner;
mod claude_code_agent;
mod codex_agent;
mod copilot_agent;
mod runner_test;
mod scenario_extractor;
mod session;
mod stage;
mod stop_reason;
mod surface_policy;
mod token_usage;
mod workspace;

use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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
#[command(disable_version_flag = true)]
#[command(after_help = "Run `tesaki` to start the interactive REPL, or `tesaki --loop N` for autonomous mode.")]
struct Cli {
    /// Run autonomous loop for N iterations (or until done/stalled)
    #[arg(long, short = 'l')]
    r#loop: Option<u32>,
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
/// - CORE: Requires changes to the core codebase
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
    #[allow(dead_code)]
    blocker: BlockerType,
    /// Whether this is a stub scenario (should always be false in promotion_candidates,
    /// since namako review filters them out; kept for defense-in-depth)
    #[serde(default)]
    is_stub: bool,
}

#[derive(Debug, Deserialize)]
struct MissingBindings {
    candidate_name: String,
    #[serde(default)]
    missing_step_texts: Vec<String>,
}


fn main() -> Result<()> {
    // Initialize logging - configure via RUST_LOG env var
    logging::init();

    let cli = Cli::parse();
    let log_path = std::env::var_os("TESAKI_LOG_PATH").map(PathBuf::from);
    // Use Off mode for cleaner REPL output - state is shown in summary
    let logger = logging::JsonlLogger::new_with_console(
        log_path,
        logging::ConsoleMode::Off,
    );
    
    if let Some(iterations) = cli.r#loop {
        // Autonomous mode: run loop directly without REPL
        repl::run_loop_headless(
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            iterations,
            &logger,
        )
    } else {
        // Interactive mode: start REPL
        repl::run_repl(
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            &logger,
        )
    }
}

/// Resolved configuration from CLI flags and/or config file
#[allow(dead_code)]
struct ResolvedArgs {
    specs_dir: PathBuf,
    adapter_cmd: String,
    namako_cli: Option<String>,
    max_cert_updates: Option<u32>,
    runner: Option<String>,
    runner_cmd: Option<String>,
    max_retries: Option<u32>,
    max_runtime_seconds: Option<u64>,
    max_files_changed: Option<usize>,
    surfaces: Option<config::SurfacesConfig>,
    model: Option<String>,
    stream_output: bool,
}

/// Resolve configuration from CLI flags, falling back to config file if flags are missing.
/// CLI flags always override config file values.
#[allow(dead_code)]
fn resolve_config_or_flags(
    spec_root: Option<PathBuf>,
    adapter: Option<String>,
    max_cert_updates: Option<u32>,
    runner: Option<String>,
    runner_cmd: Option<String>,
    max_retries: Option<u32>,
    max_runtime_seconds: Option<u64>,
    max_files_changed: Option<usize>,
) -> Result<ResolvedArgs> {
    // If both required flags are provided, use them directly
    if let (Some(spec_root), Some(adapter)) = (&spec_root, &adapter) {
        let spec_root = fs::canonicalize(spec_root)
            .with_context(|| format!("Failed to canonicalize spec_root: {}", spec_root.display()))?;
        let adapter = canonicalize_adapter_cmd(adapter)?;
        return Ok(ResolvedArgs {
            specs_dir: spec_root,
            adapter_cmd: adapter,
            namako_cli: None,
            max_cert_updates,
            runner,
            runner_cmd,
            max_retries,
            max_runtime_seconds,
            max_files_changed,
            surfaces: None,
            model: None,
            stream_output: false,
        });
    }

    // Try to discover config
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let discovery = config::discover_config(&cwd)?;

    match discovery {
        config::ConfigDiscoveryResult::Found(cfg) => {
            eprintln!("Using config: {}", cfg.config_path.display());

            // CLI flags override config values
            let specs_dir = spec_root
                .map(|p| fs::canonicalize(&p).unwrap_or(p))
                .unwrap_or(cfg.specs_dir);
            let adapter_cmd = adapter
                .map(|a| canonicalize_adapter_cmd(&a).unwrap_or(a))
                .unwrap_or(cfg.adapter_cmd);

            Ok(ResolvedArgs {
                specs_dir,
                adapter_cmd,
                namako_cli: cfg.namako_cli.clone(),
                max_cert_updates: max_cert_updates.or(cfg.max_cert_updates),
                runner: runner.or(cfg.runner),
                runner_cmd: runner_cmd.or(cfg.runner_cmd),
                max_retries: max_retries.or(cfg.max_retries),
                max_runtime_seconds: max_runtime_seconds.or(cfg.max_runtime_seconds),
                max_files_changed: max_files_changed.or(cfg.max_files_changed),
                surfaces: cfg.surfaces,
                model: cfg.model,
                stream_output: cfg.stream_output,
            })
        }
        config::ConfigDiscoveryResult::NotFound { .. } => {
            // Check if we have at least partial flags
            if spec_root.is_some() || adapter.is_some() {
                anyhow::bail!(
                    "Configuration is required when no config file is found.\n\n\
                    Create .tesaki/config.toml in your repository."
                );
            }

            config::print_config_error();
            std::process::exit(1);
        }
    }
}

/// Print resolved configuration (internal helper).
#[allow(dead_code)]
fn run_config_print() -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let discovery = config::discover_config(&cwd)?;

    match discovery {
        config::ConfigDiscoveryResult::Found(cfg) => {
            println!("Config file: {}", cfg.config_path.display());
            println!("Config root: {}", cfg.config_root.display());
            println!();
            println!("Resolved values:");
            println!("  specs_dir: {}", cfg.specs_dir.display());
            println!("  adapter_cmd: {}", cfg.adapter_cmd);
            let namako = resolve_namako_cli(None, cfg.namako_cli.clone(), &cfg.specs_dir);
            println!(
                "  namako_cli: {} ({})",
                namako.command,
                namako.source.label()
            );
            if let Some(ref runner) = cfg.runner {
                println!("  runner: {}", runner);
            }
            if let Some(ref runner_cmd) = cfg.runner_cmd {
                println!("  runner_cmd: {}", runner_cmd);
            }
            if let Some(ref planner) = cfg.planner {
                println!("  planner: {}", planner);
            }
            if let Some(ref planner_cmd) = cfg.planner_cmd {
                println!("  planner_cmd: {}", planner_cmd);
            }
            if let Some(max_retries) = cfg.max_retries {
                println!("  max_retries: {}", max_retries);
            }
            if let Some(max_cert_updates) = cfg.max_cert_updates {
                println!("  max_cert_updates: {}", max_cert_updates);
            }
            if let Some(max_runtime_seconds) = cfg.max_runtime_seconds {
                println!("  max_runtime_seconds: {}", max_runtime_seconds);
            }
            if let Some(max_files_changed) = cfg.max_files_changed {
                println!("  max_files_changed: {}", max_files_changed);
            }
            Ok(())
        }
        config::ConfigDiscoveryResult::NotFound { searched_dirs } => {
            eprintln!("No config file found.");
            eprintln!();
            eprintln!("Searched directories:");
            for dir in searched_dirs.iter().take(5) {
                eprintln!("  - {}", dir.display());
            }
            if searched_dirs.len() > 5 {
                eprintln!("  ... and {} more", searched_dirs.len() - 5);
            }
            eprintln!();
            config::print_config_error();
            std::process::exit(1);
        }
    }
}

#[allow(dead_code)]
fn run_status_cmd(
    spec_root: &PathBuf,
    adapter: &str,
    namako_cli: NamakoCliResolution,
    logger: &logging::JsonlLogger,
) -> Result<()> {
    use crate::packet_parser::{parse_gate_json, parse_review_json, parse_status_json};
    use crate::repo_state::RepoState;
    use crate::stage::detect_stage;

    let namako = namako_cli.command.clone();
    log_session_start(
        logger,
        spec_root,
        adapter,
        &namako_cli,
        None,
        None,
    );
    let gate_json = run_namako_gate_json(&namako, adapter, spec_root, logger)?;
    let gate_packet = parse_gate_json(&gate_json)?;

    let out_dir = spec_root.join("target/namako_artifacts/tesaki");
    fs::create_dir_all(&out_dir)?;
    let status_path = out_dir.join("status.json");
    let review_path = out_dir.join("review.json");

    let status_json = run_namako_status(&namako, adapter, spec_root, None, logger)?;
    fs::write(&status_path, &status_json)?;
    let review_json = run_namako_review(&namako, adapter, spec_root, logger)?;
    fs::write(&review_path, &review_json)?;

    let status_packet = parse_status_json(&status_json)?;
    let review_packet = parse_review_json(&review_json)?;

    let repo_state = RepoState::compute(&status_packet, &review_packet, &gate_packet, None)?;
    let stage = detect_stage(&repo_state);
    println!("RepoState: {}", repo_state.summary());
    println!("Stage: {}", stage.name());
    println!("Propagation: {}", repo_state.propagation_summary().to_line());

    log_session_end(logger, stop_reason::StopReason::Done, None);
    Ok(())
}

#[allow(dead_code)]
fn run_explain_cmd(
    spec_root: &PathBuf,
    adapter: &str,
    namako_cli: NamakoCliResolution,
    logger: &logging::JsonlLogger,
) -> Result<()> {
    use crate::mission_selector::select_mission_type;
    use crate::packet_parser::{parse_gate_json, parse_review_json, parse_status_json};
    use crate::repo_state::RepoState;
    use crate::stage::detect_stage;

    let namako = namako_cli.command.clone();
    log_session_start(
        logger,
        spec_root,
        adapter,
        &namako_cli,
        None,
        None,
    );
    let gate_json = run_namako_gate_json(&namako, adapter, spec_root, logger)?;
    let gate_packet = parse_gate_json(&gate_json)?;

    let out_dir = spec_root.join("target/namako_artifacts/tesaki");
    fs::create_dir_all(&out_dir)?;
    let status_path = out_dir.join("status.json");
    let review_path = out_dir.join("review.json");

    let status_json = run_namako_status(&namako, adapter, spec_root, None, logger)?;
    fs::write(&status_path, &status_json)?;
    let review_json = run_namako_review(&namako, adapter, spec_root, logger)?;
    fs::write(&review_path, &review_json)?;

    let status_packet = parse_status_json(&status_json)?;
    let review_packet = parse_review_json(&review_json)?;

    let repo_state = RepoState::compute(&status_packet, &review_packet, &gate_packet, None)?;
    let stage = detect_stage(&repo_state);
    println!("Stage: {}", stage.name());
    println!("RepoState: {}", repo_state.summary());

    if let Some(task) = repo_state.top_candidate() {
        println!("Top issue: {} ({:?})", task.name, task.priority);
        println!("Why: {}", task.description);
    }

    if let Some(mission) = select_mission_type(&repo_state) {
        println!("Proposed mission: {}", mission.name());
        if let Some(target) = mission.target_label() {
            println!("Target: {}", target);
        }
    } else {
        println!("Proposed mission: none (no work remaining)");
    }

    log_session_end(logger, stop_reason::StopReason::Done, None);
    Ok(())
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

#[allow(dead_code)]
fn run_next(
    spec_root: &PathBuf,
    adapter: &str,
    out: Option<PathBuf>,
    namako_cli: NamakoCliResolution,
    max_cert_updates: u32,
    logger: &logging::JsonlLogger,
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
    let namako = namako_cli.command.clone();
    log_session_start(
        logger,
        &spec_root,
        &adapter,
        &namako_cli,
        None,
        None,
    );

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
    let status_content =
        run_namako_status(&namako, &adapter, &spec_root, run_report_opt, logger)?;
    fs::write(&status_path, &status_content)?;

    // Parse status
    let status: StatusJson =
        serde_json::from_str(&status_content).context("Failed to parse status.json")?;

    let mut action = status.recommended_next_action.clone();
    eprintln!("  Action: {}", action);

    // Step 2: Run namako review
    eprintln!("[2/4] Running namako review...");
    let review_path = out_dir.join("review.json");
    let review_content = run_namako_review(&namako, &adapter, &spec_root, logger)?;
    fs::write(&review_path, &review_content)?;

    // Parse review
    let review: ReviewJson =
        serde_json::from_str(&review_content).context("Failed to parse review.json")?;

    eprintln!(
        "  Executable: {} | Deferred: {} | Promotable: {}",
        review.coverage_summary.executable_scenarios_total,
        review.coverage_summary.deferred_items_total,
        review.promotion_candidates.len()
    );

    // Step 3: Handle NEEDS_UPDATE_CERT_APPROVAL with update-cert governance
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
            eprintln!("  Autonomous updates disabled (max_cert_updates=0)");
            update_cert_message = Some(
                "Autonomous updates disabled. Set max_cert_updates in .tesaki/config.toml to enable.".to_string()
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
                logger,
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
            logger,
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
    run_report_path: Option<&PathBuf>,
    logger: &logging::JsonlLogger,
) -> Result<String> {
    let args: Vec<&str> = namako.split_whitespace().collect();
    let (program, namako_args) = args.split_first().context("Empty namako command")?;

    let mut cmd_args: Vec<String> = namako_args.iter().map(|s| s.to_string()).collect();
    cmd_args.push("status".to_string());
    cmd_args.push("-a".to_string());
    cmd_args.push(adapter.to_string());
    cmd_args.push("--json".to_string());

    // Pass --run-report automatically if the file exists (per TODO.md §2.1)
    if let Some(run_report) = run_report_path {
        if run_report.exists() {
            cmd_args.push("--run-report".to_string());
            cmd_args.push(run_report.display().to_string());
        }
    }

    log_command_run(logger, "namako", &cmd_args, spec_root, None);
    let output = Command::new(program)
        .args(&cmd_args)
        .current_dir(spec_root)
        .output()
        .context("Failed to run namako status")?;

    log_command_result(logger, "namako", &cmd_args, &output);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("namako status failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if stdout.trim().is_empty() {
        anyhow::bail!("namako status returned empty output");
    }

    Ok(stdout)
}

fn run_namako_review(
    namako: &str,
    adapter: &str,
    spec_root: &PathBuf,
    logger: &logging::JsonlLogger,
) -> Result<String> {
    let args: Vec<&str> = namako.split_whitespace().collect();
    let (program, namako_args) = args.split_first().context("Empty namako command")?;

    let mut cmd_args: Vec<String> = namako_args.iter().map(|s| s.to_string()).collect();
    cmd_args.push("review".to_string());
    cmd_args.push("-a".to_string());
    cmd_args.push(adapter.to_string());

    log_command_run(logger, "namako", &cmd_args, spec_root, None);
    let output = Command::new(program)
        .args(&cmd_args)
        .current_dir(spec_root)
        .output()
        .context("Failed to run namako review")?;

    log_command_result(logger, "namako", &cmd_args, &output);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("namako review failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if stdout.trim().is_empty() {
        anyhow::bail!("namako review returned empty output");
    }

    Ok(stdout)
}

fn run_namako_explain(
    namako: &str,
    adapter: &str,
    spec_root: &PathBuf,
    scenario_key: &str,
    out_path: &PathBuf,
    logger: &logging::JsonlLogger,
) -> Result<()> {
    let args: Vec<&str> = namako.split_whitespace().collect();
    let (program, namako_args) = args.split_first().context("Empty namako command")?;

    let mut cmd_args: Vec<String> = namako_args.iter().map(|s| s.to_string()).collect();
    cmd_args.push("explain".to_string());
    cmd_args.push("-a".to_string());
    cmd_args.push(adapter.to_string());
    cmd_args.push("--scenario-key".to_string());
    cmd_args.push(scenario_key.to_string());
    cmd_args.push("--out".to_string());
    cmd_args.push(out_path.display().to_string());

    log_command_run(logger, "namako", &cmd_args, spec_root, None);
    let output = Command::new(program)
        .args(&cmd_args)
        .current_dir(spec_root)
        .output()
        .context("Failed to run namako explain")?;

    log_command_result(logger, "namako", &cmd_args, &output);

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
    logger: &logging::JsonlLogger,
) -> Result<()> {
    let args: Vec<&str> = namako.split_whitespace().collect();
    let (program, namako_args) = args.split_first().context("Empty namako command")?;

    let mut cmd_args: Vec<String> = namako_args.iter().map(|s| s.to_string()).collect();
    cmd_args.push("update-cert".to_string());
    cmd_args.push("-a".to_string());
    cmd_args.push(adapter.to_string());
    cmd_args.push("--run-report".to_string());
    cmd_args.push(run_report_path.display().to_string());
    cmd_args.push("--output".to_string());
    cmd_args.push(cert_output_path.display().to_string());

    log_command_run(logger, "namako", &cmd_args, spec_root, None);
    let output = Command::new(program)
        .args(&cmd_args)
        .current_dir(spec_root)
        .output()
        .context("Failed to run namako update-cert")?;

    log_command_result(logger, "namako", &cmd_args, &output);

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
) -> Result<()> {
    use prompts::{
        render_next_task_base, render_next_task_done, render_next_task_fix_lint,
        render_next_task_fix_run, render_next_task_needs_approval, render_next_task_run_gate,
        render_next_task_unknown, render_next_task_artifacts,
        NextTaskBaseContext, NextTaskDoneContext, NextTaskFixLintContext, NextTaskFixRunContext,
        NextTaskNeedsApprovalContext, NextTaskRunGateContext, NextTaskArtifactsContext,
        CandidateContext, MissingBindingContext, FailureDisplayContext, DriftDetailContext,
        TESAKI_VERSION,
    };

    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // Filter out stubs (defense-in-depth)
    // Per TODO.md §2: Tesaki must NEVER select @Stub scenarios as tasks.
    let eligible_candidates: Vec<_> = review.promotion_candidates.iter()
        .filter(|c| !c.is_stub)
        .collect();
    let drift_kind = status
        .drift
        .as_ref()
        .map(|d| d.kind.as_str())
        .unwrap_or("NONE");

    // Render base header
    let base_ctx = NextTaskBaseContext {
        timestamp: timestamp.clone(),
        action: action.to_string(),
        executable_scenarios_total: review.coverage_summary.executable_scenarios_total,
        deferred_items_total: review.coverage_summary.deferred_items_total,
        promotion_candidates_total: review.promotion_candidates.len(),
        eligible_candidates_count: eligible_candidates.len(),
        drift_kind: drift_kind.to_string(),
    };

    let mut content = render_next_task_base(&base_ctx)
        .unwrap_or_else(|e| {
            log::error!("Failed to render next_task base: {}", e);
            format!("# NEXT_TASK.md\n\n**Action:** `{}`\n\n---\n\n", action)
        });

    // Convert candidates to context
    let candidate_contexts: Vec<CandidateContext> = eligible_candidates
        .iter()
        .take(3)
        .map(|c| CandidateContext {
            scenario_name: c.scenario_name.clone(),
            feature_path: c.feature_path.clone(),
            rule_name: c.rule_name.clone(),
            reuse_score: c.reuse_score,
            new_step_texts_estimate: c.new_step_texts_estimate,
        })
        .collect();

    let missing_binding_contexts: Vec<MissingBindingContext> = review
        .missing_bindings_for_top_candidates
        .iter()
        .take(3)
        .map(|mb| MissingBindingContext {
            candidate_name: mb.candidate_name.clone(),
            missing_step_texts: mb.missing_step_texts.clone(),
        })
        .collect();

    // Action-specific content
    let action_content = match action {
        "DONE" => {
            let ctx = NextTaskDoneContext {
                eligible_candidates: candidate_contexts,
                missing_bindings: missing_binding_contexts,
                update_cert_message: update_cert_message.cloned(),
            };
            render_next_task_done(&ctx).unwrap_or_else(|e| {
                log::error!("Failed to render next_task done: {}", e);
                "## Task: DONE\n\nAll gates are green.\n".to_string()
            })
        }

        "FIX_LINT" => {
            let ctx = NextTaskFixLintContext {
                missing_bindings: missing_binding_contexts,
            };
            render_next_task_fix_lint(&ctx).unwrap_or_else(|e| {
                log::error!("Failed to render next_task fix_lint: {}", e);
                "## Task: Fix Lint Errors\n\nLint failed.\n".to_string()
            })
        }

        "FIX_RUN" => {
            let failures: Vec<FailureDisplayContext> = status
                .last_run_failures
                .iter()
                .map(|f| FailureDisplayContext {
                    scenario_key: f.scenario_key.clone(),
                    scenario_name: f.scenario_name.clone(),
                    failure_kind: f.failure_kind.clone(),
                })
                .collect();

            let ctx = NextTaskFixRunContext {
                failures,
                explain_path: explain_path.map(|p| p.display().to_string()),
            };
            render_next_task_fix_run(&ctx).unwrap_or_else(|e| {
                log::error!("Failed to render next_task fix_run: {}", e);
                "## Task: Fix Failing Scenarios\n\nTest execution failed.\n".to_string()
            })
        }

        "NEEDS_UPDATE_CERT_APPROVAL" => {
            let drift_details: Vec<DriftDetailContext> = status
                .drift
                .as_ref()
                .map(|d| {
                    d.details
                        .iter()
                        .map(|detail| DriftDetailContext {
                            field: detail.field.clone(),
                            baseline: detail.baseline.clone(),
                            current: detail.current.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default();

            let ctx = NextTaskNeedsApprovalContext {
                drift_details,
                update_cert_message: update_cert_message.cloned(),
                max_cert_updates,
            };
            render_next_task_needs_approval(&ctx).unwrap_or_else(|e| {
                log::error!("Failed to render next_task needs_approval: {}", e);
                "## STOP: Approval Required\n\nDrift detected.\n".to_string()
            })
        }

        "RUN_LINT" | "RUN" | "RUN_VERIFY" => {
            let ctx = NextTaskRunGateContext {
                action: action.to_string(),
            };
            render_next_task_run_gate(&ctx).unwrap_or_else(|e| {
                log::error!("Failed to render next_task run_gate: {}", e);
                format!("## Task: Run Gate `{}`\n\nThe pipeline needs to be executed.\n", action)
            })
        }

        _ => {
            render_next_task_unknown(action).unwrap_or_else(|e| {
                log::error!("Failed to render next_task unknown: {}", e);
                format!("## Task: Unknown State `{}`\n\nThe recommended action is not recognized.\n", action)
            })
        }
    };

    content.push_str("\n");
    content.push_str(&action_content);

    // Artifacts section
    let artifacts_ctx = NextTaskArtifactsContext {
        out_dir: out_dir.display().to_string(),
        explain_path: explain_path.map(|p| p.display().to_string()),
        version: TESAKI_VERSION.to_string(),
    };

    content.push_str("\n");
    content.push_str(&render_next_task_artifacts(&artifacts_ctx).unwrap_or_else(|e| {
        log::error!("Failed to render next_task artifacts: {}", e);
        format!("\n---\n\n## Artifacts\n\n| Artifact | Path |\n|----------|------|\n| Status | `{}/status.json` |\n", out_dir.display())
    }));

    fs::write(path, content)?;
    Ok(())
}

// ============================================================================
// v1.7 Runner Integration: single-mission run loop
// ============================================================================

/// Run the autonomous development loop (v1.7).
///
/// This implements the canonical UX flow per GOLD_PLAN.md §10.7.9:
/// 1. Measure via Namako packets
/// 2. Select next task from packets (or enter stop condition)
/// 3. Create mission bundle
/// 4. Invoke runner backend
/// 5. Validate via namako gate --json
/// 6. Apply update-cert governance (if verify-only failure)
/// 7. Retry if failure is retryable and attempts remain
/// 8. Transition or stop
#[allow(clippy::too_many_arguments)]
fn run_run(
    spec_root: &PathBuf,
    adapter: &str,
    namako_cli: NamakoCliResolution,
    max_cert_updates: u32,
    runner_name: &str,
    runner_cmd: Option<String>,
    max_runtime_seconds: u32,
    max_files_changed: u32,
    max_retries: u32,
    model: Option<String>,
    stream_output: bool,
    stage: Option<String>,
    surfaces: Option<config::SurfacesConfig>,
    surface_overrides: Option<crate::surface_policy::SurfacePolicy>,
    allow_dirty: bool,
    model_overrides: Option<config::ModelOverrides>,
    logger: &logging::JsonlLogger,
) -> Result<()> {
    use crate::mission::{MissionBundle, MissionBudgets, MissionInputs};
    use crate::mission::SurfaceDefinitions;
    use crate::mission_selector::select_with_constraints;
    use crate::model_tier::select_model_for_attempt;
    use crate::packet_parser::{parse_gate_json, parse_review_json, parse_status_json};
    use crate::runner::{Runner, RunnerConfig, OutcomeClassification};
    use crate::runner_test::MockRunner;
use crate::claude_code_agent::ClaudeCodeAgent;
use crate::codex_agent::CodexAgent;
    use crate::repo_state::RepoState;
    use crate::stage::{Stage, StageConstraint, detect_stage};
    use crate::stop_reason::{StopReason, RunResult};
    use crate::workspace::Workspace;
    use crate::gate::{GateOutcome, ProcessInvoker, UpdateCertInvoker};
    use crate::surface_policy::SurfaceDefinition;

    // Canonicalize spec_root
    let spec_root = fs::canonicalize(spec_root)
        .context("Failed to canonicalize spec_root path")?;

    // Canonicalize adapter command
    let adapter = canonicalize_adapter_cmd(adapter)?;

    // Set up budgets
    let budgets = MissionBudgets {
        max_files_changed,
        max_scenarios_promoted: 3,
        max_runtime_seconds,
        max_retries,
        max_cert_updates,
    };

    // Set up workspace
    let workspace = Workspace::from_specs_dir(&spec_root, &adapter, budgets.clone())?;

    // Check workspace is clean (unless allow_dirty is set)
    let workspace_state = workspace.check_clean()?;
    if !workspace_state.is_clean && !allow_dirty {
        let result = RunResult::error(
            StopReason::HumanRequired,
            format!(
                "Workspace has uncommitted changes. Please commit or stash before running.\nDirty files: {:?}",
                workspace_state.dirty_files
            ),
        );
        emit_run_result(&result, &spec_root)?;
        log_session_end(logger, StopReason::HumanRequired, result.details.clone());
        eprintln!("STOP: {}", result.reason);
        eprintln!("Details: {}", result.details.as_deref().unwrap_or(""));
        return Ok(());
    }

    // Determine namako CLI command
    let namako = namako_cli.command.clone();

    log_session_start(
        logger,
        &spec_root,
        &adapter,
        &namako_cli,
        Some(runner_name),
        None,
    );

    // Create the runner backend
    let runner: Box<dyn Runner> = match runner_name {
        "mock" => Box::new(MockRunner::success()),
        "claude" => {
            if let Err(e) = ClaudeCodeAgent::check_available() {
                let result = RunResult::error(StopReason::EnvironmentError, format!("{}", e));
                emit_run_result(&result, &spec_root)?;
                log_session_end(logger, StopReason::EnvironmentError, result.details.clone());
                eprintln!("STOP: {}", result.reason);
                return Ok(());
            }
            Box::new(ClaudeCodeAgent::new(
                runner_cmd,
                None,
                spec_root.clone(),
            )?)
        }
        "codex" => {
            if let Err(e) = CodexAgent::check_available() {
                let result = RunResult::error(StopReason::EnvironmentError, format!("{}", e));
                emit_run_result(&result, &spec_root)?;
                log_session_end(logger, StopReason::EnvironmentError, result.details.clone());
                eprintln!("STOP: {}", result.reason);
                return Ok(());
            }
            Box::new(CodexAgent::new(
                runner_cmd,
                None,
                spec_root.clone(),
            )?)
        }
        "copilot" => {
            if let Err(e) = copilot_agent::CopilotAgent::check_available() {
                let result = RunResult::error(StopReason::EnvironmentError, format!("{}", e));
                emit_run_result(&result, &spec_root)?;
                log_session_end(logger, StopReason::EnvironmentError, result.details.clone());
                eprintln!("STOP: {}", result.reason);
                return Ok(());
            }
            Box::new(copilot_agent::CopilotAgent::new_with_timeout_and_stream(
                runner_cmd,
                None,
                spec_root.clone(),
                Some(std::time::Duration::from_secs(max_runtime_seconds as u64)),
                stream_output,
            )?)
        }
        _ => anyhow::bail!("Unknown runner: {}", runner_name),
    };

    eprintln!("=== Tesaki v1.9 ===");
    eprintln!("Spec root: {}", spec_root.display());
    eprintln!("Runner: {}", runner.name());
    eprintln!();

    // Step 1: Measure via Namako packets
    eprintln!("[1/6] Running namako gate --json (pre-mission state)...");
    let gate_json = run_namako_gate_json(&namako, &adapter, &spec_root, logger)?;
    let gate_packet = parse_gate_json(&gate_json)?;
    let gate_outcome = GateOutcome::from_json_str(&gate_json);
    let gate_passes = gate_outcome == GateOutcome::Pass;

    eprintln!(
        "  Gate: {}",
        if gate_passes { "PASS (all phases green)" } else { "Not all phases passing" }
    );

    // Step 2: Get status and review packets
    eprintln!("[2/6] Running namako status/review...");
    let out_dir = spec_root.join("target/namako_artifacts/tesaki");
    fs::create_dir_all(&out_dir)?;

    let status_path = out_dir.join("status.json");
    let review_path = out_dir.join("review.json");
    let run_report_path = spec_root.join("target/namako_artifacts/run_report.json");

    let run_report_opt = if run_report_path.exists() {
        Some(&run_report_path)
    } else {
        None
    };

    let status_json =
        run_namako_status(&namako, &adapter, &spec_root, run_report_opt, logger)?;
    fs::write(&status_path, &status_json)?;
    let review_json = run_namako_review(&namako, &adapter, &spec_root, logger)?;
    fs::write(&review_path, &review_json)?;

    let status_packet = parse_status_json(&status_json)?;
    let review_packet = parse_review_json(&review_json)?;

    let repo_state = RepoState::compute(&status_packet, &review_packet, &gate_packet, None)?;
    eprintln!("  RepoState: {}", repo_state.summary());
    let pre_fingerprint = fingerprint_packets(&status_json, &review_json, &gate_json);

    let stage_override = match stage.as_deref() {
        Some(value) => Stage::from_str(value)
            .ok_or_else(|| anyhow::anyhow!("Invalid stage '{}'", value))?,
        None => detect_stage(&repo_state),
    };
    eprintln!(
        "  Stage: {} {}",
        stage_override.name(),
        if stage.is_some() { "(manual)" } else { "(auto)" }
    );

    // Step 3: Select mission type
    eprintln!("[3/6] Selecting mission...");
    let constraint = StageConstraint {
        stage: if stage.is_some() { Some(stage_override) } else { None },
        surface_overrides,
    };

    let selection = select_with_constraints(&repo_state, &constraint);
    let (mission_type, active_stage, surface_policy) = match selection {
        Some(result) => result,
        None => {
            if repo_state.has_work() {
                let result = RunResult::blocked("No eligible mission for the selected stage");
                emit_run_result(&result, &spec_root)?;
                log_session_end(logger, StopReason::Blocked, result.details.clone());
                eprintln!("STOP: BLOCKED - No eligible mission for stage");
                return Ok(());
            }
            let result = RunResult::done(0, 0);
            emit_run_result(&result, &spec_root)?;
            log_session_end(logger, StopReason::Done, None);
            eprintln!("STOP: DONE - No work remaining");
            return Ok(());
        }
    };

    if let Some(record) = load_no_progress_record(&spec_root) {
        if record.matches(&mission_type, &pre_fingerprint) {
            let details = format!(
                "No new evidence since last no-progress mission ({}).",
                record.mission_type
            );
            let result = RunResult::error(StopReason::NoProgress, details.clone());
            emit_run_result(&result, &spec_root)?;
            log_session_end(logger, StopReason::NoProgress, Some(details));
            eprintln!("STOP: NO_PROGRESS - cooldown in effect");
            return Ok(());
        }
    }

    // Compute directories for dynamic context extraction
    let steps_dir = spec_root.join("test/tests/src/steps");
    let steps_dir_opt = if steps_dir.is_dir() { Some(steps_dir.as_path()) } else { None };
    let specs_dir = spec_root.join("test/specs/features");
    let specs_dir_opt = if specs_dir.is_dir() { Some(specs_dir.as_path()) } else { None };
    
    let brief = mission_type.generate_brief_with_exemplars(&repo_state, steps_dir_opt, specs_dir_opt);

    eprintln!("  Type: {}", mission_type.name());
    if let Some(target) = mission_type.target_label() {
        eprintln!("  Target: {}", target);
    }
    eprintln!(
        "  Surfaces: Spec {} • Tests {} • SUT {}",
        lock_label(surface_policy.spec),
        lock_label(surface_policy.tests_bindings),
        lock_label(surface_policy.sut)
    );
    log_mission_proposed(logger, &mission_type, &active_stage, &surface_policy);

    let mut surface_definitions = SurfaceDefinitions {
        spec: SurfaceDefinition::spec(),
        tests_bindings: SurfaceDefinition::tests_bindings(),
        sut: SurfaceDefinition::sut(),
    };
    if let Some(overrides) = surfaces {
        if let Some(spec) = overrides.spec {
            if !spec.patterns.is_empty() {
                surface_definitions.spec.patterns = spec.patterns;
            }
        }
        if let Some(tests) = overrides.tests {
            if !tests.patterns.is_empty() {
                surface_definitions.tests_bindings.patterns = tests.patterns;
            }
        }
        if let Some(sut) = overrides.sut {
            if !sut.patterns.is_empty() {
                surface_definitions.sut.patterns = sut.patterns;
            }
        }
    }

    // Tracking for governance
    let mut attempts_made: u32 = 0;
    let mut cert_updates_made: u32 = 0;
    let mut prev_attempt_failed = false;
    let invoker = ProcessInvoker;

    // Retry loop per TODO.md §B2
    loop {
        attempts_made += 1;
        eprintln!("\n--- Attempt {}/{} ---", attempts_made, max_retries + 1);

        // Step 4: Create mission bundle (MISSION.md per v1.8)
        eprintln!("[4/6] Creating mission bundle...");

        let tesaki_dir = spec_root.join(".tesaki");
        fs::create_dir_all(&tesaki_dir)?;

        let repo_state_json = serde_json::to_string_pretty(&repo_state)
            .context("Failed to serialize repo_state.json")?;

        let inputs = MissionInputs {
            status_json: status_json.clone(),
            review_json: review_json.clone(),
            gate_json: gate_json.clone(),
            explain_json: None,
            workspace_json: workspace.to_json()?,
            repo_state_json,
        };

        let mission = MissionBundle::create(
            &tesaki_dir,
            &mission_type,
            &brief,
            &active_stage,
            &surface_policy,
            &surface_definitions,
            &inputs,
            budgets.clone(),
        )?;
        eprintln!("  Mission: {}", mission.id);
        eprintln!("  Path: {}", mission.path.display());

        // Step 5: Invoke runner
        eprintln!("[5/6] Invoking runner ({})...", runner.name());

        // Select model: explicit override > tiered selection
        let selected_model = if model.is_some() {
            model.clone()
        } else {
            let tier = select_model_for_attempt(
                &mission_type,
                attempts_made,
                prev_attempt_failed,
                model_overrides.as_ref(),
            );
            Some(tier.to_string())
        };

        if let Some(ref m) = selected_model {
            eprintln!("  Model: {} (attempt {}, prev_failed={})", m, attempts_made, prev_attempt_failed);
        }

        let runner_config = RunnerConfig {
            max_runtime_seconds,
            working_dir: workspace.working_dir().to_path_buf(),
            model: selected_model,
            stream_output,
        };

        let planned_invocation = runner.planned_invocation(&mission.path, &runner_config);
        if let Some(invocation) = &planned_invocation {
            log_command_run(
                logger,
                &invocation.program,
                &invocation.args,
                PathBuf::from(&invocation.working_dir).as_path(),
                Some(invocation.env.clone()),
            );
        }

        let outcome = runner.run(&mission.path, &runner_config)?;
        eprintln!("  Outcome: {:?} (exit: {:?}, elapsed: {:.1}s)",
            outcome.classification, outcome.exit_code, outcome.elapsed_seconds);

        log_mission_executed(logger, mission.id.as_str(), runner.name(), &outcome);
        if let Some(invocation) = &planned_invocation {
            log_runner_command_result(logger, invocation, &outcome);
        }

        let stop_reason_path = mission.path.join("RUNNER_OUTPUT").join("stop_reason.json");
        let stop_reason_json = serde_json::to_string_pretty(&outcome)
            .context("Failed to serialize runner outcome")?;
        fs::write(&stop_reason_path, stop_reason_json)
            .context("Failed to write RUNNER_OUTPUT/stop_reason.json")?;

        // Determine stop reason from runner outcome
        let runner_stop = match outcome.classification {
            OutcomeClassification::Ok => None,
            OutcomeClassification::Failed => Some(StopReason::RunnerFailed),
            OutcomeClassification::Timeout => Some(StopReason::Budget),
            OutcomeClassification::EnvironmentError => Some(StopReason::EnvironmentError),
            OutcomeClassification::RateLimited => Some(StopReason::RateLimited),
        };

        // If runner failed, check if retryable
        if let Some(stop) = runner_stop {
            if !stop.is_retryable() || attempts_made > max_retries {
                let failed_path = mission.preserve_failed()?;
                let result = RunResult::error(stop.clone(), format!("Runner failed: {:?}", outcome.classification))
                    .with_mission_path(failed_path.display().to_string())
                    .with_missions(attempts_made);
                emit_run_result(&result, &spec_root)?;
                log_session_end(logger, stop.clone(), result.details.clone());
                eprintln!("STOP: {} - After {} attempt(s)", stop, attempts_made);
                eprintln!("Failed mission preserved at: {}", failed_path.display());
                return Ok(());
            }
            // Retryable and attempts remain
            eprintln!("  Runner failed but retryable. {} attempts remaining.", max_retries + 1 - attempts_made);
            prev_attempt_failed = true;
            let _ = mission.preserve_failed()?;
            continue;
        }

        let changes = workspace.compute_changes()?;
        if changes.total_files_changed == 0 {
            let details = "Runner made no file changes.".to_string();
            let failed_path = mission.preserve_failed()?;
            let result = RunResult::error(StopReason::NoProgress, details.clone())
                .with_mission_path(failed_path.display().to_string())
                .with_missions(attempts_made);
            emit_run_result(&result, &spec_root)?;
            save_no_progress_record(&spec_root, &mission_type, &pre_fingerprint)?;
            log_session_end(logger, StopReason::NoProgress, Some(details));
            eprintln!("STOP: NO_PROGRESS - Runner made no changes");
            eprintln!("Failed mission preserved at: {}", failed_path.display());
            return Ok(());
        }

        // Step 6: Validate via namako gate --json
        eprintln!("[6/6] Validating (namako gate --json)...");

        let post_gate_json = run_namako_gate_json(&namako, &adapter, &spec_root, logger)?;
        mission.write_gate_result(&post_gate_json)?;

        let gate_outcome = GateOutcome::from_json_str(&post_gate_json);
        eprintln!("  Gate outcome: {:?}", gate_outcome);
        log_post_gate(logger, gate_outcome, mission.path.join("POST_GATE.json"));

        let post_status_path = out_dir.join("status.post.json");
        let post_review_path = out_dir.join("review.post.json");
        let post_status_json =
            run_namako_status(&namako, &adapter, &spec_root, run_report_opt, logger)?;
        fs::write(&post_status_path, &post_status_json)?;
        let post_review_json = run_namako_review(&namako, &adapter, &spec_root, logger)?;
        fs::write(&post_review_path, &post_review_json)?;

        let post_status_packet = parse_status_json(&post_status_json)?;
        let post_review_packet = parse_review_json(&post_review_json)?;
        let post_gate_packet = parse_gate_json(&post_gate_json)?;
        let post_state = RepoState::compute(&post_status_packet, &post_review_packet, &post_gate_packet, None)?;

        if !has_progress(&repo_state, &post_state, &mission_type) {
            let details = "Post-gate evidence shows no progress for the mission target.".to_string();
            let failed_path = mission.preserve_failed()?;
            let result = RunResult::error(StopReason::NoProgress, details.clone())
                .with_mission_path(failed_path.display().to_string())
                .with_missions(attempts_made);
            emit_run_result(&result, &spec_root)?;
            save_no_progress_record(&spec_root, &mission_type, &pre_fingerprint)?;
            log_session_end(logger, StopReason::NoProgress, Some(details));
            eprintln!("STOP: NO_PROGRESS - No improvement detected");
            eprintln!("Failed mission preserved at: {}", failed_path.display());
            return Ok(());
        }

        match gate_outcome {
            GateOutcome::Pass => {
                // Success!
                let result = RunResult::done(attempts_made, cert_updates_made)
                    .with_mission_path(mission.path.display().to_string());
                emit_run_result(&result, &spec_root)?;
                log_session_end(logger, StopReason::Done, None);
                eprintln!("\nSUCCESS: Mission completed after {} attempt(s)", attempts_made);
                eprintln!("Mission bundle: {}", mission.path.display());
                return Ok(());
            }

            GateOutcome::FailVerifyOnly => {
                // Per TODO.md §A3: Check if update-cert is allowed
                if cert_updates_made >= max_cert_updates {
                    eprintln!("  Verify failed but update-cert limit reached ({}/{})",
                        cert_updates_made, max_cert_updates);
                    // Treat as GATE_FAILED, check if retryable
                } else {
                    eprintln!("  Verify-only failure - attempting update-cert ({}/{} used)...",
                        cert_updates_made, max_cert_updates);

                    // Paths for update-cert
                    let run_report_path = spec_root.join("target/namako_artifacts/run_report.json");
                    let cert_path = spec_root.join("certification.json");

                    // Read old identity for logging
                    let old_identity = read_certification_identity(&cert_path);

                    // Run update-cert
                    let mut update_args: Vec<String> = namako.split_whitespace().map(|s| s.to_string()).collect();
                    update_args.push("update-cert".to_string());
                    update_args.push("-a".to_string());
                    update_args.push(adapter.to_string());
                    update_args.push("--run-report".to_string());
                    update_args.push(run_report_path.display().to_string());
                    update_args.push("--output".to_string());
                    update_args.push(cert_path.display().to_string());
                    log_command_run(logger, "namako", &update_args, &spec_root, None);
                    let update_result = invoker.run_update_cert(
                        &namako,
                        &adapter,
                        &spec_root,
                        &run_report_path,
                        &cert_path,
                    );
                    log_command_result_from_text(
                        logger,
                        "namako",
                        &update_args,
                        update_result.exit_status.unwrap_or(-1),
                        Some(update_result.stdout.clone()),
                        Some(update_result.stderr.clone()),
                    );

                    if update_result.success {
                        cert_updates_made += 1;
                        eprintln!("  ✓ Update-cert succeeded");

                        // Log the update
                        let new_identity = read_certification_identity(&cert_path);
                        if let Some(new_id) = new_identity {
                            let log_entry = UpdateCertLogEntry {
                                timestamp_utc: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                                old_identity,
                                new_identity: new_id,
                                reason: "VERIFY_ONLY_FAIL".to_string(),
                                updates_this_run: cert_updates_made,
                                max_updates_allowed: max_cert_updates,
                            };
                            let log_path = spec_root.join("target/namako_artifacts/tesaki").join(UPDATE_CERT_LOG);
                            append_to_log(&log_path, &log_entry);
                        }

                        // Re-gate to confirm
                        eprintln!("  Re-validating after update-cert...");
                        let recheck_json = run_namako_gate_json(&namako, &adapter, &spec_root, logger)?;

                        // Write POST_GATE_AFTER_UPDATE_CERT.json per TODO.md §A4
                        let after_cert_path = mission.path.join("POST_GATE_AFTER_UPDATE_CERT.json");
                        fs::write(&after_cert_path, &recheck_json)?;

                        let recheck_outcome = GateOutcome::from_json_str(&recheck_json);

                        if recheck_outcome.is_pass() {
                            eprintln!("  ✓ Gate passes after update-cert");
                            let result = RunResult::done(attempts_made, cert_updates_made)
                                .with_mission_path(mission.path.display().to_string());
                            emit_run_result(&result, &spec_root)?;
                            log_session_end(logger, StopReason::Done, None);
                            eprintln!("\nSUCCESS: Mission completed after {} attempt(s), {} cert update(s)",
                                attempts_made, cert_updates_made);
                            return Ok(());
                        } else {
                            eprintln!("  ✗ Gate still failing after update-cert: {:?}", recheck_outcome);
                            // Fall through to GATE_FAILED handling
                        }
                    } else {
                        eprintln!("  ✗ Update-cert failed: {}", update_result.stderr);
                        // Fall through to GATE_FAILED handling
                    }
                }

                // GATE_FAILED - check if retryable
                if StopReason::GateFailed.is_retryable() && attempts_made <= max_retries {
                    eprintln!("  Gate failed but retryable. {} attempts remaining.",
                        max_retries + 1 - attempts_made);
                    let _ = mission.preserve_failed()?;
                    continue;
                }

                let failed_path = mission.preserve_failed()?;
                let result = RunResult::error(StopReason::GateFailed, "Post-run gate failed (verify-only after update-cert)")
                    .with_mission_path(failed_path.display().to_string())
                    .with_missions(attempts_made)
                    .with_cert_updates(cert_updates_made);
                emit_run_result(&result, &spec_root)?;
                log_session_end(logger, StopReason::GateFailed, result.details.clone());
                eprintln!("\nSTOP: GATE_FAILED - Verify still failing after update-cert");
                eprintln!("Failed mission preserved at: {}", failed_path.display());
                return Ok(());
            }

            GateOutcome::FailOther => {
                // lint or run failed - NO update-cert attempt per TODO.md §A3
                eprintln!("  Gate failed (lint or run) - no update-cert attempt");

                if StopReason::GateFailed.is_retryable() && attempts_made <= max_retries {
                    eprintln!("  Gate failed but retryable. {} attempts remaining.",
                        max_retries + 1 - attempts_made);
                    let _ = mission.preserve_failed()?;
                    continue;
                }

                let failed_path = mission.preserve_failed()?;
                let result = RunResult::error(StopReason::GateFailed, "Post-run gate failed (lint or run)")
                    .with_mission_path(failed_path.display().to_string())
                    .with_missions(attempts_made);
                emit_run_result(&result, &spec_root)?;
                log_session_end(logger, StopReason::GateFailed, result.details.clone());
                eprintln!("\nSTOP: GATE_FAILED - Post-run validation failed");
                eprintln!("Failed mission preserved at: {}", failed_path.display());
                return Ok(());
            }
        }
    }
}

/// Run namako gate --json and return the JSON output.
fn run_namako_gate_json(
    namako: &str,
    adapter: &str,
    spec_root: &PathBuf,
    logger: &logging::JsonlLogger,
) -> Result<String> {
    let args: Vec<&str> = namako.split_whitespace().collect();
    let (program, namako_args) = args.split_first().context("Empty namako command")?;

    let mut cmd_args: Vec<String> = namako_args.iter().map(|s| s.to_string()).collect();
    cmd_args.push("gate".to_string());
    cmd_args.push("-s".to_string());
    cmd_args.push(".".to_string());
    cmd_args.push("-a".to_string());
    cmd_args.push(adapter.to_string());
    cmd_args.push("--json".to_string());
    cmd_args.push("--auto-cert".to_string()); // Self-heal on baseline drift
    log_command_run(logger, "namako", &cmd_args, spec_root, None);

    let output = Command::new(program)
        .args(&cmd_args)
        .current_dir(spec_root)
        .output()
        .context("Failed to run namako gate --json")?;

    log_command_result(logger, "namako", &cmd_args, &output);

    // gate --json outputs to stdout even on failure (with status in JSON)
    let stdout = String::from_utf8(output.stdout)
        .context("namako gate output is not valid UTF-8")?;

    // If stdout is empty but we have stderr, the command itself failed
    if stdout.trim().is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("namako gate --json produced no output. stderr: {}", stderr);
    }

    Ok(stdout)
}

#[derive(Debug, Clone, Copy)]
enum NamakoCliSource {
    Explicit,
    Config,
    Path,
    WorkspaceDefault,
}

impl NamakoCliSource {
    fn label(&self) -> &'static str {
        match self {
            NamakoCliSource::Explicit => "explicit",
            NamakoCliSource::Config => "from config",
            NamakoCliSource::Path => "from PATH",
            NamakoCliSource::WorkspaceDefault => "workspace default",
        }
    }
}

#[derive(Debug, Clone)]
struct NamakoCliResolution {
    command: String,
    source: NamakoCliSource,
}

impl NamakoCliResolution {
    pub(crate) fn explicit(command: String) -> Self {
        Self {
            command,
            source: NamakoCliSource::Explicit,
        }
    }
}

fn resolve_namako_cli(
    explicit: Option<String>,
    config_value: Option<String>,
    spec_root: &PathBuf,
) -> NamakoCliResolution {
    if let Some(cmd) = explicit {
        return NamakoCliResolution {
            command: cmd,
            source: NamakoCliSource::Explicit,
        };
    }

    if let Some(cmd) = config_value {
        return NamakoCliResolution {
            command: cmd,
            source: NamakoCliSource::Config,
        };
    }

    if let Some(path) = find_executable_on_path("namako") {
        return NamakoCliResolution {
            command: path.display().to_string(),
            source: NamakoCliSource::Path,
        };
    }

    let namako_root = spec_root
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .map(|p| p.join("namako"))
        .unwrap_or_else(|| PathBuf::from("/home/ccarpenter/Personal/specops/namako"));
    let namako_root = if namako_root.join("Cargo.toml").is_file() {
        namako_root
    } else {
        PathBuf::from("/home/ccarpenter/Personal/specops/namako")
    };
    let command = format!(
        "cargo run -p namako-cli --manifest-path {}/Cargo.toml -q --",
        namako_root.display()
    );
    NamakoCliResolution {
        command,
        source: NamakoCliSource::WorkspaceDefault,
    }
}

fn find_executable_on_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub(crate) fn log_session_start(
    logger: &logging::JsonlLogger,
    spec_root: &PathBuf,
    adapter: &str,
    namako_cli: &NamakoCliResolution,
    runner: Option<&str>,
    planner: Option<&str>,
) {
    let cwd = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .display()
        .to_string();
    let config = logging::ConfigLog {
        specs_dir: spec_root.display().to_string(),
        adapter_cmd: adapter.to_string(),
        namako_cli: Some(namako_cli.command.clone()),
        namako_cli_source: Some(namako_cli.source.label().to_string()),
        runner: runner.map(|s| s.to_string()),
        planner: planner.map(|s| s.to_string()),
    };
    logger.log_event(logging::LogEvent::SessionStart {
        cwd,
        config: Some(config),
        runner: runner.map(|s| s.to_string()),
    });
}

pub(crate) fn log_session_end(
    logger: &logging::JsonlLogger,
    reason: stop_reason::StopReason,
    details: Option<String>,
) {
    logger.log_event(logging::LogEvent::SessionEnd {
        stop_reason: stop_reason_label(reason).to_string(),
        details,
    });
}

pub(crate) fn log_command_run(
    logger: &logging::JsonlLogger,
    tool: &str,
    args: &[String],
    cwd: &Path,
    env: Option<Vec<(String, String)>>,
) {
    logger.log_event(logging::LogEvent::CommandRun {
        tool: tool.to_string(),
        args: args.to_vec(),
        cwd: cwd.display().to_string(),
        env,
    });
}

pub(crate) fn log_command_result(
    logger: &logging::JsonlLogger,
    tool: &str,
    args: &[String],
    output: &Output,
) {
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);
    let payload = logging::CommandResultLog {
        tool: tool.to_string(),
        args: args.to_vec(),
        exit_code,
        stdout: Some(stdout),
        stderr: Some(stderr),
        stdout_path: None,
        stderr_path: None,
    };
    let logged = logger.log_command_result(payload);
    logger.log_event(logging::LogEvent::CommandResult {
        tool: logged.tool,
        args: logged.args,
        exit_code: logged.exit_code,
        stdout: logged.stdout,
        stderr: logged.stderr,
        stdout_path: logged.stdout_path,
        stderr_path: logged.stderr_path,
    });
}

pub(crate) fn log_command_result_from_text(
    logger: &logging::JsonlLogger,
    tool: &str,
    args: &[String],
    exit_code: i32,
    stdout: Option<String>,
    stderr: Option<String>,
) {
    let payload = logging::CommandResultLog {
        tool: tool.to_string(),
        args: args.to_vec(),
        exit_code,
        stdout,
        stderr,
        stdout_path: None,
        stderr_path: None,
    };
    let logged = logger.log_command_result(payload);
    logger.log_event(logging::LogEvent::CommandResult {
        tool: logged.tool,
        args: logged.args,
        exit_code: logged.exit_code,
        stdout: logged.stdout,
        stderr: logged.stderr,
        stdout_path: logged.stdout_path,
        stderr_path: logged.stderr_path,
    });
}

fn surface_log(surface_policy: &crate::surface_policy::SurfacePolicy) -> logging::SurfaceLog {
    logging::SurfaceLog {
        spec: lock_label(surface_policy.spec).to_string(),
        tests: lock_label(surface_policy.tests_bindings).to_string(),
        sut: lock_label(surface_policy.sut).to_string(),
    }
}

pub(crate) fn log_mission_proposed(
    logger: &logging::JsonlLogger,
    mission_type: &crate::mission_type::MissionType,
    stage: &crate::stage::Stage,
    surface_policy: &crate::surface_policy::SurfacePolicy,
) {
    let target = mission_type.target_label().unwrap_or_else(|| "none".to_string());
    logger.log_event(logging::LogEvent::MissionProposed {
        mission_type: mission_type.name().to_string(),
        stage: stage.name().to_string(),
        target,
        surfaces: surface_log(surface_policy),
    });
}

pub(crate) fn log_mission_executed(
    logger: &logging::JsonlLogger,
    mission_id: &str,
    runner: &str,
    outcome: &crate::runner::RunnerOutcome,
) {
    logger.log_event(logging::LogEvent::MissionExecuted {
        mission_id: mission_id.to_string(),
        runner: runner.to_string(),
        outcome: outcome.clone(),
    });
}

pub(crate) fn log_post_gate(
    logger: &logging::JsonlLogger,
    outcome: crate::gate::GateOutcome,
    post_gate_path: PathBuf,
) {
    logger.log_event(logging::LogEvent::PostGate {
        outcome: format!("{:?}", outcome),
        post_gate_path: post_gate_path.display().to_string(),
    });
}

pub(crate) fn log_runner_command_result(
    logger: &logging::JsonlLogger,
    invocation: &crate::runner::RunnerInvocation,
    outcome: &crate::runner::RunnerOutcome,
) {
    let stdout = outcome
        .stdout_path
        .as_ref()
        .and_then(|p| fs::read_to_string(p).ok());
    let stderr = outcome
        .stderr_path
        .as_ref()
        .and_then(|p| fs::read_to_string(p).ok());
    log_command_result_from_text(
        logger,
        &invocation.program,
        &invocation.args,
        outcome.exit_code.unwrap_or(-1),
        stdout,
        stderr,
    );
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct NoProgressRecord {
    mission_type: String,
    target: Option<String>,
    packets_fingerprint: String,
    timestamp_utc: String,
}

impl NoProgressRecord {
    fn matches(&self, mission_type: &crate::mission_type::MissionType, fingerprint: &str) -> bool {
        self.mission_type == mission_type.name()
            && self.target == mission_type.target_label()
            && self.packets_fingerprint == fingerprint
    }
}

fn no_progress_path(spec_root: &PathBuf) -> PathBuf {
    spec_root.join(".tesaki").join("no_progress.json")
}

fn load_no_progress_record(spec_root: &PathBuf) -> Option<NoProgressRecord> {
    let path = no_progress_path(spec_root);
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn save_no_progress_record(
    spec_root: &PathBuf,
    mission_type: &crate::mission_type::MissionType,
    fingerprint: &str,
) -> Result<()> {
    let record = NoProgressRecord {
        mission_type: mission_type.name().to_string(),
        target: mission_type.target_label(),
        packets_fingerprint: fingerprint.to_string(),
        timestamp_utc: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    };
    let path = no_progress_path(spec_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&record)?;
    fs::write(path, json)?;
    Ok(())
}

fn fingerprint_packets(status: &str, review: &str, gate: &str) -> String {
    let mut data = String::new();
    data.push_str(status);
    data.push_str(review);
    data.push_str(gate);
    let hash = blake3::hash(data.as_bytes());
    hash.to_hex().to_string()
}

fn has_progress(
    pre: &crate::repo_state::RepoState,
    post: &crate::repo_state::RepoState,
    mission_type: &crate::mission_type::MissionType,
) -> bool {
    let gate_improved = !pre.all_gates_pass() && post.all_gates_pass();
    let issue_count_decrease = post.total_issue_count() < pre.total_issue_count();

    match mission_type {
        crate::mission_type::MissionType::CreateMissingBindings { scenario_key, .. } => {
            let pre_target = count_missing_bindings(pre, Some(scenario_key));
            let post_target = count_missing_bindings(post, Some(scenario_key));
            (pre_target > 0 && post_target < pre_target)
                || post.binding_issues.len() < pre.binding_issues.len()
                || gate_improved
        }
        crate::mission_type::MissionType::ImplementBehaviorForScenario { scenario_key, .. } => {
            let pre_target = count_failures(pre, scenario_key);
            let post_target = count_failures(post, scenario_key);
            (pre_target > 0 && post_target == 0)
                || post.sut_issues.len() < pre.sut_issues.len()
                || gate_improved
        }
        crate::mission_type::MissionType::FixRegressionFromGateFailure { failure } => {
            let pre_target = count_failures(pre, &failure.scenario_key);
            let post_target = count_failures(post, &failure.scenario_key);
            (pre_target > 0 && post_target == 0)
                || post.sut_issues.len() < pre.sut_issues.len()
                || gate_improved
        }
        crate::mission_type::MissionType::NormalizeIdentityTags { .. }
        | crate::mission_type::MissionType::RefineFeatureIntent { .. }
        | crate::mission_type::MissionType::AddOrClarifyScenario { .. } => {
            post.spec_issues.len() < pre.spec_issues.len()
                || post.structure_issues.len() < pre.structure_issues.len()
                || gate_improved
        }
        crate::mission_type::MissionType::StrengthenThenAssertions { .. }
        | crate::mission_type::MissionType::RefactorBindingsForClarity { .. }
        | crate::mission_type::MissionType::SummarizeAndClose
        | crate::mission_type::MissionType::CleanupAfterSuccess => {
            gate_improved || issue_count_decrease
        }
        crate::mission_type::MissionType::ExplainState
        | crate::mission_type::MissionType::TriageFailures => true,
    }
}

fn count_missing_bindings(state: &crate::repo_state::RepoState, scenario_key: Option<&str>) -> usize {
    state
        .binding_issues
        .iter()
        .filter(|issue| issue.kind == crate::repo_state::BindingIssueKind::MissingBinding)
        .filter(|issue| match (scenario_key, issue.scenario_key.as_deref()) {
            (Some(target), Some(key)) => key == target,
            (Some(_), None) => false,
            (None, _) => true,
        })
        .count()
}

fn count_failures(state: &crate::repo_state::RepoState, scenario_key: &str) -> usize {
    state
        .sut_issues
        .iter()
        .filter(|issue| issue.scenario_key == scenario_key)
        .count()
}

fn stop_reason_label(reason: stop_reason::StopReason) -> &'static str {
    match reason {
        stop_reason::StopReason::Done => "DONE",
        stop_reason::StopReason::Blocked => "BLOCKED",
        stop_reason::StopReason::HumanRequired => "HUMAN_REQUIRED",
        stop_reason::StopReason::EnvironmentError => "ENVIRONMENT_ERROR",
        stop_reason::StopReason::Budget => "BUDGET",
        stop_reason::StopReason::RunnerFailed => "RUNNER_FAILED",
        stop_reason::StopReason::NoProgress => "NO_PROGRESS",
        stop_reason::StopReason::GateFailed => "GATE_FAILED",
        stop_reason::StopReason::RateLimited => "RATE_LIMITED",
    }
}

/// Emit the run result to .tesaki/last_run.json.
fn emit_run_result(result: &stop_reason::RunResult, spec_root: &PathBuf) -> Result<()> {
    let tesaki_dir = spec_root.join(".tesaki");
    fs::create_dir_all(&tesaki_dir)?;

    let json = serde_json::to_string_pretty(result)?;
    fs::write(tesaki_dir.join("last_run.json"), &json)?;

    Ok(())
}

fn lock_label(lock: crate::surface_policy::SurfaceLock) -> &'static str {
    match lock {
        crate::surface_policy::SurfaceLock::Locked => "LOCKED",
        crate::surface_policy::SurfaceLock::Unlocked => "UNLOCKED",
    }
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

    /// Test that stub scenarios are filtered from eligible candidates (defense-in-depth).
    /// Per TODO.md §2: Tesaki must NEVER select @Stub scenarios as tasks.
    #[test]
    fn test_stub_scenarios_excluded_from_eligible_candidates() {
        // Create candidates with various properties
        let candidates = vec![
            PromotionCandidate {
                scenario_name: "Real scenario".to_string(),
                feature_path: "features/test.feature".to_string(),
                rule_name: "Rule(01)".to_string(),
                reuse_score: 5.0,
                new_step_texts_estimate: 2,
                blocker: BlockerType::Unknown,
                is_stub: false, // Real candidate
            },
            PromotionCandidate {
                scenario_name: "Stub scenario".to_string(),
                feature_path: "features/_orphan_stubs.feature".to_string(),
                rule_name: "default".to_string(),
                reuse_score: 0.0,
                new_step_texts_estimate: 1,
                blocker: BlockerType::Unknown,
                is_stub: true, // STUB - should be excluded
            },
            PromotionCandidate {
                scenario_name: "Another real scenario".to_string(),
                feature_path: "features/core.feature".to_string(),
                rule_name: "Rule(01)".to_string(),
                reuse_score: 3.0,
                new_step_texts_estimate: 1,
                blocker: BlockerType::Core,
                is_stub: false,
            },
        ];

        // Simulate filtering (as in generate_next_task)
        let eligible: Vec<_> = candidates.iter()
            .filter(|c| !c.is_stub)
            .collect();

        // Both real scenarios should be eligible (stubs excluded)
        assert_eq!(eligible.len(), 2);
        assert_eq!(eligible[0].scenario_name, "Real scenario");
        assert_eq!(eligible[1].scenario_name, "Another real scenario");

        // Verify stub was excluded
        let has_stub = eligible.iter().any(|c| c.scenario_name == "Stub scenario");
        assert!(!has_stub, "Stub scenario should be excluded from eligible list");
    }
}
