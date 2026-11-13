//! Controls for the shared execution-evidence boundary (`docs/EVIDENCE.md`).
#![cfg(feature = "npap")]

use std::collections::BTreeSet;
use std::sync::Mutex;

use namako_engine::engine::{ResolutionResult, Resolver};
use namako_engine::evidence::{
    Scope, StepPayload, Violation, dispatch_step, scenario_result, skipped_step, validate_run,
};
use namako_engine::npap::{
    BindingSignature, ResolvedPlan, RunReport, ScenarioResult, ScenarioStatus, SemanticBinding,
    SemanticStepRegistry, StepStatus, generate_binding_id,
};

const FEATURE: &str = "\
@Feature(evidence)
Feature: Evidence

  Background:
    Given a server is running

  @Rule(01)
  Rule: Moves

    @Scenario(01)
    Scenario: count
      When the client sends 3 messages
      Then the payload is
        \"\"\"
        hello
        \"\"\"

    @Scenario(02)
    Scenario: table
      When the client sends 7 messages
      Then the rows are
        | a | b |
        | 1 | 2 |
";

/// Same scenarios, written in the opposite order.
const FEATURE_REORDERED: &str = "\
@Feature(evidence)
Feature: Evidence

  Background:
    Given a server is running

  @Rule(01)
  Rule: Moves

    @Scenario(02)
    Scenario: table
      When the client sends 7 messages
      Then the rows are
        | a | b |
        | 1 | 2 |

    @Scenario(01)
    Scenario: count
      When the client sends 3 messages
      Then the payload is
        \"\"\"
        hello
        \"\"\"
";

const OUTLINE: &str = "\
@Feature(outline)
Feature: Outline

  @Rule(01)
  Rule: Sends

    @Scenario(01)
    Scenario Outline: sends
      When the client sends <n> messages

      Examples:
        | n |
        | 4 |
";

fn binding(kind: &str, expr: &str, arity: u32, doc: bool, table: bool) -> SemanticBinding {
    SemanticBinding {
        binding_id: generate_binding_id(kind, expr),
        kind: kind.to_owned(),
        expression: expr.to_owned(),
        signature: BindingSignature {
            captures_arity: arity,
            accepts_docstring: doc,
            accepts_datatable: table,
        },
        impl_hash: format!("impl:{expr}"),
        source_symbol: None,
    }
}

fn registry() -> SemanticStepRegistry {
    SemanticStepRegistry::new(vec![
        binding("Given", "a server is running", 0, false, false),
        binding("When", "the client sends {int} messages", 1, false, false),
        binding("Then", "the payload is", 0, true, false),
        binding("Then", "the rows are", 0, false, true),
    ])
}

fn resolve_all(reg: &SemanticStepRegistry, files: &[(&str, &str)]) -> ResolutionResult {
    let resolver = match Resolver::new(reg) {
        Ok(r) => r,
        Err(e) => panic!("registry: {e:?}"),
    };
    let result = resolver.resolve(files.iter().copied());
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    result
}

fn plan_of(reg: &SemanticStepRegistry) -> ResolvedPlan {
    match resolve_all(reg, &[("features/evidence.feature", FEATURE)]).plan {
        Some(plan) => plan,
        None => panic!("resolution of features/evidence.feature produced no plan"),
    }
}

fn bind<'a>(reg: &'a SemanticStepRegistry, id: &str) -> Option<&'a SemanticBinding> {
    reg.bindings.iter().find(|b| b.binding_id == id)
}

/// Faithful run of every scenario; `observed` records what each step saw.
fn faithful_report(
    plan: &ResolvedPlan,
    reg: &SemanticStepRegistry,
    observed: &Mutex<Vec<String>>,
) -> RunReport {
    let scenarios = plan
        .scenarios
        .iter()
        .map(|s| {
            let steps = s
                .steps
                .iter()
                .map(|p| {
                    dispatch_step(p, bind(reg, &p.binding_id), &p.captures, |pl| {
                        observed
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .push(render(pl));
                        Ok(())
                    })
                })
                .collect();
            scenario_result(s.scenario_key.clone(), steps)
        })
        .collect();
    rehash(plan, scenarios)
}

fn render(p: &StepPayload<'_>) -> String {
    format!(
        "{}|{:?}|{:?}|{:?}",
        p.step_text, p.captures, p.docstring, p.datatable
    )
}

fn rehash(plan: &ResolvedPlan, scenarios: Vec<ScenarioResult>) -> RunReport {
    RunReport::new(
        plan.header.feature_fingerprint_hash.clone(),
        plan.header.step_registry_hash.clone(),
        plan.header.resolved_plan_hash.clone(),
        scenarios,
    )
}

fn violations(
    plan: &ResolvedPlan,
    reg: &SemanticStepRegistry,
    report: &RunReport,
) -> Vec<Violation> {
    match validate_run(plan, reg, report, Scope::Full) {
        Ok(s) => panic!("expected rejection, got {s:?}"),
        Err(v) => v,
    }
}

// ---------------------------------------------------------------- validator

#[test]
fn exact_faithful_report_passes() {
    let reg = registry();
    let plan = plan_of(&reg);
    let report = faithful_report(&plan, &reg, &Mutex::new(Vec::new()));
    let summary = validate_run(&plan, &reg, &report, Scope::Full).unwrap();
    assert_eq!(summary.scenarios, 2);
    assert_eq!(summary.steps, 6);
}

#[test]
fn empty_report_with_matching_header_is_rejected() {
    let reg = registry();
    let plan = plan_of(&reg);
    let v = violations(&plan, &reg, &rehash(&plan, Vec::new()));
    assert_eq!(v.len(), 2);
    assert!(
        v.iter()
            .all(|x| matches!(x, Violation::MissingScenario { .. }))
    );
}

#[test]
fn incomplete_report_is_rejected() {
    let reg = registry();
    let plan = plan_of(&reg);
    let mut full = faithful_report(&plan, &reg, &Mutex::new(Vec::new()));
    full.scenarios.pop();
    let v = violations(&plan, &reg, &rehash(&plan, full.scenarios));
    assert!(matches!(v.as_slice(), [Violation::MissingScenario { .. }]));
}

#[test]
fn duplicate_scenario_is_rejected() {
    let reg = registry();
    let plan = plan_of(&reg);
    let mut s = faithful_report(&plan, &reg, &Mutex::new(Vec::new())).scenarios;
    s.push(s[0].clone());
    let v = violations(&plan, &reg, &rehash(&plan, s));
    assert!(
        v.iter()
            .any(|x| matches!(x, Violation::DuplicateScenario { .. }))
    );
}

#[test]
fn extra_scenario_is_rejected() {
    let reg = registry();
    let plan = plan_of(&reg);
    let mut s = faithful_report(&plan, &reg, &Mutex::new(Vec::new())).scenarios;
    let mut extra = s[0].clone();
    extra.scenario_key = "zz_not_planned".to_owned();
    s.push(extra);
    let v = violations(&plan, &reg, &rehash(&plan, s));
    assert!(matches!(v.as_slice(), [Violation::ExtraScenario { .. }]));
}

#[test]
fn failed_step_under_passed_scenario_is_rejected() {
    let reg = registry();
    let plan = plan_of(&reg);
    let mut s = faithful_report(&plan, &reg, &Mutex::new(Vec::new())).scenarios;
    s[0].steps[1].status = StepStatus::Failed;
    let v = violations(&plan, &reg, &rehash(&plan, s));
    assert!(
        v.iter()
            .any(|x| matches!(x, Violation::StatusContradiction { .. }))
    );
}

#[test]
fn honestly_failed_scenario_is_not_passed() {
    let reg = registry();
    let plan = plan_of(&reg);
    let mut s = faithful_report(&plan, &reg, &Mutex::new(Vec::new())).scenarios;
    s[0].steps[1].status = StepStatus::Failed;
    s[0].status = ScenarioStatus::Failed;
    let v = violations(&plan, &reg, &rehash(&plan, s));
    assert!(matches!(v.as_slice(), [Violation::NotPassed { .. }]));
}

#[test]
fn echoed_identity_mismatch_is_rejected() {
    let reg = registry();
    let plan = plan_of(&reg);
    let mut s = faithful_report(&plan, &reg, &Mutex::new(Vec::new())).scenarios;
    s[1].steps[2].executed_payload_hash = "0".repeat(64);
    s[1].steps[0].executed_impl_hash = "stale".to_owned();
    let v = violations(&plan, &reg, &rehash(&plan, s));
    let fields: Vec<_> = v
        .iter()
        .filter_map(|x| match x {
            Violation::StepIdentityMismatch { field, .. } => Some(*field),
            _ => None,
        })
        .collect();
    assert_eq!(fields, ["executed_impl_hash", "executed_payload_hash"]);
}

#[test]
fn bad_report_hash_is_rejected() {
    let reg = registry();
    let plan = plan_of(&reg);
    let mut report = faithful_report(&plan, &reg, &Mutex::new(Vec::new()));
    report.header.run_report_hash = "f".repeat(64);
    let v = violations(&plan, &reg, &report);
    assert!(matches!(
        v.as_slice(),
        [Violation::ReportHashMismatch { .. }]
    ));
}

#[test]
fn stale_header_is_rejected() {
    let reg = registry();
    let plan = plan_of(&reg);
    let s = faithful_report(&plan, &reg, &Mutex::new(Vec::new())).scenarios;
    let report = RunReport::new(
        plan.header.feature_fingerprint_hash.clone(),
        plan.header.step_registry_hash.clone(),
        "stale".to_owned(),
        s,
    );
    let v = violations(&plan, &reg, &report);
    assert!(matches!(
        v.as_slice(),
        [Violation::HeaderMismatch {
            field: "resolved_plan_hash",
            ..
        }]
    ));
}

#[test]
fn explicit_partial_scope_earns_only_declared_scope() {
    let reg = registry();
    let plan = plan_of(&reg);
    let mut s = faithful_report(&plan, &reg, &Mutex::new(Vec::new())).scenarios;
    let kept = s.remove(0);
    let report = rehash(&plan, vec![kept.clone()]);
    let scope: BTreeSet<String> = [kept.scenario_key].into();
    let summary = validate_run(&plan, &reg, &report, Scope::Selected(&scope)).unwrap();
    assert_eq!(summary.scenarios, 1);
    // The same report cannot pass full scope.
    assert!(validate_run(&plan, &reg, &report, Scope::Full).is_err());
    // Empty selection earns nothing and accepts nothing extra.
    let none = BTreeSet::new();
    let empty = rehash(&plan, Vec::new());
    let summary = validate_run(&plan, &reg, &empty, Scope::Selected(&none)).unwrap();
    assert_eq!(summary.scenarios, 0);
    assert!(validate_run(&plan, &reg, &report, Scope::Selected(&none)).is_err());
    // Unknown selected keys are refused.
    let bogus: BTreeSet<String> = ["nope".to_owned()].into();
    assert!(validate_run(&plan, &reg, &empty, Scope::Selected(&bogus)).is_err());
}

// ---------------------------------------------------------------- dispatch

#[test]
fn capturing_binding_observes_exact_payload() {
    let reg = registry();
    let plan = plan_of(&reg);
    let observed = Mutex::new(Vec::new());
    faithful_report(&plan, &reg, &observed);
    let mut seen = observed.into_inner().unwrap();
    seen.sort();
    assert!(seen.contains(&r#"the client sends 3 messages|["3"]|None|None"#.to_owned()));
    assert!(
        seen.contains(&r#"the payload is|[]|Some("\nhello\n")|None"#.to_owned()),
        "{seen:?}"
    );
    assert!(seen.contains(&r#"the rows are|[]|None|Some([["a", "b"], ["1", "2"]])"#.to_owned()));
    assert!(seen.contains(&"a server is running|[]|None|None".to_owned()));
}

#[test]
fn outline_dispatch_carries_example_values() {
    let reg = registry();
    let result = resolve_all(&reg, &[("features/outline.feature", OUTLINE)]);
    let plan = result.plan.expect("plan");
    let observed = Mutex::new(Vec::new());
    let report = faithful_report(&plan, &reg, &observed);
    validate_run(&plan, &reg, &report, Scope::Full).unwrap();
    let seen = observed.into_inner().unwrap();
    assert_eq!(seen, [r#"the client sends 4 messages|["4"]|None|None"#]);
}

#[test]
fn changed_captures_refuse_before_execution() {
    let reg = registry();
    let plan = plan_of(&reg);
    let step = &plan.scenarios[0].steps[1];
    let mut ran = false;
    let r = dispatch_step(
        step,
        bind(&reg, &step.binding_id),
        &["9".to_owned()],
        |_| {
            ran = true;
            Ok(())
        },
    );
    assert!(!ran);
    assert_eq!(r.status, StepStatus::Failed);
    assert!(r.executed_payload_hash.is_empty());
}

#[test]
fn missing_docstring_refuses_before_execution() {
    let reg = registry();
    let plan = plan_of(&reg);
    let mut step = plan.scenarios[0].steps[2].clone();
    assert!(step.docstring.is_some());
    step.docstring = None;
    let r = dispatch_step(&step, bind(&reg, &step.binding_id), &[], |_| Ok(()));
    assert_eq!(r.status, StepStatus::Failed);
}

#[test]
fn unsupported_datatable_refuses_before_execution() {
    let reg = registry();
    let plan = plan_of(&reg);
    let mut step = plan.scenarios[0].steps[0].clone();
    step.datatable = Some(vec![vec!["x".to_owned()]]);
    let r = dispatch_step(&step, bind(&reg, &step.binding_id), &[], |_| Ok(()));
    assert_eq!(r.status, StepStatus::Failed);
}

#[test]
fn changed_payload_cannot_earn_passed() {
    let reg = registry();
    let plan = plan_of(&reg);
    let mut step = plan.scenarios[0].steps[2].clone();
    step.docstring = Some("tampered".to_owned()); // hash still the planned one
    let r = dispatch_step(&step, bind(&reg, &step.binding_id), &[], |_| Ok(()));
    assert_eq!(r.status, StepStatus::Failed);
}

#[test]
fn skipped_steps_make_a_skipped_scenario() {
    let reg = registry();
    let plan = plan_of(&reg);
    let s = &plan.scenarios[0];
    let r = scenario_result(
        s.scenario_key.clone(),
        s.steps.iter().map(skipped_step).collect(),
    );
    assert_eq!(r.status, ScenarioStatus::Skipped);
}

// ---------------------------------------------------------------- explain keys

#[test]
fn scenario_sources_follow_stable_keys_through_reorder_and_background() {
    let reg = registry();
    let a = resolve_all(&reg, &[("features/evidence.feature", FEATURE)]);
    let b = resolve_all(&reg, &[("features/evidence.feature", FEATURE_REORDERED)]);
    let keys = |r: &ResolutionResult| -> Vec<String> {
        r.plan
            .as_ref()
            .unwrap()
            .scenarios
            .iter()
            .map(|s| s.scenario_key.clone())
            .collect()
    };
    assert_eq!(keys(&a), keys(&b));
    for r in [&a, &b] {
        assert_eq!(
            r.scenario_sources.keys().cloned().collect::<Vec<_>>(),
            keys(r),
            "every planned key has exactly one source"
        );
    }
    let count = keys(&a)[0].clone();
    assert_eq!(a.scenario_sources[&count].line, 11);
    assert_eq!(b.scenario_sources[&count].line, 18);
    assert!(
        !a.scenario_sources
            .contains_key("features/evidence.feature:L11")
    );
}

#[test]
fn scenario_sources_cover_outlines() {
    let reg = registry();
    let r = resolve_all(&reg, &[("features/outline.feature", OUTLINE)]);
    let plan = r.plan.as_ref().unwrap();
    assert_eq!(plan.scenarios.len(), 1);
    let src = &r.scenario_sources[&plan.scenarios[0].scenario_key];
    assert_eq!(src.feature_path, "features/outline.feature");
    assert_eq!(src.line, 8);
}
