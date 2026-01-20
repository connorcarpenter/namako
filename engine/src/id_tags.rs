//! Explicit ID Tag Parsing for Namako v1.5
//!
//! This module implements parsing of `@Feature(name)`, `@Rule_nn`, `@Scenario_nn` tags
//! per GOLD_PLAN §10.5.1.

use regex::Regex;
use std::sync::LazyLock;

/// Regex for @Feature(name) tag - captures the name inside parentheses
/// Matches: @Feature(connection_lifecycle) or Feature(connection_lifecycle)
static FEATURE_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^@?[Ff]eature\(([a-zA-Z][a-zA-Z0-9_]*)\)$").unwrap()
});

/// Regex for @Rule_nn tag - captures the numeric index
/// Matches: @Rule_01, @Rule_1, Rule_01, etc.
static RULE_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^@?[Rr]ule_(\d+)$").unwrap()
});

/// Regex for @Scenario_nn tag - captures the numeric index
/// Matches: @Scenario_01, @Scenario_1, Scenario_01, etc.
static SCENARIO_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^@?[Ss]cenario_(\d+)$").unwrap()
});

/// Parsed explicit ID from a Feature's tags
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureId(pub String);

/// Parsed explicit ID from a Rule's tags
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleId(pub u32);

/// Parsed explicit ID from a Scenario's tags
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioId(pub u32);

/// Extract @Feature(name) from a list of tags
/// Returns None if no valid feature tag found
pub fn extract_feature_id(tags: &[String]) -> Option<FeatureId> {
    for tag in tags {
        if let Some(caps) = FEATURE_TAG_RE.captures(tag) {
            if let Some(name) = caps.get(1) {
                return Some(FeatureId(name.as_str().to_string()));
            }
        }
    }
    None
}

/// Extract @Rule_nn from a list of tags
/// Returns None if no valid rule tag found
pub fn extract_rule_id(tags: &[String]) -> Option<RuleId> {
    for tag in tags {
        if let Some(caps) = RULE_TAG_RE.captures(tag) {
            if let Some(num_str) = caps.get(1) {
                if let Ok(num) = num_str.as_str().parse::<u32>() {
                    return Some(RuleId(num));
                }
            }
        }
    }
    None
}

/// Extract @Scenario_nn from a list of tags
/// Returns None if no valid scenario tag found
pub fn extract_scenario_id(tags: &[String]) -> Option<ScenarioId> {
    for tag in tags {
        if let Some(caps) = SCENARIO_TAG_RE.captures(tag) {
            if let Some(num_str) = caps.get(1) {
                if let Ok(num) = num_str.as_str().parse::<u32>() {
                    return Some(ScenarioId(num));
                }
            }
        }
    }
    None
}

/// Derives scenario key from explicit IDs per GOLD_PLAN §10.5.1
///
/// Format: `FeatureName:Rule_nn:Scenario_nn` or `FeatureName:Scenario_nn` (no rule)
///
/// # Arguments
/// * `feature_id` - The @Feature(name) value
/// * `rule_id` - The @Rule_nn value (None for feature-level scenarios)
/// * `scenario_id` - The @Scenario_nn value
pub fn derive_scenario_key_from_ids(
    feature_id: &FeatureId,
    rule_id: Option<&RuleId>,
    scenario_id: &ScenarioId,
) -> String {
    match rule_id {
        Some(RuleId(r)) => format!(
            "{}:Rule_{:02}:Scenario_{:02}",
            feature_id.0, r, scenario_id.0
        ),
        None => format!(
            "{}:Scenario_{:02}",
            feature_id.0, scenario_id.0
        ),
    }
}

/// Derives scenario outline example key with EID extension
///
/// Format: `FeatureName:Rule_nn:Scenario_nn:E<eid>` or `FeatureName:Scenario_nn:E<eid>`
///
/// # Arguments
/// * `feature_id` - The @Feature(name) value
/// * `rule_id` - The @Rule_nn value (None for feature-level scenarios)
/// * `scenario_id` - The @Scenario_nn value
/// * `example_id` - The EID column value or fallback index
pub fn derive_scenario_outline_key_from_ids(
    feature_id: &FeatureId,
    rule_id: Option<&RuleId>,
    scenario_id: &ScenarioId,
    example_id: &str,
) -> String {
    let base = derive_scenario_key_from_ids(feature_id, rule_id, scenario_id);
    format!("{}:E{}", base, example_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_feature_id() {
        let tags = vec!["@Feature(connection_lifecycle)".to_string()];
        assert_eq!(
            extract_feature_id(&tags),
            Some(FeatureId("connection_lifecycle".to_string()))
        );
    }

    #[test]
    fn test_extract_feature_id_without_at() {
        let tags = vec!["Feature(smoke)".to_string()];
        assert_eq!(
            extract_feature_id(&tags),
            Some(FeatureId("smoke".to_string()))
        );
    }

    #[test]
    fn test_extract_rule_id() {
        let tags = vec!["@Rule_01".to_string()];
        assert_eq!(extract_rule_id(&tags), Some(RuleId(1)));
    }

    #[test]
    fn test_extract_rule_id_double_digit() {
        let tags = vec!["@Rule_12".to_string()];
        assert_eq!(extract_rule_id(&tags), Some(RuleId(12)));
    }

    #[test]
    fn test_extract_scenario_id() {
        let tags = vec!["@Scenario_05".to_string()];
        assert_eq!(extract_scenario_id(&tags), Some(ScenarioId(5)));
    }

    #[test]
    fn test_derive_scenario_key_with_rule() {
        let feature = FeatureId("connection_lifecycle".to_string());
        let rule = RuleId(1);
        let scenario = ScenarioId(3);
        assert_eq!(
            derive_scenario_key_from_ids(&feature, Some(&rule), &scenario),
            "connection_lifecycle:Rule_01:Scenario_03"
        );
    }

    #[test]
    fn test_derive_scenario_key_without_rule() {
        let feature = FeatureId("smoke".to_string());
        let scenario = ScenarioId(1);
        assert_eq!(
            derive_scenario_key_from_ids(&feature, None, &scenario),
            "smoke:Scenario_01"
        );
    }

    #[test]
    fn test_derive_scenario_outline_key() {
        let feature = FeatureId("auth".to_string());
        let rule = RuleId(2);
        let scenario = ScenarioId(1);
        assert_eq!(
            derive_scenario_outline_key_from_ids(&feature, Some(&rule), &scenario, "valid_token"),
            "auth:Rule_02:Scenario_01:Evalid_token"
        );
    }
}
