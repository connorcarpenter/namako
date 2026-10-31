//! CLI wrapper over the shared execution-evidence validator
//! (`namako_engine::evidence`), used by `verify`, `update-cert` and `gate`.

use anyhow::{bail, Result};

use namako_engine::evidence::{validate_run, Scope};
use namako_engine::npap::{ResolvedPlan, RunReport, SemanticStepRegistry};

/// Refuses unless `report` is complete, faithful, passing evidence for every
/// scenario in `plan`. Every violation is printed; `context` names the caller.
pub fn require_complete(
    context: &str,
    plan: &ResolvedPlan,
    registry: &SemanticStepRegistry,
    report: &RunReport,
) -> Result<()> {
    match validate_run(plan, registry, report, Scope::Full) {
        Ok(_) => Ok(()),
        Err(violations) => {
            eprintln!(
                "✗ {context}: run report is not valid evidence ({}):",
                violations.len()
            );
            for (i, v) in violations.iter().enumerate() {
                eprintln!("\n  {}. {v}", i + 1);
            }
            bail!(
                "{context}: run report rejected with {} violation(s)",
                violations.len()
            );
        }
    }
}
