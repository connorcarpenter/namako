//! Execution-evidence identity boundary.
//!
//! One place decides whether a [`RunReport`] is complete, faithful evidence
//! for a [`ResolvedPlan`]. `namako verify`, `namako update-cert` and
//! `namako gate` all call [`validate_run`]; none of them re-derives
//! these rules.
//!
//! The same boundary owns the adapter side: [`dispatch_step`] hands a step
//! implementation the exact planned payload (captures, `DocString`,
//! `DataTable`), refuses unsupported or non-corresponding payload before
//! execution, and derives the executed identity from the dispatched values
//! instead of echoing the plan.
//!
//! Spec: `docs/EVIDENCE.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Display, Formatter};

use crate::npap::{
    HASH_CONTRACT_VERSION, NPAP_VERSION, PlannedStep, ResolvedPlan, ResolvedScenario, RunReport,
    ScenarioResult, ScenarioStatus, SemanticBinding, SemanticStepRegistry, StepResult, StepStatus,
    execution_payload_hash,
};

/// Which plan scenarios the evidence must cover.
#[derive(Debug, Clone, Copy)]
pub enum Scope<'a> {
    /// Every scenario in the plan (certification scope).
    Full,
    /// Exactly these scenario keys; earns only this declared scope.
    Selected(&'a BTreeSet<String>),
}

/// What a valid report proves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    /// Scenarios proven passed.
    pub scenarios: usize,
    /// Steps proven passed.
    pub steps: usize,
}

/// One reason a report is not valid evidence for the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    /// A header field differs from the plan or the protocol constants.
    HeaderMismatch {
        /// Header field name.
        field: &'static str,
        /// Value in the report.
        report: String,
        /// Expected value.
        expected: String,
    },
    /// `run_report_hash` does not match the hash recomputed from content.
    ReportHashMismatch {
        /// Hash the report claims.
        claimed: String,
        /// Hash recomputed from the report content.
        recomputed: String,
    },
    /// Report scenarios are not sorted by `scenario_key`.
    UnsortedScenarios,
    /// A selected key does not exist in the plan.
    UnknownSelection {
        /// The selected key.
        key: String,
    },
    /// A selected scenario has no result.
    MissingScenario {
        /// Scenario key.
        key: String,
    },
    /// A result exists for a scenario outside the selected scope.
    ExtraScenario {
        /// Scenario key.
        key: String,
    },
    /// The same scenario appears more than once.
    DuplicateScenario {
        /// Scenario key.
        key: String,
    },
    /// Step count differs from the plan.
    StepCountMismatch {
        /// Scenario key.
        key: String,
        /// Planned step count.
        planned: usize,
        /// Reported step count.
        reported: usize,
    },
    /// A step identity field differs from the plan or registry.
    StepIdentityMismatch {
        /// Scenario key.
        key: String,
        /// 1-based step index.
        step: usize,
        /// Field name.
        field: &'static str,
        /// Expected value.
        expected: String,
        /// Reported value.
        reported: String,
    },
    /// Scenario status contradicts its step statuses.
    StatusContradiction {
        /// Scenario key.
        key: String,
        /// Claimed scenario status.
        status: ScenarioStatus,
    },
    /// A selected scenario did not pass.
    NotPassed {
        /// Scenario key.
        key: String,
        /// Claimed scenario status.
        status: ScenarioStatus,
        /// `(1-based step, message)` for each non-passed step.
        steps: Vec<(usize, String)>,
    },
}

impl Display for Violation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::HeaderMismatch {
                field,
                report,
                expected,
            } => write!(
                f,
                "STALE OR DRIFTED ARTIFACT: {field} mismatch\n    Run report: {report}\n    Current:    {expected}"
            ),
            Self::ReportHashMismatch {
                claimed,
                recomputed,
            } => write!(
                f,
                "run_report_hash mismatch: report claims {claimed}, content hashes to {recomputed}"
            ),
            Self::UnsortedScenarios => write!(f, "report scenarios are not sorted by scenario_key"),
            Self::UnknownSelection { key } => {
                write!(f, "selected scenario '{key}' is not in the plan")
            }
            Self::MissingScenario { key } => {
                write!(f, "MISSING: no result for planned scenario {key}")
            }
            Self::ExtraScenario { key } => write!(f, "EXTRA: result for unselected scenario {key}"),
            Self::DuplicateScenario { key } => {
                write!(f, "DUPLICATE: scenario {key} reported more than once")
            }
            Self::StepCountMismatch {
                key,
                planned,
                reported,
            } => write!(f, "{key}: planned {planned} step(s), report has {reported}"),
            Self::StepIdentityMismatch {
                key,
                step,
                field,
                expected,
                reported,
            } => write!(
                f,
                "{key} step {step}: {field} mismatch\n    Expected: {expected}\n    Reported: {reported}"
            ),
            Self::StatusContradiction { key, status } => write!(
                f,
                "{key}: scenario status {} contradicts its step statuses",
                status_name(*status)
            ),
            Self::NotPassed { key, status, steps } => {
                write!(f, "FAILED: {key} ({})", status_name(*status))?;
                for (i, msg) in steps {
                    write!(f, "\n    Step {i}: {msg}")?;
                }
                Ok(())
            }
        }
    }
}

/// Validates that `report` is complete, faithful, passing evidence for the
/// selected `scope` of `plan`, executed against `registry`.
///
/// # Errors
///
/// Returns every violation found; an empty error list is never returned.
pub fn validate_run(
    plan: &ResolvedPlan,
    registry: &SemanticStepRegistry,
    report: &RunReport,
    scope: Scope<'_>,
) -> Result<Summary, Vec<Violation>> {
    let mut v = Vec::new();
    check_header(plan, report, &mut v);

    let planned: BTreeMap<&str, &ResolvedScenario> = plan
        .scenarios
        .iter()
        .map(|s| (s.scenario_key.as_str(), s))
        .collect();
    let selected: BTreeSet<&str> = match scope {
        Scope::Full => planned.keys().copied().collect(),
        Scope::Selected(keys) => {
            for key in keys {
                if !planned.contains_key(key.as_str()) {
                    v.push(Violation::UnknownSelection { key: key.clone() });
                }
            }
            keys.iter().map(String::as_str).collect()
        }
    };
    let bindings: BTreeMap<&str, &SemanticBinding> = registry
        .bindings
        .iter()
        .map(|b| (b.binding_id.as_str(), b))
        .collect();

    if !report
        .scenarios
        .is_sorted_by(|a, b| a.scenario_key <= b.scenario_key)
    {
        v.push(Violation::UnsortedScenarios);
    }

    let mut seen = BTreeSet::new();
    let mut summary = Summary {
        scenarios: 0,
        steps: 0,
    };
    for result in &report.scenarios {
        let key = result.scenario_key.as_str();
        if !seen.insert(key) {
            v.push(Violation::DuplicateScenario {
                key: key.to_owned(),
            });
            continue;
        }
        let Some(planned_scenario) = planned.get(key).filter(|_| selected.contains(key)) else {
            v.push(Violation::ExtraScenario {
                key: key.to_owned(),
            });
            continue;
        };
        let before = v.len();
        check_scenario(&planned_scenario.steps, result, &bindings, &mut v);
        if v.len() == before {
            summary.scenarios += 1;
            summary.steps += result.steps.len();
        }
    }
    for key in &selected {
        if planned.contains_key(key) && !seen.contains(key) {
            v.push(Violation::MissingScenario {
                key: (*key).to_owned(),
            });
        }
    }

    if v.is_empty() { Ok(summary) } else { Err(v) }
}

/// Checks the report header against protocol constants and the plan.
fn check_header(plan: &ResolvedPlan, report: &RunReport, v: &mut Vec<Violation>) {
    let h = &report.header;
    let mut field = |name: &'static str, got: &str, want: &str| {
        if got != want {
            v.push(Violation::HeaderMismatch {
                field: name,
                report: got.to_owned(),
                expected: want.to_owned(),
            });
        }
    };
    field(
        "npap_version",
        &h.npap_version.to_string(),
        &NPAP_VERSION.to_string(),
    );
    field(
        "hash_contract_version",
        &h.hash_contract_version,
        HASH_CONTRACT_VERSION,
    );
    field(
        "feature_fingerprint_hash",
        &h.feature_fingerprint_hash,
        &plan.header.feature_fingerprint_hash,
    );
    field(
        "step_registry_hash",
        &h.step_registry_hash,
        &plan.header.step_registry_hash,
    );
    field(
        "resolved_plan_hash",
        &h.resolved_plan_hash,
        &plan.header.resolved_plan_hash,
    );

    let recomputed = RunReport::new(
        h.feature_fingerprint_hash.clone(),
        h.step_registry_hash.clone(),
        h.resolved_plan_hash.clone(),
        report.scenarios.clone(),
    )
    .header
    .run_report_hash;
    if recomputed != h.run_report_hash {
        v.push(Violation::ReportHashMismatch {
            claimed: h.run_report_hash.clone(),
            recomputed,
        });
    }
}

/// Checks one reported scenario against its planned steps.
fn check_scenario(
    planned: &[PlannedStep],
    result: &ScenarioResult,
    bindings: &BTreeMap<&str, &SemanticBinding>,
    v: &mut Vec<Violation>,
) {
    let key = &result.scenario_key;
    if planned.len() != result.steps.len() {
        v.push(Violation::StepCountMismatch {
            key: key.clone(),
            planned: planned.len(),
            reported: result.steps.len(),
        });
        return;
    }
    let mut mismatch = |step: usize, field: &'static str, expected: &str, reported: &str| {
        if expected != reported {
            v.push(Violation::StepIdentityMismatch {
                key: key.clone(),
                step,
                field,
                expected: expected.to_owned(),
                reported: reported.to_owned(),
            });
        }
    };
    for (i, (p, r)) in planned.iter().zip(&result.steps).enumerate() {
        let n = i + 1;
        let recomputed = execution_payload_hash(
            &p.effective_kind,
            &p.step_text,
            &p.binding_id,
            &p.captures,
            p.docstring.as_deref(),
            p.datatable.as_deref(),
        );
        mismatch(n, "plan payload_hash", &recomputed, &p.payload_hash);
        mismatch(
            n,
            "planned_binding_id",
            &p.binding_id,
            &r.planned_binding_id,
        );
        mismatch(
            n,
            "planned_payload_hash",
            &recomputed,
            &r.planned_payload_hash,
        );
        if r.status == StepStatus::Passed {
            mismatch(
                n,
                "executed_binding_id",
                &p.binding_id,
                &r.executed_binding_id,
            );
            mismatch(
                n,
                "executed_payload_hash",
                &recomputed,
                &r.executed_payload_hash,
            );
            let impl_hash = bindings
                .get(p.binding_id.as_str())
                .map_or("<binding absent from registry>", |b| b.impl_hash.as_str());
            mismatch(n, "executed_impl_hash", impl_hash, &r.executed_impl_hash);
        }
    }

    if derive_scenario_status(&result.steps) != result.status {
        v.push(Violation::StatusContradiction {
            key: key.clone(),
            status: result.status,
        });
        return;
    }
    if result.status != ScenarioStatus::Passed {
        v.push(Violation::NotPassed {
            key: key.clone(),
            status: result.status,
            steps: result
                .steps
                .iter()
                .enumerate()
                .filter(|(_, s)| s.status != StepStatus::Passed)
                .map(|(i, s)| {
                    (
                        i + 1,
                        s.error_message
                            .clone()
                            .unwrap_or_else(|| format!("{:?} (no message)", s.status)),
                    )
                })
                .collect(),
        });
    }
}

/// The only scenario status consistent with `steps`: `Failed` if any step
/// failed, `Passed` if every step passed (including zero steps), otherwise
/// `Skipped`.
#[must_use]
pub fn derive_scenario_status(steps: &[StepResult]) -> ScenarioStatus {
    if steps.iter().any(|s| s.status == StepStatus::Failed) {
        ScenarioStatus::Failed
    } else if steps.iter().all(|s| s.status == StepStatus::Passed) {
        ScenarioStatus::Passed
    } else {
        ScenarioStatus::Skipped
    }
}

// ============================================================================
// Faithful dispatch (adapter side)
// ============================================================================

/// The exact payload handed to a step implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepPayload<'a> {
    /// Effective kind after And/But resolution.
    pub effective_kind: &'a str,
    /// Step text.
    pub step_text: &'a str,
    /// Dispatched binding.
    pub binding_id: &'a str,
    /// Capture values, in expression order.
    pub captures: &'a [String],
    /// `DocString`, if the step has one.
    pub docstring: Option<&'a str>,
    /// `DataTable` rows, if the step has one.
    pub datatable: Option<&'a [Vec<String>]>,
}

impl StepPayload<'_> {
    /// Hash of exactly these values per `GOLD_PLAN` §6.5.
    #[must_use]
    pub fn payload_hash(&self) -> String {
        execution_payload_hash(
            self.effective_kind,
            self.step_text,
            self.binding_id,
            self.captures,
            self.docstring,
            self.datatable,
        )
    }
}

/// Why [`dispatch_step`] refused a step before running it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchRefusal {
    /// No binding was supplied for the planned `binding_id`.
    UnknownBinding(String),
    /// The supplied binding is not the planned one.
    BindingMismatch {
        /// Planned binding.
        planned: String,
        /// Supplied binding.
        supplied: String,
    },
    /// Capture count differs from the binding signature.
    CaptureArity {
        /// Signature arity.
        expected: u32,
        /// Planned captures.
        planned: usize,
    },
    /// The adapter's own match of the step text yields different captures.
    CaptureMismatch {
        /// Planned captures.
        planned: Vec<String>,
        /// Adapter-matched captures.
        matched: Vec<String>,
    },
    /// The step carries a `DocString` the binding does not accept, or lacks one it requires.
    UnsupportedDocstring,
    /// The step carries a `DataTable` the binding does not accept, or lacks one it requires.
    UnsupportedDatatable,
    /// The planned `payload_hash` does not match the planned values.
    PlannedHashMismatch,
}

impl Display for DispatchRefusal {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownBinding(id) => write!(f, "unknown binding_id {id}"),
            Self::BindingMismatch { planned, supplied } => {
                write!(f, "planned binding {planned}, adapter supplied {supplied}")
            }
            Self::CaptureArity { expected, planned } => {
                write!(f, "binding takes {expected} capture(s), step has {planned}")
            }
            Self::CaptureMismatch { planned, matched } => {
                write!(
                    f,
                    "planned captures [{}], adapter matched [{}]",
                    planned.join(", "),
                    matched.join(", ")
                )
            }
            Self::UnsupportedDocstring => {
                write!(f, "DocString presence does not match binding signature")
            }
            Self::UnsupportedDatatable => {
                write!(f, "DataTable presence does not match binding signature")
            }
            Self::PlannedHashMismatch => {
                write!(f, "planned payload_hash does not match planned values")
            }
        }
    }
}

/// Refuses a step whose payload or binding cannot be faithfully dispatched.
fn admit<'a>(
    step: &'a PlannedStep,
    binding: Option<&'a SemanticBinding>,
    matched_captures: &[String],
) -> Result<StepPayload<'a>, DispatchRefusal> {
    let binding =
        binding.ok_or_else(|| DispatchRefusal::UnknownBinding(step.binding_id.clone()))?;
    if binding.binding_id != step.binding_id {
        return Err(DispatchRefusal::BindingMismatch {
            planned: step.binding_id.clone(),
            supplied: binding.binding_id.clone(),
        });
    }
    let sig = &binding.signature;
    if usize::try_from(sig.captures_arity).ok() != Some(step.captures.len()) {
        return Err(DispatchRefusal::CaptureArity {
            expected: sig.captures_arity,
            planned: step.captures.len(),
        });
    }
    if matched_captures != step.captures.as_slice() {
        return Err(DispatchRefusal::CaptureMismatch {
            planned: step.captures.clone(),
            matched: matched_captures.to_vec(),
        });
    }
    if sig.accepts_docstring != step.docstring.is_some() {
        return Err(DispatchRefusal::UnsupportedDocstring);
    }
    if sig.accepts_datatable != step.datatable.is_some() {
        return Err(DispatchRefusal::UnsupportedDatatable);
    }
    let payload = StepPayload {
        effective_kind: &step.effective_kind,
        step_text: &step.step_text,
        binding_id: &binding.binding_id,
        captures: &step.captures,
        docstring: step.docstring.as_deref(),
        datatable: step.datatable.as_deref(),
    };
    if payload.payload_hash() != step.payload_hash {
        return Err(DispatchRefusal::PlannedHashMismatch);
    }
    Ok(payload)
}

/// Dispatches one planned step faithfully.
///
/// `binding` is the adapter's registry entry for `step.binding_id`;
/// `matched_captures` is what the adapter's own matcher extracts from
/// `step.step_text` for that binding. On refusal, `run` is never called and
/// the step fails with empty executed identity. Otherwise `run` receives the
/// full payload, and the executed payload hash is computed from that payload.
pub fn dispatch_step<F>(
    step: &PlannedStep,
    binding: Option<&SemanticBinding>,
    matched_captures: &[String],
    run: F,
) -> StepResult
where
    F: FnOnce(&StepPayload<'_>) -> Result<(), String>,
{
    match admit(step, binding, matched_captures) {
        Err(refusal) => StepResult {
            planned_binding_id: step.binding_id.clone(),
            executed_binding_id: String::new(),
            planned_payload_hash: step.payload_hash.clone(),
            executed_payload_hash: String::new(),
            executed_impl_hash: String::new(),
            status: StepStatus::Failed,
            error_message: Some(format!("refused before execution: {refusal}")),
        },
        Ok(payload) => {
            let outcome = run(&payload);
            let impl_hash = binding.map(|b| b.impl_hash.clone()).unwrap_or_default();
            StepResult {
                planned_binding_id: step.binding_id.clone(),
                executed_binding_id: payload.binding_id.to_owned(),
                planned_payload_hash: step.payload_hash.clone(),
                executed_payload_hash: payload.payload_hash(),
                executed_impl_hash: impl_hash,
                status: if outcome.is_ok() {
                    StepStatus::Passed
                } else {
                    StepStatus::Failed
                },
                error_message: outcome.err(),
            }
        }
    }
}

/// Marks a not-run step as skipped, keeping its planned identity.
#[must_use]
pub fn skipped_step(step: &PlannedStep) -> StepResult {
    StepResult {
        planned_binding_id: step.binding_id.clone(),
        executed_binding_id: String::new(),
        planned_payload_hash: step.payload_hash.clone(),
        executed_payload_hash: String::new(),
        executed_impl_hash: String::new(),
        status: StepStatus::Skipped,
        error_message: None,
    }
}

/// Builds a scenario result whose status is derived from its steps.
#[must_use]
pub fn scenario_result(scenario_key: String, steps: Vec<StepResult>) -> ScenarioResult {
    ScenarioResult {
        status: derive_scenario_status(&steps),
        scenario_key,
        steps,
    }
}

/// The report's wire name for a scenario status.
const fn status_name(status: ScenarioStatus) -> &'static str {
    match status {
        ScenarioStatus::Passed => "passed",
        ScenarioStatus::Failed => "failed",
        ScenarioStatus::Skipped => "skipped",
    }
}
