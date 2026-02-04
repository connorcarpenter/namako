# RUNBOOK.md — Turnkey Loop Checklist

**Last Updated:** 2026-02-04
**Scope:** Tesaki + Namako toolchain usage (spec repo only)

---

## Turnkey Loop Checklist

### 0) Mission Default (When Asked to "Work on Naia")
If the request is to work on `naia/` using Tesaki/Namako, the default mission is:
1. Use `naia/` as the target repo.
2. Verify `.tesaki/config.toml` exists; if it already exists, do not recreate it.
3. Run the loop from `naia/` (see Step 2).
4. Keep brief notes on friction/streamlining opportunities during the loop.

### 1) Config (Required)
Create `.tesaki/config.toml` in your target repo:

```toml
specs_dir = "test/specs"
adapter_cmd = "cargo run --manifest-path test/npa/Cargo.toml --"

# Optional
agent = "copilot"          # runner + planner
max_retries = 0
max_cert_updates = 3
max_runtime_seconds = 600
max_files_changed = 10

# Pre-gate build (optional)
# pre_gate_build = "cargo check -p my-test-harness"
# pre_gate_build_mode = "auto"   # auto | always | never
```

### 2) One-Command Loop
From the repo root:

```bash
tesaki --loop 10
```

Or interactive:

```bash
tesaki
> loop 10
```

### 3) Expected Cascade (Normal)
If you add scenarios, lint can fail due to missing bindings. This is expected.
Tesaki will report:

```
Expected cascade: missing bindings created.
```

The next mission should be `CreateMissingBindings`.

### 4) Stop Criteria (When It’s Over)
Tesaki will stop when:
- `DONE`: All gates pass and no issues remain
- `NO_PROGRESS`: Runner made no changes or evidence didn’t improve
- `GATE_FAILED`: Lint/run/verify failed after retries
- `HUMAN_REQUIRED`: Workspace dirty or manual approval needed
- `BUDGET`: Runtime or attempt limits hit

### 5) Pre-Gate Build Behavior
- **auto**: skipped for spec-only missions (Tests/SUT locked)
- **always**: run build check every mission
- **never**: skip build check entirely

### 6) Validate This Repo (Tooling)
From `namako/`:

```bash
cargo test
```

Or target a package:

```bash
cargo test -p tesaki
cargo test -p namako-codegen
```

---

---

## Quality Guardrails

These checks run automatically to prevent low-quality spec edits from entering the pipeline:

### Spec Quality Gate (runs after AddOrClarifyScenario)

| Rule | What it blocks |
|------|---------------|
| `NO_PLACEHOLDER_STEPS` | `Given a test scenario`, `Then no panic occurs`, `Then the system intentionally fails` |
| `DOMAIN_NOUN_REQUIRED` | Scenarios whose name + steps share no significant word with the parent Rule header |
| `NO_ORPHAN_STUBS` | Stub markers (`<stub>`, `<placeholder>`, `<todo>`) outside `_orphan_stubs.feature` |

If any rule fires the mission is marked NO_PROGRESS and the violation details are injected into the next mission context.

### Regression Rollback

If a mission increases the total adjusted issue count, Tesaki rolls back spec-repo changes via `git checkout -- .` and skips that mission type for the remainder of the session. A hard stop is triggered when adjusted regression exceeds 5 issues.

### Evidence-Driven Selection

A mission type is only selected when its corresponding packet evidence exists:

| Mission type | Required evidence |
|---|---|
| NormalizeIdentityTags | `structure_issues` with MissingIdentityTag kind |
| CreateMissingBindings | `binding_issues` with a concrete `scenario_key` |
| AddOrClarifyScenario | `spec_issues` > 0 or rules with zero scenarios |
| FixRegressionFromGateFailure | `sut_issues` > 0 |

Missions with no concrete target are skipped.

### Failure Feedback

Gate errors from the previous mission are captured in `.tesaki/last_failure.json` and injected into the next mission's MISSION.md under "Previous attempt failed". This prevents repeated blind retries.

*This runbook reflects current tool behavior. Update alongside tool changes.*
