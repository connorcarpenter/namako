//! Plan validation module for pre-flight surface policy checks.
//!
//! This module validates proposed file changes against surface policy
//! BEFORE they are actually written to disk, providing early feedback
//! to the AI agent about policy violations.

use anyhow::Result;

/// A proposed plan from the runner containing files to modify.
#[derive(Debug, Clone)]
pub struct ProposedPlan {
    pub files_to_modify: Vec<String>,
}

/// Result of validating a proposed plan against surface policy.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub valid: bool,
    pub violations: Vec<String>,
    pub guidance: String,
}

/// Validate a proposed plan against surface policy.
///
/// This function checks if the proposed file changes would violate
/// the current surface policy BEFORE any changes are made.
pub fn validate_plan(
    plan: &ProposedPlan,
    spec_patterns: &[String],
    tests_patterns: &[String],
    sut_patterns: &[String],
    spec_locked: bool,
    tests_locked: bool,
    sut_locked: bool,
) -> ValidationResult {
    let mut violations = Vec::new();

    for file in &plan.files_to_modify {
        let in_spec = matches_any_pattern(file, spec_patterns);
        let in_tests = matches_any_pattern(file, tests_patterns);
        let in_sut = matches_any_pattern(file, sut_patterns);

        if in_spec && spec_locked {
            violations.push(format!("{} (spec surface is LOCKED)", file));
        } else if in_tests && tests_locked {
            violations.push(format!("{} (tests surface is LOCKED)", file));
        } else if in_sut && sut_locked {
            violations.push(format!("{} (sut surface is LOCKED)", file));
        }
    }

    let valid = violations.is_empty();
    let guidance = if valid {
        "All proposed file changes are within allowed surfaces.".to_string()
    } else {
        format!(
            "⚠️  The following files are in LOCKED surfaces and cannot be modified:\n{}\n\n\
            RECOMMENDATION: Focus only on files in UNLOCKED surfaces, or request that the \
            appropriate surface be unlocked if this change is necessary.",
            violations.iter().map(|v| format!("  - {}", v)).collect::<Vec<_>>().join("\n")
        )
    };

    ValidationResult {
        valid,
        violations,
        guidance,
    }
}

/// Extract proposed file changes from runner output.
///
/// This is a best-effort extraction that looks for common patterns
/// in AI agent output indicating which files they plan to modify.
///
/// Returns None if no clear plan is found.
pub fn extract_proposed_files(runner_output: &str) -> Option<Vec<String>> {
    let mut proposed_files = Vec::new();

    // Common patterns AI agents use to announce file changes:
    // - "I will modify these files:"
    // - "Files to change:"
    // - "Editing:"
    // - "I'll modify/edit/update FILE"
    // - Markdown code blocks with file paths

    let lines: Vec<&str> = runner_output.lines().collect();
    let mut in_file_list = false;

    for (i, line) in lines.iter().enumerate() {
        let lower = line.to_lowercase();

        // Detect start of file list
        if (lower.contains("will modify") || lower.contains("files to") ||
            lower.contains("editing:") || lower.contains("changing:")) &&
           (lower.contains("file") || lower.contains("these")) {
            in_file_list = true;
            continue;
        }

        // Extract files from list format
        if in_file_list {
            // Stop at empty line or next section
            if line.trim().is_empty() || line.starts_with('#') {
                in_file_list = false;
                continue;
            }

            // Extract file paths (common formats: "- file.rs", "1. file.rs", "* file.rs")
            if let Some(file) = extract_file_from_list_item(line) {
                proposed_files.push(file);
            }
        }

        // Also look for inline mentions: "I'll modify src/foo.rs"
        if let Some(file) = extract_file_from_inline_mention(line) {
            if !proposed_files.contains(&file) {
                proposed_files.push(file);
            }
        }

        // Look ahead for code blocks with file paths
        if line.trim().starts_with("```") && i + 1 < lines.len() {
            if let Some(file) = extract_file_from_code_block_header(&lines[i..]) {
                if !proposed_files.contains(&file) {
                    proposed_files.push(file);
                }
            }
        }
    }

    if proposed_files.is_empty() {
        None
    } else {
        Some(proposed_files)
    }
}

fn extract_file_from_list_item(line: &str) -> Option<String> {
    let trimmed = line.trim();

    // Remove list markers: "- ", "* ", "1. ", etc.
    let content = trimmed
        .trim_start_matches(|c: char| c.is_numeric() || c == '.' || c == '-' || c == '*' || c.is_whitespace());

    // Extract file path (typically ends in common extensions)
    if content.contains('/') && has_file_extension(content) {
        return Some(content.trim().split_whitespace().next()?.to_string());
    }

    None
}

fn extract_file_from_inline_mention(line: &str) -> Option<String> {
    let lower = line.to_lowercase();

    if !(lower.contains("modify") || lower.contains("edit") ||
          lower.contains("update") || lower.contains("change")) {
        return None;
    }

    // Look for file paths in the line
    for word in line.split_whitespace() {
        if word.contains('/') && has_file_extension(word) {
            return Some(word.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '.' && c != '_' && c != '-').to_string());
        }
    }

    None
}

fn extract_file_from_code_block_header(lines: &[&str]) -> Option<String> {
    // Check if next line after ``` looks like a file path
    if lines.len() > 1 {
        let header = lines[1].trim();
        if header.contains('/') && has_file_extension(header) {
            return Some(header.split_whitespace().next()?.to_string());
        }
    }
    None
}

fn has_file_extension(s: &str) -> bool {
    s.contains('.') && (
        s.ends_with(".rs") || s.ends_with(".feature") ||
        s.ends_with(".ts") || s.ends_with(".js") ||
        s.ends_with(".py") || s.ends_with(".go") ||
        s.ends_with(".java") || s.ends_with(".kt") ||
        s.ends_with(".rb") || s.ends_with(".php") ||
        s.ends_with(".cs") || s.ends_with(".cpp") ||
        s.ends_with(".c") || s.ends_with(".h") ||
        s.ends_with(".md") || s.ends_with(".toml") ||
        s.ends_with(".json") || s.ends_with(".yaml") ||
        s.ends_with(".yml") || s.ends_with(".xml")
    )
}

/// Simple glob-style pattern matching (copied from base_runner for now).
fn matches_any_pattern(path: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pat| glob_match(pat, path))
}

fn glob_match(pattern: &str, path: &str) -> bool {
    let pat = pattern.replace('\\', "/");
    let path = path.replace('\\', "/");

    let pat_parts: Vec<&str> = pat.split('/').collect();
    let path_parts: Vec<&str> = path.split('/').collect();

    glob_match_parts(&pat_parts, &path_parts)
}

fn glob_match_parts(pat: &[&str], path: &[&str]) -> bool {
    match (pat.first(), path.first()) {
        (None, None) => true,
        (Some(&"**"), _) => {
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
    if let Some(ext) = pattern.strip_prefix('*') {
        return segment.ends_with(ext);
    }
    pattern == segment
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_plan_all_allowed() {
        let plan = ProposedPlan {
            files_to_modify: vec!["test/step_definitions.rs".to_string()],
        };

        let result = validate_plan(
            &plan,
            &["specs/**/*.feature".to_string()],
            &["test/**".to_string()],
            &["src/**".to_string()],
            true,  // spec locked
            false, // tests unlocked
            true,  // sut locked
        );

        assert!(result.valid);
        assert!(result.violations.is_empty());
    }

    #[test]
    fn test_validate_plan_with_violation() {
        let plan = ProposedPlan {
            files_to_modify: vec![
                "test/step_definitions.rs".to_string(),
                "specs/feature.feature".to_string(),
            ],
        };

        let result = validate_plan(
            &plan,
            &["specs/**/*.feature".to_string()],
            &["test/**".to_string()],
            &["src/**".to_string()],
            true,  // spec locked
            false, // tests unlocked
            true,  // sut locked
        );

        assert!(!result.valid);
        assert_eq!(result.violations.len(), 1);
        assert!(result.violations[0].contains("specs/feature.feature"));
        assert!(result.violations[0].contains("spec surface is LOCKED"));
    }

    #[test]
    fn test_extract_proposed_files_list_format() {
        let output = r#"
I will modify these files:
- test/step_definitions.rs
- src/lib.rs

Let me make these changes.
"#;

        let files = extract_proposed_files(output).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.contains(&"test/step_definitions.rs".to_string()));
        assert!(files.contains(&"src/lib.rs".to_string()));
    }

    #[test]
    fn test_extract_proposed_files_inline_format() {
        let output = "I'll modify src/main.rs to fix the bug.";

        let files = extract_proposed_files(output).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], "src/main.rs");
    }

    #[test]
    fn test_extract_proposed_files_no_clear_plan() {
        let output = "Let me think about this problem...";

        let files = extract_proposed_files(output);
        assert!(files.is_none());
    }

    #[test]
    fn test_glob_matching() {
        assert!(matches_any_pattern("test/steps.rs", &["test/**".to_string()]));
        assert!(matches_any_pattern("specs/feature.feature", &["specs/**/*.feature".to_string()]));
        assert!(!matches_any_pattern("src/main.rs", &["test/**".to_string()]));
    }
}
