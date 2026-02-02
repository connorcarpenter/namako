# Optimization Analysis: Token Economy, Quality, and Safety Rails

**Date:** 2025-01-27
**Context:** Meta-reflection on Tesaki/Namako system design

---

## The Research vs. What We Did

### Research Said:
> "Don't overengineer. Trust the runner to read files and figure out patterns."
> "Simple, focused loops beat complex orchestration."

### What I Did:
Added 121 lines of context to `create_missing_bindings.md.j2` including:
- Binding pattern examples (Given/When/Then templates)
- Context type reference (`TestWorldMut` vs `TestWorldRef`)
- Step-by-step instructions
- Validation criteria
- Important notes

### The Tension:
Is this "spoon-feeding" or "necessary context"?

---

## Analysis: Where Token Waste Occurs

### 1. **Static Boilerplate** (WASTEFUL)
The binding pattern examples I added are:
- Always the same (~40 lines of Rust code)
- Available in the codebase already
- Burnable tokens on every mission

**Cost:** ~500 tokens per mission, repeated every time.

### 2. **Dynamic Mission Context** (VALUABLE)
The list of missing steps:
- Different for each mission
- Required for the runner to know WHAT to create
- Not available elsewhere in a clean format

**Cost:** ~20 tokens per step × N steps. Worth it.

### 3. **Instructions** (QUESTIONABLE)
"Find an existing file that matches the feature domain..."
- Smart runners figure this out
- Dumb runners need guidance
- Middle ground: Link to a reference file?

---

## Token Optimization Opportunities

### Opportunity 1: Remove Static Examples
**Current:** Include full binding patterns in every mission
**Better:** One line: "See `test/tests/src/steps/connection.rs` for binding patterns"
**Savings:** ~400 tokens per mission

### Opportunity 2: Minimal Brief Mode
Create two modes:
- **Rich mode** (first attempt): Full context, ~1000 tokens
- **Minimal mode** (retries): Just the step list, ~100 tokens

```jinja
{% if attempt == 1 %}
{{ full_instructions }}
{% else %}
Previous attempt failed. Missing steps: {{ steps }}
{% endif %}
```

### Opportunity 3: Trust the INPUTS Folder
The mission bundle has an `INPUTS/` folder with:
- `lint_errors.json` (exact step texts)
- `gate.json` (full gate output)
- `status.json` (full status)

The runner can read these. We don't need to duplicate in MISSION.md.

**Current MISSION.md:** 83 lines
**Minimal MISSION.md:** Could be 15 lines:
```markdown
# Mission 038
Type: CreateMissingBindings
Target: messaging_channel_semantics
Surfaces: Spec LOCKED, Tests UNLOCKED, SUT LOCKED

## Objective
Create bindings for missing steps. See INPUTS/lint_errors.json for the list.

## Verify
namako gate --specs-dir test/specs
```

---

## Quality Assurance Mechanisms

### What We Have ✅
1. **Gate validation** — Every mission ends with `namako gate`
2. **Progress detection** — `has_progress()` checks mission-specific metrics
3. **Stall detection** — 3 consecutive no-progress = stop

### What We're Missing ⚠️
1. **Compilation check before gate** — Gate can fail on lint if code doesn't compile
2. **Regression threshold** — No hard stop if issues increase significantly
3. **Budget enforcement** — Max files changed is in context but not enforced

### Recommended Additions:
```rust
// In repl.rs, after runner completes:
if total_delta > 10 {
    println!("⛔ Significant regression (+{} issues) - stopping", total_delta);
    break;
}
```

---

## Safety Rails Against Runaway Train

### Current Rails ✅
1. **Max iterations** — `--loop 10` caps total missions
2. **Stall detection** — 3× no-progress = stop
3. **Surface locks** — Can't edit spec during SUT implementation

### Missing Rails ⚠️

| Risk | Current Mitigation | Recommended |
|------|-------------------|-------------|
| Infinite loop | Max iterations | ✅ Already good |
| Runaway regression | None | Add regression threshold |
| Token burn | None | Add attempt limit per mission |
| Time burn | Max runtime per mission | ✅ Already good |
| Edit violations | Surface policy | Enforce in runner? |

### Recommended: Hard Regression Stop
```rust
const MAX_REGRESSION_TOLERATED: i32 = 5;

if total_delta > MAX_REGRESSION_TOLERATED {
    eprintln!("🛑 EMERGENCY STOP: Regression of +{} issues exceeds threshold", total_delta);
    eprintln!("   Last mission made things worse. Human review required.");
    break;
}
```

### Recommended: Attempt Limit Per Mission Type
Don't retry the same mission type more than 2× in a row:
```rust
if same_mission_type_consecutive >= 2 && !made_progress {
    skip_mission_type_for_session(&mission_type);
    continue;
}
```

---

## The Minimal Loop Philosophy

The research is right. The ideal loop is:

```
while has_work() && under_budget():
    state = compute()
    mission = select(state)
    execute(mission)  # Runner reads codebase itself
    if regressed(): rollback()
    if stalled(): skip_type()
```

The runner (Copilot/Claude) has:
- Full codebase access
- Ability to grep/glob/read files
- Understanding of patterns from context

**We should provide:**
- WHAT to do (mission type, target)
- WHERE to edit (surface locks)
- HOW to verify (gate command)

**We should NOT provide:**
- Example code (it can read existing files)
- Detailed patterns (it can infer from codebase)
- Step-by-step hand-holding (it's a coding agent)

---

## Concrete Recommendations

### 1. Slim Down Templates (Token Savings)
Reduce `create_missing_bindings.md.j2` from 121 lines to ~30:
```markdown
# CreateMissingBindings
Target: {{ scenario_key }}
Surfaces: Tests UNLOCKED, others LOCKED

## Missing Steps
{% for step in missing_steps %}
- `{{ step }}`
{% endfor %}

## Verify
namako gate --specs-dir test/specs
```

The runner will figure out the rest.

### 2. Add Regression Threshold (Safety)
Stop if issues increase by more than 5 in one mission.

### 3. Add Consecutive Failure Skip (Efficiency)
Skip a mission type if it fails 2× in a row.

### 4. Trust the Runner (Philosophy)
The research is clear: simple loops win. Let the runner be smart.

---

## Summary

| Dimension | Current | Recommended | Impact |
|-----------|---------|-------------|--------|
| Token usage | ~1500/mission | ~400/mission | 73% reduction |
| Quality | Gate validation | + regression threshold | Safer |
| Safety | Max iterations + stalls | + regression stop + type skip | More robust |
| Complexity | Medium | Lower | Easier to maintain |

The key insight: **Trust the agent, verify the result.**

---

## Actual Token Usage (Last Run)

**Model:** claude-opus-4.5 (via GitHub Copilot CLI)

| Mission | Type | Tokens In | Tokens Out | Cached | Time |
|---------|------|-----------|------------|--------|------|
| 037 | CreateMissingBindings | 4.8M | 21.8k | 4.6M | 7m 42s |
| 038 | FixRegressionFromGateFailure | 1.0M | 6.0k | 935.5k | 2m 35s |

### Analysis

**4.8M input tokens** for CreateMissingBindings is enormous. This includes:
- Full codebase context (files the agent read)
- MISSION.md + POLICY.md (~3k tokens)
- All the grep/glob/read operations

**The caching is effective** - 4.6M cached means repeat patterns aren't re-computed.

**But 4.8M is still expensive.** At Opus rates, that's ~$72 per mission just for input.

### Where Are Tokens Going?

The agent reads a LOT of files:
```
Read client/src/client.rs lines 345-365
Read test/specs/features/03_messaging.feature (185 lines)
Read test/tests/src/steps/messaging.rs lines 70-100
... etc
```

This is the agent exploring the codebase. We can't easily reduce this - it's the agent being thorough.

### What We CAN Control

1. **MISSION.md size** (~1500 tokens currently, could be ~300)
2. **Retry context** (don't re-read everything on retry)
3. **Model selection** (Opus vs Sonnet vs Haiku)

### Model Recommendation

| Task Type | Recommended Model | Rationale |
|-----------|------------------|-----------|
| CreateMissingBindings | Sonnet | Repetitive pattern matching |
| ImplementBehaviorForScenario | Opus | Complex reasoning needed |
| FixRegressionFromGateFailure | Opus | Debugging requires deep thought |
| AddOrClarifyScenario | Sonnet | Spec writing is structured |

### Estimated Savings with Model Tiering

| Current (All Opus) | With Tiering | Savings |
|--------------------|--------------|---------|
| ~$75/mission avg | ~$25/mission avg | 67% |

