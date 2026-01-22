//! JSONL logging for Tesaki diagnostics.
//!
//! This module provides two logging mechanisms:
//! 1. Standard `log` crate macros for human-readable console output
//! 2. Optional JSONL file logging for machine-readable diagnostics
//!
//! Configure console verbosity via `RUST_LOG` environment variable.
//! Example: `RUST_LOG=tesaki=debug tesaki`

use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Initialize logging with env_logger.
///
/// Call this once at program startup.
pub fn init() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("tesaki=info"))
        .format(|buf, record| writeln!(buf, "[{}] {}", record.level(), record.args()))
        .init();
}

const MAX_LOG_BYTES: usize = 2000;
const MAX_CONSOLE_BYTES: usize = 4000;

#[derive(Debug)]
pub struct JsonlLogger {
    path: Option<PathBuf>,
    counter: AtomicU64,
    console: ConsoleMode,
}

impl JsonlLogger {
    pub fn new_with_console(path: Option<PathBuf>, console: ConsoleMode) -> Self {
        Self {
            path,
            counter: AtomicU64::new(0),
            console,
        }
    }

    pub fn enabled(&self) -> bool {
        self.path.is_some()
    }

    pub fn log_event(&self, event: LogEvent) {
        if self.console != ConsoleMode::Off {
            print_event(&event, self.console);
        }

        let path = match &self.path {
            Some(path) => path,
            None => return,
        };

        let record = LogRecord {
            timestamp_utc: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            event,
        };

        let line = match serde_json::to_string(&record) {
            Ok(json) => json,
            Err(err) => {
                eprintln!("WARNING: Failed to serialize log event: {}", err);
                return;
            }
        };

        if let Err(err) = append_line(path, &line) {
            eprintln!("WARNING: Failed to write log event: {}", err);
        }
    }

    pub fn log_command_result(&self, input: CommandResultLog) -> CommandResultLog {
        if !self.enabled() {
            return input;
        }

        let mut updated = input;
        if let Some(stdout) = &updated.stdout {
            let (snippet, full_path) = self.truncate_output("stdout", stdout);
            updated.stdout = snippet;
            updated.stdout_path = full_path;
        }
        if let Some(stderr) = &updated.stderr {
            let (snippet, full_path) = self.truncate_output("stderr", stderr);
            updated.stderr = snippet;
            updated.stderr_path = full_path;
        }

        updated
    }

    pub fn truncate_output(&self, label: &str, content: &str) -> (Option<String>, Option<String>) {
        if content.is_empty() {
            return (None, None);
        }

        if content.len() <= MAX_LOG_BYTES {
            return (Some(content.to_string()), None);
        }

        let truncated = truncate_string(content, MAX_LOG_BYTES);
        let full_path = self.write_full_output(label, content);
        (Some(truncated), full_path)
    }

    fn write_full_output(&self, label: &str, content: &str) -> Option<String> {
        let log_path = self.path.as_ref()?;
        let base_dir = log_path.parent().unwrap_or_else(|| Path::new("."));
        let output_dir = base_dir.join("tesaki_logs");
        if fs::create_dir_all(&output_dir).is_err() {
            return None;
        }

        let id = self.counter.fetch_add(1, Ordering::Relaxed);
        let file_name = format!("{}_{}.txt", label, id);
        let full_path = output_dir.join(file_name);
        if fs::write(&full_path, content.as_bytes()).is_err() {
            return None;
        }

        Some(full_path.display().to_string())
    }
}

#[derive(Debug, Serialize)]
pub struct LogRecord {
    pub timestamp_utc: String,
    #[serde(flatten)]
    pub event: LogEvent,
}

#[derive(Debug, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum LogEvent {
    SessionStart {
        cwd: String,
        config: Option<ConfigLog>,
        runner: Option<String>,
    },
    PlannerPlan {
        parse_status: String,
        plan_json: Option<String>,
        error: Option<String>,
    },
    AllowlistReject {
        tool: String,
        args: Vec<String>,
        reason: String,
    },
    CommandRun {
        tool: String,
        args: Vec<String>,
        cwd: String,
        env: Option<Vec<(String, String)>>,
    },
    CommandResult {
        tool: String,
        args: Vec<String>,
        exit_code: i32,
        stdout: Option<String>,
        stderr: Option<String>,
        stdout_path: Option<String>,
        stderr_path: Option<String>,
    },
    MissionProposed {
        mission_type: String,
        stage: String,
        target: String,
        surfaces: SurfaceLog,
    },
    MissionExecuted {
        mission_id: String,
        runner: String,
        outcome: crate::runner::RunnerOutcome,
    },
    PostGate {
        outcome: String,
        post_gate_path: String,
    },
    SessionEnd {
        stop_reason: String,
        details: Option<String>,
    },
}

#[derive(Debug, Serialize)]
pub struct SurfaceLog {
    pub spec: String,
    pub tests: String,
    pub sut: String,
}

#[derive(Debug, Serialize)]
pub struct ConfigLog {
    pub specs_dir: String,
    pub adapter_cmd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namako_cli: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namako_cli_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planner: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CommandResultLog {
    pub tool: String,
    pub args: Vec<String>,
    pub exit_code: i32,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub stdout_path: Option<String>,
    pub stderr_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleMode {
    Off,
    Commands,
}

fn print_event(event: &LogEvent, mode: ConsoleMode) {
    match event {
        LogEvent::AllowlistReject { tool, args, reason } => {
            warn!(
                "Command rejected: {} {} ({}). Try an allowlisted command like `namako status --json`",
                tool,
                args.join(" "),
                reason
            );
        }
        LogEvent::CommandRun { tool, args, .. } => {
            info!("> {} {}", tool, args.join(" "));
        }
        LogEvent::CommandResult {
            tool,
            args,
            exit_code,
            stdout,
            stderr,
            stdout_path,
            stderr_path,
            ..
        } => {
            if mode == ConsoleMode::Commands {
                debug!("{} exited with {}", tool, exit_code);
                let is_gate_json = is_gate_json_command(tool, args);
                let mut gate_summarized = false;
                if let Some(out) = stdout {
                    if is_gate_json {
                        if let Some(summary) = summarize_gate_output(out) {
                            println!("{}", summary);
                            gate_summarized = true;
                        } else {
                            print_stream("stdout", out, stdout_path.as_deref());
                        }
                    } else {
                        print_stream("stdout", out, stdout_path.as_deref());
                    }
                }
                if let Some(err) = stderr {
                    if is_gate_json && gate_summarized {
                        let trimmed = err.trim();
                        if !trimmed.is_empty() {
                            let clipped = truncate_string(trimmed, MAX_CONSOLE_BYTES);
                            debug!("gate stderr (suppressed): {}", clipped);
                        }
                    } else {
                        print_stream("stderr", err, stderr_path.as_deref());
                    }
                }
            }
        }
        _ => {}
    }
}

fn append_line(path: &Path, line: &str) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{}", line)?;
    Ok(())
}

fn is_gate_json_command(tool: &str, args: &[String]) -> bool {
    if tool != "namako" {
        return false;
    }
    let has_gate = args.iter().any(|arg| arg == "gate");
    let has_json = args.iter().any(|arg| arg == "--json");
    has_gate && has_json
}

#[derive(Debug, Deserialize)]
struct GateSummary {
    lint: GateSummaryEntry,
    run: GateSummaryEntry,
    verify: GateSummaryEntry,
}

#[derive(Debug, Deserialize)]
struct GateSummaryEntry {
    status: String,
    #[serde(default)]
    reason: Option<String>,
}

fn summarize_gate_output(stdout: &str) -> Option<String> {
    let summary: GateSummary = serde_json::from_str(stdout).ok()?;
    let lint = gate_entry_label(&summary.lint);
    let run = gate_entry_label(&summary.run);
    let verify = gate_entry_label(&summary.verify);
    let mut line = format!("Gate: lint={}, run={}, verify={}.", lint, run, verify);
    line.push_str(" Details: run `namako status --json` or `tesaki explain`.");
    Some(line)
}

fn gate_entry_label(entry: &GateSummaryEntry) -> String {
    let status = entry.status.as_str();
    if let Some(reason) = entry.reason.as_ref() {
        if status == "fail" {
            return format!("fail ({})", reason);
        }
        return format!("{} ({})", status, reason);
    }
    status.to_string()
}

fn truncate_string(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let suffix = "...[truncated]";
    let keep = max_bytes.saturating_sub(suffix.len());
    let mut end = keep;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{}", &s[..end], suffix)
}

fn print_stream(label: &str, content: &str, full_path: Option<&str>) {
    let mut text = content.to_string();
    if text.len() > MAX_CONSOLE_BYTES {
        text = truncate_string(&text, MAX_CONSOLE_BYTES);
        if let Some(path) = full_path {
            text.push_str(&format!("\n[full {} logged at {}]", label, path));
        }
    }
    if !text.trim().is_empty() {
        println!("{}:\n{}", label, text.trim_end());
    }
}
