//! Prompt template loading and rendering module.
//!
//! This module provides a templating system for generating mission documents
//! using MiniJinja templates. Templates are embedded at compile time for
//! single-binary deployment.

use anyhow::{Context, Result};
use minijinja::{context, Environment, Value};
use serde::{Deserialize, Serialize};

use crate::mission::{MissionBudgets, SurfaceDefinitions};
use crate::mission_type::MissionType;
use crate::repo_state::RepoState;
use crate::surface_policy::SurfacePolicy;

/// The current Tesaki version for template footer.
pub const TESAKI_VERSION: &str = "1.9";

/// Create a MiniJinja environment with all templates loaded.
pub fn create_environment() -> Environment<'static> {
    let mut env = Environment::new();

    // Register custom filters
    env.add_filter("upper", upper_filter);

    // Mission templates
    env.add_template(
        "mission/MISSION.md.j2",
        include_str!("../prompts/mission/MISSION.md.j2"),
    )
    .expect("Failed to load MISSION.md.j2");

    env.add_template(
        "mission/POLICY.md.j2",
        include_str!("../prompts/mission/POLICY.md.j2"),
    )
    .expect("Failed to load POLICY.md.j2");

    // Mission brief templates
    env.add_template(
        "mission/briefs/create_missing_bindings.md.j2",
        include_str!("../prompts/mission/briefs/create_missing_bindings.md.j2"),
    )
    .expect("Failed to load create_missing_bindings.md.j2");

    env.add_template(
        "mission/briefs/implement_behavior.md.j2",
        include_str!("../prompts/mission/briefs/implement_behavior.md.j2"),
    )
    .expect("Failed to load implement_behavior.md.j2");

    env.add_template(
        "mission/briefs/fix_regression.md.j2",
        include_str!("../prompts/mission/briefs/fix_regression.md.j2"),
    )
    .expect("Failed to load fix_regression.md.j2");

    env.add_template(
        "mission/briefs/refine_feature_intent.md.j2",
        include_str!("../prompts/mission/briefs/refine_feature_intent.md.j2"),
    )
    .expect("Failed to load refine_feature_intent.md.j2");

    env.add_template(
        "mission/briefs/add_clarify_scenario.md.j2",
        include_str!("../prompts/mission/briefs/add_clarify_scenario.md.j2"),
    )
    .expect("Failed to load add_clarify_scenario.md.j2");

    env.add_template(
        "mission/briefs/normalize_identity_tags.md.j2",
        include_str!("../prompts/mission/briefs/normalize_identity_tags.md.j2"),
    )
    .expect("Failed to load normalize_identity_tags.md.j2");

    env.add_template(
        "mission/briefs/strengthen_then.md.j2",
        include_str!("../prompts/mission/briefs/strengthen_then.md.j2"),
    )
    .expect("Failed to load strengthen_then.md.j2");

    env.add_template(
        "mission/briefs/refactor_bindings.md.j2",
        include_str!("../prompts/mission/briefs/refactor_bindings.md.j2"),
    )
    .expect("Failed to load refactor_bindings.md.j2");

    env.add_template(
        "mission/briefs/summarize_and_close.md.j2",
        include_str!("../prompts/mission/briefs/summarize_and_close.md.j2"),
    )
    .expect("Failed to load summarize_and_close.md.j2");

    env.add_template(
        "mission/briefs/cleanup_after_success.md.j2",
        include_str!("../prompts/mission/briefs/cleanup_after_success.md.j2"),
    )
    .expect("Failed to load cleanup_after_success.md.j2");

    env.add_template(
        "mission/briefs/assess_spec_coverage.md.j2",
        include_str!("../prompts/mission/briefs/assess_spec_coverage.md.j2"),
    )
    .expect("Failed to load assess_spec_coverage.md.j2");

    env.add_template(
        "mission/briefs/explain_state.md.j2",
        include_str!("../prompts/mission/briefs/explain_state.md.j2"),
    )
    .expect("Failed to load explain_state.md.j2");

    env.add_template(
        "mission/briefs/triage_failures.md.j2",
        include_str!("../prompts/mission/briefs/triage_failures.md.j2"),
    )
    .expect("Failed to load triage_failures.md.j2");

    // Component templates
    env.add_template(
        "components/surfaces_table.md.j2",
        include_str!("../prompts/components/surfaces_table.md.j2"),
    )
    .expect("Failed to load surfaces_table.md.j2");

    env.add_template(
        "components/budgets_table.md.j2",
        include_str!("../prompts/components/budgets_table.md.j2"),
    )
    .expect("Failed to load budgets_table.md.j2");

    env.add_template(
        "components/budgets_full_table.md.j2",
        include_str!("../prompts/components/budgets_full_table.md.j2"),
    )
    .expect("Failed to load budgets_full_table.md.j2");

    env.add_template(
        "components/footer.md.j2",
        include_str!("../prompts/components/footer.md.j2"),
    )
    .expect("Failed to load footer.md.j2");

    // Next task templates
    env.add_template(
        "next_task/base.md.j2",
        include_str!("../prompts/next_task/base.md.j2"),
    )
    .expect("Failed to load base.md.j2");

    env.add_template(
        "next_task/done.md.j2",
        include_str!("../prompts/next_task/done.md.j2"),
    )
    .expect("Failed to load done.md.j2");

    env.add_template(
        "next_task/fix_lint.md.j2",
        include_str!("../prompts/next_task/fix_lint.md.j2"),
    )
    .expect("Failed to load fix_lint.md.j2");

    env.add_template(
        "next_task/fix_run.md.j2",
        include_str!("../prompts/next_task/fix_run.md.j2"),
    )
    .expect("Failed to load fix_run.md.j2");

    env.add_template(
        "next_task/needs_approval.md.j2",
        include_str!("../prompts/next_task/needs_approval.md.j2"),
    )
    .expect("Failed to load needs_approval.md.j2");

    env.add_template(
        "next_task/run_gate.md.j2",
        include_str!("../prompts/next_task/run_gate.md.j2"),
    )
    .expect("Failed to load run_gate.md.j2");

    env.add_template(
        "next_task/unknown.md.j2",
        include_str!("../prompts/next_task/unknown.md.j2"),
    )
    .expect("Failed to load unknown.md.j2");

    env.add_template(
        "next_task/artifacts.md.j2",
        include_str!("../prompts/next_task/artifacts.md.j2"),
    )
    .expect("Failed to load artifacts.md.j2");

    env
}

/// Custom filter to convert strings to uppercase.
fn upper_filter(value: Value) -> String {
    value.to_string().to_uppercase()
}

/// Context for rendering MISSION.md.
#[derive(Debug, Serialize)]
pub struct MissionContext {
    pub mission_id: String,
    pub mission_type: String,
    pub stage: String,
    pub target: Option<String>,
    pub objective: String,
    pub context: String,
    pub validation_criteria: Vec<String>,
    pub surface_policy: SurfacePolicyContext,
    pub surface_definitions: SurfaceDefinitionsContext,
    pub budgets: BudgetsContext,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_failure: Option<PreviousFailureContext>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PreviousFailureContext {
    pub mission_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub stop_reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// Context for rendering POLICY.md.
#[derive(Debug, Serialize)]
pub struct PolicyContext {
    pub surface_policy: SurfacePolicyContext,
    pub surface_definitions: SurfaceDefinitionsContext,
    pub budgets: BudgetsContext,
    pub version: String,
}

/// Serializable surface policy for templates.
#[derive(Debug, Serialize)]
pub struct SurfacePolicyContext {
    pub spec: String,
    pub tests_bindings: String,
    pub sut: String,
}

impl From<&SurfacePolicy> for SurfacePolicyContext {
    fn from(policy: &SurfacePolicy) -> Self {
        Self {
            spec: format_lock(policy.spec),
            tests_bindings: format_lock(policy.tests_bindings),
            sut: format_lock(policy.sut),
        }
    }
}

/// Serializable surface definitions for templates.
#[derive(Debug, Serialize)]
pub struct SurfaceDefinitionsContext {
    pub spec: SurfaceDefContext,
    pub tests_bindings: SurfaceDefContext,
    pub sut: SurfaceDefContext,
}

#[derive(Debug, Serialize)]
pub struct SurfaceDefContext {
    pub patterns: Vec<String>,
}

impl From<&SurfaceDefinitions> for SurfaceDefinitionsContext {
    fn from(defs: &SurfaceDefinitions) -> Self {
        Self {
            spec: SurfaceDefContext {
                patterns: defs.spec.patterns.clone(),
            },
            tests_bindings: SurfaceDefContext {
                patterns: defs.tests_bindings.patterns.clone(),
            },
            sut: SurfaceDefContext {
                patterns: defs.sut.patterns.clone(),
            },
        }
    }
}

/// Serializable budgets for templates.
#[derive(Debug, Serialize)]
pub struct BudgetsContext {
    pub max_files_changed: u32,
    pub max_scenarios_promoted: u32,
    pub max_runtime_seconds: u32,
    pub max_retries: u32,
}

impl From<&MissionBudgets> for BudgetsContext {
    fn from(budgets: &MissionBudgets) -> Self {
        Self {
            max_files_changed: budgets.max_files_changed,
            max_scenarios_promoted: budgets.max_scenarios_promoted,
            max_runtime_seconds: budgets.max_runtime_seconds,
            max_retries: budgets.max_retries,
        }
    }
}

fn format_lock(lock: crate::surface_policy::SurfaceLock) -> String {
    match lock {
        crate::surface_policy::SurfaceLock::Locked => "LOCKED".to_string(),
        crate::surface_policy::SurfaceLock::Unlocked => "UNLOCKED".to_string(),
    }
}

/// Render MISSION.md using templates.
pub fn render_mission_md(ctx: &MissionContext) -> Result<String> {
    let env = create_environment();
    let template = env
        .get_template("mission/MISSION.md.j2")
        .context("Failed to get MISSION.md.j2 template")?;

    template
        .render(context! {
            mission_id => ctx.mission_id,
            mission_type => ctx.mission_type,
            stage => ctx.stage,
            target => ctx.target,
            objective => ctx.objective,
            context => ctx.context,
            validation_criteria => ctx.validation_criteria,
            surface_policy => ctx.surface_policy,
            surface_definitions => ctx.surface_definitions,
            budgets => ctx.budgets,
            version => ctx.version,
            previous_failure => ctx.previous_failure,
        })
        .context("Failed to render MISSION.md")
}

/// Render POLICY.md using templates.
pub fn render_policy_md(ctx: &PolicyContext) -> Result<String> {
    let env = create_environment();
    let template = env
        .get_template("mission/POLICY.md.j2")
        .context("Failed to get POLICY.md.j2 template")?;

    template
        .render(context! {
            surface_policy => ctx.surface_policy,
            surface_definitions => ctx.surface_definitions,
            budgets => ctx.budgets,
            version => ctx.version,
        })
        .context("Failed to render POLICY.md")
}

/// Context for mission brief rendering.
///
/// This struct is prepared for future use when mission briefs are fully
/// template-driven. Currently, `MissionBrief` in mission_type.rs provides
/// the structured data.
#[allow(dead_code)]
#[derive(Debug, Serialize)]
pub struct BriefContext {
    // CreateMissingBindings
    pub scenario_key: Option<String>,
    pub missing_steps: Option<Vec<String>>,
    pub all_missing_steps: Option<Vec<String>>,  // All unique missing steps for batching
    pub binding_exemplars: Option<Vec<BindingExemplar>>,  // Examples from repo

    // ImplementBehaviorForScenario
    pub scenario_name: Option<String>,

    // FixRegressionFromGateFailure
    pub failure: Option<FailureContext>,

    // RefineFeatureIntent, AddOrClarifyScenario, NormalizeIdentityTags
    pub feature_path: Option<String>,
    pub rule_name: Option<String>,
    pub missing_tags: Option<Vec<String>>,
    pub current_scenario_count: Option<usize>,  // For AddOrClarifyScenario
    pub rules_without_scenarios: Option<Vec<String>>,  // For AddOrClarifyScenario

    // RefactorBindingsForClarity
    pub binding_ids: Option<Vec<String>>,

    // SummarizeAndClose, ExplainState
    pub repo_summary: Option<String>,

    // TriageFailures
    pub failure_count: Option<usize>,
}

/// A binding exemplar to include in mission context
#[derive(Debug, Serialize, Clone)]
pub struct BindingExemplar {
    pub step_text: String,
    pub binding_code: String,
    pub file_path: String,
}

#[allow(dead_code)]
#[derive(Debug, Serialize)]
pub struct FailureContext {
    pub scenario_key: String,
    pub scenario_name: String,
    pub failure_kind: String,
}

impl BriefContext {
    /// Create context from a MissionType and RepoState.
    pub fn from_mission_type(mission_type: &MissionType, state: &RepoState) -> Self {
        match mission_type {
            MissionType::CreateMissingBindings {
                scenario_key,
                missing_steps,
            } => {
                // Collect ALL unique missing step texts for comprehensive batching
                let all_missing: Vec<String> = state.binding_issues
                    .iter()
                    .filter(|b| matches!(b.kind, crate::repo_state::BindingIssueKind::MissingBinding))
                    .filter_map(|b| b.step_text.clone())
                    .collect();
                
                let mut unique_steps = all_missing.clone();
                unique_steps.sort();
                unique_steps.dedup();
                
                Self {
                    scenario_key: Some(scenario_key.clone()),
                    missing_steps: Some(missing_steps.clone()),
                    all_missing_steps: Some(unique_steps),
                    binding_exemplars: None, // TODO: Could extract from existing bindings
                    ..Default::default()
                }
            },
            MissionType::ImplementBehaviorForScenario {
                scenario_key,
                scenario_name,
                ..
            } => Self {
                scenario_key: Some(scenario_key.clone()),
                scenario_name: Some(scenario_name.clone()),
                ..Default::default()
            },
            MissionType::FixRegressionFromGateFailure { failure } => Self {
                failure: Some(FailureContext {
                    scenario_key: failure.scenario_key.clone(),
                    scenario_name: failure.scenario_name.clone(),
                    failure_kind: failure.failure_kind.clone(),
                }),
                ..Default::default()
            },
            MissionType::RefineFeatureIntent { feature_path } => Self {
                feature_path: Some(feature_path.clone()),
                ..Default::default()
            },
            MissionType::AddOrClarifyScenario {
                feature_path,
                rule_name,
            } => {
                // Count executable scenarios from feature files (excluding @Deferred) so promotions count as progress.
                let scenario_count = state
                    .scenario_count_for_feature(feature_path)
                    .unwrap_or(0);
                
                // Find rules that might need scenarios
                let rules_needing_scenarios: Vec<String> = state.spec_issues
                    .iter()
                    .filter(|i| i.feature_path == *feature_path)
                    .filter(|i| matches!(i.kind, crate::repo_state::SpecIssueKind::MissingCoverage))
                    .filter_map(|i| i.rule_name.clone())
                    .collect();
                
                Self {
                    feature_path: Some(feature_path.clone()),
                    rule_name: rule_name.clone(),
                    current_scenario_count: Some(scenario_count),
                    rules_without_scenarios: if rules_needing_scenarios.is_empty() { 
                        None 
                    } else { 
                        Some(rules_needing_scenarios) 
                    },
                    ..Default::default()
                }
            },
            MissionType::NormalizeIdentityTags {
                feature_path,
                missing_tags,
            } => Self {
                feature_path: Some(feature_path.clone()),
                missing_tags: Some(missing_tags.clone()),
                ..Default::default()
            },
            MissionType::StrengthenThenAssertions { scenario_key, .. } => Self {
                scenario_key: Some(scenario_key.clone()),
                ..Default::default()
            },
            MissionType::RefactorBindingsForClarity { binding_ids } => Self {
                binding_ids: Some(binding_ids.clone()),
                ..Default::default()
            },
            MissionType::SummarizeAndClose => Self {
                repo_summary: Some(state.summary()),
                ..Default::default()
            },
            MissionType::CleanupAfterSuccess => Self::default(),
            MissionType::AssessSpecCoverage => Self {
                repo_summary: Some(state.summary()),
                ..Default::default()
            },
            MissionType::ExplainState => Self {
                repo_summary: Some(state.summary()),
                ..Default::default()
            },
            MissionType::TriageFailures => Self {
                failure_count: Some(state.last_run_failures.len()),
                ..Default::default()
            },
            MissionType::DraftSpecScenarios { feature_path, rule_name } => Self {
                feature_path: Some(feature_path.clone()),
                rule_name: rule_name.clone(),
                ..Default::default()
            },
            MissionType::PromoteScenariosToExecutable { feature_path, scenario_name, rule_name } => Self {
                feature_path: Some(feature_path.clone()),
                rule_name: Some(rule_name.clone()),
                scenario_name: Some(scenario_name.clone()),
                ..Default::default()
            },
        }
    }
}

impl Default for BriefContext {
    fn default() -> Self {
        Self {
            scenario_key: None,
            missing_steps: None,
            all_missing_steps: None,
            binding_exemplars: None,
            scenario_name: None,
            failure: None,
            feature_path: None,
            rule_name: None,
            missing_tags: None,
            current_scenario_count: None,
            rules_without_scenarios: None,
            binding_ids: None,
            repo_summary: None,
            failure_count: None,
        }
    }
}

/// Get the template name for a mission type's brief.
///
/// Available for use when mission briefs transition to full template rendering.
#[allow(dead_code)]
pub fn brief_template_name(mission_type: &MissionType) -> &'static str {
    match mission_type {
        MissionType::CreateMissingBindings { .. } => {
            "mission/briefs/create_missing_bindings.md.j2"
        }
        MissionType::ImplementBehaviorForScenario { .. } => {
            "mission/briefs/implement_behavior.md.j2"
        }
        MissionType::FixRegressionFromGateFailure { .. } => "mission/briefs/fix_regression.md.j2",
        MissionType::RefineFeatureIntent { .. } => "mission/briefs/refine_feature_intent.md.j2",
        MissionType::AddOrClarifyScenario { .. } => "mission/briefs/add_clarify_scenario.md.j2",
        MissionType::NormalizeIdentityTags { .. } => "mission/briefs/normalize_identity_tags.md.j2",
        MissionType::StrengthenThenAssertions { .. } => "mission/briefs/strengthen_then.md.j2",
        MissionType::RefactorBindingsForClarity { .. } => "mission/briefs/refactor_bindings.md.j2",
        MissionType::SummarizeAndClose => "mission/briefs/summarize_and_close.md.j2",
        MissionType::CleanupAfterSuccess => "mission/briefs/cleanup_after_success.md.j2",
        MissionType::AssessSpecCoverage => "mission/briefs/assess_spec_coverage.md.j2",
        MissionType::ExplainState => "mission/briefs/explain_state.md.j2",
        MissionType::TriageFailures => "mission/briefs/triage_failures.md.j2",
        MissionType::DraftSpecScenarios { .. } => "mission/briefs/add_clarify_scenario.md.j2",
        MissionType::PromoteScenariosToExecutable { .. } => "mission/briefs/add_clarify_scenario.md.j2",
    }
}

/// Render a mission brief from its template.
///
/// Available for use when mission briefs transition to full template rendering.
#[allow(dead_code)]
pub fn render_brief(mission_type: &MissionType, state: &RepoState) -> Result<String> {
    let env = create_environment();
    let template_name = brief_template_name(mission_type);
    let template = env
        .get_template(template_name)
        .with_context(|| format!("Failed to get template: {}", template_name))?;

    let ctx = BriefContext::from_mission_type(mission_type, state);

    template
        .render(context! {
            scenario_key => ctx.scenario_key,
            missing_steps => ctx.missing_steps,
            all_missing_steps => ctx.all_missing_steps,
            binding_exemplars => ctx.binding_exemplars,
            scenario_name => ctx.scenario_name,
            failure => ctx.failure,
            feature_path => ctx.feature_path,
            rule_name => ctx.rule_name,
            missing_tags => ctx.missing_tags,
            current_scenario_count => ctx.current_scenario_count,
            rules_without_scenarios => ctx.rules_without_scenarios,
            binding_ids => ctx.binding_ids,
            repo_summary => ctx.repo_summary,
            failure_count => ctx.failure_count,
        })
        .with_context(|| format!("Failed to render brief template: {}", template_name))
}

// =============================================================================
// NEXT_TASK Templates
// =============================================================================

/// Context for NEXT_TASK.md base header.
#[derive(Debug, Serialize)]
pub struct NextTaskBaseContext {
    pub timestamp: String,
    pub action: String,
    pub executable_scenarios_total: u32,
    pub deferred_items_total: u32,
    pub promotion_candidates_total: usize,
    pub eligible_candidates_count: usize,
    pub drift_kind: String,
}

/// Context for DONE action variant.
#[derive(Debug, Serialize)]
pub struct NextTaskDoneContext {
    pub eligible_candidates: Vec<CandidateContext>,
    pub missing_bindings: Vec<MissingBindingContext>,
    pub update_cert_message: Option<String>,
}

/// Context for FIX_LINT action variant.
#[derive(Debug, Serialize)]
pub struct NextTaskFixLintContext {
    pub missing_bindings: Vec<MissingBindingContext>,
}

/// Context for FIX_RUN action variant.
#[derive(Debug, Serialize)]
pub struct NextTaskFixRunContext {
    pub failures: Vec<FailureDisplayContext>,
    pub explain_path: Option<String>,
}

/// Context for NEEDS_UPDATE_CERT_APPROVAL action variant.
#[derive(Debug, Serialize)]
pub struct NextTaskNeedsApprovalContext {
    pub drift_details: Vec<DriftDetailContext>,
    pub update_cert_message: Option<String>,
    pub max_cert_updates: u32,
}

/// Context for RUN_GATE action variants.
#[derive(Debug, Serialize)]
pub struct NextTaskRunGateContext {
    pub action: String,
}

/// Context for artifacts section.
#[derive(Debug, Serialize)]
pub struct NextTaskArtifactsContext {
    pub out_dir: String,
    pub explain_path: Option<String>,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct CandidateContext {
    pub scenario_name: String,
    pub feature_path: String,
    pub rule_name: String,
    pub reuse_score: f32,
    pub new_step_texts_estimate: u32,
}

#[derive(Debug, Serialize)]
pub struct MissingBindingContext {
    pub candidate_name: String,
    pub missing_step_texts: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct FailureDisplayContext {
    pub scenario_key: String,
    pub scenario_name: String,
    pub failure_kind: String,
}

#[derive(Debug, Serialize)]
pub struct DriftDetailContext {
    pub field: String,
    pub baseline: String,
    pub current: String,
}

/// Render NEXT_TASK.md base header.
pub fn render_next_task_base(ctx: &NextTaskBaseContext) -> Result<String> {
    let env = create_environment();
    let template = env
        .get_template("next_task/base.md.j2")
        .context("Failed to get base.md.j2 template")?;

    template
        .render(context! {
            timestamp => ctx.timestamp,
            action => ctx.action,
            executable_scenarios_total => ctx.executable_scenarios_total,
            deferred_items_total => ctx.deferred_items_total,
            promotion_candidates_total => ctx.promotion_candidates_total,
            eligible_candidates_count => ctx.eligible_candidates_count,
            drift_kind => ctx.drift_kind,
        })
        .context("Failed to render next_task base")
}

/// Render DONE action content.
pub fn render_next_task_done(ctx: &NextTaskDoneContext) -> Result<String> {
    let env = create_environment();
    let template = env
        .get_template("next_task/done.md.j2")
        .context("Failed to get done.md.j2 template")?;

    template
        .render(context! {
            eligible_candidates => ctx.eligible_candidates,
            missing_bindings => ctx.missing_bindings,
            update_cert_message => ctx.update_cert_message,
        })
        .context("Failed to render next_task done")
}

/// Render FIX_LINT action content.
pub fn render_next_task_fix_lint(ctx: &NextTaskFixLintContext) -> Result<String> {
    let env = create_environment();
    let template = env
        .get_template("next_task/fix_lint.md.j2")
        .context("Failed to get fix_lint.md.j2 template")?;

    template
        .render(context! {
            missing_bindings => ctx.missing_bindings,
        })
        .context("Failed to render next_task fix_lint")
}

/// Render FIX_RUN action content.
pub fn render_next_task_fix_run(ctx: &NextTaskFixRunContext) -> Result<String> {
    let env = create_environment();
    let template = env
        .get_template("next_task/fix_run.md.j2")
        .context("Failed to get fix_run.md.j2 template")?;

    template
        .render(context! {
            failures => ctx.failures,
            explain_path => ctx.explain_path,
        })
        .context("Failed to render next_task fix_run")
}

/// Render NEEDS_UPDATE_CERT_APPROVAL action content.
pub fn render_next_task_needs_approval(ctx: &NextTaskNeedsApprovalContext) -> Result<String> {
    let env = create_environment();
    let template = env
        .get_template("next_task/needs_approval.md.j2")
        .context("Failed to get needs_approval.md.j2 template")?;

    template
        .render(context! {
            drift_details => ctx.drift_details,
            update_cert_message => ctx.update_cert_message,
            max_cert_updates => ctx.max_cert_updates,
        })
        .context("Failed to render next_task needs_approval")
}

/// Render RUN_GATE action content.
pub fn render_next_task_run_gate(ctx: &NextTaskRunGateContext) -> Result<String> {
    let env = create_environment();
    let template = env
        .get_template("next_task/run_gate.md.j2")
        .context("Failed to get run_gate.md.j2 template")?;

    template
        .render(context! {
            action => ctx.action,
        })
        .context("Failed to render next_task run_gate")
}

/// Render unknown action content.
pub fn render_next_task_unknown(action: &str) -> Result<String> {
    let env = create_environment();
    let template = env
        .get_template("next_task/unknown.md.j2")
        .context("Failed to get unknown.md.j2 template")?;

    template
        .render(context! {
            action => action,
        })
        .context("Failed to render next_task unknown")
}

/// Render artifacts section.
pub fn render_next_task_artifacts(ctx: &NextTaskArtifactsContext) -> Result<String> {
    let env = create_environment();
    let template = env
        .get_template("next_task/artifacts.md.j2")
        .context("Failed to get artifacts.md.j2 template")?;

    template
        .render(context! {
            out_dir => ctx.out_dir,
            explain_path => ctx.explain_path,
            version => ctx.version,
        })
        .context("Failed to render next_task artifacts")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface_policy::SurfaceLock;

    #[test]
    fn test_all_templates_parse() {
        let env = create_environment();

        // List of all template names we expect to exist
        let expected_templates = [
            "mission/MISSION.md.j2",
            "mission/POLICY.md.j2",
            "mission/briefs/create_missing_bindings.md.j2",
            "mission/briefs/implement_behavior.md.j2",
            "mission/briefs/fix_regression.md.j2",
            "mission/briefs/refine_feature_intent.md.j2",
            "mission/briefs/add_clarify_scenario.md.j2",
            "mission/briefs/normalize_identity_tags.md.j2",
            "mission/briefs/strengthen_then.md.j2",
            "mission/briefs/refactor_bindings.md.j2",
            "mission/briefs/summarize_and_close.md.j2",
            "mission/briefs/cleanup_after_success.md.j2",
            "mission/briefs/explain_state.md.j2",
            "mission/briefs/triage_failures.md.j2",
            "components/surfaces_table.md.j2",
            "components/budgets_table.md.j2",
            "components/budgets_full_table.md.j2",
            "components/footer.md.j2",
            "next_task/base.md.j2",
            "next_task/done.md.j2",
            "next_task/fix_lint.md.j2",
            "next_task/fix_run.md.j2",
            "next_task/needs_approval.md.j2",
            "next_task/run_gate.md.j2",
            "next_task/unknown.md.j2",
            "next_task/artifacts.md.j2",
        ];

        for name in &expected_templates {
            env.get_template(name)
                .unwrap_or_else(|e| panic!("Template {} should parse: {}", name, e));
        }
    }

    #[test]
    fn test_render_mission_md() {
        let ctx = MissionContext {
            mission_id: "001-test-abc12345".to_string(),
            mission_type: "CreateMissingBindings".to_string(),
            stage: "ImplementTests".to_string(),
            target: Some("feature:Rule(01):Scenario(01)".to_string()),
            objective: "Create step bindings".to_string(),
            context: "Missing bindings detected".to_string(),
            validation_criteria: vec![
                "Scenario is executable".to_string(),
                "Gate passes".to_string(),
            ],
            surface_policy: SurfacePolicyContext {
                spec: "LOCKED".to_string(),
                tests_bindings: "UNLOCKED".to_string(),
                sut: "LOCKED".to_string(),
            },
            surface_definitions: SurfaceDefinitionsContext {
                spec: SurfaceDefContext {
                    patterns: vec!["test/specs/**/*.feature".to_string()],
                },
                tests_bindings: SurfaceDefContext {
                    patterns: vec!["test/**".to_string()],
                },
                sut: SurfaceDefContext {
                    patterns: vec!["src/**".to_string()],
                },
            },
            budgets: BudgetsContext {
                max_files_changed: 10,
                max_scenarios_promoted: 3,
                max_runtime_seconds: 600,
                max_retries: 2,
            },
            version: TESAKI_VERSION.to_string(),
            previous_failure: None,
        };

        let result = render_mission_md(&ctx).unwrap();

        assert!(result.contains("# Mission 001-test-abc12345"));
        assert!(result.contains("**Type:** CreateMissingBindings"));
        assert!(result.contains("**Target:** feature:Rule(01):Scenario(01)"));
        assert!(result.contains("## 🎯 Objective"));
        assert!(result.contains("| Surface | Policy | Allowed Paths |"));
        assert!(result.contains("| Spec | LOCKED |"));
        assert!(result.contains(&format!("Tesaki v{}", TESAKI_VERSION)));
    }

    #[test]
    fn test_render_policy_md() {
        let ctx = PolicyContext {
            surface_policy: SurfacePolicyContext {
                spec: "LOCKED".to_string(),
                tests_bindings: "UNLOCKED".to_string(),
                sut: "LOCKED".to_string(),
            },
            surface_definitions: SurfaceDefinitionsContext {
                spec: SurfaceDefContext {
                    patterns: vec!["test/specs/**/*.feature".to_string()],
                },
                tests_bindings: SurfaceDefContext {
                    patterns: vec!["test/**".to_string()],
                },
                sut: SurfaceDefContext {
                    patterns: vec!["src/**".to_string()],
                },
            },
            budgets: BudgetsContext {
                max_files_changed: 10,
                max_scenarios_promoted: 3,
                max_runtime_seconds: 600,
                max_retries: 2,
            },
            version: TESAKI_VERSION.to_string(),
        };

        let result = render_policy_md(&ctx).unwrap();

        assert!(result.contains("# Mission Policy"));
        assert!(result.contains("**NO COMMITS**"));
        assert!(result.contains("specs repository ONLY"));
        assert!(result.contains("Max retries | 2"));
    }

    #[test]
    fn test_render_brief_create_missing_bindings() {
        let mission_type = MissionType::CreateMissingBindings {
            scenario_key: "feature:Rule(01):Scenario(01)".to_string(),
            missing_steps: vec![
                "Given a test".to_string(),
                "When something happens".to_string(),
            ],
        };
        let state = RepoState::default();

        let result = render_brief(&mission_type, &state).unwrap();

        // Slimmed template (per OPTIMIZATION_ANALYSIS.md)
        assert!(result.contains("**Target:** feature:Rule(01):Scenario(01)"));
        assert!(result.contains("**Bindings Needed:** 2"));
        assert!(result.contains("`Given a test`"));
        assert!(result.contains("`When something happens`"));
    }

    #[test]
    fn test_render_next_task_base() {
        let ctx = NextTaskBaseContext {
            timestamp: "2026-01-22T10:00:00Z".to_string(),
            action: "DONE".to_string(),
            executable_scenarios_total: 5,
            deferred_items_total: 3,
            promotion_candidates_total: 10,
            eligible_candidates_count: 7,
            drift_kind: "NONE".to_string(),
        };

        let result = render_next_task_base(&ctx).unwrap();

        assert!(result.contains("**Generated:** 2026-01-22T10:00:00Z"));
        assert!(result.contains("**Action:** `DONE`"));
        assert!(result.contains("| Executable Scenarios | 5 |"));
    }

    #[test]
    fn test_surface_policy_context_from() {
        let policy = SurfacePolicy {
            spec: SurfaceLock::Locked,
            tests_bindings: SurfaceLock::Unlocked,
            sut: SurfaceLock::Locked,
        };

        let ctx: SurfacePolicyContext = (&policy).into();

        assert_eq!(ctx.spec, "LOCKED");
        assert_eq!(ctx.tests_bindings, "UNLOCKED");
        assert_eq!(ctx.sut, "LOCKED");
    }

    #[test]
    fn test_budgets_context_from() {
        let budgets = MissionBudgets {
            max_files_changed: 10,
            max_scenarios_promoted: 3,
            max_runtime_seconds: 600,
            max_retries: 2,
            max_cert_updates: 1,
        };

        let ctx: BudgetsContext = (&budgets).into();

        assert_eq!(ctx.max_files_changed, 10);
        assert_eq!(ctx.max_retries, 2);
    }
}
