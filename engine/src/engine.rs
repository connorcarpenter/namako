//! Resolution Engine for Namako v1
//!
//! This module implements the core resolution logic that matches Gherkin steps
//! to registered bindings, producing a `ResolvedPlan`.
//!
//! Per GOLD_PLAN §5.3, the engine:
//! - Parses all `.feature` files
//! - Fetches adapter manifest (semantic registry)
//! - Resolves each step to exactly one binding
//! - Validates signatures (captures arity, docstring/datatable expectations)
//! - Generates `resolved_plan.json`

use std::collections::{HashMap, HashSet};

use cucumber_expressions::Expression;
use gherkin::{Feature, GherkinEnv};
use regex::Regex;

use crate::npap::{
    PlannedStep, ResolvedPlan, ResolvedScenario, SemanticBinding,
    SemanticStepRegistry, compute_feature_fingerprint, derive_scenario_key,
};

#[cfg(feature = "npap")]
use crate::id_tags::{
    derive_scenario_key_from_ids, extract_feature_id, extract_rule_id, extract_scenario_id,
};


/// Errors that can occur during resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionError {
    /// No binding found for step (0 matches)
    MissingStep {
        step_text: String,
        step_kind: String,
        feature_path: String,
        line: u32,
    },
    /// Multiple bindings match step (>1 matches)
    AmbiguousStep {
        step_text: String,
        step_kind: String,
        feature_path: String,
        line: u32,
        matching_bindings: Vec<String>,
    },
    /// Signature mismatch (captures arity, docstring, datatable)
    SignatureMismatch {
        step_text: String,
        binding_id: String,
        expected_captures: u32,
        actual_captures: u32,
        step_has_docstring: bool,
        binding_accepts_docstring: bool,
        step_has_datatable: bool,
        binding_accepts_datatable: bool,
    },
    /// Feature file parsing failed
    ParseError {
        path: String,
        message: String,
    },
    /// Cucumber expression is invalid
    InvalidExpression {
        binding_id: String,
        expression: String,
        message: String,
    },
    /// Feature is missing @Feature(name) tag
    MissingFeatureId {
        feature_path: String,
        feature_name: String,
    },
    /// Rule is missing @Rule(nn) tag
    MissingRuleId {
        feature_path: String,
        rule_name: String,
    },
    /// Scenario is missing @Scenario(nn) tag
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
}

impl std::fmt::Display for ResolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingStep {
                step_text,
                step_kind,
                feature_path,
                line,
            } => {
                write!(
                    f,
                    "Missing step: no binding found for {step_kind} \"{step_text}\" \
                     at {feature_path}:{line}"
                )
            }
            Self::AmbiguousStep {
                step_text,
                step_kind,
                feature_path,
                line,
                matching_bindings,
            } => {
                write!(
                    f,
                    "Ambiguous step: {step_kind} \"{step_text}\" at {feature_path}:{line} \
                     matches {} bindings: {}",
                    matching_bindings.len(),
                    matching_bindings.join(", ")
                )
            }
            Self::SignatureMismatch {
                step_text,
                binding_id,
                expected_captures,
                actual_captures,
                step_has_docstring,
                binding_accepts_docstring,
                step_has_datatable,
                binding_accepts_datatable,
            } => {
                let mut reasons = Vec::new();
                if expected_captures != actual_captures {
                    reasons.push(format!(
                        "captures: expected {expected_captures}, got {actual_captures}"
                    ));
                }
                if *step_has_docstring && !binding_accepts_docstring {
                    reasons.push("step has docstring but binding doesn't accept it".to_string());
                }
                if *step_has_datatable && !binding_accepts_datatable {
                    reasons.push("step has datatable but binding doesn't accept it".to_string());
                }
                write!(
                    f,
                    "Signature mismatch for step \"{step_text}\" → binding {binding_id}: {}",
                    reasons.join("; ")
                )
            }
            Self::ParseError { path, message } => {
                write!(f, "Failed to parse feature file {path}: {message}")
            }
            Self::InvalidExpression {
                binding_id,
                expression,
                message,
            } => {
                write!(
                    f,
                    "Invalid cucumber expression in binding {binding_id}: \
                     \"{expression}\" - {message}"
                )
            }
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
                    "Missing @Rule(nn) tag: rule \"{rule_name}\" in {feature_path} \
                     must have a @Rule(nn) tag (e.g., @Rule(01))"
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
                    "Missing @Scenario(nn) tag: scenario \"{scenario_name}\"{context} in {feature_path} \
                     must have a @Scenario(nn) tag (e.g., @Scenario(01))"
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
                    "Duplicate @Rule({rule_id:02}) tag in {feature_path}"
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
                    "Duplicate @Scenario({scenario_id:02}) tag{context} in {feature_path}"
                )
            }
        }
    }
}

impl std::error::Error for ResolutionError {}

/// Result of resolving all features against a registry.
#[derive(Debug)]
pub struct ResolutionResult {
    /// The resolved plan (if no errors)
    pub plan: Option<ResolvedPlan>,
    /// Errors encountered during resolution
    pub errors: Vec<ResolutionError>,
    /// Warnings (non-fatal)
    pub warnings: Vec<String>,
    /// Orphan bindings (binding_id, kind, expression) — bindings in registry not used by any scenario
    pub orphan_bindings: Vec<OrphanBinding>,
    /// Binding IDs used by @Deferred scenarios (for orphan detection but not in executable plan)
    pub deferred_binding_ids: Vec<String>,
}

/// An orphan binding — a binding in the registry that is not used by any scenario.
/// Per GOLD_PLAN §10.5.2, orphans are a hard error in v1.5.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrphanBinding {
    /// The binding ID
    pub binding_id: String,
    /// The step kind (Given, When, Then)
    pub kind: String,
    /// The cucumber expression
    pub expression: String,
}

/// Compiled binding with parsed regex for matching.
struct CompiledBinding {
    /// The original binding metadata
    binding: SemanticBinding,
    /// Compiled regex for step matching (from cucumber expression)
    regex: Regex,
    /// Number of capture groups in the expression
    capture_count: usize,
}

/// Main resolution engine.
#[derive(Debug)]
pub struct ResolutionEngine {
    /// Compiled bindings indexed by kind
    bindings_by_kind: HashMap<String, Vec<CompiledBinding>>,
    /// Step registry hash for inclusion in plan
    step_registry_hash: String,
    /// All binding IDs in the registry (for orphan detection)
    all_binding_ids: Vec<(String, String, String)>, // (binding_id, kind, expression)
}

impl std::fmt::Debug for CompiledBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledBinding")
            .field("binding", &self.binding)
            .field("regex", &self.regex.as_str())
            .field("capture_count", &self.capture_count)
            .finish()
    }
}

impl ResolutionEngine {
    /// Creates a new resolution engine from a semantic step registry.
    ///
    /// # Errors
    ///
    /// Returns errors for any invalid cucumber expressions in the registry.
    pub fn new(registry: &SemanticStepRegistry) -> Result<Self, Vec<ResolutionError>> {
        let mut errors = Vec::new();
        let mut bindings_by_kind: HashMap<String, Vec<CompiledBinding>> = HashMap::new();

        // Collect all binding IDs for orphan detection
        let all_binding_ids: Vec<(String, String, String)> = registry
            .bindings
            .iter()
            .map(|b| (b.binding_id.clone(), b.kind.clone(), b.expression.clone()))
            .collect();

        for binding in &registry.bindings {
            // Convert cucumber expression to regex pattern
            match cucumber_expression_to_regex(&binding.expression) {
                Ok((regex, capture_count)) => {
                    let compiled = CompiledBinding {
                        binding: binding.clone(),
                        regex,
                        capture_count,
                    };
                    bindings_by_kind
                        .entry(binding.kind.clone())
                        .or_default()
                        .push(compiled);
                }
                Err(e) => {
                    errors.push(ResolutionError::InvalidExpression {
                        binding_id: binding.binding_id.clone(),
                        expression: binding.expression.clone(),
                        message: e,
                    });
                }
            }
        }

        if !errors.is_empty() {
            return Err(errors);
        }

        Ok(Self {
            bindings_by_kind,
            step_registry_hash: registry.step_registry_hash.clone(),
            all_binding_ids,
        })
    }

    /// Resolves all features in a directory against the registry.
    ///
    /// # Arguments
    ///
    /// * `features` - Iterator of (relative_path, gherkin_source) pairs
    pub fn resolve<'a, I>(&self, features: I) -> ResolutionResult
    where
        I: Iterator<Item = (&'a str, &'a str)>,
    {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut resolved_scenarios = Vec::new();
        let mut deferred_binding_ids: Vec<String> = Vec::new();
        let mut feature_files: Vec<(&str, &str)> = Vec::new();

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
                        &mut deferred_binding_ids,
                        &mut errors,
                        &mut warnings,
                    );
                }
                Err(e) => {
                    errors.push(ResolutionError::ParseError {
                        path: path.to_string(),
                        message: format!("{e:?}"),
                    });
                }
            }
        }

        if !errors.is_empty() {
            return ResolutionResult {
                plan: None,
                errors,
                warnings,
                orphan_bindings: Vec::new(),
                deferred_binding_ids: Vec::new(),
            };
        }

        // Compute feature fingerprint
        let feature_fingerprint_hash =
            compute_feature_fingerprint(feature_files.into_iter());

        // Build the resolved plan
        let plan = ResolvedPlan::new(
            feature_fingerprint_hash,
            self.step_registry_hash.clone(),
            resolved_scenarios,
        );

        // Compute orphan bindings: bindings in registry not used by any scenario
        // (including @Deferred scenarios for orphan detection purposes)
        let orphan_bindings = self.compute_orphan_bindings(&plan, &deferred_binding_ids);

        ResolutionResult {
            plan: Some(plan),
            errors,
            warnings,
            orphan_bindings,
            deferred_binding_ids,
        }
    }

    /// Computes orphan bindings — bindings in the registry not used by any scenario.
    ///
    /// Per GOLD_PLAN §10.5.2, orphans are detected by comparing registry binding IDs
    /// against binding IDs used in the resolved plan AND in @Deferred scenarios.
    fn compute_orphan_bindings(&self, plan: &ResolvedPlan, deferred_binding_ids: &[String]) -> Vec<OrphanBinding> {
        // Collect all used binding IDs from the resolved plan
        let mut used_binding_ids: HashSet<&str> = plan
            .scenarios
            .iter()
            .flat_map(|s| s.steps.iter())
            .map(|step| step.binding_id.as_str())
            .collect();

        // Also include binding IDs from @Deferred scenarios
        for bid in deferred_binding_ids {
            used_binding_ids.insert(bid.as_str());
        }

        // Find bindings in registry not used by any scenario
        self.all_binding_ids
            .iter()
            .filter(|(binding_id, _, _)| !used_binding_ids.contains(binding_id.as_str()))
            .map(|(binding_id, kind, expression)| OrphanBinding {
                binding_id: binding_id.clone(),
                kind: kind.clone(),
                expression: expression.clone(),
            })
            .collect()
    }

    /// Resolves a single feature file.
    fn resolve_feature(
        &self,
        path: &str,
        feature: &Feature,
        resolved_scenarios: &mut Vec<ResolvedScenario>,
        deferred_binding_ids: &mut Vec<String>,
        errors: &mut Vec<ResolutionError>,
        _warnings: &mut Vec<String>,
    ) {
        // PHASE 2.2a: Extract and validate feature-level ID tags
        #[cfg(feature = "npap")]
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

        // PHASE 2.2b: Track scenario keys to detect duplicates within this feature
        let mut seen_scenario_keys: HashSet<String> = HashSet::new();
        let mut seen_rule_ids: HashSet<u32> = HashSet::new();

        // Resolve feature-level background steps once (they get prepended to each scenario)
        let feature_background_steps = self.resolve_background_steps(
            feature.background.as_ref(),
            path,
            errors,
        );

        // PHASE 2.2c: Process feature-level scenarios (no rule context)
        for scenario in &feature.scenarios {
            // For @Deferred scenarios: resolve steps to get binding IDs for orphan detection
            // but don't add to the executable plan
            if is_deferred_scenario(scenario) {
                self.collect_deferred_binding_ids(scenario, path, deferred_binding_ids);
                continue;
            }

            // Extract and validate scenario-level ID tags
            #[cfg(feature = "npap")]
            let scenario_id = match extract_scenario_id(&scenario.tags) {
                Some(id) => id,
                None => {
                    errors.push(ResolutionError::MissingScenarioId {
                        feature_path: path.to_string(),
                        scenario_name: scenario.name.clone(),
                        rule_name: None,
                    });
                    continue; // Skip this scenario
                }
            };

            // Derive scenario key from explicit IDs (v1.5 format)
            #[cfg(feature = "npap")]
            let scenario_key = derive_scenario_key_from_ids(&feature_id, None, &scenario_id);

            // Fallback to line-based key if feature flag not enabled (v1 compat)
            #[cfg(not(feature = "npap"))]
            let scenario_key = {
                let line_u32 = u32::try_from(scenario.position.line).unwrap_or(0);
                derive_scenario_key(path, line_u32)
            };

            // Check for duplicate scenario keys within this feature
            if !seen_scenario_keys.insert(scenario_key.clone()) {
                errors.push(ResolutionError::DuplicateScenarioKey {
                    scenario_key: scenario_key.clone(),
                    feature_path: path.to_string(),
                    scenario_name: scenario.name.clone(),
                });
                continue; // Skip this scenario
            }

            // Start with background steps, then scenario steps
            let mut resolved_steps = feature_background_steps.clone();
            let mut last_keyword = if resolved_steps.is_empty() { "Given" } else { "Given" };

            for step in &scenario.steps {
                // Track effective kind for And/But
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

        // PHASE 2.2d: Process rules and their scenarios
        for rule in &feature.rules {
            // Extract and validate rule-level ID tags
            #[cfg(feature = "npap")]
            let rule_id = match extract_rule_id(&rule.tags) {
                Some(id) => id,
                None => {
                    errors.push(ResolutionError::MissingRuleId {
                        feature_path: path.to_string(),
                        rule_name: rule.name.clone(),
                    });
                    continue; // Skip this rule
                }
            };

            // Check for duplicate rule IDs within this feature
            #[cfg(feature = "npap")]
            if !seen_rule_ids.insert(rule_id.0) {
                errors.push(ResolutionError::DuplicateRuleId {
                    feature_path: path.to_string(),
                    rule_id: rule_id.0,
                });
                continue; // Skip this rule
            }

            // Rules can have their own background (in addition to feature background)
            let rule_background_steps = self.resolve_background_steps(
                rule.background.as_ref(),
                path,
                errors,
            );

            for scenario in &rule.scenarios {
                // For @Deferred scenarios: resolve steps to get binding IDs for orphan detection
                // but don't add to the executable plan
                if is_deferred_scenario(scenario) {
                    self.collect_deferred_binding_ids(scenario, path, deferred_binding_ids);
                    continue;
                }

                // Extract and validate scenario-level ID tags
                #[cfg(feature = "npap")]
                let scenario_id = match extract_scenario_id(&scenario.tags) {
                    Some(id) => id,
                    None => {
                        errors.push(ResolutionError::MissingScenarioId {
                            feature_path: path.to_string(),
                            scenario_name: scenario.name.clone(),
                            rule_name: Some(rule.name.clone()),
                        });
                        continue; // Skip this scenario
                    }
                };

                // Derive scenario key from explicit IDs (v1.5 format)
                #[cfg(feature = "npap")]
                let scenario_key = derive_scenario_key_from_ids(&feature_id, Some(&rule_id), &scenario_id);

                // Fallback to line-based key if feature flag not enabled (v1 compat)
                #[cfg(not(feature = "npap"))]
                let scenario_key = {
                    let line_u32 = u32::try_from(scenario.position.line).unwrap_or(0);
                    derive_scenario_key(path, line_u32)
                };

                // Check for duplicate scenario keys within this rule
                if !seen_scenario_keys.insert(scenario_key.clone()) {
                    errors.push(ResolutionError::DuplicateScenarioKey {
                        scenario_key: scenario_key.clone(),
                        feature_path: path.to_string(),
                        scenario_name: scenario.name.clone(),
                    });
                    continue; // Skip this scenario
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

    /// Collects binding IDs from @Deferred scenario steps for orphan detection.
    /// These bindings are considered "used" for orphan detection purposes,
    /// but the scenarios themselves are not included in the executable plan.
    fn collect_deferred_binding_ids(
        &self,
        scenario: &gherkin::Scenario,
        path: &str,
        deferred_binding_ids: &mut Vec<String>,
    ) {
        let mut last_keyword = "Given";
        for step in &scenario.steps {
            let effective_kind = resolve_effective_kind(&step.keyword, &mut last_keyword);
            // Try to resolve the step - if it matches, collect the binding ID
            // We ignore errors here since we're just collecting for orphan detection
            if let Ok(planned) = self.resolve_step(step, path, &effective_kind) {
                deferred_binding_ids.push(planned.binding_id);
            }
        }
    }

    /// Resolves background steps if present.
    fn resolve_background_steps(
        &self,
        background: Option<&gherkin::Background>,
        path: &str,
        errors: &mut Vec<ResolutionError>,
    ) -> Vec<PlannedStep> {
        let mut resolved_steps = Vec::new();

        if let Some(bg) = background {
            let mut last_keyword = "Given";
            for step in &bg.steps {
                let effective_kind = resolve_effective_kind(&step.keyword, &mut last_keyword);
                match self.resolve_step(step, path, &effective_kind) {
                    Ok(planned) => resolved_steps.push(planned),
                    Err(e) => errors.push(e),
                }
            }
        }

        resolved_steps
    }

    /// Resolves a single step to a binding.
    fn resolve_step(
        &self,
        step: &gherkin::Step,
        feature_path: &str,
        effective_kind: &str,
    ) -> Result<PlannedStep, ResolutionError> {
        let step_text = &step.value;
        let line = u32::try_from(step.position.line).unwrap_or(0);

        // Find matching bindings
        let matching = self.find_matching_bindings(effective_kind, step_text);

        match matching.len() {
            0 => Err(ResolutionError::MissingStep {
                step_text: step_text.clone(),
                step_kind: effective_kind.to_string(),
                feature_path: feature_path.to_string(),
                line,
            }),
            1 => {
                let (binding, captures) = &matching[0];

                // Validate signature
                self.validate_signature(step, binding, captures)?;

                // Extract docstring content (gherkin 0.15 uses Option<String>)
                let docstring = step.docstring.clone();

                // Extract datatable
                let datatable = step.table.as_ref().map(|t| {
                    t.rows
                        .iter()
                        .map(|row| row.iter().cloned().collect())
                        .collect()
                });

                // Build planned step with all 6 required arguments
                Ok(PlannedStep::new(
                    effective_kind.to_string(),
                    step_text.clone(),
                    binding.binding_id.clone(),
                    captures.clone(),
                    docstring,
                    datatable,
                ))
            }
            _ => Err(ResolutionError::AmbiguousStep {
                step_text: step_text.clone(),
                step_kind: effective_kind.to_string(),
                feature_path: feature_path.to_string(),
                line,
                matching_bindings: matching
                    .iter()
                    .map(|(b, _)| b.binding_id.clone())
                    .collect(),
            }),
        }
    }

    /// Finds all bindings that match the given step.
    fn find_matching_bindings(
        &self,
        kind: &str,
        step_text: &str,
    ) -> Vec<(&SemanticBinding, Vec<String>)> {
        let mut matches = Vec::new();

        if let Some(bindings) = self.bindings_by_kind.get(kind) {
            for compiled in bindings {
                if let Some(caps) = compiled.regex.captures(step_text) {
                    // Extract captured values (skip group 0 which is the full match)
                    let captures: Vec<String> = (1..=compiled.capture_count)
                        .filter_map(|i| caps.get(i).map(|m| m.as_str().to_string()))
                        .collect();
                    matches.push((&compiled.binding, captures));
                }
            }
        }

        matches
    }

    /// Validates that step requirements match binding signature.
    fn validate_signature(
        &self,
        step: &gherkin::Step,
        binding: &SemanticBinding,
        captures: &[String],
    ) -> Result<(), ResolutionError> {
        let actual_captures = u32::try_from(captures.len()).unwrap_or(0);
        let expected_captures = binding.signature.captures_arity;

        let step_has_docstring = step.docstring.is_some();
        let step_has_datatable = step.table.is_some();

        let binding_accepts_docstring = binding.signature.accepts_docstring;
        let binding_accepts_datatable = binding.signature.accepts_datatable;

        let captures_match = actual_captures == expected_captures;
        let docstring_ok = !step_has_docstring || binding_accepts_docstring;
        let datatable_ok = !step_has_datatable || binding_accepts_datatable;

        if captures_match && docstring_ok && datatable_ok {
            Ok(())
        } else {
            Err(ResolutionError::SignatureMismatch {
                step_text: step.value.clone(),
                binding_id: binding.binding_id.clone(),
                expected_captures,
                actual_captures,
                step_has_docstring,
                binding_accepts_docstring,
                step_has_datatable,
                binding_accepts_datatable,
            })
        }
    }
}

/// Resolves And/But/Star keywords to their effective kind.
fn resolve_effective_kind(keyword: &str, last_keyword: &mut &str) -> String {
    let trimmed = keyword.trim();
    match trimmed {
        "Given" | "given" => {
            *last_keyword = "Given";
            "Given".to_string()
        }
        "When" | "when" => {
            *last_keyword = "When";
            "When".to_string()
        }
        "Then" | "then" => {
            *last_keyword = "Then";
            "Then".to_string()
        }
        "And" | "and" | "But" | "but" | "*" => {
            // Inherit from previous step
            (*last_keyword).to_string()
        }
        _ => trimmed.to_string(),
    }
}

/// Checks if a scenario has the @Deferred tag and should be excluded from execution.
///
/// Per GOLD_PLAN, scenarios tagged with @Deferred are excluded from the executable
/// plan but included in review as promotion candidates.
fn is_deferred_scenario(scenario: &gherkin::Scenario) -> bool {
    scenario.tags.iter().any(|tag| {
        let tag_lower = tag.to_lowercase();
        tag_lower == "deferred" || tag_lower == "@deferred"
    })
}

/// Converts a cucumber expression to a regex pattern.
///
/// Returns (compiled regex, number of capture groups).
fn cucumber_expression_to_regex(expr: &str) -> Result<(Regex, usize), String> {
    // Use cucumber_expressions crate's regex() static method
    let regex = Expression::regex(expr)
        .map_err(|e| format!("Failed to parse expression: {e}"))?;

    // Count capture groups (subtract 1 for the full match group 0)
    let capture_count = regex.captures_len().saturating_sub(1);

    Ok((regex, capture_count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npap::{BindingSignature, SemanticBinding};

    fn make_binding(kind: &str, expression: &str, captures_arity: u32) -> SemanticBinding {
        SemanticBinding {
            binding_id: crate::npap::generate_binding_id(kind, expression),
            kind: kind.to_string(),
            expression: expression.to_string(),
            signature: BindingSignature {
                captures_arity,
                accepts_docstring: false,
                accepts_datatable: false,
            },
            impl_hash: "test_hash".to_string(),
            source_symbol: None,
        }
    }

    #[test]
    fn test_resolution_engine_creation() {
        let registry = SemanticStepRegistry::new(vec![
            make_binding("Given", "a server is running", 0),
            make_binding("When", "a client connects", 0),
        ]);

        let engine = ResolutionEngine::new(&registry);
        assert!(engine.is_ok());
    }

    #[test]
    fn test_resolution_engine_invalid_expression() {
        let mut binding = make_binding("Given", "valid expression", 0);
        binding.expression = "invalid {".to_string(); // Unclosed placeholder

        let registry = SemanticStepRegistry::new(vec![binding]);
        let engine = ResolutionEngine::new(&registry);

        assert!(engine.is_err());
        let errors = engine.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], ResolutionError::InvalidExpression { .. }));
    }

    #[test]
    fn test_simple_resolution() {
        let registry = SemanticStepRegistry::new(vec![
            make_binding("Given", "a server is running", 0),
            make_binding("When", "a client connects", 0),
            make_binding("Then", "the client is connected", 0),
        ]);

        let engine = ResolutionEngine::new(&registry).unwrap();

        let feature_source = r#"
@Feature(simple_test)
Feature: Simple test

  @Scenario(01)
  Scenario: Client connects
    Given a server is running
    When a client connects
    Then the client is connected
"#;

        let features = vec![("test.feature", feature_source)];
        let result = engine.resolve(features.into_iter());

        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
        assert!(result.plan.is_some());

        let plan = result.plan.unwrap();
        assert_eq!(plan.scenarios.len(), 1);
        assert_eq!(plan.scenarios[0].steps.len(), 3);

        // Verify effective_kind is set correctly
        assert_eq!(plan.scenarios[0].steps[0].effective_kind, "Given");
        assert_eq!(plan.scenarios[0].steps[1].effective_kind, "When");
        assert_eq!(plan.scenarios[0].steps[2].effective_kind, "Then");
    }

    #[test]
    fn test_and_but_resolution() {
        let registry = SemanticStepRegistry::new(vec![
            make_binding("Given", "a server is running", 0),
            make_binding("Given", "the server is configured", 0),
            make_binding("When", "a client connects", 0),
            make_binding("Then", "the client is connected", 0),
            make_binding("Then", "the server accepts it", 0),
        ]);

        let engine = ResolutionEngine::new(&registry).unwrap();

        let feature_source = r#"
@Feature(and_but_test)
Feature: And/But test

  @Scenario(01)
  Scenario: With And/But
    Given a server is running
    And the server is configured
    When a client connects
    Then the client is connected
    But the server accepts it
"#;

        let features = vec![("test.feature", feature_source)];
        let result = engine.resolve(features.into_iter());

        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
        let plan = result.plan.unwrap();

        // And after Given should be Given
        assert_eq!(plan.scenarios[0].steps[1].effective_kind, "Given");
        // But after Then should be Then
        assert_eq!(plan.scenarios[0].steps[4].effective_kind, "Then");
    }

    #[test]
    fn test_missing_step_error() {
        let registry = SemanticStepRegistry::new(vec![
            make_binding("Given", "a server is running", 0),
        ]);

        let engine = ResolutionEngine::new(&registry).unwrap();

        let feature_source = r#"
@Feature(missing_step)
Feature: Missing step

  @Scenario(01)
  Scenario: No binding
    Given a server is running
    When no binding exists for this
"#;

        let features = vec![("test.feature", feature_source)];
        let result = engine.resolve(features.into_iter());

        assert_eq!(result.errors.len(), 1);
        assert!(matches!(result.errors[0], ResolutionError::MissingStep { .. }));
    }

    #[test]
    fn test_captures_extraction() {
        let registry = SemanticStepRegistry::new(vec![
            make_binding("Given", "a user named {string}", 1),
        ]);

        let engine = ResolutionEngine::new(&registry).unwrap();

        let feature_source = r#"
@Feature(captures)
Feature: Captures

  @Scenario(01)
  Scenario: User
    Given a user named "Alice"
"#;

        let features = vec![("test.feature", feature_source)];
        let result = engine.resolve(features.into_iter());

        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
        let plan = result.plan.unwrap();
        assert_eq!(plan.scenarios[0].steps[0].captures, vec!["Alice"]);
    }

    #[test]
    fn test_signature_mismatch_captures() {
        let registry = SemanticStepRegistry::new(vec![
            make_binding("Given", "a user named {string}", 2), // expects 2 captures
        ]);

        let engine = ResolutionEngine::new(&registry).unwrap();

        let feature_source = r#"
@Feature(signature_mismatch)
Feature: Signature mismatch

  @Scenario(01)
  Scenario: Wrong arity
    Given a user named "Alice"
"#;

        let features = vec![("test.feature", feature_source)];
        let result = engine.resolve(features.into_iter());

        assert_eq!(result.errors.len(), 1);
        assert!(matches!(
            result.errors[0],
            ResolutionError::SignatureMismatch { .. }
        ));
    }

    #[test]
    fn test_cucumber_expression_to_regex() {
        let (regex, count) = cucumber_expression_to_regex("a user named {string}").unwrap();
        assert!(regex.is_match("a user named \"Alice\""));
        assert!(count >= 1, "expected at least 1 capture for {{string}}, got {count}");

        let (regex, count) = cucumber_expression_to_regex("I have {int} apples").unwrap();
        assert!(regex.is_match("I have 5 apples"));
        // {int} may have multiple internal groups, just verify it works
        assert!(count >= 1, "expected at least 1 capture for {{int}}, got {count}");
    }

    #[test]
    fn test_orphan_binding_detection() {
        // Registry has 4 bindings, but only 3 are used
        let registry = SemanticStepRegistry::new(vec![
            make_binding("Given", "a server is running", 0),
            make_binding("When", "a client connects", 0),
            make_binding("Then", "the client is connected", 0),
            make_binding("Then", "an orphan binding that is never used", 0), // orphan
        ]);

        let engine = ResolutionEngine::new(&registry).unwrap();

        let feature_source = r#"
@Feature(orphan_test)
Feature: Simple test

  @Scenario(01)
  Scenario: Client connects
    Given a server is running
    When a client connects
    Then the client is connected
"#;

        let features = vec![("test.feature", feature_source)];
        let result = engine.resolve(features.into_iter());

        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
        assert!(result.plan.is_some());

        // Should detect 1 orphan binding
        assert_eq!(result.orphan_bindings.len(), 1);
        assert_eq!(
            result.orphan_bindings[0].expression,
            "an orphan binding that is never used"
        );
        assert_eq!(result.orphan_bindings[0].kind, "Then");
    }

    #[test]
    fn test_no_orphans_when_all_used() {
        // Registry has exactly the bindings used
        let registry = SemanticStepRegistry::new(vec![
            make_binding("Given", "a server is running", 0),
            make_binding("When", "a client connects", 0),
        ]);

        let engine = ResolutionEngine::new(&registry).unwrap();

        let feature_source = r#"
@Feature(all_used)
Feature: All used

  @Scenario(01)
  Scenario: Both bindings used
    Given a server is running
    When a client connects
"#;

        let features = vec![("test.feature", feature_source)];
        let result = engine.resolve(features.into_iter());

        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
        assert!(result.orphan_bindings.is_empty());
    }
}
