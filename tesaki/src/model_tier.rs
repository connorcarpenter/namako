//! Model tiering and selection for Tesaki v1.9.
//!
//! This module provides intelligent model selection based on:
//! 1. Mission type recommendations (opus/sonnet/haiku)
//! 2. Escalation on retry (haiku→sonnet→opus)
//! 3. Configuration overrides (per-mission-type or force_model)

use crate::config::ModelOverrides;
use crate::mission_type::MissionType;

/// Model tiers in order of capability (least to most).
const MODEL_TIERS: [&str; 3] = ["haiku", "sonnet", "opus"];

/// Select the appropriate model for a mission attempt.
///
/// # Arguments
/// * `mission_type` - The type of mission to execute
/// * `attempt` - Current attempt number (1-indexed)
/// * `prev_failure` - Whether the previous attempt failed
/// * `overrides` - Optional configuration overrides
///
/// # Returns
/// The model tier to use ("haiku", "sonnet", or "opus")
pub fn select_model_for_attempt(
    mission_type: &MissionType,
    attempt: u32,
    prev_failure: bool,
    overrides: Option<&ModelOverrides>,
) -> &'static str {
    // Check for force_model override first
    if let Some(overrides) = overrides {
        if let Some(ref force) = overrides.force_model {
            return leak_string(force);
        }
    }

    // Get base recommendation from mission type
    let base = mission_type.recommended_model();

    // Check for per-mission-type override
    if let Some(overrides) = overrides {
        if let Some(override_model) = overrides.get_override(mission_type.name()) {
            // Use override as base, but still allow escalation
            return escalate_if_needed(leak_string(override_model), attempt, prev_failure);
        }
    }

    // Apply escalation logic on retry
    escalate_if_needed(base, attempt, prev_failure)
}

/// Escalate to a higher model tier if retrying after failure.
fn escalate_if_needed(base: &'static str, attempt: u32, prev_failure: bool) -> &'static str {
    if attempt == 1 || !prev_failure {
        return base;
    }

    // Find current tier index
    let current_idx = MODEL_TIERS.iter().position(|&m| m == base).unwrap_or(0);

    // Escalate by attempt-1 levels (capped at opus)
    let escalation = (attempt - 1) as usize;
    let new_idx = (current_idx + escalation).min(MODEL_TIERS.len() - 1);

    MODEL_TIERS[new_idx]
}

/// Convert a String to a &'static str by leaking.
/// This is safe because model names are config-driven and few in number.
fn leak_string(s: &str) -> &'static str {
    // Normalize to canonical tier names
    let lower = s.to_lowercase();
    match lower.as_str() {
        "opus" | "claude-opus-4.5" | "claude-opus" => "opus",
        "sonnet" | "claude-sonnet-4" | "claude-sonnet" => "sonnet",
        "haiku" | "claude-haiku" | "claude-haiku-4.5" => "haiku",
        _ => {
            // For non-standard models, leak the string
            Box::leak(s.to_string().into_boxed_str())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission_type::MissionType;

    #[test]
    fn first_attempt_uses_recommended_model() {
        let mission = MissionType::CreateMissingBindings {
            scenario_key: "s".into(),
            missing_steps: vec![],
        };
        assert_eq!(select_model_for_attempt(&mission, 1, false, None), "sonnet");

        let mission = MissionType::AddOrClarifyScenario {
            feature_path: "f".into(),
            rule_name: None,
        };
        assert_eq!(select_model_for_attempt(&mission, 1, false, None), "opus");

        let mission = MissionType::NormalizeIdentityTags {
            feature_path: "f".into(),
            missing_tags: vec![],
        };
        assert_eq!(select_model_for_attempt(&mission, 1, false, None), "haiku");
    }

    #[test]
    fn escalates_on_retry_with_failure() {
        let mission = MissionType::CreateMissingBindings {
            scenario_key: "s".into(),
            missing_steps: vec![],
        };
        // First attempt: sonnet
        assert_eq!(select_model_for_attempt(&mission, 1, false, None), "sonnet");
        // Second attempt after failure: opus
        assert_eq!(select_model_for_attempt(&mission, 2, true, None), "opus");
    }

    #[test]
    fn escalates_haiku_through_sonnet_to_opus() {
        let mission = MissionType::NormalizeIdentityTags {
            feature_path: "f".into(),
            missing_tags: vec![],
        };
        // First attempt: haiku
        assert_eq!(select_model_for_attempt(&mission, 1, false, None), "haiku");
        // Second attempt after failure: sonnet
        assert_eq!(select_model_for_attempt(&mission, 2, true, None), "sonnet");
        // Third attempt after failure: opus
        assert_eq!(select_model_for_attempt(&mission, 3, true, None), "opus");
        // Fourth attempt: still opus (capped)
        assert_eq!(select_model_for_attempt(&mission, 4, true, None), "opus");
    }

    #[test]
    fn no_escalation_without_failure() {
        let mission = MissionType::NormalizeIdentityTags {
            feature_path: "f".into(),
            missing_tags: vec![],
        };
        // Retry without failure flag doesn't escalate
        assert_eq!(select_model_for_attempt(&mission, 2, false, None), "haiku");
        assert_eq!(select_model_for_attempt(&mission, 3, false, None), "haiku");
    }

    #[test]
    fn force_model_overrides_everything() {
        let mission = MissionType::NormalizeIdentityTags {
            feature_path: "f".into(),
            missing_tags: vec![],
        };
        let overrides = ModelOverrides {
            force_model: Some("opus".to_string()),
            overrides: std::collections::HashMap::new(),
        };
        // Force model ignores recommended and escalation
        assert_eq!(
            select_model_for_attempt(&mission, 1, false, Some(&overrides)),
            "opus"
        );
        assert_eq!(
            select_model_for_attempt(&mission, 2, true, Some(&overrides)),
            "opus"
        );
    }

    #[test]
    fn per_mission_type_override() {
        let mission = MissionType::CreateMissingBindings {
            scenario_key: "s".into(),
            missing_steps: vec![],
        };
        let mut overrides = ModelOverrides::default();
        overrides
            .overrides
            .insert("CreateMissingBindings".to_string(), "opus".to_string());

        // Override to opus
        assert_eq!(
            select_model_for_attempt(&mission, 1, false, Some(&overrides)),
            "opus"
        );
    }

    #[test]
    fn opus_stays_at_opus() {
        let mission = MissionType::AddOrClarifyScenario {
            feature_path: "f".into(),
            rule_name: None,
        };
        // Already at opus
        assert_eq!(select_model_for_attempt(&mission, 1, false, None), "opus");
        // Retry doesn't go beyond opus
        assert_eq!(select_model_for_attempt(&mission, 2, true, None), "opus");
        assert_eq!(select_model_for_attempt(&mission, 3, true, None), "opus");
    }
}
