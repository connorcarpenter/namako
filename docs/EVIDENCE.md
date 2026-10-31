# Execution evidence

Spec for `namako_engine::evidence` (feature `npap`). One typed validator,
`validate_run`, is the only judge of a run report; `verify`,
`update-cert` and `gate` all call it (through `cli/src/evidence.rs` where a
full run is required).

## Validation (`validate_run(plan, registry, report, scope)`)

The report is accepted only if every check passes; all violations are returned.

- Header: protocol version, hash contract and the plan's feature, registry and
  resolved-plan hashes match the plan.
- `run_report_hash` equals the hash recomputed from the report body.
- Scenarios are sorted by key, with no duplicates, no keys outside the plan,
  and none missing from the scope.
- Each scenario has exactly the plan's steps. Per step: planned binding id and
  payload hash match the plan; the plan's payload hash is recomputed from its
  values. A Passed step must also carry the executed binding id, payload hash
  and the registry's `impl_hash`.
- Scenario status equals the status derived from its steps (any Failed →
  Failed; all Passed → Passed; else Skipped). A failed step under a Passed
  scenario is a contradiction.
- Every in-scope scenario passed.

Scope: `Full` requires the whole plan. `Selected(keys)` earns only those keys;
an unknown selected key is refused, and an empty report earns nothing.

## Faithful dispatch (`dispatch_step`)

An adapter executes a planned step through `dispatch_step(step, binding,
matched_captures, run)`. Before running it checks that the binding is known and
matches the plan, that capture count and values match the binding and the
match, that docstring and DataTable are only passed to bindings that accept
them, and that the payload hash recomputes. Any failure refuses before
execution: the step is Failed with message `refused before execution: …` and
empty executed fields. Otherwise `run` receives the exact captures, docstring
and table, and the executed payload hash is derived from those dispatched
values, never echoed from the plan.

## Stable identity

Scenario keys are the engine's id-tag keys. A Scenario Outline plans one
scenario per Examples row, keyed `{scenario}:E{eid}` (the `EID` column, else
the row's position), with `<name>` placeholders substituted in step text,
docstring and table. Source locations live in `ResolutionResult.scenario_sources`
(key → feature path, line), separate from the key, and follow it through
reorders, backgrounds and outlines. `namako explain` looks a scenario up by key
and rejects unknown keys.

## Removed / not wanted

- **Line-number scenario keys in `explain`** (`parse_scenario_key` of
  `path:Lnn`): removed in nydus-36h because keys must survive reordering;
  sources come from `scenario_sources`. Ruled by the execution-evidence
  contract (Usher, usher-home@6663416f).
- **Adapter-echoed executed hashes**: an executed payload hash copied from the
  plan is not evidence; it must come from `dispatch_step`. Same ruling.
