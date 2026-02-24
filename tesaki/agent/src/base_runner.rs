//! Shared base runner utilities for Namako runners.

pub use servling::{run_cli_runner, CliRunnerConfig, CliRunnerOutcome};

/// Check whether any files changed by the runner fall outside the allowed surface.
pub fn check_surface_violations(
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
    }
    violations
}

/// Simple glob-style pattern matching.
pub fn matches_any_pattern(path: &str, patterns: &[String]) -> bool {
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
    fn test_glob_match_double_star() {
        assert!(glob_match("test/specs/**/*.feature", "test/specs/features/a.feature"));
        assert!(glob_match("src/**", "src/main.rs"));
        assert!(glob_match("src/**", "src/sub/deep/file.rs"));
        assert!(!glob_match("src/**", "test/main.rs"));
    }
}
