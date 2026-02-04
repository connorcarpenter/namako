//! Shared base runner utilities for ClaudeCode and Codex runners.
//!
//! This module contains common code shared by both CLI-based runners to avoid duplication.

use crate::runner::{OutcomeClassification, RunnerConfig, RunnerOutcome};
use crate::token_usage::TokenUsage;
use anyhow::Result;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Result of waiting for a child process.
#[allow(dead_code)]
pub(crate) enum WaitResult {
    Completed(std::process::Output),
    Timeout,
    Error(std::io::Error),
}

struct StreamLine {
    text: String,
    newline: bool,
}

fn format_stream_line(line: &str) -> Option<StreamLine> {
    let trimmed = line.trim();
    if !trimmed.starts_with('{') {
        return Some(StreamLine {
            text: line.to_string(),
            newline: true,
        });
    }

    let value: Value = match serde_json::from_str(trimmed) {
        Ok(val) => val,
        Err(_) => {
            return Some(StreamLine {
                text: line.to_string(),
                newline: true,
            })
        }
    };

    let line_type = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match line_type {
        "stream_event" => format_claude_stream_event(&value),
        "user" => format_claude_user_event(&value),
        "assistant" | "result" | "system" => None,
        _ => Some(StreamLine {
            text: line.to_string(),
            newline: true,
        }),
    }
}

fn format_claude_stream_event(value: &Value) -> Option<StreamLine> {
    let event = value.get("event")?;
    let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match event_type {
        "content_block_delta" => {
            let delta = event.get("delta")?;
            let delta_type = delta.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if delta_type == "text_delta" {
                let text = delta.get("text").and_then(|v| v.as_str()).unwrap_or("");
                if text.is_empty() {
                    return None;
                }
                return Some(StreamLine {
                    text: text.to_string(),
                    newline: false,
                });
            }
            None
        }
        "content_block_start" => {
            let block = event.get("content_block")?;
            let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if block_type == "tool_use" {
                let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("tool");
                return Some(StreamLine {
                    text: format!("\n[tool] {}", name),
                    newline: true,
                });
            }
            None
        }
        "message_stop" => Some(StreamLine {
            text: String::new(),
            newline: true,
        }),
        _ => None,
    }
}

fn format_claude_user_event(value: &Value) -> Option<StreamLine> {
    if let Some(tool_result) = value.get("tool_use_result") {
        let stdout = tool_result
            .get("stdout")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let stderr = tool_result
            .get("stderr")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let content = if !stdout.trim().is_empty() { stdout } else { stderr };
        if !content.trim().is_empty() {
            return Some(StreamLine {
                text: format!("\n[tool_result] {}", truncate_line(content, 200)),
                newline: true,
            });
        }
    }

    let content_array = value
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array());

    if let Some(items) = content_array {
        for item in items {
            let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if item_type == "tool_result" {
                let content = item.get("content").and_then(|v| v.as_str()).unwrap_or("");
                if !content.trim().is_empty() {
                    return Some(StreamLine {
                        text: format!("\n[tool_result] {}", truncate_line(content, 200)),
                        newline: true,
                    });
                }
            }
        }
    }

    None
}

/// Wait for a child process with a timeout.
#[allow(dead_code)]
pub(crate) fn wait_with_timeout(mut child: Child, timeout: Duration) -> WaitResult {
    // Simple polling approach for timeout
    let start = Instant::now();
    let poll_interval = Duration::from_millis(100);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // Process exited
                let stdout = child
                    .stdout
                    .take()
                    .map(|mut s| {
                        let mut buf = Vec::new();
                        let _ = s.read_to_end(&mut buf);
                        buf
                    })
                    .unwrap_or_default();

                let stderr = child
                    .stderr
                    .take()
                    .map(|mut s| {
                        let mut buf = Vec::new();
                        let _ = s.read_to_end(&mut buf);
                        buf
                    })
                    .unwrap_or_default();

                return WaitResult::Completed(std::process::Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            Ok(None) => {
                // Still running
                if start.elapsed() > timeout {
                    // Kill the process
                    let _ = child.kill();
                    let _ = child.wait(); // Reap the zombie
                    return WaitResult::Timeout;
                }
                std::thread::sleep(poll_interval);
            }
            Err(e) => return WaitResult::Error(e),
        }
    }
}

/// Wait for a child process with streaming output.
/// If stream is true, output is tee'd to stdout/stderr in real-time.
/// Returns (exit_status, stdout_bytes, stderr_bytes).
fn wait_with_streaming(
    mut child: Child,
    timeout: Duration,
    stream: bool,
) -> Result<(ExitStatus, Vec<u8>, Vec<u8>), std::io::Error> {
    let stdout_handle = child.stdout.take();
    let stderr_handle = child.stderr.take();

    let stdout_buf = Arc::new(Mutex::new(Vec::new()));
    let stderr_buf = Arc::new(Mutex::new(Vec::new()));

    let stdout_buf_clone = Arc::clone(&stdout_buf);
    let stderr_buf_clone = Arc::clone(&stderr_buf);

    // Spawn thread to read stdout
    let stdout_thread = stdout_handle.map(|stdout| {
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                if let Ok(line) = line {
                    if stream {
                        if let Some(rendered) = format_stream_line(&line) {
                            if rendered.newline {
                                let _ = writeln!(std::io::stdout(), "{}", rendered.text);
                            } else {
                                let _ = write!(std::io::stdout(), "{}", rendered.text);
                                let _ = std::io::stdout().flush();
                            }
                        }
                    }
                    let mut buf = stdout_buf_clone.lock().unwrap();
                    buf.extend_from_slice(line.as_bytes());
                    buf.push(b'\n');
                }
            }
        })
    });

    // Spawn thread to read stderr
    let stderr_thread = stderr_handle.map(|stderr| {
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                if let Ok(line) = line {
                    if stream {
                        if let Some(rendered) = format_stream_line(&line) {
                            if rendered.newline {
                                let _ = writeln!(std::io::stderr(), "{}", rendered.text);
                            } else {
                                let _ = write!(std::io::stderr(), "{}", rendered.text);
                                let _ = std::io::stderr().flush();
                            }
                        }
                    }
                    let mut buf = stderr_buf_clone.lock().unwrap();
                    buf.extend_from_slice(line.as_bytes());
                    buf.push(b'\n');
                }
            }
        })
    });

    // Poll for completion with timeout
    let start = Instant::now();
    let poll_interval = Duration::from_millis(100);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // Wait for reader threads to finish
                if let Some(t) = stdout_thread {
                    let _ = t.join();
                }
                if let Some(t) = stderr_thread {
                    let _ = t.join();
                }

                let stdout = match Arc::try_unwrap(stdout_buf) {
                    Ok(mutex) => mutex.into_inner().unwrap_or_default(),
                    Err(arc) => arc.lock().unwrap().clone(),
                };
                let stderr = match Arc::try_unwrap(stderr_buf) {
                    Ok(mutex) => mutex.into_inner().unwrap_or_default(),
                    Err(arc) => arc.lock().unwrap().clone(),
                };

                return Ok((status, stdout, stderr));
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "Process timed out",
                    ));
                }
                thread::sleep(poll_interval);
            }
            Err(e) => return Err(e),
        }
    }
}

/// Run a CLI runner with the given command template and config.
///
/// This is the core execution logic shared by both ClaudeCode and Codex runners.
pub(crate) fn run_cli_runner(
    command_template: &str,
    mission_dir: &Path,
    config: &RunnerConfig,
    extract_error: bool,
) -> Result<RunnerOutcome> {
    let parts: Vec<&str> = command_template.split_whitespace().collect();

    if parts.is_empty() {
        return Ok(RunnerOutcome {
            exit_code: None,
            classification: OutcomeClassification::EnvironmentError,
            elapsed_seconds: 0.0,
            stdout_path: None,
            stderr_path: None,
            error_message: Some("Empty command".to_string()),
            token_usage: None,
        });
    }

    let program = parts[0];
    let args: Vec<&str> = parts[1..].to_vec();

    let start = Instant::now();
    let timeout = Duration::from_secs(config.max_runtime_seconds as u64);

    // Set up environment variables
    let mission_dir_abs =
        std::fs::canonicalize(mission_dir).unwrap_or_else(|_| mission_dir.to_path_buf());

    // Read the MISSION.md file to provide as stdin prompt
    let mission_path = mission_dir.join("MISSION.md");
    let prompt = match std::fs::read_to_string(&mission_path) {
        Ok(content) => content,
        Err(e) => {
            return Ok(RunnerOutcome {
                exit_code: None,
                classification: OutcomeClassification::EnvironmentError,
                elapsed_seconds: start.elapsed().as_secs_f64(),
                stdout_path: None,
                stderr_path: None,
                error_message: Some(format!(
                    "Failed to read MISSION.md from {}: {}",
                    mission_path.display(),
                    e
                )),
                token_usage: None,
            });
        }
    };

    let mut cmd = Command::new(program);
    cmd.args(&args)
        .current_dir(&config.working_dir)
        .env("TESAKI_MISSION_DIR", &mission_dir_abs)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Ok(RunnerOutcome {
                exit_code: None,
                classification: OutcomeClassification::EnvironmentError,
                elapsed_seconds: start.elapsed().as_secs_f64(),
                stdout_path: None,
                stderr_path: None,
                error_message: Some(format!("Failed to start runner: {}", e)),
                token_usage: None,
            });
        }
    };

    // Write the prompt to stdin
    if let Some(mut stdin) = child.stdin.take() {
        if let Err(e) = stdin.write_all(prompt.as_bytes()) {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(RunnerOutcome {
                exit_code: None,
                classification: OutcomeClassification::EnvironmentError,
                elapsed_seconds: start.elapsed().as_secs_f64(),
                stdout_path: None,
                stderr_path: None,
                error_message: Some(format!("Failed to write prompt to stdin: {}", e)),
                token_usage: None,
            });
        }
        // stdin is dropped here, closing the pipe
    }

    // Wait with streaming support
    let (status, stdout_bytes, stderr_bytes) = match wait_with_streaming(child, timeout, config.stream_output) {
        Ok(result) => result,
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
            return Ok(RunnerOutcome {
                exit_code: None,
                classification: OutcomeClassification::Timeout,
                elapsed_seconds: start.elapsed().as_secs_f64(),
                stdout_path: None,
                stderr_path: None,
                error_message: Some(format!(
                    "Runner exceeded timeout of {} seconds",
                    config.max_runtime_seconds
                )),
                token_usage: None,
            });
        }
        Err(e) => {
            return Ok(RunnerOutcome {
                exit_code: None,
                classification: OutcomeClassification::EnvironmentError,
                elapsed_seconds: start.elapsed().as_secs_f64(),
                stdout_path: None,
                stderr_path: None,
                error_message: Some(format!("Error waiting for runner: {}", e)),
                token_usage: None,
            });
        }
    };

    // Create a synthetic Output for downstream processing
    let output = std::process::Output {
        status,
        stdout: stdout_bytes,
        stderr: stderr_bytes,
    };

    let elapsed = start.elapsed().as_secs_f64();
    let exit_code = output.status.code();
    let classification = if output.status.success() {
        OutcomeClassification::Ok
    } else if is_rate_limited(&output) {
        OutcomeClassification::RateLimited
    } else {
        OutcomeClassification::Failed
    };

    // Write stdout/stderr to mission RUNNER_OUTPUT/ if non-empty
    let output_dir = mission_dir.join("RUNNER_OUTPUT");
    let _ = std::fs::create_dir_all(&output_dir);

    let stdout_path = if !output.stdout.is_empty() {
        let path = output_dir.join("runner_stdout.txt");
        let _ = std::fs::write(&path, &output.stdout);
        Some(path.display().to_string())
    } else {
        None
    };

    let stderr_path = if !output.stderr.is_empty() {
        let path = output_dir.join("runner_stderr.txt");
        let _ = std::fs::write(&path, &output.stderr);
        Some(path.display().to_string())
    } else {
        None
    };

    let error_message = if extract_error && classification == OutcomeClassification::Failed {
        extract_error_message(&output)
    } else {
        None
    };

    // Parse token usage from stderr using the dedicated module
    let stderr_str = String::from_utf8_lossy(&output.stderr);
    let token_usage = TokenUsage::parse(&stderr_str);
    let token_usage = if token_usage.has_data() { Some(token_usage) } else { None };

    // Write token usage to RUNNER_OUTPUT if present
    if let Some(ref usage) = token_usage {
        let usage_path = output_dir.join("token_usage.json");
        if let Ok(usage_json) = serde_json::to_string_pretty(usage) {
            let _ = std::fs::write(&usage_path, usage_json);
        }
    }

    Ok(RunnerOutcome {
        exit_code,
        classification,
        elapsed_seconds: elapsed,
        stdout_path,
        stderr_path,
        error_message,
        token_usage,
    })
}

/// Detect rate limiting from AI provider output.
/// Checks for common rate limit patterns in stdout/stderr.
fn is_rate_limited(output: &std::process::Output) -> bool {
    let stdout = String::from_utf8_lossy(&output.stdout).to_lowercase();
    let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
    let combined = format!("{} {}", stdout, stderr);

    // Common rate limit patterns from various AI providers
    let patterns = [
        "rate limit",
        "rate-limit",
        "ratelimit",
        "hit your limit",
        "you've hit your limit",
        "quota exceeded",
        "insufficient credits",
        "out of credits",
        "payment required",
        "billing",
        "too many requests",
        "429",
        "try again later",
        "resets ",  // "resets Jan 25, 4pm"
        "usage limit",
        "api limit",
    ];

    patterns.iter().any(|p| combined.contains(p))
}

fn extract_error_message(output: &std::process::Output) -> Option<String> {
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.trim().is_empty() {
        return Some(truncate_line(&stderr, 200));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.trim().is_empty() {
        return Some(truncate_line(&stdout, 200));
    }
    None
}

fn truncate_line(input: &str, max_chars: usize) -> String {
    let mut line = input.lines().next().unwrap_or("").trim().to_string();
    if line.len() > max_chars {
        line.truncate(max_chars);
        line.push_str("…");
    }
    line
}

/// Check whether any files changed by the runner fall outside the allowed surface.
///
/// Returns a list of file paths that violate the policy.  An empty vec means clean.
/// `working_dir` is the repo root; `changed_files` are the relative paths from git diff.
pub(crate) fn check_surface_violations(
    changed_files: &[String],
    spec_patterns: &[String],
    tests_patterns: &[String],
    sut_patterns: &[String],
    policy_spec: bool,       // true = unlocked
    policy_tests: bool,      // true = unlocked
    policy_sut: bool,        // true = unlocked
) -> Vec<String> {
    let mut violations = Vec::new();
    for file in changed_files {
        let in_spec = matches_any_pattern(file, spec_patterns);
        let in_tests = matches_any_pattern(file, tests_patterns);
        let in_sut = matches_any_pattern(file, sut_patterns);

        if in_spec && !policy_spec {
            violations.push(format!("{} (spec surface LOCKED)", file));
        } else if in_tests && !policy_tests {
            violations.push(format!("{} (tests surface LOCKED)", file));
        } else if in_sut && !policy_sut {
            violations.push(format!("{} (sut surface LOCKED)", file));
        }
        // Files in no known surface are allowed (e.g., .tesaki/ internals)
    }
    violations
}

/// Simple glob-style pattern matching.
/// Supports `**` (any path segments) and `*` (any single segment).
fn matches_any_pattern(path: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pat| glob_match(pat, path))
}

fn glob_match(pattern: &str, path: &str) -> bool {
    // Normalise separators
    let pat = pattern.replace('\\', "/");
    let path = path.replace('\\', "/");

    // Split into segments
    let pat_parts: Vec<&str> = pat.split('/').collect();
    let path_parts: Vec<&str> = path.split('/').collect();

    glob_match_parts(&pat_parts, &path_parts)
}

fn glob_match_parts(pat: &[&str], path: &[&str]) -> bool {
    match (pat.first(), path.first()) {
        (None, None) => true,
        (Some(&"**"), _) => {
            // ** matches zero or more path segments
            glob_match_parts(&pat[1..], path)
                || (!path.is_empty() && glob_match_parts(pat, &path[1..]))
        }
        (Some(p), Some(s)) => {
            segment_match(p, s) && glob_match_parts(&pat[1..], &path[1..])
        }
        _ => false,
    }
}

fn segment_match(pattern: &str, segment: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    // Simple: treat *.ext as "ends with .ext"
    if let Some(ext) = pattern.strip_prefix('*') {
        return segment.ends_with(ext);
    }
    pattern == segment
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    fn make_output(stdout: &str, stderr: &str) -> std::process::Output {
        std::process::Output {
            status: std::process::ExitStatus::from_raw(256), // exit code 1
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[test]
    fn test_rate_limit_detection_claude() {
        let output = make_output("You've hit your limit · resets Jan 25, 4pm (America/Denver)", "");
        assert!(is_rate_limited(&output));
    }

    #[test]
    fn test_rate_limit_detection_429() {
        let output = make_output("", "Error 429: Too many requests");
        assert!(is_rate_limited(&output));
    }

    #[test]
    fn test_rate_limit_detection_quota() {
        let output = make_output("API quota exceeded. Please try again later.", "");
        assert!(is_rate_limited(&output));
    }

    #[test]
    fn test_rate_limit_detection_credits() {
        let output = make_output("", "Payment required: insufficient credits");
        assert!(is_rate_limited(&output));
    }

    #[test]
    fn test_rate_limit_not_triggered_on_normal_error() {
        let output = make_output("", "Error: file not found");
        assert!(!is_rate_limited(&output));
    }

    #[test]
    fn test_rate_limit_case_insensitive() {
        let output = make_output("RATE LIMIT exceeded", "");
        assert!(is_rate_limited(&output));
    }

    #[test]
    fn test_surface_check_locked_spec_violated() {
        let changed = vec!["test/specs/features/a.feature".to_string()];
        let spec_pats = vec!["test/specs/**/*.feature".to_string()];
        let tests_pats = vec!["test/tests/**".to_string()];
        let sut_pats = vec!["src/**".to_string()];

        let violations = check_surface_violations(
            &changed, &spec_pats, &tests_pats, &sut_pats,
            false, // spec LOCKED
            true,  // tests unlocked
            true,  // sut unlocked
        );
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("spec surface LOCKED"));
    }

    #[test]
    fn test_surface_check_all_unlocked_no_violations() {
        let changed = vec![
            "test/specs/features/a.feature".to_string(),
            "test/tests/steps.rs".to_string(),
            "src/main.rs".to_string(),
        ];
        let spec_pats = vec!["test/specs/**/*.feature".to_string()];
        let tests_pats = vec!["test/tests/**".to_string()];
        let sut_pats = vec!["src/**".to_string()];

        let violations = check_surface_violations(
            &changed, &spec_pats, &tests_pats, &sut_pats,
            true, true, true,
        );
        assert!(violations.is_empty());
    }

    #[test]
    fn test_glob_match_double_star() {
        assert!(glob_match("test/specs/**/*.feature", "test/specs/features/a.feature"));
        assert!(glob_match("src/**", "src/main.rs"));
        assert!(glob_match("src/**", "src/sub/deep/file.rs"));
        assert!(!glob_match("src/**", "test/main.rs"));
    }

    #[test]
    fn test_glob_match_single_star() {
        assert!(glob_match("test/*", "test/foo"));
        assert!(!glob_match("test/*", "test/foo/bar"));
    }
}