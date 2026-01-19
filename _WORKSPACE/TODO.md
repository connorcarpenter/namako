# TODO.md — Explicit ID Tags Implementation Plan

**Created:** 2026-01-19
**Feature:** Explicit ID Tags (`@Feature/@Rule_nn/@Scenario_nn`)
**GOLD_PLAN Section:** §10.5.1
**Target Agent:** Claude Haiku 4.5 (Executor)

---

## Executive Summary

This plan implements **Explicit ID Tags** for Namako v1.5, enabling refactor-stable scenario identity. Currently, `scenario_key` is derived from `path:Lnn` (file path + line number), which breaks when scenarios are reordered. After this change, `scenario_key` will be derived from explicit Gherkin tags: `@Feature(name):@Rule_nn:@Scenario_nn`.

### Key Changes
1. Parse `@Feature(name)`, `@Rule_nn`, `@Scenario_nn` tags from Gherkin AST
2. Change `scenario_key` derivation from `path:Lnn` → `Feature:Rule_nn:Scenario_nn`
3. Add validation for missing/duplicate ID tags (hard error in `namako lint`)
4. Migrate all 16 feature files to include explicit ID tags
5. Update certification (run `namako update-cert` after migration)

---

## Phase 1: Core Parsing Infrastructure

### Task 1.1: Create Tag Parsing Module
**File:** `namako/src/id_tags.rs` (NEW FILE)

Create a new module to handle explicit ID tag parsing:

```rust
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
/// Format: `FeatureName:Rule_nn:Scenario_nn`
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
/// Format: `FeatureName:Rule_nn:Scenario_nn:E<eid>`
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
```

**Action Items:**
1. Create file `namako/src/id_tags.rs` with the above content
2. Add `pub mod id_tags;` to `namako/src/lib.rs`
3. Run `cargo build -p namako` to verify compilation

---

### Task 1.2: Add Validation Error Types
**File:** `namako/src/engine.rs`

Add new error variants for ID tag validation:

**Find this block (around line 25-55):**
```rust
/// Errors that can occur during resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionError {
    /// No binding found for step (0 matches)
    MissingStep {
```

**Add these new variants after the existing ones (before the closing `}`):**
```rust
    /// Feature is missing @Feature(name) tag
    MissingFeatureId {
        feature_path: String,
        feature_name: String,
    },
    /// Rule is missing @Rule_nn tag
    MissingRuleId {
        feature_path: String,
        rule_name: String,
    },
    /// Scenario is missing @Scenario_nn tag
    MissingScenarioId {
        feature_path: String,
        scenario_name: String,
        rule_name: Option<String>,
    },
    /// Duplicate scenario key detected
    DuplicateScenarioKey {
        scenario_key: String,
        feature_path: String,
        scenario_name: String,
    },
    /// Duplicate rule ID within a feature
    DuplicateRuleId {
        feature_path: String,
        rule_id: u32,
    },
    /// Duplicate scenario ID within a rule or feature
    DuplicateScenarioId {
        feature_path: String,
        rule_name: Option<String>,
        scenario_id: u32,
    },
```

**Find the `impl std::fmt::Display for ResolutionError` block and add these match arms:**
```rust
            Self::MissingFeatureId {
                feature_path,
                feature_name,
            } => {
                write!(
                    f,
                    "Missing @Feature(name) tag: feature \"{feature_name}\" in {feature_path} \
                     must have a @Feature(snake_case_name) tag"
                )
            }
            Self::MissingRuleId {
                feature_path,
                rule_name,
            } => {
                write!(
                    f,
                    "Missing @Rule_nn tag: rule \"{rule_name}\" in {feature_path} \
                     must have a @Rule_nn tag (e.g., @Rule_01)"
                )
            }
            Self::MissingScenarioId {
                feature_path,
                scenario_name,
                rule_name,
            } => {
                let context = rule_name
                    .as_ref()
                    .map(|r| format!(" in rule \"{r}\""))
                    .unwrap_or_default();
                write!(
                    f,
                    "Missing @Scenario_nn tag: scenario \"{scenario_name}\"{context} in {feature_path} \
                     must have a @Scenario_nn tag (e.g., @Scenario_01)"
                )
            }
            Self::DuplicateScenarioKey {
                scenario_key,
                feature_path,
                scenario_name,
            } => {
                write!(
                    f,
                    "Duplicate scenario key: \"{scenario_key}\" for scenario \"{scenario_name}\" \
                     in {feature_path}"
                )
            }
            Self::DuplicateRuleId {
                feature_path,
                rule_id,
            } => {
                write!(
                    f,
                    "Duplicate @Rule_{rule_id:02} tag in {feature_path}"
                )
            }
            Self::DuplicateScenarioId {
                feature_path,
                rule_name,
                scenario_id,
            } => {
                let context = rule_name
                    .as_ref()
                    .map(|r| format!(" in rule \"{r}\""))
                    .unwrap_or_default();
                write!(
                    f,
                    "Duplicate @Scenario_{scenario_id:02} tag{context} in {feature_path}"
                )
            }
```

**Action Items:**
1. Add the error variants to `ResolutionError` enum
2. Add the `Display` implementations for each new variant
3. Run `cargo build -p namako` to verify

---

## Phase 2: Engine Refactoring

### Task 2.1: Update Engine Imports
**File:** `namako/src/engine.rs`

**Find this line (around line 20-22):**
```rust
use crate::npap::{
    PlannedStep, ResolvedPlan, ResolvedScenario, SemanticBinding,
    SemanticStepRegistry, compute_feature_fingerprint, derive_scenario_key,
};
```

**Replace with:**
```rust
use crate::npap::{
    PlannedStep, ResolvedPlan, ResolvedScenario, SemanticBinding,
    SemanticStepRegistry, compute_feature_fingerprint,
};
use crate::id_tags::{
    extract_feature_id, extract_rule_id, extract_scenario_id,
    derive_scenario_key_from_ids, FeatureId, RuleId, ScenarioId,
};
use std::collections::HashSet;
```

**Action Items:**
1. Update the imports as shown
2. Run `cargo build -p namako` to verify (will have unused warnings until next task)

---

### Task 2.2: Refactor `resolve_feature` Method
**File:** `namako/src/engine.rs`

The `resolve_feature` method needs to be completely refactored. Find the method starting around line 294:

```rust
    /// Resolves a single feature file.
    fn resolve_feature(
        &self,
        path: &str,
        feature: &Feature,
        resolved_scenarios: &mut Vec<ResolvedScenario>,
        errors: &mut Vec<ResolutionError>,
        _warnings: &mut Vec<String>,
    ) {
```

**Replace the ENTIRE `resolve_feature` method with:**

```rust
    /// Resolves a single feature file with explicit ID tag validation.
    fn resolve_feature(
        &self,
        path: &str,
        feature: &Feature,
        resolved_scenarios: &mut Vec<ResolvedScenario>,
        errors: &mut Vec<ResolutionError>,
        _warnings: &mut Vec<String>,
        seen_keys: &mut HashSet<String>,
    ) {
        // Step 1: Extract and validate @Feature(name) tag
        let feature_id = match extract_feature_id(&feature.tags) {
            Some(id) => id,
            None => {
                errors.push(ResolutionError::MissingFeatureId {
                    feature_path: path.to_string(),
                    feature_name: feature.name.clone(),
                });
                return; // Cannot proceed without feature ID
            }
        };

        // Resolve feature-level background steps once
        let feature_background_steps = self.resolve_background_steps(
            feature.background.as_ref(),
            path,
            errors,
        );

        // Track seen rule IDs for duplicate detection
        let mut seen_rule_ids: HashSet<u32> = HashSet::new();

        // Handle feature-level scenarios (no rule) - track scenario IDs
        let mut feature_scenario_ids: HashSet<u32> = HashSet::new();
        for scenario in &feature.scenarios {
            // Skip deferred scenarios
            if is_deferred_scenario(scenario) {
                continue;
            }

            // Extract and validate @Scenario_nn tag
            let scenario_id = match extract_scenario_id(&scenario.tags) {
                Some(id) => id,
                None => {
                    errors.push(ResolutionError::MissingScenarioId {
                        feature_path: path.to_string(),
                        scenario_name: scenario.name.clone(),
                        rule_name: None,
                    });
                    continue;
                }
            };

            // Check for duplicate scenario ID within feature-level scenarios
            if !feature_scenario_ids.insert(scenario_id.0) {
                errors.push(ResolutionError::DuplicateScenarioId {
                    feature_path: path.to_string(),
                    rule_name: None,
                    scenario_id: scenario_id.0,
                });
                continue;
            }

            // Derive scenario key
            let scenario_key = derive_scenario_key_from_ids(&feature_id, None, &scenario_id);

            // Check for duplicate scenario key globally
            if !seen_keys.insert(scenario_key.clone()) {
                errors.push(ResolutionError::DuplicateScenarioKey {
                    scenario_key: scenario_key.clone(),
                    feature_path: path.to_string(),
                    scenario_name: scenario.name.clone(),
                });
                continue;
            }

            // Resolve steps
            let mut resolved_steps = feature_background_steps.clone();
            let mut last_keyword = if resolved_steps.is_empty() { "Given" } else { "Given" };

            for step in &scenario.steps {
                let effective_kind = resolve_effective_kind(&step.keyword, &mut last_keyword);
                match self.resolve_step(step, path, &effective_kind) {
                    Ok(planned) => resolved_steps.push(planned),
                    Err(e) => errors.push(e),
                }
            }

            resolved_scenarios.push(ResolvedScenario {
                scenario_key,
                feature_path: path.to_string(),
                scenario_name: scenario.name.clone(),
                steps: resolved_steps,
            });
        }

        // Handle rules and their scenarios
        for rule in &feature.rules {
            // Extract and validate @Rule_nn tag
            let rule_id = match extract_rule_id(&rule.tags) {
                Some(id) => id,
                None => {
                    errors.push(ResolutionError::MissingRuleId {
                        feature_path: path.to_string(),
                        rule_name: rule.name.clone(),
                    });
                    continue;
                }
            };

            // Check for duplicate rule ID
            if !seen_rule_ids.insert(rule_id.0) {
                errors.push(ResolutionError::DuplicateRuleId {
                    feature_path: path.to_string(),
                    rule_id: rule_id.0,
                });
                continue;
            }

            // Rules can have their own background
            let rule_background_steps = self.resolve_background_steps(
                rule.background.as_ref(),
                path,
                errors,
            );

            // Track scenario IDs within this rule
            let mut rule_scenario_ids: HashSet<u32> = HashSet::new();

            for scenario in &rule.scenarios {
                // Skip deferred scenarios
                if is_deferred_scenario(scenario) {
                    continue;
                }

                // Extract and validate @Scenario_nn tag
                let scenario_id = match extract_scenario_id(&scenario.tags) {
                    Some(id) => id,
                    None => {
                        errors.push(ResolutionError::MissingScenarioId {
                            feature_path: path.to_string(),
                            scenario_name: scenario.name.clone(),
                            rule_name: Some(rule.name.clone()),
                        });
                        continue;
                    }
                };

                // Check for duplicate scenario ID within this rule
                if !rule_scenario_ids.insert(scenario_id.0) {
                    errors.push(ResolutionError::DuplicateScenarioId {
                        feature_path: path.to_string(),
                        rule_name: Some(rule.name.clone()),
                        scenario_id: scenario_id.0,
                    });
                    continue;
                }

                // Derive scenario key
                let scenario_key = derive_scenario_key_from_ids(
                    &feature_id,
                    Some(&rule_id),
                    &scenario_id,
                );

                // Check for duplicate scenario key globally
                if !seen_keys.insert(scenario_key.clone()) {
                    errors.push(ResolutionError::DuplicateScenarioKey {
                        scenario_key: scenario_key.clone(),
                        feature_path: path.to_string(),
                        scenario_name: scenario.name.clone(),
                    });
                    continue;
                }

                // Feature background first, then rule background, then scenario steps
                let mut resolved_steps = feature_background_steps.clone();
                resolved_steps.extend(rule_background_steps.clone());
                let mut last_keyword = "Given";

                for step in &scenario.steps {
                    let effective_kind = resolve_effective_kind(&step.keyword, &mut last_keyword);
                    match self.resolve_step(step, path, &effective_kind) {
                        Ok(planned) => resolved_steps.push(planned),
                        Err(e) => errors.push(e),
                    }
                }

                resolved_scenarios.push(ResolvedScenario {
                    scenario_key,
                    feature_path: path.to_string(),
                    scenario_name: scenario.name.clone(),
                    steps: resolved_steps,
                });
            }
        }
    }
```

**Action Items:**
1. Replace the entire `resolve_feature` method
2. Note that this adds a new parameter `seen_keys: &mut HashSet<String>`

---

### Task 2.3: Update `resolve` Method to Pass `seen_keys`
**File:** `namako/src/engine.rs`

Find the `resolve` method (around line 228-285). It needs to be updated to create and pass `seen_keys`.

**Find this block in the `resolve` method:**
```rust
        for (path, source) in features {
            feature_files.push((path, source));

            // Parse the feature
            let env = GherkinEnv::default();
            match Feature::parse(source, env) {
                Ok(feature) => {
                    self.resolve_feature(
                        path,
                        &feature,
                        &mut resolved_scenarios,
                        &mut errors,
                        &mut warnings,
                    );
                }
```

**Replace with:**
```rust
        // Track seen scenario keys globally for duplicate detection
        let mut seen_keys: HashSet<String> = HashSet::new();

        for (path, source) in features {
            feature_files.push((path, source));

            // Parse the feature
            let env = GherkinEnv::default();
            match Feature::parse(source, env) {
                Ok(feature) => {
                    self.resolve_feature(
                        path,
                        &feature,
                        &mut resolved_scenarios,
                        &mut errors,
                        &mut warnings,
                        &mut seen_keys,
                    );
                }
```

**Action Items:**
1. Add `let mut seen_keys: HashSet<String> = HashSet::new();` before the loop
2. Add `&mut seen_keys` as the last argument to `resolve_feature`
3. Run `cargo build -p namako` to verify

---

### Task 2.4: Update Unit Tests
**File:** `namako/src/engine.rs`

Find the test module at the bottom of the file (around line 578+). The tests need to be updated to work with the new ID tag requirements.

**Find the helper function `make_binding` and add a new test helper after it:**

```rust
    /// Creates a minimal valid feature source with explicit ID tags
    fn make_tagged_feature(name: &str, scenarios: &[(Option<u32>, u32, &str, &[&str])]) -> String {
        let mut source = format!(
            "@Feature({})\nFeature: Test Feature\n\n",
            name
        );

        for (rule_id, scenario_id, scenario_name, steps) in scenarios {
            if let Some(rid) = rule_id {
                source.push_str(&format!("  @Rule_{:02}\n  Rule: Rule {}\n\n", rid, rid));
            }
            source.push_str(&format!(
                "    @Scenario_{:02}\n    Scenario: {}\n",
                scenario_id, scenario_name
            ));
            for step in *steps {
                source.push_str(&format!("      {}\n", step));
            }
            source.push('\n');
        }

        source
    }
```

**Update the existing test `test_resolution_engine_creation` to use tagged features:**

Find and replace the test's feature source strings with properly tagged versions. The tests should still compile and pass.

**Action Items:**
1. Add the helper function
2. Update existing tests to use tagged features OR mark them as `#[ignore]` temporarily
3. Add new tests for ID tag validation (see Task 2.5)

---

### Task 2.5: Add ID Tag Validation Tests
**File:** `namako/src/engine.rs`

Add these tests to the test module:

```rust
    #[test]
    fn test_missing_feature_id_tag() {
        let registry = SemanticStepRegistry::new(vec![
            make_binding("Given", "something", 0),
        ]);
        let engine = ResolutionEngine::new(&registry).unwrap();

        // Feature without @Feature tag
        let source = r#"
Feature: Test
  Scenario: Test scenario
    Given something
"#;
        let result = engine.resolve(vec![("test.feature", source)].into_iter());

        assert!(!result.errors.is_empty());
        assert!(result.errors.iter().any(|e| matches!(e, ResolutionError::MissingFeatureId { .. })));
    }

    #[test]
    fn test_missing_scenario_id_tag() {
        let registry = SemanticStepRegistry::new(vec![
            make_binding("Given", "something", 0),
        ]);
        let engine = ResolutionEngine::new(&registry).unwrap();

        // Feature with @Feature but scenario without @Scenario_nn
        let source = r#"
@Feature(test)
Feature: Test
  Scenario: Test scenario
    Given something
"#;
        let result = engine.resolve(vec![("test.feature", source)].into_iter());

        assert!(!result.errors.is_empty());
        assert!(result.errors.iter().any(|e| matches!(e, ResolutionError::MissingScenarioId { .. })));
    }

    #[test]
    fn test_valid_explicit_ids() {
        let registry = SemanticStepRegistry::new(vec![
            make_binding("Given", "something", 0),
        ]);
        let engine = ResolutionEngine::new(&registry).unwrap();

        let source = r#"
@Feature(test_feature)
Feature: Test Feature

  @Scenario_01
  Scenario: First scenario
    Given something
"#;
        let result = engine.resolve(vec![("test.feature", source)].into_iter());

        assert!(result.errors.is_empty(), "Expected no errors but got: {:?}", result.errors);
        let plan = result.plan.unwrap();
        assert_eq!(plan.scenarios.len(), 1);
        assert_eq!(plan.scenarios[0].scenario_key, "test_feature:Scenario_01");
    }

    #[test]
    fn test_scenario_key_with_rule() {
        let registry = SemanticStepRegistry::new(vec![
            make_binding("Given", "something", 0),
        ]);
        let engine = ResolutionEngine::new(&registry).unwrap();

        let source = r#"
@Feature(my_feature)
Feature: My Feature

  @Rule_01
  Rule: First Rule

    @Scenario_01
    Scenario: Scenario in rule
      Given something
"#;
        let result = engine.resolve(vec![("test.feature", source)].into_iter());

        assert!(result.errors.is_empty(), "Expected no errors but got: {:?}", result.errors);
        let plan = result.plan.unwrap();
        assert_eq!(plan.scenarios.len(), 1);
        assert_eq!(plan.scenarios[0].scenario_key, "my_feature:Rule_01:Scenario_01");
    }

    #[test]
    fn test_duplicate_scenario_id_in_rule() {
        let registry = SemanticStepRegistry::new(vec![
            make_binding("Given", "something", 0),
        ]);
        let engine = ResolutionEngine::new(&registry).unwrap();

        let source = r#"
@Feature(test)
Feature: Test

  @Rule_01
  Rule: Rule One

    @Scenario_01
    Scenario: First
      Given something

    @Scenario_01
    Scenario: Duplicate ID
      Given something
"#;
        let result = engine.resolve(vec![("test.feature", source)].into_iter());

        assert!(result.errors.iter().any(|e| matches!(e, ResolutionError::DuplicateScenarioId { .. })));
    }
```

**Action Items:**
1. Add all the test cases to the test module
2. Run `cargo test -p namako` to verify

---

## Phase 3: Update npap.rs

### Task 3.1: Keep Legacy Functions (Backward Compatibility)
**File:** `namako/src/npap.rs`

The old `derive_scenario_key` and `derive_scenario_outline_key` functions should be kept but marked deprecated. They're still useful for other tools that haven't migrated.

**Find these functions (around line 310-340) and add deprecation warnings:**

```rust
/// Derives a scenario key per GOLD_PLAN §6.4.3.
///
/// **DEPRECATED in v1.5:** Use `id_tags::derive_scenario_key_from_ids` instead.
/// This function is kept for backward compatibility with tools that haven't migrated.
///
/// Format: `normalized_relpath:L<line_number>`
#[deprecated(
    since = "1.5.0",
    note = "Use id_tags::derive_scenario_key_from_ids instead. Line-based keys are fragile under refactoring."
)]
#[must_use]
pub fn derive_scenario_key(relative_path: &str, line_number: u32) -> String {
    let normalized_path = normalize_path(relative_path);
    format!("{normalized_path}:L{line_number}")
}
```

Do the same for `derive_scenario_outline_key`.

**Action Items:**
1. Add `#[deprecated(...)]` attributes to both functions
2. Update the doc comments to indicate deprecation
3. Run `cargo build -p namako` (expect deprecation warnings, that's OK)

---

## Phase 4: CLI Updates

### Task 4.1: Update Lint Output Messages
**File:** `namako/cli/src/lint.rs`

No significant changes needed, as the error types already have good `Display` implementations. The lint command will automatically show the new error messages.

**Action Items:**
1. Verify lint command works with the new error types
2. Test manually with a feature file missing ID tags

---

### Task 4.2: Update Review Command
**File:** `namako/cli/src/review.rs`

The review command needs to be updated to extract ID tags for deferred scenario display.

**Find the `is_deferred_scenario` function (around line 496) and verify it still works correctly with the new tag system.**

The existing code checks for `@Deferred` tags, which is orthogonal to ID tags. No changes needed unless you want to also display the scenario's explicit ID in the output.

**Optional Enhancement:** Add the explicit scenario key to `DeferredItem` struct:

```rust
/// Deferred item from DEFERRED TESTS section or @Deferred tag
#[derive(Debug, Clone, Serialize)]
pub struct DeferredItem {
    pub text: String,
    pub source_span: SourceSpan,
    /// Blocker classification (from @Blocker tag or default UNKNOWN)
    pub blocker: BlockerType,
    /// Explicit scenario ID if available (@Scenario_nn)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explicit_id: Option<String>,
}
```

**Action Items:**
1. Review the review.rs file for any hardcoded assumptions about scenario keys
2. (Optional) Add `explicit_id` field to `DeferredItem`

---

## Phase 5: Feature File Migration

### Task 5.1: Create Migration Script
**File:** `naia/test/specs/scripts/add_id_tags.py` (NEW FILE)

Create a Python script to semi-automatically add ID tags to feature files:

```python
#!/usr/bin/env python3
"""
Add explicit ID tags to .feature files for Namako v1.5.

Usage:
    python add_id_tags.py features/

This script:
1. Adds @Feature(name) tag to each Feature (derived from filename)
2. Adds @Rule_nn tags to each Rule (sequential numbering)
3. Adds @Scenario_nn tags to each Scenario (sequential within rule/feature)

Always review the output before committing!
"""

import os
import re
import sys
from pathlib import Path


def snake_case(name: str) -> str:
    """Convert a name to snake_case."""
    # Replace non-alphanumeric with underscore
    result = re.sub(r'[^a-zA-Z0-9]', '_', name)
    # Collapse multiple underscores
    result = re.sub(r'_+', '_', result)
    # Remove leading/trailing underscores
    result = result.strip('_').lower()
    return result


def derive_feature_name(filepath: str) -> str:
    """Derive feature name from file path."""
    basename = os.path.basename(filepath)
    # Remove .feature extension
    name = basename.replace('.feature', '')
    # Remove numeric prefix like "01_"
    name = re.sub(r'^\d+_', '', name)
    return snake_case(name)


def process_feature_file(filepath: str) -> str:
    """Process a single feature file and add ID tags."""
    with open(filepath, 'r') as f:
        content = f.read()

    lines = content.split('\n')
    output_lines = []

    feature_name = derive_feature_name(filepath)
    rule_counter = 0
    scenario_counter = 0
    in_rule = False
    feature_tag_added = False

    i = 0
    while i < len(lines):
        line = lines[i]
        stripped = line.strip()

        # Check for Feature: line
        if stripped.startswith('Feature:') and not feature_tag_added:
            # Insert @Feature tag before Feature line
            indent = len(line) - len(line.lstrip())
            output_lines.append(' ' * indent + f'@Feature({feature_name})')
            output_lines.append(line)
            feature_tag_added = True
            scenario_counter = 0  # Reset for feature-level scenarios
            i += 1
            continue

        # Check for Rule: line
        if stripped.startswith('Rule:'):
            rule_counter += 1
            scenario_counter = 0  # Reset scenario counter for new rule
            indent = len(line) - len(line.lstrip())
            output_lines.append(' ' * indent + f'@Rule_{rule_counter:02d}')
            output_lines.append(line)
            in_rule = True
            i += 1
            continue

        # Check for Scenario: or Scenario Outline: line
        if stripped.startswith('Scenario:') or stripped.startswith('Scenario Outline:'):
            scenario_counter += 1
            indent = len(line) - len(line.lstrip())

            # Check if previous line is already a tag line
            if output_lines and output_lines[-1].strip().startswith('@'):
                # Add to existing tags
                output_lines[-1] = output_lines[-1] + f' @Scenario_{scenario_counter:02d}'
            else:
                output_lines.append(' ' * indent + f'@Scenario_{scenario_counter:02d}')

            output_lines.append(line)
            i += 1
            continue

        output_lines.append(line)
        i += 1

    return '\n'.join(output_lines)


def main():
    if len(sys.argv) < 2:
        print("Usage: python add_id_tags.py <features_dir>")
        sys.exit(1)

    features_dir = Path(sys.argv[1])

    for filepath in sorted(features_dir.glob('*.feature')):
        print(f"Processing: {filepath}")
        result = process_feature_file(str(filepath))

        # Write back
        with open(filepath, 'w') as f:
            f.write(result)

        print(f"  ✓ Updated {filepath}")


if __name__ == '__main__':
    main()
```

**Action Items:**
1. Create the script file
2. Make it executable: `chmod +x add_id_tags.py`
3. Review the script logic before running

---

### Task 5.2: Migrate Feature Files

**IMPORTANT:** This task requires careful manual review after automated migration.

**Files to migrate (16 total):**
1. `00_common.feature` → `@Feature(common)`
2. `01_connection_lifecycle.feature` → `@Feature(connection_lifecycle)`
3. `02_transport.feature` → `@Feature(transport)`
4. `03_messaging.feature` → `@Feature(messaging)`
5. `04_time_ticks_commands.feature` → `@Feature(time_ticks_commands)`
6. `05_observability_metrics.feature` → `@Feature(observability_metrics)`
7. `06_entity_scopes.feature` → `@Feature(entity_scopes)`
8. `07_entity_replication.feature` → `@Feature(entity_replication)`
9. `08_entity_ownership.feature` → `@Feature(entity_ownership)`
10. `09_entity_publication.feature` → `@Feature(entity_publication)`
11. `10_entity_delegation.feature` → `@Feature(entity_delegation)`
12. `11_entity_authority.feature` → `@Feature(entity_authority)`
13. `12_server_events_api.feature` → `@Feature(server_events_api)`
14. `13_client_events_api.feature` → `@Feature(client_events_api)`
15. `14_world_integration.feature` → `@Feature(world_integration)`
16. `smoke.feature` → `@Feature(smoke)`

**Example Migration (smoke.feature):**

**Before:**
```gherkin
Feature: Namako Smoke Test
  Verifies the core Namako v1 pipeline works end-to-end.

  Scenario: Server starts and accepts a connecting client
    Given a server is running
    ...
```

**After:**
```gherkin
@Feature(smoke)
Feature: Namako Smoke Test
  Verifies the core Namako v1 pipeline works end-to-end.

  @Scenario_01
  Scenario: Server starts and accepts a connecting client
    Given a server is running
    ...

  @Scenario_02
  Scenario: Server can disconnect a client
    Given a server is running
    ...
```

**Example with Rules (01_connection_lifecycle.feature):**

**Before:**
```gherkin
Feature: Connection Lifecycle

  Rule: Event ordering

    Scenario: Server observes ConnectEvent when client connects
      Given a server is running
      ...
```

**After:**
```gherkin
@Feature(connection_lifecycle)
Feature: Connection Lifecycle

  @Rule_01
  Rule: Event ordering

    @Scenario_01
    Scenario: Server observes ConnectEvent when client connects
      Given a server is running
      ...

    @Scenario_02
    Scenario: Client observes ConnectEvent when connected
      Given a server is running
      ...
```

**Action Items:**
1. Run: `cd naia/test/specs && python scripts/add_id_tags.py features/`
2. **MANUALLY REVIEW** each file to ensure:
   - Feature name is appropriate (snake_case, descriptive)
   - Rule numbering is sequential (01, 02, ...)
   - Scenario numbering restarts within each rule
   - No duplicate IDs
   - Existing tags like `@Deferred`, `@Blocker(...)` are preserved
3. Commit the changes: `git add features/ && git commit -m "Add explicit ID tags to all feature files (v1.5)"`

---

## Phase 6: Testing and Certification

### Task 6.1: Run Full Gate Check
**Commands:**

```bash
# From naia/test/specs directory
cd /home/ccarpenter/Personal/specops/naia/test/specs

# Build namako CLI first
cd ../../../namako
cargo build -p namako-cli --release

# Run lint to validate ID tags
cd ../naia/test/specs
cargo run --manifest-path ../../../namako/cli/Cargo.toml -- lint \
  -s . \
  -a "cargo run --manifest-path ../npa/Cargo.toml --" \
  -o target/namako_artifacts/resolved_plan.json

# Check for errors - should pass if all tags are correct
```

**If lint fails:**
- Read the error messages carefully
- Fix missing/duplicate ID tags in the feature files
- Re-run lint

**Action Items:**
1. Run lint and fix any errors
2. Document any issues encountered

---

### Task 6.2: Run Full CI Gate
**Commands:**

```bash
# From naia/test/specs directory
bash scripts/namako_ci.sh
```

This will run:
1. `namako lint` - resolves features
2. `npa run` - executes tests
3. `namako verify` - checks against baseline

**Expected behavior:** The verify step will FAIL because the scenario keys have changed format.

**Action Items:**
1. Run CI gate
2. Confirm lint and run pass
3. Expect verify to fail (this is correct - keys changed)

---

### Task 6.3: Update Certification Baseline
**Commands:**

```bash
# From naia/test/specs directory
cargo run --manifest-path ../../../namako/cli/Cargo.toml -- update-cert \
  -s . \
  --run-report target/namako_artifacts/run_report.json \
  -o certification.json
```

This updates the certification baseline to reflect the new scenario keys.

**Action Items:**
1. Run update-cert
2. Review the new certification.json
3. Verify the hash values have changed
4. Commit: `git add certification.json && git commit -m "Update certification baseline for v1.5 explicit ID tags"`

---

### Task 6.4: Run Determinism Check
**Commands:**

```bash
# From naia/test/specs directory
bash scripts/determinism_check.sh
```

This ensures the new key derivation is deterministic.

**Action Items:**
1. Run determinism check
2. Verify it passes
3. If it fails, investigate non-determinism in key generation

---

## Phase 7: Documentation Updates

### Task 7.1: Update CURRENT_STATUS.md
**File:** `namako/_WORKSPACE/CURRENT_STATUS.md`

Update the v1.5 section to mark this feature complete:

```markdown
## 3. V1.5 Features Status (AI-Enablement — IMMEDIATE PRIORITY)

| Feature | Section | Status |
|---------|---------|--------|
| Explicit ID tags (@Feature/@Rule_nn/@Scenario_nn) | §10.5.1 | ✅ COMPLETE |
| Orphan binding hard error + `namako stub` | §10.5.2 | 🔲 Not Started |
...
```

**Action Items:**
1. Update the status table
2. Add completion date

---

### Task 7.2: Update NEXT_STEPS.md
**File:** `namako/_WORKSPACE/NEXT_STEPS.md`

Mark Sprint 1 as complete and provide notes:

```markdown
#### Sprint 1: Foundation — Explicit ID Tags
**Duration:** 2-3 days
**Status:** ✅ COMPLETE

**Completed Work:**
- Created `id_tags.rs` module for tag parsing
- Updated `engine.rs` with ID validation
- Migrated all 16 feature files
- Updated certification baseline
- All gates green
```

**Action Items:**
1. Update Sprint 1 status
2. Add completion notes

---

## Verification Checklist

Before declaring this feature complete, verify:

- [ ] `cargo build -p namako` succeeds with no errors
- [ ] `cargo test -p namako` passes all tests
- [ ] `namako lint` succeeds for all feature files
- [ ] All feature files have `@Feature(name)` tag
- [ ] All rules have `@Rule_nn` tags
- [ ] All scenarios have `@Scenario_nn` tags
- [ ] Scenario keys in `resolved_plan.json` use new format (e.g., `smoke:Scenario_01`)
- [ ] `namako_ci.sh` passes
- [ ] `determinism_check.sh` passes
- [ ] `certification.json` has been updated
- [ ] CURRENT_STATUS.md updated
- [ ] NEXT_STEPS.md updated

---

## Rollback Plan

If issues are discovered after deployment:

1. Revert feature file changes: `git checkout HEAD~1 -- naia/test/specs/features/`
2. Revert engine changes in namako
3. Restore old certification.json
4. Document the issue in OUTPUT.md

---

## Dependencies

This task has no external dependencies. It can be completed entirely within the BOOTSTRAP allowed edit surface:
- `namako/**` (Namako engine code)
- `naia/test/specs/**` (feature files, scripts)

---

*End of TODO.md*
