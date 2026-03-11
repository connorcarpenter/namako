//! Scenario Extractor: Find example scenarios from feature files.
//!
//! This module extracts scenario examples from Gherkin feature files to provide
//! dynamic, relevant context for AddOrClarifyScenario missions.

use std::fs;
use std::path::Path;

use anyhow::Result;

/// Extract the first complete scenario from a feature file as an example.
///
/// Returns the scenario text (including @Rule and @Scenario tags, Given/When/Then steps)
/// suitable for inclusion in mission context.
pub fn extract_example_scenario(feature_path: &Path) -> Result<Option<String>> {
    if !feature_path.is_file() {
        return Ok(None);
    }

    let content = fs::read_to_string(feature_path)?;

    // Find the first scenario
    let lines: Vec<&str> = content.lines().collect();
    let mut in_scenario = false;
    let mut scenario_lines = Vec::new();
    let mut rule_tag = String::new();

    for line in &lines {
        let trimmed = line.trim();

        // Track the most recent @Rule tag
        if trimmed.starts_with("@Rule(") {
            rule_tag = trimmed.to_string();
            continue;
        }

        // Start capturing on @Scenario tag
        if trimmed.starts_with("@Scenario(") {
            if in_scenario {
                // We already have a scenario, stop here
                break;
            }
            in_scenario = true;
            if !rule_tag.is_empty() {
                scenario_lines.push(rule_tag.clone());
            }
            scenario_lines.push(trimmed.to_string());
            continue;
        }

        if in_scenario {
            // Stop at next Rule or empty line after Then
            if trimmed.starts_with("@Rule(") || trimmed.starts_with("Rule:") {
                break;
            }
            if trimmed.starts_with("@Scenario(") {
                break;
            }

            // Include Scenario:, Given, When, Then, And lines
            if trimmed.starts_with("Scenario:")
                || trimmed.starts_with("Given ")
                || trimmed.starts_with("When ")
                || trimmed.starts_with("Then ")
                || trimmed.starts_with("And ")
                || trimmed.starts_with("But ")
            {
                scenario_lines.push(format!("  {}", trimmed));
            }

            // Stop after we have a complete scenario (after Then line and next empty/tag)
            if scenario_lines.len() >= 3 && trimmed.is_empty() {
                // Check if we have at least Given/When/Then
                let has_given = scenario_lines.iter().any(|l| l.contains("Given "));
                let has_when = scenario_lines.iter().any(|l| l.contains("When "));
                let has_then = scenario_lines.iter().any(|l| l.contains("Then "));
                if has_given && has_when && has_then {
                    break;
                }
            }
        }
    }

    if scenario_lines.is_empty() {
        return Ok(None);
    }

    Ok(Some(scenario_lines.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_extract_example_scenario() {
        let content = r#"
Feature: Test Feature

  @Rule(01)
  Rule: Some rule

    @Scenario(01)
    Scenario: First test scenario
      Given a precondition
      When an action occurs
      Then an outcome is observed

    @Scenario(02)
    Scenario: Second scenario
      Given another precondition
      When another action
      Then another outcome
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(content.as_bytes()).unwrap();

        let result = extract_example_scenario(temp_file.path()).unwrap();
        assert!(result.is_some());
        let scenario = result.unwrap();

        assert!(scenario.contains("@Rule(01)"));
        assert!(scenario.contains("@Scenario(01)"));
        assert!(scenario.contains("Scenario: First test scenario"));
        assert!(scenario.contains("Given a precondition"));
        assert!(scenario.contains("When an action occurs"));
        assert!(scenario.contains("Then an outcome is observed"));

        // Should NOT contain the second scenario
        assert!(!scenario.contains("Second scenario"));
    }

    #[test]
    fn test_extract_no_scenarios() {
        let content = "Feature: Empty feature\n\n# Just comments\n";

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(content.as_bytes()).unwrap();

        let result = extract_example_scenario(temp_file.path()).unwrap();
        assert!(result.is_none());
    }
}
