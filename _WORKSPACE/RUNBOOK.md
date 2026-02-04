# RUNBOOK.md — Turnkey Loop Execution

**Last Updated:** 2026-02-03

---

## Quick Start

### 1. Configure Target Repo

Create `.tesaki/config.toml` in your target repo:

```toml
specs_dir = "test/specs"
adapter_cmd = "cargo run --manifest-path test/npa/Cargo.toml --"

# Optional settings
agent = "copilot"           # runner backend (copilot, claude, codex, mock)
max_retries = 0             # Fresh context > stale retries
max_cert_updates = 3
max_runtime_seconds = 600
max_files_changed = 10
quality_gates_enabled = true

# Pre-gate build (optional)
# pre_gate_build = "cargo check -p my-test-harness"
# pre_gate_build_mode = "auto"  # auto | always | never
```

### 2. Run the Loop

```bash
# From target repo root
tesaki --loop 10
```

Or interactive:
```bash
tesaki
> loop 10
```

---

## Stop Conditions

Tesaki stops when:

| Reason | Meaning |
|--------|---------|
| `DONE` | All gates pass, no issues remain |
| `NO_PROGRESS` | No changes made after attempts |
| `GATE_FAILED` | Lint/run/verify failed |
| `POLICY_VIOLATION` | Runner edited locked surface |
| `HUMAN_REQUIRED` | Manual intervention needed |
| `BUDGET` | Runtime/attempt limits hit |

---

## Expected Cascade (Normal)

After `AddOrClarifyScenario`, lint may fail due to missing bindings. This is expected:

```
Expected cascade: missing bindings created.
```

The next mission will be `CreateMissingBindings`.

---

## Quality Guardrails

### Spec Quality Gate (after AddOrClarifyScenario)

| Rule | Blocks |
|------|--------|
| `NO_PLACEHOLDER_STEPS` | Generic steps: "Given a test scenario", "Then no panic occurs" |
| `DOMAIN_NOUN_REQUIRED` | Scenarios unrelated to parent Rule |
| `NO_ORPHAN_STUBS` | Stub markers outside `_orphan_stubs.feature` |

Set `quality_gates_enabled = false` in config to disable.

### Surface Policy Enforcement

If a mission edits files outside its allowed surface:
1. Changes are rolled back
2. Mission marked `POLICY_VIOLATION`
3. Session stops

### Evidence-Driven Selection

Missions require concrete evidence:

| Mission Type | Required Evidence |
|--------------|-------------------|
| `FixRegressionFromGateFailure` | `sut_issues` > 0 |
| `CreateMissingBindings` | `binding_issues` with scenario_key |
| `DraftSpecScenarios` | Rule with 0 scenarios, no deferred |
| `PromoteScenariosToExecutable` | Deferred scenarios exist |
| `AddOrClarifyScenario` | Partial coverage |

---

## Debugging

```bash
# Check mission details
tesaki diagnose M-abc123

# Validate tooling
cd namako/
cargo test -p tesaki
```

---

## Pre-Gate Build Modes

| Mode | Behavior |
|------|----------|
| `auto` | Skip for spec-only missions (Tests/SUT locked) |
| `always` | Run build check every mission |
| `never` | Skip build check entirely |

---

*Update this runbook alongside tool changes.*
