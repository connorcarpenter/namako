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

## Flywheel Features (v2.0)

### Failure Memory

When a mission fails due to policy violation:
- Violated files and surfaces are captured
- Next mission sees `⚠️ Previous Mission Failed` section
- Clear guidance on what NOT to repeat

**No configuration needed** - works automatically.

### Persistent Lessons

Cross-session learning via `.tesaki/lessons.json`:
- Tracks: failure modes, approaches tried, what blocked progress
- Auto-injected into missions targeting same issues
- Marked resolved when issue is fixed

**Configuration:**
```toml
enable_lessons = true  # Default: true
```

### Cost Tracking

Session summaries now include:
- Estimated cost in USD
- Cost per issue resolved
- Efficiency rating (Excellent/Good/Poor/Critical)
- Warnings for poor efficiency

**Configuration:**
```toml
enable_cost_tracking = true          # Default: true
cost_alert_threshold_usd = 20.0      # Default: 20.0
```

### Intelligent Escalation

When the loop stalls, Tesaki provides actionable options:
- Detect why: policy blocking, repeated failure, no progress
- Suggest actions: unlock surface, skip issue, provide hint
- Display numbered choices

**Configuration:**
```toml
max_consecutive_failures = 2  # Default: 2
```

### Stall Diagnosis Reports

On stop, generates `.tesaki/last_stall_diagnosis.md` with:
- What Happened (attempts, approaches tried)
- Why It Stalled (blocking factors)
- What To Try (actionable recommendations)

**Always enabled** - saved automatically on non-success exits.

---

## Interpreting Escalation Prompts

If you see an escalation message like:
```
🚧 HUMAN INTERVENTION REQUIRED

Situation: Agent blocked by surface policy
Blocker: spec surface is locked

Options:
1. Unlock spec surface
2. Skip this issue
```

**How to respond:**

1. **Option 1: Unlock surface**
   - Edit `.tesaki/config.toml`, add/modify surfaces config
   - Or run with flag: `tesaki --loop 5 --unlock-spec`

2. **Option 2: Skip issue**
   - Note the issue key (e.g., `feature:auth:login`)
   - Add to skip list or mark as manual-only

3. **Option 3: Provide hint** (if applicable)
   - Create `.tesaki/hints.md` with context
   - Or modify spec to be clearer

**When to unlock a surface:**
- The fix genuinely requires editing that surface
- You're confident the change is safe
- Temporary unlock for specific issue is acceptable

**When to skip:**
- Known-hard problem requiring human expertise
- Requires external system changes
- Out of scope for current work

---

## Debugging

```bash
# Check mission details
tesaki diagnose M-abc123

# Review lessons database
cat .tesaki/lessons.json | jq

# Read last stall diagnosis
cat .tesaki/last_stall_diagnosis.md

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

## New Configuration Options (v2.0)

```toml
# Flywheel features
enable_failure_memory = true         # Default: true
enable_lessons = true                 # Default: true
enable_cost_tracking = true           # Default: true
cost_alert_threshold_usd = 20.0      # Alert if >$20 with no progress
max_consecutive_failures = 2         # Escalate after N failures
```

---

*Update this runbook alongside tool changes.*
