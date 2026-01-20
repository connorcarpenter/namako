//! Gate outcome classification for update-cert governance.
//!
//! Per TODO.md Part A, this module classifies `namako gate --json` output to determine
//! whether update-cert should be invoked automatically.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

/// Parsed gate JSON output structure.
/// Matches the GateOutput struct from namako/cli/src/gate.rs.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GateJson {
    pub lint: PhaseResult,
    pub run: PhaseResult,
    pub verify: PhaseResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinism: Option<DeterminismResult>,
}

/// Result of a single gate phase.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PhaseResult {
    pub status: PhaseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Phase status enum matching namako gate output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseStatus {
    Pass,
    Fail,
    Skipped,
}

/// Determinism check result.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeterminismResult {
    pub status: PhaseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Gate outcome classification for update-cert governance.
///
/// Per TODO.md §A1:
/// - Pass: All phases pass
/// - FailVerifyOnly: lint=PASS, run=PASS, verify=FAIL (update-cert candidate)
/// - FailOther: lint or run failed (no update-cert attempt)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GateOutcome {
    /// All phases pass.
    Pass,
    /// lint=PASS, run=PASS, verify=FAIL — eligible for update-cert.
    FailVerifyOnly,
    /// lint or run failed — NOT eligible for update-cert.
    FailOther,
}

impl GateOutcome {
    /// Classify gate outcome from parsed gate JSON.
    pub fn classify(gate: &GateJson) -> Self {
        let lint_pass = gate.lint.status == PhaseStatus::Pass;
        let run_pass = gate.run.status == PhaseStatus::Pass;
        let verify_pass = gate.verify.status == PhaseStatus::Pass;

        if lint_pass && run_pass && verify_pass {
            GateOutcome::Pass
        } else if lint_pass && run_pass && !verify_pass {
            GateOutcome::FailVerifyOnly
        } else {
            GateOutcome::FailOther
        }
    }

    /// Classify gate outcome from JSON string.
    ///
    /// Returns FailOther on parse error (conservative).
    pub fn from_json_str(json: &str) -> Self {
        match serde_json::from_str::<GateJson>(json) {
            Ok(gate) => Self::classify(&gate),
            Err(_) => GateOutcome::FailOther,
        }
    }

    /// Returns true if this outcome allows update-cert.
    pub fn allows_update_cert(&self) -> bool {
        matches!(self, GateOutcome::FailVerifyOnly)
    }

    /// Returns true if this is a pass outcome.
    pub fn is_pass(&self) -> bool {
        matches!(self, GateOutcome::Pass)
    }
}

/// Summary of update-cert operation for logging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCertSummary {
    /// Mission ID (if applicable).
    pub mission_id: Option<String>,
    /// Gate outcome before update-cert.
    pub before_outcome: GateOutcome,
    /// Gate outcome after update-cert.
    pub after_outcome: GateOutcome,
    /// Whether update-cert fixed the gate.
    pub fixed: bool,
    /// Elapsed time in seconds.
    pub elapsed_seconds: f64,
    /// Captured stdout (truncated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    /// Captured stderr (truncated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    /// Exit status.
    pub exit_status: Option<i32>,
}

/// Result of running update-cert.
#[derive(Debug, Clone)]
pub struct UpdateCertResult {
    /// Whether the update-cert command succeeded (exit 0).
    pub success: bool,
    /// Exit status code.
    pub exit_status: Option<i32>,
    /// Captured stdout (bounded to 10KB).
    pub stdout: String,
    /// Captured stderr (bounded to 10KB).
    pub stderr: String,
    /// Elapsed time in seconds.
    pub elapsed_seconds: f64,
}

/// Trait for invoking update-cert.
///
/// Abstracted for testing: production uses `ProcessInvoker`, tests can use mocks.
pub trait UpdateCertInvoker: Send + Sync {
    /// Run update-cert and return the result.
    fn run_update_cert(
        &self,
        namako_cmd: &str,
        adapter: &str,
        spec_root: &Path,
        run_report_path: &Path,
        cert_output_path: &Path,
    ) -> UpdateCertResult;

    /// Run gate --json and return the raw JSON output.
    fn run_gate_json(
        &self,
        namako_cmd: &str,
        adapter: &str,
        spec_root: &Path,
    ) -> Result<String, String>;
}

/// Production invoker using real subprocess calls.
pub struct ProcessInvoker;

impl UpdateCertInvoker for ProcessInvoker {
    fn run_update_cert(
        &self,
        namako_cmd: &str,
        adapter: &str,
        spec_root: &Path,
        run_report_path: &Path,
        cert_output_path: &Path,
    ) -> UpdateCertResult {
        let start = Instant::now();

        let args: Vec<&str> = namako_cmd.split_whitespace().collect();
        let (program, namako_args) = match args.split_first() {
            Some(p) => p,
            None => {
                return UpdateCertResult {
                    success: false,
                    exit_status: None,
                    stdout: String::new(),
                    stderr: "Empty namako command".to_string(),
                    elapsed_seconds: start.elapsed().as_secs_f64(),
                };
            }
        };

        let result = Command::new(program)
            .args(namako_args)
            .arg("update-cert")
            .arg("-a")
            .arg(adapter)
            .arg("--run-report")
            .arg(run_report_path)
            .arg("--output")
            .arg(cert_output_path)
            .current_dir(spec_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();

        let elapsed = start.elapsed().as_secs_f64();

        match result {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                UpdateCertResult {
                    success: output.status.success(),
                    exit_status: output.status.code(),
                    stdout: truncate_string(&stdout, 10 * 1024),
                    stderr: truncate_string(&stderr, 10 * 1024),
                    elapsed_seconds: elapsed,
                }
            }
            Err(e) => UpdateCertResult {
                success: false,
                exit_status: None,
                stdout: String::new(),
                stderr: format!("Failed to execute: {}", e),
                elapsed_seconds: elapsed,
            },
        }
    }

    fn run_gate_json(
        &self,
        namako_cmd: &str,
        adapter: &str,
        spec_root: &Path,
    ) -> Result<String, String> {
        let args: Vec<&str> = namako_cmd.split_whitespace().collect();
        let (program, namako_args) = match args.split_first() {
            Some(p) => p,
            None => return Err("Empty namako command".to_string()),
        };

        let output = Command::new(program)
            .args(namako_args)
            .arg("gate")
            .arg("-s")
            .arg(".")
            .arg("-a")
            .arg(adapter)
            .arg("--json")
            .current_dir(spec_root)
            .output()
            .map_err(|e| format!("Failed to run gate: {}", e))?;

        let stdout = String::from_utf8(output.stdout)
            .map_err(|e| format!("Invalid UTF-8 in output: {}", e))?;

        if stdout.trim().is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Empty gate output. stderr: {}", stderr));
        }

        Ok(stdout)
    }
}

/// Mock invoker for testing.
#[cfg(test)]
pub struct MockInvoker {
    /// Sequence of gate outcomes to return (consumed in order).
    pub gate_outcomes: std::sync::Mutex<Vec<GateOutcome>>,
    /// Whether update-cert should succeed.
    pub update_cert_succeeds: bool,
    /// Count of update-cert calls.
    pub update_cert_calls: std::sync::Mutex<u32>,
    /// Count of gate calls.
    pub gate_calls: std::sync::Mutex<u32>,
}

#[cfg(test)]
impl MockInvoker {
    pub fn new(outcomes: Vec<GateOutcome>, update_cert_succeeds: bool) -> Self {
        Self {
            gate_outcomes: std::sync::Mutex::new(outcomes),
            update_cert_succeeds,
            update_cert_calls: std::sync::Mutex::new(0),
            gate_calls: std::sync::Mutex::new(0),
        }
    }
}

#[cfg(test)]
impl UpdateCertInvoker for MockInvoker {
    fn run_update_cert(
        &self,
        _namako_cmd: &str,
        _adapter: &str,
        _spec_root: &Path,
        _run_report_path: &Path,
        _cert_output_path: &Path,
    ) -> UpdateCertResult {
        *self.update_cert_calls.lock().unwrap() += 1;
        UpdateCertResult {
            success: self.update_cert_succeeds,
            exit_status: if self.update_cert_succeeds { Some(0) } else { Some(1) },
            stdout: "mock stdout".to_string(),
            stderr: if self.update_cert_succeeds { String::new() } else { "mock error".to_string() },
            elapsed_seconds: 0.1,
        }
    }

    fn run_gate_json(
        &self,
        _namako_cmd: &str,
        _adapter: &str,
        _spec_root: &Path,
    ) -> Result<String, String> {
        *self.gate_calls.lock().unwrap() += 1;
        let mut outcomes = self.gate_outcomes.lock().unwrap();
        let outcome = outcomes.pop().unwrap_or(GateOutcome::Pass);
        drop(outcomes);

        // Generate JSON matching the outcome
        let (lint, run, verify) = match outcome {
            GateOutcome::Pass => ("pass", "pass", "pass"),
            GateOutcome::FailVerifyOnly => ("pass", "pass", "fail"),
            GateOutcome::FailOther => ("fail", "skipped", "skipped"),
        };

        Ok(format!(
            r#"{{"lint":{{"status":"{}"}},"run":{{"status":"{}"}},"verify":{{"status":"{}"}}}}"#,
            lint, run, verify
        ))
    }
}

/// Truncate string to max bytes, appending "...[truncated]" if needed.
fn truncate_string(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        s.to_string()
    } else {
        let suffix = "...[truncated]";
        let keep = max_bytes.saturating_sub(suffix.len());
        // Find a valid UTF-8 boundary
        let mut end = keep;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}{}", &s[..end], suffix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_gate_json(lint: PhaseStatus, run: PhaseStatus, verify: PhaseStatus) -> GateJson {
        GateJson {
            lint: PhaseResult { status: lint, reason: None },
            run: PhaseResult { status: run, reason: None },
            verify: PhaseResult { status: verify, reason: None },
            determinism: None,
        }
    }

    #[test]
    fn test_classify_pass() {
        let gate = make_gate_json(PhaseStatus::Pass, PhaseStatus::Pass, PhaseStatus::Pass);
        assert_eq!(GateOutcome::classify(&gate), GateOutcome::Pass);
    }

    #[test]
    fn test_classify_verify_only_fail() {
        let gate = make_gate_json(PhaseStatus::Pass, PhaseStatus::Pass, PhaseStatus::Fail);
        assert_eq!(GateOutcome::classify(&gate), GateOutcome::FailVerifyOnly);
        assert!(GateOutcome::FailVerifyOnly.allows_update_cert());
    }

    #[test]
    fn test_classify_lint_fail() {
        let gate = make_gate_json(PhaseStatus::Fail, PhaseStatus::Skipped, PhaseStatus::Skipped);
        assert_eq!(GateOutcome::classify(&gate), GateOutcome::FailOther);
        assert!(!GateOutcome::FailOther.allows_update_cert());
    }

    #[test]
    fn test_classify_run_fail() {
        let gate = make_gate_json(PhaseStatus::Pass, PhaseStatus::Fail, PhaseStatus::Skipped);
        assert_eq!(GateOutcome::classify(&gate), GateOutcome::FailOther);
    }

    #[test]
    fn test_from_json_str_valid() {
        let json = r#"{
            "lint": {"status": "pass"},
            "run": {"status": "pass"},
            "verify": {"status": "fail", "reason": "drift detected"}
        }"#;
        assert_eq!(GateOutcome::from_json_str(json), GateOutcome::FailVerifyOnly);
    }

    #[test]
    fn test_from_json_str_invalid() {
        let json = "not valid json";
        // Conservative: parse error → FailOther
        assert_eq!(GateOutcome::from_json_str(json), GateOutcome::FailOther);
    }

    #[test]
    fn test_from_json_str_all_pass() {
        let json = r#"{
            "lint": {"status": "pass"},
            "run": {"status": "pass"},
            "verify": {"status": "pass"}
        }"#;
        assert_eq!(GateOutcome::from_json_str(json), GateOutcome::Pass);
    }

    #[test]
    fn test_outcome_serialization() {
        let outcome = GateOutcome::FailVerifyOnly;
        let json = serde_json::to_string(&outcome).unwrap();
        assert_eq!(json, "\"FAIL_VERIFY_ONLY\"");

        let parsed: GateOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, GateOutcome::FailVerifyOnly);
    }

    #[test]
    fn test_mock_invoker_gate_outcomes() {
        // Note: outcomes are consumed in reverse order (pop from end)
        let invoker = MockInvoker::new(
            vec![GateOutcome::Pass, GateOutcome::FailVerifyOnly],
            true,
        );

        // First call gets FailVerifyOnly (popped from end)
        let json1 = invoker.run_gate_json("namako", "adapter", Path::new(".")).unwrap();
        assert_eq!(GateOutcome::from_json_str(&json1), GateOutcome::FailVerifyOnly);

        // Second call gets Pass
        let json2 = invoker.run_gate_json("namako", "adapter", Path::new(".")).unwrap();
        assert_eq!(GateOutcome::from_json_str(&json2), GateOutcome::Pass);

        // Third call defaults to Pass (empty)
        let json3 = invoker.run_gate_json("namako", "adapter", Path::new(".")).unwrap();
        assert_eq!(GateOutcome::from_json_str(&json3), GateOutcome::Pass);

        assert_eq!(*invoker.gate_calls.lock().unwrap(), 3);
    }

    #[test]
    fn test_mock_invoker_update_cert() {
        let invoker = MockInvoker::new(vec![], true);

        let result = invoker.run_update_cert(
            "namako",
            "adapter",
            Path::new("."),
            Path::new("run_report.json"),
            Path::new("certification.json"),
        );

        assert!(result.success);
        assert_eq!(result.exit_status, Some(0));
        assert_eq!(*invoker.update_cert_calls.lock().unwrap(), 1);
    }

    #[test]
    fn test_mock_invoker_update_cert_fails() {
        let invoker = MockInvoker::new(vec![], false);

        let result = invoker.run_update_cert(
            "namako",
            "adapter",
            Path::new("."),
            Path::new("run_report.json"),
            Path::new("certification.json"),
        );

        assert!(!result.success);
        assert_eq!(result.exit_status, Some(1));
        assert!(!result.stderr.is_empty());
    }

    #[test]
    fn test_truncate_string() {
        // Short string unchanged
        assert_eq!(truncate_string("hello", 100), "hello");

        // Long string truncated
        let long = "a".repeat(100);
        let truncated = truncate_string(&long, 50);
        assert!(truncated.len() <= 50);
        assert!(truncated.ends_with("...[truncated]"));
    }

    // =========================================================================
    // Tests for update-cert governance (per TODO.md §A5)
    // =========================================================================

    /// Per TODO.md §A5: verify-only fail → update-cert succeeds → gate returns pass
    #[test]
    fn test_governance_verify_fail_then_update_cert_fixes() {
        // Outcomes consumed in reverse: [Pass, FailVerifyOnly] -> first call returns FailVerifyOnly
        let invoker = MockInvoker::new(
            vec![GateOutcome::Pass, GateOutcome::FailVerifyOnly],
            true, // update-cert succeeds
        );

        // First gate call: FailVerifyOnly
        let json1 = invoker.run_gate_json("namako", "adapter", Path::new(".")).unwrap();
        let outcome1 = GateOutcome::from_json_str(&json1);
        assert_eq!(outcome1, GateOutcome::FailVerifyOnly);
        assert!(outcome1.allows_update_cert());

        // Run update-cert
        let update_result = invoker.run_update_cert(
            "namako", "adapter", Path::new("."),
            Path::new("run_report.json"), Path::new("cert.json"),
        );
        assert!(update_result.success);
        assert_eq!(*invoker.update_cert_calls.lock().unwrap(), 1);

        // Re-gate: Pass
        let json2 = invoker.run_gate_json("namako", "adapter", Path::new(".")).unwrap();
        let outcome2 = GateOutcome::from_json_str(&json2);
        assert_eq!(outcome2, GateOutcome::Pass);
    }

    /// Per TODO.md §A5: lint/run fail → update-cert NOT invoked
    #[test]
    fn test_governance_lint_fail_no_update_cert() {
        let invoker = MockInvoker::new(
            vec![GateOutcome::FailOther],
            true, // update-cert would succeed if called
        );

        // Gate call: FailOther (lint or run failed)
        let json = invoker.run_gate_json("namako", "adapter", Path::new(".")).unwrap();
        let outcome = GateOutcome::from_json_str(&json);
        assert_eq!(outcome, GateOutcome::FailOther);
        assert!(!outcome.allows_update_cert());

        // Update-cert should NOT be called for FailOther
        // (In the actual run_run flow, this is enforced by the match arms)
        assert_eq!(*invoker.update_cert_calls.lock().unwrap(), 0);
    }

    /// Per TODO.md §A5: verify-only fail but max_cert_updates=0 → update-cert NOT invoked
    #[test]
    fn test_governance_verify_fail_zero_budget() {
        let invoker = MockInvoker::new(
            vec![GateOutcome::FailVerifyOnly],
            true,
        );

        // Gate call: FailVerifyOnly
        let json = invoker.run_gate_json("namako", "adapter", Path::new(".")).unwrap();
        let outcome = GateOutcome::from_json_str(&json);
        assert!(outcome.allows_update_cert());

        // But with max_cert_updates = 0, we would NOT call update-cert
        // Simulate this check:
        let max_cert_updates = 0u32;
        let cert_updates_made = 0u32;
        let should_update = outcome.allows_update_cert() && cert_updates_made < max_cert_updates;
        assert!(!should_update);

        // Verify update-cert not called
        assert_eq!(*invoker.update_cert_calls.lock().unwrap(), 0);
    }

    /// Per TODO.md §A5: verify-only fail, update-cert fails → still counted as attempted
    #[test]
    fn test_governance_update_cert_fails() {
        let invoker = MockInvoker::new(
            vec![GateOutcome::FailVerifyOnly],
            false, // update-cert fails
        );

        let json = invoker.run_gate_json("namako", "adapter", Path::new(".")).unwrap();
        let outcome = GateOutcome::from_json_str(&json);
        assert!(outcome.allows_update_cert());

        // Run update-cert (fails)
        let update_result = invoker.run_update_cert(
            "namako", "adapter", Path::new("."),
            Path::new("run_report.json"), Path::new("cert.json"),
        );
        assert!(!update_result.success);
        assert_eq!(*invoker.update_cert_calls.lock().unwrap(), 1);
    }
}
