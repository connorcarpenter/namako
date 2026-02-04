//! Spec quality checks for Tesaki.
//!
//! Runs after AddOrClarifyScenario to catch low-quality scenarios
//! before they count as progress. Rules:
//!   - NO_PLACEHOLDER_STEPS: ban "Given a test scenario", "Then no panic occurs",
//!     "Then the system intentionally fails"
//!   - DOMAIN_NOUN_REQUIRED: at least one significant word from the Rule header
//!     must appear in the scenario name or steps
//!   - NO_ORPHAN_STUBS: scenarios outside _orphan_stubs.feature must not
//!     reference stub-only step patterns

use serde::{Deserialize, Serialize};

/// A single quality-rule violation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecQualityViolation {
    /// Which rule was violated.
    pub rule: String,
    /// Feature file path.
    pub file: String,
    /// Name of the offending scenario.
    pub scenario_name: String,
    /// Human-readable details.
    pub details: String,
}

/// Aggregate result of running all quality checks on a feature file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecQualityResult {
    /// True when zero violations were found.
    pub passed: bool,
    /// All violations, ordered by rule then scenario.
    pub violations: Vec<SpecQualityViolation>,
}

impl SpecQualityResult {
    /// Format violations as a compact markdown block (≤ top 5).
    pub fn to_markdown(&self) -> String {
        if self.passed {
            return "Spec quality: OK".to_string();
        }
        let mut md = format!("### Spec Quality Violations ({})\n\n", self.violations.len());
        for v in self.violations.iter().take(5) {
            md.push_str(&format!(
                "- **[{}]** `{}` — {}\n",
                v.rule, v.scenario_name, v.details
            ));
        }
        if self.violations.len() > 5 {
            md.push_str(&format!("\n*…and {} more*\n", self.violations.len() - 5));
        }
        md
    }
}

// -------------------------------------------------------------------------
// Placeholder patterns that must never appear in executable scenarios
// -------------------------------------------------------------------------
const PLACEHOLDER_PATTERNS: &[(&str, &str)] = &[
    ("given a test scenario", "NO_PLACEHOLDER_STEPS"),
    ("then no panic occurs", "NO_PLACEHOLDER_STEPS"),
    ("then the system intentionally fails", "NO_PLACEHOLDER_STEPS"),
];

// -------------------------------------------------------------------------
// Common English words filtered out when extracting domain nouns
// -------------------------------------------------------------------------
const COMMON_WORDS: &[&str] = &[
    "the", "and", "for", "with", "from", "when", "that", "this", "should", "must",
    "can", "are", "was", "has", "have", "been", "will", "not", "but", "also",
    "after", "before", "while", "about", "into", "than", "then", "does", "each",
    "which", "their", "there", "what", "where", "only", "same", "some", "any",
    "being", "both", "another", "such", "more", "most", "other", "over", "under",
];

// -------------------------------------------------------------------------
// Parsed scenario from a .feature file
// -------------------------------------------------------------------------
struct ParsedScenario {
    name: String,
    steps: Vec<String>,
    rule_name: Option<String>,
}

/// Parse all scenarios out of a feature-file body.
/// Tracks the last-seen `Rule:` line so each scenario knows its parent rule.
fn parse_scenarios(content: &str) -> Vec<ParsedScenario> {
    let mut scenarios = Vec::new();
    let mut current_rule: Option<String> = None;
    let mut current_scenario: Option<ParsedScenario> = None;

    for line in content.lines() {
        let trimmed = line.trim();

        // Track current rule
        if let Some(rest) = trimmed.strip_prefix("Rule:") {
            current_rule = Some(rest.trim().to_string());
        }

        // New scenario boundary
        let scenario_name = trimmed
            .strip_prefix("Scenario:")
            .or_else(|| trimmed.strip_prefix("Scenario Outline:"));

        if let Some(name) = scenario_name {
            if let Some(prev) = current_scenario.take() {
                scenarios.push(prev);
            }
            current_scenario = Some(ParsedScenario {
                name: name.trim().to_string(),
                steps: Vec::new(),
                rule_name: current_rule.clone(),
            });
            continue;
        }

        // Accumulate step lines into the current scenario
        if let Some(ref mut sc) = current_scenario {
            if trimmed.starts_with("Given ")
                || trimmed.starts_with("When ")
                || trimmed.starts_with("Then ")
                || trimmed.starts_with("And ")
                || trimmed.starts_with("But ")
            {
                sc.steps.push(trimmed.to_string());
            }
        }
    }

    if let Some(last) = current_scenario {
        scenarios.push(last);
    }
    scenarios
}

// -------------------------------------------------------------------------
// Check: NO_PLACEHOLDER_STEPS
// -------------------------------------------------------------------------
fn check_placeholder_steps(
    file: &str,
    scenario: &ParsedScenario,
    violations: &mut Vec<SpecQualityViolation>,
) {
    let all_text = format!("{} {}", scenario.name, scenario.steps.join(" ")).to_lowercase();
    for &(pattern, _rule) in PLACEHOLDER_PATTERNS {
        if all_text.contains(pattern) {
            violations.push(SpecQualityViolation {
                rule: "NO_PLACEHOLDER_STEPS".to_string(),
                file: file.to_string(),
                scenario_name: scenario.name.clone(),
                details: format!("Placeholder text detected: \"{}\"", pattern),
            });
            return; // one violation per scenario is enough
        }
    }
}

// -------------------------------------------------------------------------
// Check: DOMAIN_NOUN_REQUIRED
// -------------------------------------------------------------------------
fn extract_domain_words(rule_name: &str) -> Vec<String> {
    rule_name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 3)
        .map(|w| w.to_lowercase())
        .filter(|w| !COMMON_WORDS.contains(&w.as_str()))
        .collect()
}

fn check_domain_noun(
    file: &str,
    scenario: &ParsedScenario,
    violations: &mut Vec<SpecQualityViolation>,
) {
    let rule_name = match &scenario.rule_name {
        Some(r) => r,
        None => return, // no rule context → skip
    };

    let domain_words = extract_domain_words(rule_name);
    if domain_words.is_empty() {
        return; // rule name has no extractable nouns
    }

    let all_text = format!("{} {}", scenario.name, scenario.steps.join(" ")).to_lowercase();

    if !domain_words.iter().any(|w| all_text.contains(w.as_str())) {
        violations.push(SpecQualityViolation {
            rule: "DOMAIN_NOUN_REQUIRED".to_string(),
            file: file.to_string(),
            scenario_name: scenario.name.clone(),
            details: format!(
                "No domain noun from Rule '{}' found. Expected one of: [{}]",
                rule_name,
                domain_words.join(", ")
            ),
        });
    }
}

// -------------------------------------------------------------------------
// Check: NO_ORPHAN_STUBS (only for files NOT named _orphan_stubs.feature)
// -------------------------------------------------------------------------
const ORPHAN_STEP_MARKERS: &[&str] = &["<stub>", "<placeholder>", "<todo>"];

fn check_orphan_stubs(
    file: &str,
    scenario: &ParsedScenario,
    violations: &mut Vec<SpecQualityViolation>,
) {
    // Only enforce outside the designated orphan-stubs file
    if file.contains("_orphan_stubs") {
        return;
    }
    let all_lower: Vec<String> = scenario.steps.iter().map(|s| s.to_lowercase()).collect();
    for step in &all_lower {
        for &marker in ORPHAN_STEP_MARKERS {
            if step.contains(marker) {
                violations.push(SpecQualityViolation {
                    rule: "NO_ORPHAN_STUBS".to_string(),
                    file: file.to_string(),
                    scenario_name: scenario.name.clone(),
                    details: format!("Orphan stub marker '{}' used outside _orphan_stubs.feature", marker),
                });
                return;
            }
        }
    }
}

// -------------------------------------------------------------------------
// Public entry point
// -------------------------------------------------------------------------

/// Run all spec-quality checks on the given feature file content.
///
/// `feature_path` is the relative path (used only for reporting).
/// `content` is the full text of the .feature file.
pub fn check_feature_quality(feature_path: &str, content: &str) -> SpecQualityResult {
    let scenarios = parse_scenarios(content);
    let mut violations = Vec::new();

    for scenario in &scenarios {
        check_placeholder_steps(feature_path, scenario, &mut violations);
        check_domain_noun(feature_path, scenario, &mut violations);
        check_orphan_stubs(feature_path, scenario, &mut violations);
    }

    SpecQualityResult {
        passed: violations.is_empty(),
        violations,
    }
}

// =============================================================================
// Tests
// =============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // parse_scenarios
    // -----------------------------------------------------------------------
    #[test]
    fn parse_scenarios_extracts_rule_context() {
        let feature = r#"
Feature: Widget
  Rule: Widget creation
    Scenario: Create a widget
      Given a user
      When they click create
      Then a widget appears
"#;
        let scenarios = parse_scenarios(feature);
        assert_eq!(scenarios.len(), 1);
        assert_eq!(scenarios[0].name, "Create a widget");
        assert_eq!(scenarios[0].rule_name.as_deref(), Some("Widget creation"));
        assert_eq!(scenarios[0].steps.len(), 3);
    }

    #[test]
    fn parse_scenarios_multiple_rules() {
        let feature = r#"
  Rule: Alpha rule
    Scenario: Alpha scenario
      Given alpha step

  Rule: Beta rule
    Scenario: Beta scenario
      Given beta step
"#;
        let scenarios = parse_scenarios(feature);
        assert_eq!(scenarios.len(), 2);
        assert_eq!(scenarios[0].rule_name.as_deref(), Some("Alpha rule"));
        assert_eq!(scenarios[1].rule_name.as_deref(), Some("Beta rule"));
    }

    // -----------------------------------------------------------------------
    // NO_PLACEHOLDER_STEPS
    // -----------------------------------------------------------------------
    #[test]
    fn placeholder_step_detected() {
        let feature = r#"
  Rule: Some rule
    Scenario: Bad scenario
      Given a test scenario
      When something
      Then the result
"#;
        let result = check_feature_quality("test.feature", feature);
        assert!(!result.passed);
        assert!(result.violations.iter().any(|v| v.rule == "NO_PLACEHOLDER_STEPS"));
    }

    #[test]
    fn no_panic_occurs_detected() {
        let feature = r#"
  Rule: Some rule
    Scenario: Panic test
      Given valid input
      When processed
      Then no panic occurs
"#;
        let result = check_feature_quality("test.feature", feature);
        assert!(!result.passed);
        assert!(result.violations.iter().any(|v| v.rule == "NO_PLACEHOLDER_STEPS"));
    }

    #[test]
    fn system_intentionally_fails_detected() {
        let feature = r#"
  Rule: Some rule
    Scenario: Intentional failure
      Given setup
      When triggered
      Then the system intentionally fails
"#;
        let result = check_feature_quality("test.feature", feature);
        assert!(!result.passed);
        assert!(result.violations.iter().any(|v| v.rule == "NO_PLACEHOLDER_STEPS"));
    }

    #[test]
    fn clean_scenario_passes_placeholder_check() {
        let feature = r#"
  Rule: Order processing
    Scenario: Process a valid order
      Given a valid order exists
      When the order is submitted
      Then the order status is confirmed
"#;
        let result = check_feature_quality("test.feature", feature);
        // Should pass NO_PLACEHOLDER_STEPS (domain noun check may or may not pass)
        assert!(!result.violations.iter().any(|v| v.rule == "NO_PLACEHOLDER_STEPS"));
    }

    // -----------------------------------------------------------------------
    // DOMAIN_NOUN_REQUIRED
    // -----------------------------------------------------------------------
    #[test]
    fn domain_noun_found_in_scenario_name() {
        let feature = r#"
  Rule: Widget creation
    Scenario: Create a new widget successfully
      Given a logged in user
      When they submit the form
      Then a widget is created
"#;
        let result = check_feature_quality("test.feature", feature);
        assert!(!result.violations.iter().any(|v| v.rule == "DOMAIN_NOUN_REQUIRED"));
    }

    #[test]
    fn domain_noun_found_in_steps() {
        let feature = r#"
  Rule: Widget creation
    Scenario: Happy path
      Given a valid widget request
      When submitted
      Then success
"#;
        let result = check_feature_quality("test.feature", feature);
        assert!(!result.violations.iter().any(|v| v.rule == "DOMAIN_NOUN_REQUIRED"));
    }

    #[test]
    fn domain_noun_missing_triggers_violation() {
        let feature = r#"
  Rule: Widget creation
    Scenario: Happy path
      Given a logged in user
      When they click submit
      Then a success message appears
"#;
        let result = check_feature_quality("test.feature", feature);
        assert!(result.violations.iter().any(|v| v.rule == "DOMAIN_NOUN_REQUIRED"));
    }

    #[test]
    fn no_rule_context_skips_domain_check() {
        // Scenario without a Rule parent — domain check is skipped
        let feature = r#"
  Scenario: Orphan scenario
    Given something
    When something else
    Then a result
"#;
        let result = check_feature_quality("test.feature", feature);
        assert!(!result.violations.iter().any(|v| v.rule == "DOMAIN_NOUN_REQUIRED"));
    }

    #[test]
    fn rule_with_only_common_words_skips_domain_check() {
        let feature = r#"
  Rule: The And For
    Scenario: Some scenario
      Given a step
      When another step
      Then a result
"#;
        let result = check_feature_quality("test.feature", feature);
        assert!(!result.violations.iter().any(|v| v.rule == "DOMAIN_NOUN_REQUIRED"));
    }

    // -----------------------------------------------------------------------
    // NO_ORPHAN_STUBS
    // -----------------------------------------------------------------------
    #[test]
    fn orphan_stub_marker_detected_outside_stubs_file() {
        let feature = r#"
  Rule: Some rule
    Scenario: Stub scenario
      Given a <stub> exists
      When triggered
      Then done
"#;
        let result = check_feature_quality("features/my_feature.feature", feature);
        assert!(result.violations.iter().any(|v| v.rule == "NO_ORPHAN_STUBS"));
    }

    #[test]
    fn orphan_stub_marker_allowed_in_orphan_stubs_file() {
        let feature = r#"
  Rule: Some rule
    Scenario: Stub scenario
      Given a <stub> exists
      When triggered
      Then done
"#;
        let result = check_feature_quality("features/_orphan_stubs.feature", feature);
        assert!(!result.violations.iter().any(|v| v.rule == "NO_ORPHAN_STUBS"));
    }

    #[test]
    fn placeholder_and_todo_markers_detected() {
        let feature = r#"
  Rule: Some rule
    Scenario: Todo scenario
      Given a <placeholder> input
      When processed
      Then result
"#;
        let result = check_feature_quality("test.feature", feature);
        assert!(result.violations.iter().any(|v| v.rule == "NO_ORPHAN_STUBS"));
    }

    // -----------------------------------------------------------------------
    // to_markdown
    // -----------------------------------------------------------------------
    #[test]
    fn to_markdown_ok() {
        let r = SpecQualityResult { passed: true, violations: vec![] };
        assert!(r.to_markdown().contains("OK"));
    }

    #[test]
    fn to_markdown_with_violations() {
        let r = SpecQualityResult {
            passed: false,
            violations: vec![
                SpecQualityViolation {
                    rule: "NO_PLACEHOLDER_STEPS".to_string(),
                    file: "a.feature".to_string(),
                    scenario_name: "Bad".to_string(),
                    details: "placeholder".to_string(),
                },
            ],
        };
        let md = r.to_markdown();
        assert!(md.contains("NO_PLACEHOLDER_STEPS"));
        assert!(md.contains("Bad"));
    }
}
