//! Binding Extractor: Find similar bindings from existing codebase.
//!
//! This module extracts step binding examples from the repository to provide
//! dynamic, relevant context for CreateMissingBindings missions. Instead of
//! showing generic examples, we show real bindings from the same codebase.

use std::fs;
use std::path::Path;

use anyhow::Result;
use regex::Regex;

use crate::prompts::BindingExemplar;

/// Extract binding exemplars from a steps directory.
///
/// Scans Rust files for step bindings (Given/When/Then macros) and extracts
/// the step text and binding code. Returns exemplars sorted by relevance to
/// the missing steps.
#[allow(dead_code)]
pub fn extract_binding_exemplars(
    steps_dir: &Path,
    missing_steps: &[String],
    max_exemplars: usize,
) -> Result<Vec<BindingExemplar>> {
    let mut all_bindings = Vec::new();

    // Find all .rs files in the steps directory
    if steps_dir.is_dir() {
        for entry in fs::read_dir(steps_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "rs") {
                if let Ok(bindings) = parse_bindings_from_file(&path) {
                    all_bindings.extend(bindings);
                }
            }
        }
    }

    if all_bindings.is_empty() {
        return Ok(Vec::new());
    }

    // Score each binding by similarity to missing steps
    let mut scored: Vec<(BindingExemplar, f32)> = all_bindings
        .into_iter()
        .map(|b| {
            let score = best_similarity_score(&b.step_text, missing_steps);
            (b, score)
        })
        .collect();

    // Sort by score descending
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Take top N with non-zero scores
    let exemplars: Vec<BindingExemplar> = scored
        .into_iter()
        .filter(|(_, score)| *score > 0.0)
        .take(max_exemplars)
        .map(|(b, _)| b)
        .collect();

    Ok(exemplars)
}

/// Parse binding functions from a Rust source file.
#[allow(dead_code)]
fn parse_bindings_from_file(path: &Path) -> Result<Vec<BindingExemplar>> {
    let content = fs::read_to_string(path)?;
    let file_path = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown.rs".to_string());

    parse_bindings_from_content(&content, &file_path)
}

/// Parse binding functions from source content.
fn parse_bindings_from_content(content: &str, file_path: &str) -> Result<Vec<BindingExemplar>> {
    let mut exemplars = Vec::new();

    // Regex to match step macro + function definition
    // Matches: #[given("...")] fn name(...) { ... }
    // or: #[when("...")] fn name(...) { ... }
    // or: #[then("...")] fn name(...) { ... }
    let binding_re = Regex::new(
        r#"(?s)#\[(given|when|then)\("([^"]+)"\)\]\s*fn\s+(\w+)\s*\([^)]*\)\s*(?:->[\s\w<>,()]+)?\s*\{"#,
    )?;

    for cap in binding_re.captures_iter(content) {
        let macro_type = &cap[1];
        let step_text = &cap[2];
        // Find the function body (simplified: take up to closing brace at same indentation)
        if let Some(start) = cap.get(0) {
            if let Some(body) = extract_function_body(&content[start.start()..]) {
                let binding_code = format!("#[{}(\"{}\")]\n{}", macro_type, step_text, body);

                exemplars.push(BindingExemplar {
                    step_text: step_text.to_string(),
                    binding_code,
                    file_path: format!("test/tests/src/steps/{}", file_path),
                });
            }
        }
    }

    Ok(exemplars)
}

/// Extract a function body (simplified extraction for display purposes).
/// Returns a trimmed version suitable for an example.
fn extract_function_body(content: &str) -> Option<String> {
    // Find the function signature line and body
    let mut brace_count = 0;
    let mut started = false;
    let mut end_pos = 0;

    for (i, c) in content.chars().enumerate() {
        if c == '{' {
            started = true;
            brace_count += 1;
        } else if c == '}' {
            brace_count -= 1;
            if started && brace_count == 0 {
                end_pos = i + 1;
                break;
            }
        }
    }

    if end_pos > 0 {
        let full = &content[..end_pos];
        // Skip the macro line, just get fn ... { ... }
        let fn_start = full.find("fn ")?;
        Some(full[fn_start..].to_string())
    } else {
        None
    }
}

/// Calculate best similarity score between a binding's step text and any missing step.
#[allow(dead_code)]
fn best_similarity_score(binding_step: &str, missing_steps: &[String]) -> f32 {
    missing_steps
        .iter()
        .map(|missing| word_overlap_score(binding_step, missing))
        .fold(0.0f32, f32::max)
}

/// Simple word overlap similarity score.
/// Returns 0.0-1.0 based on what fraction of words overlap.
fn word_overlap_score(a: &str, b: &str) -> f32 {
    let words_a: std::collections::HashSet<String> = a
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && w.len() > 2) // Skip short words like "a", "is"
        .map(|w| w.to_lowercase())
        .collect();

    let words_b: std::collections::HashSet<String> = b
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && w.len() > 2)
        .map(|w| w.to_lowercase())
        .collect();

    if words_a.is_empty() || words_b.is_empty() {
        return 0.0;
    }

    let intersection = words_a.intersection(&words_b).count() as f32;
    let union = words_a.union(&words_b).count() as f32;

    intersection / union // Jaccard similarity
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bindings_from_content() {
        let content = r#"
use something;

#[given("a server is running")]
fn given_server_running(ctx: &mut TestWorldMut) {
    let scenario = ctx.init();
    scenario.server_start();
}

#[when("the client connects")]
fn when_client_connects(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    scenario.client_connect();
}

#[then("the connection is established")]
fn then_connection_established(ctx: &TestWorldRef) -> AssertOutcome<()> {
    if ctx.is_connected() {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}
"#;

        let exemplars = parse_bindings_from_content(content, "test.rs").unwrap();
        assert_eq!(exemplars.len(), 3);
        assert_eq!(exemplars[0].step_text, "a server is running");
        assert_eq!(exemplars[1].step_text, "the client connects");
        assert_eq!(exemplars[2].step_text, "the connection is established");
    }

    #[test]
    fn test_word_overlap_score() {
        // Identical
        assert!(word_overlap_score("the server is running", "the server is running") > 0.9);

        // Similar
        let score = word_overlap_score("a server is running", "the server has started");
        assert!(score > 0.0);
        assert!(score < 1.0);

        // Very different
        let score = word_overlap_score("a server is running", "the client sends message");
        assert!(score < 0.5);
    }

    #[test]
    fn test_word_overlap_score_empty() {
        assert_eq!(word_overlap_score("", "something"), 0.0);
        assert_eq!(word_overlap_score("something", ""), 0.0);
    }
}
