//! Error Parser: Structured parsing of cargo build and test errors.
//!
//! This module provides structured error parsing for:
//! - Cargo build errors (rustc compiler errors)
//! - Cargo test failures
//! - General command failures
//!
//! Parsed errors are included in mission retry context to help the runner
//! understand and fix specific issues.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::LazyLock;

/// A structured compile error from cargo build.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompileError {
    /// Source file path (relative to project root)
    pub file: String,
    /// Line number
    pub line: u32,
    /// Column number (if available)
    pub column: Option<u32>,
    /// Error level: "error", "warning"
    pub level: String,
    /// Error code (e.g., "E0425")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Error message
    pub message: String,
}

impl CompileError {
    /// Format as a concise one-liner for context injection.
    pub fn to_oneliner(&self) -> String {
        let loc = if let Some(col) = self.column {
            format!("{}:{}:{}", self.file, self.line, col)
        } else {
            format!("{}:{}", self.file, self.line)
        };
        
        if let Some(ref code) = self.code {
            format!("{}: {} [{}]: {}", loc, self.level, code, self.message)
        } else {
            format!("{}: {}: {}", loc, self.level, self.message)
        }
    }
}

/// Result of a pre-gate build check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildCheckResult {
    /// Whether the build succeeded
    pub success: bool,
    /// Exit code (if available)
    pub exit_code: Option<i32>,
    /// Parsed compile errors (top N)
    pub errors: Vec<CompileError>,
    /// Total error count (before truncation)
    pub total_errors: usize,
    /// Raw stderr (truncated)
    pub stderr_excerpt: String,
    /// Elapsed time in seconds
    pub elapsed_seconds: f64,
    /// Whether a specific test harness was detected and targeted
    #[serde(default)]
    pub targeted_harness: bool,
    /// Warning message (non-fatal issues)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

impl BuildCheckResult {
    /// Format errors as markdown for mission context.
    pub fn to_markdown(&self, max_errors: usize) -> String {
        if self.success {
            return "✅ Build succeeded".to_string();
        }
        
        let mut md = String::from("### ❌ Build Failed\n\n");
        
        if self.errors.is_empty() {
            md.push_str(&format!("```\n{}\n```\n", self.stderr_excerpt));
        } else {
            md.push_str("**Compile errors:**\n```\n");
            for err in self.errors.iter().take(max_errors) {
                md.push_str(&err.to_oneliner());
                md.push('\n');
            }
            md.push_str("```\n");
            
            if self.total_errors > max_errors {
                md.push_str(&format!("\n*...and {} more errors*\n", self.total_errors - max_errors));
            }
        }
        
        md
    }
}

// Regex patterns for parsing cargo output
// Example: "error[E0425]: cannot find value `x` in this scope"
static ERROR_HEADER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(error|warning)(?:\[(E\d+)\])?: (.+)$").unwrap()
});

// Example: "  --> src/main.rs:42:13"
static LOCATION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*-->\s*([^:]+):(\d+):(\d+)").unwrap()
});

// Alternative location format: "  --> src/main.rs:42"
static LOCATION_NO_COL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*-->\s*([^:]+):(\d+)$").unwrap()
});

/// Parse cargo build/check stderr into structured errors.
pub fn parse_cargo_errors(stderr: &str) -> Vec<CompileError> {
    let mut errors = Vec::new();
    let lines: Vec<&str> = stderr.lines().collect();
    
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        
        // Match error/warning header
        if let Some(caps) = ERROR_HEADER_RE.captures(line) {
            let level = caps.get(1).map(|m| m.as_str()).unwrap_or("error").to_string();
            let code = caps.get(2).map(|m| m.as_str().to_string());
            let message = caps.get(3).map(|m| m.as_str()).unwrap_or("").to_string();
            
            // Look for location in next few lines
            let mut file = String::new();
            let mut line_num = 0u32;
            let mut column = None;
            
            for j in (i + 1)..std::cmp::min(i + 5, lines.len()) {
                if let Some(loc_caps) = LOCATION_RE.captures(lines[j]) {
                    file = loc_caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
                    line_num = loc_caps.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
                    column = loc_caps.get(3).and_then(|m| m.as_str().parse().ok());
                    break;
                } else if let Some(loc_caps) = LOCATION_NO_COL_RE.captures(lines[j]) {
                    file = loc_caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
                    line_num = loc_caps.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
                    break;
                }
            }
            
            // Only include if we found a valid location
            if !file.is_empty() && line_num > 0 && level == "error" {
                errors.push(CompileError {
                    file,
                    line: line_num,
                    column,
                    level,
                    code,
                    message,
                });
            }
        }
        
        i += 1;
    }
    
    errors
}

/// Auto-detect a test harness package in the working directory.
///
/// Looks for common test harness patterns:
/// - test/harness/Cargo.toml -> package name from [package] section
/// - test/Cargo.toml with a test-related name
///
/// Returns the package name if found, None otherwise.
fn detect_test_harness(working_dir: &Path) -> Option<String> {
    // Common locations for test harness Cargo.toml
    let candidates = [
        working_dir.join("test/harness/Cargo.toml"),
        working_dir.join("tests/harness/Cargo.toml"),
        working_dir.join("test_harness/Cargo.toml"),
    ];
    
    for cargo_path in &candidates {
        if cargo_path.exists() {
            if let Ok(content) = fs::read_to_string(cargo_path) {
                // Simple regex to extract package name
                static PKG_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
                    Regex::new(r#"(?m)^\s*name\s*=\s*"([^"]+)""#).unwrap()
                });
                if let Some(caps) = PKG_NAME_RE.captures(&content) {
                    return Some(caps.get(1).unwrap().as_str().to_string());
                }
            }
        }
    }
    
    None
}

/// Run a pre-gate build check.
///
/// Runs `cargo check` (or custom build command) and parses errors.
/// Returns structured result with errors for context injection.
///
/// If no build command is provided, auto-detects a test harness package
/// and builds only that, avoiding compilation of unrelated workspace crates.
///
/// **Resilience policy:** If no test harness is detected and the fallback
/// workspace-wide check fails, we still return success=true with a warning.
/// This prevents unrelated broken crates from blocking the spec loop.
pub fn run_pre_gate_build(
    working_dir: &Path,
    build_cmd: Option<&str>,
) -> BuildCheckResult {
    let start = std::time::Instant::now();
    
    // Track whether we're doing a targeted harness build or fallback
    let mut targeted_harness = false;
    
    // Build command as owned Strings for the auto-detect case
    let (program, args): (String, Vec<String>) = if let Some(cmd) = build_cmd {
        // Explicit command provided - treat as targeted
        targeted_harness = true;
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            ("cargo".to_string(), vec!["check".to_string()])
        } else {
            (parts[0].to_string(), parts[1..].iter().map(|s| s.to_string()).collect())
        }
    } else {
        // Auto-detect test harness package for targeted build
        if let Some(harness_pkg) = detect_test_harness(working_dir) {
            targeted_harness = true;
            // Build only the test harness, not the entire workspace
            ("cargo".to_string(), vec![
                "check".to_string(),
                "-p".to_string(),
                harness_pkg,
            ])
        } else {
            // Fallback: use cargo check (faster than build, less likely to hit unrelated issues)
            // NOT targeted - failures here are non-blocking
            ("cargo".to_string(), vec!["check".to_string()])
        }
    };
    
    let result = Command::new(&program)
        .args(&args)
        .current_dir(working_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    
    let elapsed = start.elapsed().as_secs_f64();
    
    match result {
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let errors = parse_cargo_errors(&stderr);
            let total_errors = errors.len();
            
            // Truncate stderr for excerpt
            let stderr_excerpt = if stderr.len() > 2000 {
                format!("{}...[truncated]", &stderr[..2000])
            } else {
                stderr.to_string()
            };
            
            let raw_success = output.status.success();
            
            // Resilience: if not targeted and build failed, warn but don't block
            let (success, warning) = if !raw_success && !targeted_harness {
                (true, Some("Workspace-wide check failed (unrelated crates may be broken). Proceeding anyway.".to_string()))
            } else {
                (raw_success, None)
            };
            
            BuildCheckResult {
                success,
                exit_code: output.status.code(),
                errors,
                total_errors,
                stderr_excerpt,
                elapsed_seconds: elapsed,
                targeted_harness,
                warning,
            }
        }
        Err(e) => BuildCheckResult {
            success: false,
            exit_code: None,
            errors: vec![],
            total_errors: 0,
            stderr_excerpt: format!("Failed to execute build command: {}", e),
            elapsed_seconds: elapsed,
            targeted_harness,
            warning: None,
        },
    }
}


/// Check if a build is needed before running gate.
///
/// Returns true if we should run a pre-gate build check.
/// Currently always returns true, but could be enhanced to skip
/// if no source files changed since last successful build.
pub fn should_run_pre_gate_build(_working_dir: &Path) -> bool {
    // For now, always run the check. In the future, we could:
    // - Check mtime of Cargo.lock vs last build
    // - Track successful builds in .tesaki/build_cache.json
    // - Skip if git status shows no relevant changes
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_error() {
        let stderr = r#"error[E0425]: cannot find value `foo` in this scope
   --> src/main.rs:42:13
    |
42  |     let x = foo;
    |             ^^^ not found in this scope
"#;
        let errors = parse_cargo_errors(stderr);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].file, "src/main.rs");
        assert_eq!(errors[0].line, 42);
        assert_eq!(errors[0].column, Some(13));
        assert_eq!(errors[0].code, Some("E0425".to_string()));
        assert!(errors[0].message.contains("cannot find value"));
    }

    #[test]
    fn test_parse_multiple_errors() {
        let stderr = r#"error[E0425]: cannot find value `foo` in this scope
   --> src/main.rs:10:5

error[E0308]: mismatched types
   --> src/lib.rs:20:10
"#;
        let errors = parse_cargo_errors(stderr);
        assert_eq!(errors.len(), 2);
        assert_eq!(errors[0].file, "src/main.rs");
        assert_eq!(errors[0].line, 10);
        assert_eq!(errors[1].file, "src/lib.rs");
        assert_eq!(errors[1].line, 20);
    }

    #[test]
    fn test_parse_warning_ignored() {
        let stderr = r#"warning: unused variable: `x`
   --> src/main.rs:5:9

error[E0425]: cannot find value `y`
   --> src/main.rs:10:5
"#;
        let errors = parse_cargo_errors(stderr);
        // Only errors, not warnings
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].level, "error");
    }

    #[test]
    fn test_parse_no_errors() {
        let stderr = "    Compiling myproject v0.1.0\n    Finished dev [unoptimized + debuginfo]";
        let errors = parse_cargo_errors(stderr);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_error_to_oneliner() {
        let err = CompileError {
            file: "src/main.rs".to_string(),
            line: 42,
            column: Some(13),
            level: "error".to_string(),
            code: Some("E0425".to_string()),
            message: "cannot find value `foo`".to_string(),
        };
        let oneliner = err.to_oneliner();
        assert!(oneliner.contains("src/main.rs:42:13"));
        assert!(oneliner.contains("[E0425]"));
        assert!(oneliner.contains("cannot find value"));
    }

    #[test]
    fn test_build_result_to_markdown() {
        let result = BuildCheckResult {
            success: false,
            exit_code: Some(1),
            errors: vec![
                CompileError {
                    file: "src/main.rs".to_string(),
                    line: 10,
                    column: Some(5),
                    level: "error".to_string(),
                    code: Some("E0425".to_string()),
                    message: "cannot find value".to_string(),
                },
            ],
            total_errors: 1,
            stderr_excerpt: "".to_string(),
            elapsed_seconds: 1.5,
            targeted_harness: true,
            warning: None,
        };
        
        let md = result.to_markdown(5);
        assert!(md.contains("Build Failed"));
        assert!(md.contains("src/main.rs:10:5"));
    }

    #[test]
    fn test_build_result_success_markdown() {
        let result = BuildCheckResult {
            success: true,
            exit_code: Some(0),
            errors: vec![],
            total_errors: 0,
            stderr_excerpt: "".to_string(),
            elapsed_seconds: 1.0,
            targeted_harness: false,
            warning: None,
        };
        
        let md = result.to_markdown(5);
        assert!(md.contains("Build succeeded"));
    }
}
