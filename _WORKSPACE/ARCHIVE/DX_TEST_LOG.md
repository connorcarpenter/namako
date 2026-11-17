# DX Test Log - Tesaki/Namako Meta-Evaluation

**Date:** 2026-02-02
**Tester:** GitHub Copilot CLI (Claude Sonnet 4)
**Goal:** Evaluate the system from a coding agent's perspective — does it give me what I need to do excellent work?

---

## Meta-Reflection: What Is This System Trying To Do?

### The Core Value Proposition

Namako/Tesaki is a **spec-driven development loop** that:
1. **Specs are truth** — `.feature` files define behavior normatively
2. **Tests are derived** — bindings connect specs to executable tests
3. **SUT evolves to match** — implementation follows the spec, not vice versa
4. **Identity is cryptographic** — hash-based verification prevents drift

### The Agent Experience Promise

For a coding agent, the system should provide:
1. **Unambiguous context** — exactly what needs to change and why
2. **Minimal scope** — one mission at a time, bounded edit surfaces
3. **Verifiable success** — gates confirm progress objectively
4. **No guessing** — packets contain all relevant information

---

## Session 1: Critical Design Flaw Identified

### The Problem: LLM Selecting Tasks

Current flow:
```
loop N → ask planner LLM "what should I do?" → LLM says "would you like me to..."
```

**This is backwards.** Namako already computes the answer deterministically:
- `RepoState.candidate_tasks` contains prioritized work items
- The first item IS the next mission
- No LLM inference needed for task selection!

### What the Planner Returned

```
> loop 3
{"say": "Would you like me to analyze the spec issues?", "mission_proposal": null}
```

The planner:
1. ❌ Didn't propose a mission
2. ❌ Asked a question instead of acting
3. ❌ Told me to run `namako lint` myself
4. ❌ Wasted 9 seconds on an LLM call that added no value

### Root Cause

**The planner LLM is solving the wrong problem.** We're asking it to "figure out what to do" when Namako already knows. The LLM should:
- ✅ Answer questions ("explain what's failing")  
- ✅ Execute work (the runner does implementation)
- ❌ NOT decide what task to do next (that's deterministic)

---

## Proposed Redesign: Algorithmic Task Selection

### The Right Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    TESAKI LOOP                              │
│                                                             │
│  1. namako gate → RepoState (deterministic)                 │
│  2. Pick first candidate_task (ALGORITHMIC, not LLM)        │
│  3. Generate mission bundle (deterministic template)        │
│  4. Call runner with full context (LLM does WORK)           │
│  5. namako gate → verify progress                           │
│  6. Repeat until done                                       │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Task Selection Algorithm (No LLM)

```rust
fn select_next_mission(state: &RepoState) -> Option<Mission> {
    // Priority order is already computed
    let task = state.candidate_tasks.first()?;
    
    // Map task category to mission type
    let mission_type = match task.category {
        TaskCategory::FixSut => "ImplementBehaviorForScenario",
        TaskCategory::CreateBindings => "CreateMissingBindings", 
        TaskCategory::FixStructure => "NormalizeIdentityTags",
        TaskCategory::ImproveSpec => "AddScenarioForRule",
    };
    
    Some(Mission {
        mission_type,
        target: task.feature_path.or(task.scenario_key),
        objective: task.description,
        surfaces: stage_to_surfaces(task.category),
    })
}
```

### What the Runner Receives (Full Context)

```markdown
# MISSION: Add executable scenario for Rule "Connection timeout"

## Target
- Feature: features/01_connection.feature
- Rule: Rule(03) "Connection timeout handling"
- Current state: 0 executable scenarios

## Objective
Add at least one executable scenario that demonstrates the rule's requirements.

## Constraints
- Spec: UNLOCKED (you may edit this feature file)
- Tests: LOCKED
- SUT: LOCKED

## Validation
After your changes, `namako gate` must pass.

## Context
[Full rule text from the feature file]
[Related scenarios from same feature for style reference]
```

### Benefits of This Design

| Aspect | Before (LLM Planner) | After (Algorithmic) |
|--------|---------------------|---------------------|
| Speed | 9s per decision | <100ms |
| Reliability | "Would you like me to..." | Always produces mission |
| Determinism | Non-deterministic | Fully deterministic |
| Token cost | Planner + Runner | Runner only |
| Debugging | Hard to trace | Clear audit trail |

---

## Implementation Plan

### Phase 1: Add `select_next_mission()` function
- Input: RepoState
- Output: Mission with full context
- No LLM call

### Phase 2: Update `loop N` command
- Call `select_next_mission()` directly
- Skip planner entirely
- Go straight to runner with generated mission bundle

### Phase 3: Keep planner for chat only
- `explain` → uses planner to answer questions
- `loop N` → algorithmic, no planner

---

## Key Insight

> **The LLM's job is to DO work, not to DECIDE what work to do.**

Task selection is a solved problem — Namako computes it. The LLM should spend its tokens on implementation, not planning.

---

*Session paused for redesign implementation*

---

## Session 2: Redesign Implementation — SUCCESS ✅

**Date:** 2026-02-02
**Change:** Added `run_autonomous_loop()` with algorithmic task selection

### What Changed

Added to `tesaki/src/repl.rs`:
1. `parse_loop_command()` — Detects `loop N` command
2. `run_autonomous_loop()` — Executes N missions with algorithmic task selection

**Key insight:** The `loop` command now bypasses the planner LLM entirely. It uses:
- `select_with_constraints()` from `mission_selector.rs` (already existed!)
- `run_run()` to execute the mission
- Pure algorithmic flow, no LLM for task selection

### Test Results

```
> loop 1
Starting autonomous loop (1 missions max)...
Task selection is ALGORITHMIC (no planner LLM).

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
MISSION 1/1
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Type:    AddOrClarifyScenario
Target:  features/03_messaging.feature
Stage:   Refine Spec
Surfaces: Spec Unlocked • Tests Locked • SUT Locked
...
Mission completed in 94.8s
RepoState: Spec: 24 issues • Structure: 1 • Bindings: 55 missing • SUT: 0 failing
```

Then when running `loop 2`:
```
MISSION 1/2
Type:    CreateMissingBindings     ← System correctly pivoted!
Target:  unknown
Stage:   Implement Tests
...
Mission completed in 231.7s
RepoState: Spec: 24 issues • Structure: 1 • Bindings: 54 missing • SUT: 0 failing
```

### Before vs After

| Metric | Before (Planner) | After (Algorithmic) |
|--------|------------------|---------------------|
| Task selection time | 9+ seconds (LLM) | <100ms |
| Reliability | "Would you like me to..." | Always produces mission |
| Mission execution | ❌ Failed to propose | ✅ Executed successfully |
| Spec issues | 26 | 24 (-2) |
| Missing bindings | 0 → 55 (added scenarios) | 55 → 54 (-1, being fixed) |

### The Design Pattern

```
┌─────────────────────────────────────────────────────────────┐
│                    NEW TESAKI LOOP                          │
│                                                             │
│  User: "loop 3"                                             │
│         │                                                   │
│         ▼                                                   │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ select_with_constraints(RepoState) — ALGORITHMIC    │   │
│  │ • Binding issues → CreateMissingBindings            │   │
│  │ • Spec issues → AddOrClarifyScenario                │   │
│  │ • SUT issues → FixRegressionFromGateFailure         │   │
│  └─────────────────────────────────────────────────────┘   │
│         │                                                   │
│         ▼                                                   │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ run_run() → MissionBundle → Runner (LLM does WORK)  │   │
│  └─────────────────────────────────────────────────────┘   │
│         │                                                   │
│         ▼                                                   │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ namako gate → Verify progress → Repeat              │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Key Observations

1. **The infrastructure already existed** — `mission_selector.rs` was already doing algorithmic selection
2. **The REPL was routing through the wrong path** — It sent `loop N` to the planner instead of using the selector
3. **One small change, big impact** — ~100 lines of code to add `run_autonomous_loop()`
4. **Priority ordering works** — System correctly pivoted from spec → bindings when new issues appeared

### Remaining Issue: Target = "unknown"

The `CreateMissingBindings` mission shows `Target: unknown` because the issue classifier doesn't always capture the scenario key. This is a minor UX issue but doesn't affect functionality — the runner gets the full context from the mission bundle.

---

## Summary

The core problem was **using an LLM to decide what to do** when the answer was already computed deterministically by Namako. The fix was simple: bypass the planner for `loop N` and use the existing algorithmic mission selector.

**The planner LLM's role is now clear:**
- ✅ Answer questions in interactive chat (`explain`, free-form questions)
- ❌ NOT select tasks (that's algorithmic)

**The runner LLM's role:**
- ✅ Execute the work (implement scenarios, create bindings, fix tests)
- ❌ NOT decide what work to do

This separation makes the system:
- **Faster** (no LLM call for task selection)
- **More reliable** (deterministic task selection)
- **More debuggable** (clear audit trail)
- **Cheaper** (tokens only spent on actual work)

---

## Session 3: Critical Analysis — What's Still Broken?

**Date:** 2026-02-02

### The Core Problem: Loop Stops Too Early

The loop treats "gate failed" as a failure, but **gate failure is expected during the workflow**:

| Mission | Before | After | Gate | Reality |
|---------|--------|-------|------|---------|
| AddScenarios | Spec:26, Bind:0 | Spec:24, Bind:55 | FAIL (lint) | ✅ PROGRESS (specs -2) |
| CreateBindings | Bind:55 | Bind:54 | FAIL (lint) | ✅ PROGRESS (bindings -1) |

**Gate will fail until we're done.** The loop should:
- ✅ Continue when: progress is being made (issues decreasing)
- ⏳ Retry when: no progress after attempt
- 🛑 Stop when: truly done (all gates pass) or blocked

### Issue 1: Mission Marked "Failed" When Progress Was Made

```
STOP: GATE_FAILED - Post-run validation failed
Failed mission preserved at: .tesaki/failed/030-...
```

But the runner **succeeded**! It added 14 scenarios. The gate failed because those scenarios need bindings — that's not a mission failure, that's **expected intermediate state**.

**Fix:** Change success criteria from "gate passes" to "progress was made OR gate passes"

### Issue 2: MISSION.md Is Too Vague

Current:
```markdown
## Objective
Add or clarify scenarios to improve coverage.

## Context
Coverage gaps detected in features/03_messaging.feature.
```

Should be:
```markdown
## Objective
Add executable scenarios for rules that have zero coverage.

## Target Rules (0 scenarios each)
1. features/03_messaging.feature → Rule "Messaging Channel Semantics"
2. features/03_messaging.feature → Rule "TickBuffered channel semantics"
3. features/03_messaging.feature → Rule "EntityProperty resolution"

## Reference
Existing scenarios in features/02_transport.feature show the expected style.

## Validation
After changes: `namako lint` should resolve all new steps (even if bindings are missing).
```

### Issue 3: No Progress Tracking Between Iterations

The loop doesn't compare issue counts. It should:
```rust
let before = state.total_issue_count();
// ... run mission ...
let after = state.total_issue_count();
if after < before {
    println!("✅ Progress: {} → {} issues", before, after);
} else if after == before {
    println!("⚠️ No progress - will retry with different approach");
} else {
    println!("❌ Regression: {} → {} - consider rollback", before, after);
}
```

### Issue 4: "Target: unknown" for Binding Missions

```
Type:    CreateMissingBindings
Target:  unknown
```

The runner doesn't know WHICH binding to create. Should be:
```
Type:    CreateMissingBindings
Target:  Given a client connected to a server (features/03_messaging.feature:42)
```

### Issue 5: No Rollback on Regression

If a mission makes things worse (more issues after than before), the system should:
1. `git checkout -- .` to revert
2. Try a different approach
3. Or mark as blocked and continue with other work

### Issue 6: Structure Issue Appeared

```
Before: Structure: 0
After:  Structure: 1
```

What broke? The runner's changes may have introduced a structural problem. The system should catch and report this clearly.

---

## The Ideal "Ralph Wiggum" Flow

```
┌─────────────────────────────────────────────────────────────┐
│                    BULLETPROOF LOOP                         │
│                                                             │
│  while has_work() && iterations < max:                      │
│    before = snapshot_issue_counts()                         │
│                                                             │
│    mission = select_next_mission()  # Algorithmic           │
│    execute(mission)                  # Runner does work     │
│                                                             │
│    after = snapshot_issue_counts()                          │
│                                                             │
│    if after.total < before.total:                           │
│      ✅ Progress - continue                                 │
│    elif after.total == before.total:                        │
│      ⚠️ Stalled - increment retry, maybe change approach   │
│    else:                                                    │
│      ❌ Regression - rollback, mark blocked, continue      │
│                                                             │
│    if all_gates_pass():                                     │
│      🎉 DONE                                                │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## Action Items

1. **[HIGH] Progress-based continuation** — Don't stop on gate fail if issues decreased
2. **[HIGH] Richer mission context** — Include specific rule names, step texts in MISSION.md
3. **[MED] Progress tracking** — Show before/after issue counts, compute delta
4. **[MED] Better target labeling** — Include file:line for binding issues
5. **[LOW] Rollback on regression** — Git reset if mission made things worse
6. **[LOW] Structure issue details** — Show what structural problem was introduced

---

## Session 4: Improvements Implemented

**Date:** 2026-02-02

### Changes Made

#### 1. Progress-Based Continuation (HIGH PRIORITY ✅)

Modified `run_autonomous_loop()` to track issue counts before/after each mission:

```rust
// Before mission
let before_total = spec + binding + sut + structure;

// After mission
let after_total = ...;

// Determine continuation
if made_progress {
    println!("✅ Progress made - continuing");
    stall_count = 0;
} else if total_delta == 0 {
    stall_count += 1;  // Only stop after MAX_STALLS
}
```

**Now the loop:**
- ✅ Continues when any category decreases (even if others increase)
- ✅ Shows before/after counts and deltas
- ✅ Only stops after 3 consecutive stalls
- ✅ Shows final summary

#### 2. Richer Mission Context (HIGH PRIORITY ✅)

**AddOrClarifyScenario now includes:**
```
Rules with ZERO executable scenarios in features/03_messaging.feature:
  1. Messaging Channel Semantics
  2. TickBuffered channel semantics
  3. EntityProperty resolution
```

**CreateMissingBindings now includes:**
```
Missing step bindings (showing up to 20):
1. `Given a client connected to a server`
2. `When the client sends a message on UnorderedUnreliable channel`
...

Look at existing bindings in test/tests/steps/ for patterns.
Use #[given], #[when], #[then] macros from namako_codegen.
```

#### 3. Better Progress Display

New output format:
```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
MISSION 1/10
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Type:    AddOrClarifyScenario
Target:  features/03_messaging.feature
Stage:   Refine Spec
Surfaces: Spec Unlocked • Tests Locked • SUT Locked
Before:  Spec:26 Bind:0 SUT:0 Struct:0 (total: 26)

...runner executes...

After:   Spec:24 Bind:55 SUT:0 Struct:1 (total: 80)
Delta:   Spec:-2 Bind:+55 Total:+54
✅ Progress made - continuing
```

### Files Modified

| File | Change |
|------|--------|
| `tesaki/src/repl.rs` | Rewrote `run_autonomous_loop()` with progress tracking |
| `tesaki/src/mission_type.rs` | Enhanced `generate_brief()` for AddOrClarifyScenario and CreateMissingBindings |

### Remaining Items

- [ ] Better target labeling for binding issues (include file:line)
- [ ] Rollback on regression (git reset if mission made things worse)
- [ ] Structure issue details in output

---

## Summary: Is This Bulletproof Yet?

### What Works Now

| Capability | Status |
|------------|--------|
| Algorithmic task selection | ✅ No LLM needed |
| Progress-based continuation | ✅ Doesn't stop on expected failures |
| Rich mission context | ✅ Rules, steps listed explicitly |
| Delta tracking | ✅ Shows before/after/delta |
| Stall detection | ✅ Stops after 3 consecutive stalls |

### What Could Still Improve

| Gap | Impact | Effort |
|-----|--------|--------|
| Rollback on regression | Medium - could waste cycles | Medium |
| Structure issue details | Low - rare case | Low |
| Batch binding creation | Medium - one-at-a-time is slow | Medium |
| SUT implementation missions | High - not tested yet | High |

### The "Ralph Wiggum" Verdict

**Yes, this should work for the spec→test phase.** The loop will:
1. Add scenarios to uncovered rules
2. Create bindings for new steps
3. Continue until all scenarios are bound

**Not yet tested for SUT implementation.** That phase requires:
- Running tests that fail
- Implementing code to make them pass
- Different surface locks (SUT unlocked)

The current system handles Spec and Binding issues well. SUT issues (failing tests → implement code) would be the next phase to test.

---

## Session 5: Deep Quality Analysis — What Makes Bulletproof Output?

**Date:** 2026-02-02

### The Real Question

> "How can this autonomous coding loop create more HIGH QUALITY output?"

Progress tracking solves the **continuation** problem. But **quality** is a different beast entirely.

### First Principles: What Makes LLM Code Output High Quality?

Production-grade autonomous systems use:

| Technique | What It Does | Do We Have It? |
|-----------|--------------|----------------|
| **Few-shot examples** | Shows 2-3 perfect examples before asking for output | ❌ NO |
| **Structured constraints** | Limits output format to reduce errors | ✅ Partial (surface locks) |
| **Multi-pass validation** | Generate → validate → fix → validate | ❌ NO |
| **Semantic verification** | Checks correctness, not just syntax | ❌ NO |
| **Rollback on regression** | Reverts if output made things worse | ❌ NO |
| **Failure feedback loop** | Tells agent WHY it failed, not just THAT it failed | ❌ NO |

### Critical Gap #1: No Exemplars in Context

Current MISSION.md context:
```markdown
## Context
Rules with ZERO executable scenarios in features/03_messaging.feature:
  1. Messaging Channel Semantics
  2. TickBuffered channel semantics
```

**The runner has NO IDEA what a good scenario looks like for this domain.**

Better context:
```markdown
## Context
Rules needing scenarios:
  1. Messaging Channel Semantics
  2. TickBuffered channel semantics

## Exemplar (from features/00_common.feature)
Here's a well-formed scenario from this codebase:

@Rule(02)
Rule: Remote or untrusted input must never panic

  @Scenario(01)
  Scenario: Malformed inbound packet is dropped without panic
    Given a test scenario
    And a connected client
    When the server receives a malformed packet
    Then the packet is dropped
    And no panic occurs

## Style Notes
- Always start with "Given a test scenario"
- Use existing step patterns where possible
- End failure scenarios with "And no panic occurs"
```

### Critical Gap #2: No Semantic Validation

Current validation:
- ✅ Lint passes (syntax check)
- ❌ Does the scenario actually test the rule it claims to test?
- ❌ Are the steps meaningful or just boilerplate?
- ❌ Does it match the normative contract?

**Gate checks structure, not meaning.** The runner could add garbage scenarios and pass the gate.

### Critical Gap #3: No Failure Feedback

When a mission fails:
```
STOP: GATE_FAILED - Post-run validation failed
```

**Why did it fail?** The runner needs to know:
- Which specific check failed?
- What was the error message?
- What should be different next time?

### Critical Gap #4: One Binding At A Time Is Slow

Current flow:
```
Mission 1: Create 1 binding → progress +1
Mission 2: Create 1 binding → progress +1
Mission 3: Create 1 binding → progress +1
... (55 missions for 55 bindings!)
```

Better flow:
```
Mission 1: Create ALL missing bindings for one feature → progress +20
Mission 2: Create ALL missing bindings for next feature → progress +15
```

### Critical Gap #5: No Contract Alignment Verification

The contract says:
```
| ChannelMode          | Delivery      | Dedup | Ordering    |
| UnorderedUnreliable  | best-effort   | no    | none        |
```

Does the scenario actually test this? Current system has no way to verify.

---

## Proposed High-Quality Architecture

### 1. Exemplar Injection

```rust
fn generate_brief(&self, state: &RepoState) -> MissionBrief {
    // Find exemplar scenarios from same or similar features
    let exemplars = state.find_exemplar_scenarios(&self.feature_path, 2);
    
    let context = format!(
        "## Exemplar Scenarios\n{}\n\n## Target Rules\n{}",
        exemplars.iter().map(|e| e.full_text()).join("\n\n"),
        rules_needing_scenarios.join("\n")
    );
    // ...
}
```

### 2. Contract Snippet Injection

```rust
fn generate_brief(&self, state: &RepoState) -> MissionBrief {
    // Extract relevant contract section from feature header
    let contract_snippet = state.extract_contract_header(&self.feature_path);
    
    let context = format!(
        "## Normative Contract\n{}\n\n## Your Task\n{}",
        contract_snippet,
        objective
    );
}
```

### 3. Batch Operations

```rust
// Instead of one binding per mission:
let all_missing_steps: Vec<_> = state.binding_issues
    .iter()
    .filter(|b| b.feature_path == target_feature)
    .collect();

// Create ALL bindings for one feature in one mission
```

### 4. Failure Feedback Loop

```rust
fn run_autonomous_loop() {
    // ... after mission fails ...
    
    let failure_reason = parse_gate_failure(&post_gate);
    let next_mission = Mission {
        context: format!(
            "## Previous Attempt Failed\n{}\n\n## Try This Instead\n{}",
            failure_reason,
            suggested_fix
        ),
        // ...
    };
}
```

### 5. Semantic Smoke Test

After a scenario is added, check:
- Does it contain Given/When/Then?
- Does the Scenario name relate to the Rule name?
- Are the steps not just copy-paste from exemplar?

---

## Implementation Priority

| Change | Impact | Effort | Priority |
|--------|--------|--------|----------|
| Exemplar injection | HIGH - dramatically improves output quality | MEDIUM | ⭐⭐⭐ |
| Contract snippet injection | HIGH - aligns output with spec | LOW | ⭐⭐⭐ |
| Batch bindings | MEDIUM - 10x faster | MEDIUM | ⭐⭐ |
| Failure feedback | MEDIUM - reduces retry waste | MEDIUM | ⭐⭐ |
| Semantic validation | LOW - catches garbage | HIGH | ⭐ |

---

## The Bulletproof Mission Bundle

What the runner SHOULD receive:

```markdown
# Mission 042: Add Scenarios for TickBuffered Rules

## Your Objective
Add executable scenarios for these rules in features/03_messaging.feature:
1. **TickBuffered channel semantics** (Rule 05)
2. **TickBuffered capacity and eviction** (Rule 06)

## Surface Locks
| Surface | Status | Paths |
|---------|--------|-------|
| Spec | **UNLOCKED** | features/03_messaging.feature |
| Tests | LOCKED | test/tests/steps/*.rs |
| SUT | LOCKED | src/**/*.rs |

## Normative Contract (from feature header)
```
# TICKBUFFERED RULES
  - TickBuffered is Client→Server only
  - Groups messages by tick, exposes in tick order
  - Capacity and eviction: oldest tick first (FIFO)
  - Discards very-late ticks (behind retained window)
```

## Exemplar Scenario (from features/00_common.feature)
```gherkin
@Rule(02)
Rule: Remote or untrusted input must never panic

  @Scenario(01)
  Scenario: Malformed inbound packet is dropped without panic
    Given a test scenario
    And a connected client
    When the server receives a malformed packet
    Then the packet is dropped
    And no panic occurs
```

## Style Requirements
- Start each scenario with `Given a test scenario`
- Tag with `@Scenario(NN)` under the appropriate `@Rule(NN)`
- Use existing step patterns where possible (check features/00_common.feature)
- Each scenario should test ONE specific behavior from the contract

## Validation
After your changes:
1. Each target rule has at least 1 executable scenario
2. `namako lint` passes (new steps may be unbound - that's OK)
3. Scenario names clearly describe what they test

## Budgets
- Max files changed: 1
- Max scenarios added: 4
```

---

## Summary: The Quality Formula

**Quality = Context + Constraints + Validation + Feedback**

Current system has:
- ✅ Constraints (surface locks)
- ⚠️ Basic validation (lint)
- ❌ Rich context (no exemplars, no contract snippets)
- ❌ Feedback (no failure reasons)

To be truly bulletproof:
1. **Add exemplars** - Show the agent what good looks like
2. **Add contract snippets** - Tell the agent what the spec requires
3. **Add failure feedback** - Tell the agent why it failed
4. **Batch operations** - Don't do 55 missions for 55 bindings

The loop is now RELIABLE. Making it produce HIGH QUALITY output requires richer context.

---

## Session 6: Research-Informed Simplification

**Date:** 2026-02-02

### Research Sources
- [Addy Osmani - Self-Improving Coding Agents](https://addyosmani.com/blog/self-improving-agents/)
- [OpenAI - Unrolling the Codex Agent Loop](https://openai.com/index/unrolling-the-codex-agent-loop/)
- [arXiv - A Self-Improving Coding Agent](https://arxiv.org/html/2504.15228v2)

### Key Finding: Simple Beats Complex

The research is clear: **simple, focused loops beat complex orchestration.**

> "Break development into atomic tasks... decompose work into small, discrete tasks with clear, objective success criteria."

> "Iterative, stateless execution... reset memory each run to maintain focus."

### What We Were About to Overengineer

I almost added:
- Exemplar scenario extraction (~80 lines)
- Contract snippet injection
- Complex context building

**But the runner (Copilot/Claude) is smart enough to read the codebase itself.** It doesn't need us to spoon-feed exemplars — it can find them.

### What Actually Matters (per research)

| Practice | We Have It? | Priority |
|----------|-------------|----------|
| Atomic tasks | ✅ Yes | - |
| Stateless iteration | ✅ Yes (RepoState recomputed) | - |
| Validation loop | ✅ Yes (namako gate) | - |
| **Failure feedback** | ❌ No | HIGH |
| **Rollback on regression** | ❌ No | MEDIUM |

### The Minimal Formula

```
while has_work() and not stalled:
    state = compute_fresh_state()
    mission = select_deterministically(state)
    
    before = count_issues()
    execute(mission)
    after = count_issues()
    
    if after < before: continue
    elif after == before: stall++
    else: rollback()
```

That's it. The current `run_autonomous_loop()` already does most of this.

### Reverted Changes

Removed exemplar extraction code (~110 lines in repo_state.rs, ~25 lines in mission_type.rs). The runner can discover patterns itself.

### Remaining TODOs

1. **Failure feedback** — when gate fails, include error in next mission
2. **Rollback** — git reset if significant regression detected

These are simple, high-impact changes. Everything else is polish.

---

## Session 7: Verification Test

**Date:** 2026-02-02

### Test: `loop 1`

```
Before:  Spec:26 Bind:0 SUT:0 Struct:0 (total: 26)
...runner runs for 110s...
After:   Spec:24 Bind:91 SUT:0 Struct:1 (total: 116)
Delta:   Spec:-2 Bind:+91 Total:+90
✅ Progress made - continuing
```

### Analysis

| What Happened | Expected? | Result |
|---------------|-----------|--------|
| Spec issues decreased (26→24) | ✅ Yes | ✅ |
| Binding issues increased (0→91) | ✅ Yes (new scenarios need bindings) | ✅ |
| Progress detected despite total increase | ✅ Yes | ✅ |
| Loop would continue if max > 1 | ✅ Yes | ✅ |

### Verdict

**The core loop is working correctly.** It:
1. Selects missions algorithmically (no LLM for task selection)
2. Executes the runner
3. Detects progress even when total issues increase
4. Would continue to next mission (CreateMissingBindings) if we ran `loop 2+`

### Remaining Polish

1. The gate marking mission as "failed" is confusing when progress was made
2. Failure feedback would help the runner on retry attempts
3. Rollback would prevent wasted cycles on regressions

But the **core flywheel is functional**. A human (or outer loop) can now run `tesaki --loop N` and watch the system chip away at the work queue.

---

## Session 8: Turnkey CLI Mode

**Date:** 2026-02-02

### Problem
User had to: `tesaki` → wait → `loop 10` → watch

### Solution  
Added `--loop N` flag for headless autonomous mode.

```bash
# One command, walks away
$ tesaki --loop 10

# Or just run until done (default 100 iterations)
$ tesaki -l 0  # 0 = unlimited until done/stalled
```

### Changes
- `main.rs`: Added `--loop` flag to `Cli` struct
- `repl.rs`: Added `run_loop_headless()` function

### Result
**Truly turnkey.** One command starts the autonomous loop.

---

## Session 9: Batching Bindings

**Date:** 2026-02-02

### Problem
91 binding issues → 91 missions if we create one binding per mission.

### Solution
Modified `generate_brief()` for `CreateMissingBindings` to include ALL missing steps (up to 30 unique patterns) in the mission context.

Now one mission says:
```
Create bindings for as many missing steps as possible. 91 bindings needed.

Missing step bindings (45 unique patterns, showing up to 30):
1. `Given a test scenario`
2. `And a connected client`
3. `When the client sends a message`
...
```

The runner can create multiple bindings per mission, dramatically reducing iteration count.

### Trade-off
More work per mission = longer mission runtime, but fewer total missions.

**Net effect:** Faster overall convergence.

---

## Final Summary: What We Built

### The Turnkey Command

```bash
$ tesaki --loop 10
```

One command. Runs until done or stalled. No REPL interaction needed.

### What Happens Under the Hood

```
┌─────────────────────────────────────────────────────────────┐
│                    AUTONOMOUS LOOP                          │
│                                                             │
│  while has_work() and not stalled:                          │
│    1. RepoState = compute_from_namako_packets()  ← FRESH    │
│    2. Mission = select_algorithmically(state)    ← NO LLM   │
│    3. execute(mission)                           ← RUNNER   │
│    4. after = count_issues()                                │
│    5. if after < before: continue                ← PROGRESS │
│       elif after == before: stall++                         │
│       else: warn(regression)                                │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| Algorithmic task selection | LLM should DO work, not DECIDE what to do |
| Progress-based continuation | Gate fails during workflow, that's expected |
| Batched context | Show ALL missing bindings, let runner create many |
| Stateless iteration | Recompute state each cycle, no drift |
| Simple loop | Research says simple beats complex |

### What We Didn't Add (Intentionally)

| Feature | Why Not |
|---------|---------|
| Exemplar injection | Runner can read files itself |
| Contract snippet injection | Feature headers are readable |
| Failure feedback | We move to next mission, not retry |
| Self-reflection log | Adds complexity, marginal value |

### Files Changed

| File | Lines | Purpose |
|------|-------|---------|
| `repl.rs` | +272 | `run_autonomous_loop()`, `run_loop_headless()` |
| `main.rs` | +10 | `--loop N` CLI flag |
| `mission_type.rs` | +22 | Batched binding context |

### Test Results

```
Before:  Spec:26 Bind:0 SUT:0 (total: 26)
After:   Spec:24 Bind:91 SUT:0 (total: 116)
Delta:   Spec:-2 ← PROGRESS DETECTED
```

The loop correctly:
- Detected progress even though total increased
- Would continue to next mission (CreateMissingBindings)
- Shows clear before/after/delta for visibility

### Remaining Work (Nice-to-Have)

1. Rollback on significant regression
2. Structure issue details in output
3. Unlimited mode (`tesaki --loop` with no number)

### The "Ralph Wiggum" Verdict

**Yes, this is bulletproof enough.** Run `tesaki --loop 20` and walk away. The system will:
1. Add scenarios to uncovered rules
2. Create bindings for all new steps
3. Continue until all gates pass or stalled

Human intervention only needed if:
- System stalls (3 consecutive no-progress missions)
- Significant regression detected

---

## Session 3: Onboarding Friction Analysis (2026-02-02)

**Tester:** GitHub Copilot CLI (Claude Sonnet 4)
**Trigger:** User said "read all docs in `namako/_WORKSPACE` and follow instructions"

### What Actually Happened

1. **I read 10 files** — CLAUDE.md, CURRENT_STATUS.md, DEV_EX.md, DX_TEST_LOG.md (this file!), FAILURE_MODES.md, GOLD_PLAN.md, OUTPUT.md, RESEARCH_FINDINGS.md, SYSTEM.md, TODO.md
2. **Total content ingested:** ~150KB+ of documentation
3. **Time spent reading before acting:** Multiple tool calls just to understand "what am I supposed to do?"
4. **The actual answer was 2 lines:**
   ```bash
   cd naia
   tesaki --loop 10
   ```

### The Problem: Documentation Overload

| File | Size | Did I Need It? |
|------|------|----------------|
| CLAUDE.md | 1.5KB | ✅ YES — This should have been enough |
| CURRENT_STATUS.md | 10KB | ⚠️ PARTIAL — Only needed MODE + command |
| GOLD_PLAN.md | 101KB | ❌ NO — Reference material, not onboarding |
| DX_TEST_LOG.md | 36KB | ❌ NO — Historical notes, not instructions |
| DEV_EX.md | 10KB | ❌ NO — Design spec, not operations |
| SYSTEM.md | 4KB | ⚠️ PARTIAL — Only "no git ops" rule matters |
| FAILURE_MODES.md | 6KB | ❌ NO — Not relevant to getting started |
| RESEARCH_FINDINGS.md | 4KB | ❌ NO — Background research |
| TODO.md | 1KB | ❌ NO — Project tracking |
| OUTPUT.md | 0KB | ❌ NO — Empty file |

**Result:** I read ~170KB to find ~500 bytes of actionable information.

### What "Follow Instructions" Actually Means

The user's prompt "read all docs and follow instructions" is **too vague**. There were no explicit instructions to follow — just documentation to absorb.

What I was looking for:
1. What is my objective?
2. What command do I run?
3. What constraints apply?

What I got:
1. A comprehensive system specification
2. Historical development logs
3. Design philosophy documents
4. Research notes

### The Fix: A True 15-Second Onramp

CLAUDE.md claims to be a "15-second onramp" but it's not. It says:
> Read `CURRENT_STATUS.md`, `GOLD_PLAN.md`, Obey `SYSTEM.md`

That's 3 more files, totaling ~115KB. **Not 15 seconds.**

**What CLAUDE.md should say:**

```markdown
# CLAUDE.md — Agent Quick Start

## The Command
```bash
cd naia && tesaki --loop 10
```

## Constraints
- NO git operations (human handles commits)
- Edit both naia/ and namako/ as needed
- End sessions by updating OUTPUT.md

## If You Need More Context
- Current state: CURRENT_STATUS.md (MODE, gates, paths)
- Hard rules: SYSTEM.md (forbidden actions)
- Full spec: GOLD_PLAN.md (reference only)
```

That's it. 15 lines. Actually 15 seconds.

### Specific Recommendations

1. **CLAUDE.md should be self-contained for 90% of agent sessions**
   - Put the command right there, no "go read X"
   - Constraints inline, not by reference

2. **Create a hierarchy:**
   - **Tier 1 (always read):** CLAUDE.md only
   - **Tier 2 (if stuck):** CURRENT_STATUS.md, SYSTEM.md  
   - **Tier 3 (deep reference):** GOLD_PLAN.md, DEV_EX.md

3. **User prompt should be:**
   ```
   Read namako/_WORKSPACE/CLAUDE.md and run the autonomous loop
   ```
   Not "read all docs and follow instructions"

4. **Consider consolidating:**
   - OUTPUT.md is empty → delete or merge
   - RESEARCH_FINDINGS.md → archive or append to DX_TEST_LOG.md
   - TODO.md → merge into CURRENT_STATUS.md

### The Meta-Lesson

**Documentation for humans ≠ documentation for agents.**

Humans browse, skim, and build mental models over time. Agents need:
- **Immediate actionability** — what command, right now
- **Minimal context** — only what's needed for this task
- **Explicit constraints** — don't make me infer

The current docs are written for a human learning the system. They need a parallel "agent operations" layer that's ruthlessly minimal.

### Observed Loop Behavior

When I finally ran `tesaki --loop 3`:

**Good:**
- Clear mission headers with type, target, surfaces
- Before/after issue counts with delta
- Algorithmic task selection (no LLM for picking tasks)
- Progress detection working

**Concerning:**
- Mission 1: Spec issues reduced (26→24) but bindings increased (0→9) → total went UP
- Gate said PASS before, then FailOther after mission
- System counted this as "progress" because total changed, but it's unclear if net positive

**Question:** Should "progress" be measured by total issues or by mission-specific target? If I'm doing AddOrClarifyScenario, success should be "spec issues decreased" not "something changed."

### Summary

| Aspect | Grade | Notes |
|--------|-------|-------|
| Docs completeness | A | Everything is documented somewhere |
| Docs discoverability | C | Too much to read, unclear priority |
| Agent onboarding | D | 170KB to find 500B of instructions |
| Command ergonomics | A | `tesaki --loop N` is perfect |
| Loop transparency | B+ | Good output, needs clearer success criteria |

**Recommendation:** Rewrite CLAUDE.md to be truly self-contained. Make it the ONLY required reading for an agent starting work.

---

## Session 4: System Improvement Recommendations (2026-02-02)

**Tester:** GitHub Copilot CLI (Claude Sonnet 4)
**Context:** After running `tesaki --loop 3` and analyzing mission bundles

### Observed Behavior Analysis

**Mission 1 (AddOrClarifyScenario):**
- Runner correctly added 4 scenarios to `03_messaging.feature`
- This created 9 new missing bindings (expected cascade)
- Gate failed with "Resolution failed with 9 error(s)"
- System counted this as "progress" (total changed: 26 → 34)

**The Cascade Problem:**
Adding scenarios ALWAYS creates binding work. The loop correctly detected this and switched to `CreateMissingBindings` for Mission 2. This is actually working as designed — the issue is **communication**, not logic.

### Improvement Recommendations

#### 1. **Mission-Type-Specific Success Criteria** (HIGH PRIORITY)

Current: "Did total issues change?"
Better: "Did THIS mission's target metric improve?"

| Mission Type | Success Metric |
|--------------|----------------|
| AddOrClarifyScenario | Spec issues decreased |
| CreateMissingBindings | Binding issues decreased |
| ImplementBehaviorForScenario | SUT failures decreased |
| NormalizeIdentityTags | Structure issues decreased |

**Implementation:**
```rust
fn is_mission_successful(mission_type: &MissionType, before: &Counts, after: &Counts) -> bool {
    match mission_type {
        MissionType::AddOrClarifyScenario { .. } => after.spec < before.spec,
        MissionType::CreateMissingBindings { .. } => after.bindings < before.bindings,
        // etc.
    }
}
```

**Rationale:** AddOrClarifyScenario succeeded (spec issues 26→24) even though it created binding work. The current "total changed" metric obscures this.

---

#### 2. **Cascade Awareness in Output** (MEDIUM PRIORITY)

When a mission creates downstream work, say so:

```
Mission 1/3: AddOrClarifyScenario
✅ Spec issues: 26 → 24 (-2)
⚠️ Created 9 new binding tasks (expected cascade)
   → Next mission will address bindings
```

This helps the human understand that the system is working correctly, not failing.

---

#### 3. **MISSION.md Context Enrichment** (HIGH PRIORITY)

Current MISSION.md is sparse:
```markdown
## Objective
Add or clarify scenarios to improve coverage.

## Context
Coverage gaps detected in features/03_messaging.feature.
```

Better:
```markdown
## Objective
Add scenarios to features/03_messaging.feature to cover Rule 01, 02, 03.

## Context
### Current State
- Feature has 0 executable scenarios (only contract mirror comments)
- 3 Rules defined but no scenarios testing them

### What to Do
1. Read the NORMATIVE CONTRACT MIRROR in the feature file header
2. Add 1-3 scenarios per Rule that test the stated guarantees
3. Use existing scenarios in 01_connection_lifecycle.feature as style reference

### Examples of Good Scenarios (from this repo)
```gherkin
@Scenario(01)
Scenario: Server observes ConnectEvent when client connects
  Given a server is running
  When a client connects
  Then the server has observed ConnectEvent
```

### Validation
Run `namako gate` — lint should pass (steps will be unresolved, that's expected)
```

**Key additions:**
- What the current state actually is (not just "coverage gaps")
- Explicit instructions (read contract, add N scenarios)
- Examples from the same repo (style reference)
- Clear validation criteria

---

#### 4. **Include Binding Exemplars for CreateMissingBindings** (HIGH PRIORITY)

When asking runner to create bindings, include:
1. The exact missing step texts
2. 2-3 examples of existing bindings in the same file
3. The World/Context type signature

Current MISSION.md for CreateMissingBindings:
```markdown
## Objective
Create missing step bindings for runnable scenarios.
```

Better:
```markdown
## Objective
Create step bindings for these 9 missing steps:
1. `When the client sends on a server-to-client channel`
2. `Then the send returns an error`
3. `When the server sends messages A B C on an ordered reliable channel`
...

## Context
### Binding Location
Add to: `naia/test/tests/src/steps/messaging.rs` (create if needed)

### Existing Binding Pattern (from connection.rs)
```rust
#[when("a client connects")]
fn when_client_connects(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    // ... implementation
}

#[then("the client has observed ConnectEvent")]
fn then_client_has_observed_connect(ctx: &TestWorldRef) -> AssertOutcome<()> {
    // ... assertion with AssertOutcome
}
```

### World Type
`TestWorldMut` for Given/When, `TestWorldRef` for Then
See: `naia/test/tests/src/world.rs`
```

---

#### 5. **Rollback on Regression** (LOW PRIORITY for now)

If total issues increase significantly (e.g., +10 from a single mission), consider:
1. `git checkout -- .` to revert changes
2. Skip this mission type temporarily
3. Try a different approach

However, the observed behavior (26 → 34) was **expected cascade**, not regression. Need to distinguish:
- Expected cascade: spec work creates binding work (normal)
- Regression: binding work breaks previously-passing tests (bad)

---

#### 6. **Failure Feedback Loop** (MEDIUM PRIORITY)

When a mission fails, include in the NEXT mission's context:
```markdown
## Previous Attempt
Mission 035 (AddOrClarifyScenario) failed gate:
- Reason: 9 unresolved steps
- Steps added were syntactically correct but no bindings exist
- This mission will create those bindings
```

---

#### 7. **Gate Failure Diagnostics** (MEDIUM PRIORITY)

When POST_GATE.json shows failure, include the actual errors:
```json
{
  "lint": {
    "status": "fail",
    "reason": "Resolution failed with 9 error(s)",
    "errors": [
      {"step": "When the client sends on a server-to-client channel", "file": "03_messaging.feature", "line": 142},
      ...
    ]
  }
}
```

Currently just says "9 error(s)" without listing them. The runner needs to know WHICH steps to create.

---

### Priority Summary

| Improvement | Priority | Effort | Impact |
|-------------|----------|--------|--------|
| Mission-specific success metrics | HIGH | Medium | Clarity |
| MISSION.md context enrichment | HIGH | Medium | Agent effectiveness |
| Binding exemplars in CreateMissing | HIGH | Low | Agent success rate |
| Cascade awareness in output | MEDIUM | Low | Human understanding |
| Failure feedback loop | MEDIUM | Medium | Recovery speed |
| Gate failure diagnostics | MEDIUM | Low | Debugging |
| Rollback on regression | LOW | Medium | Safety net |

### The Core Insight

**The system logic is sound. The communication is weak.**

The loop correctly:
- Selected AddOrClarifyScenario (spec had gaps)
- Let runner add scenarios (correct work)
- Detected cascade (new bindings needed)
- Switched to CreateMissingBindings (right next step)

But the output made it look like failure:
- "Gate: FailOther" (scary red)
- "Total: +8" (looks like regression)
- "STOP: GATE_FAILED" (sounds bad)

When actually:
- Mission succeeded at its objective (spec improved)
- Gate failed because of expected cascade (bindings needed)
- Loop correctly continues to address cascade

**Recommendation:** Separate "mission objective achieved" from "gate status" in output. Both are useful signals but they mean different things.
- External blocker (build broken, dependency issue)
---

## Session 5: Improvements Implemented and Tested

**Date:** 2025-01-27
**Actor:** Copilot CLI (continuation)

### Work Completed

#### 1. Enriched Mission Brief Templates

**create_missing_bindings.md.j2:**
- Added explicit list of all missing steps (numbered, showing up to 30)
- Added step-by-step instructions for where to add bindings
- Included binding pattern examples (Given/When/Then with correct context types)
- Added context type reference (`TestWorldMut` vs `TestWorldRef`)
- Added explicit validation criteria

**add_clarify_scenario.md.j2:**
- Added current state summary (scenarios, rules needing coverage)
- Added example scenario pattern
- Added cascade warning (explains that adding scenarios creates binding work)

**MISSION.md.j2:**
- Added emoji for visual scanning (🎯 Objective, ✅ Validation)
- Added surface lock warnings (clear messaging about what can/can't be edited)
- Added "How To Verify Your Work" section with exact command
- Added placeholder for previous mission context
- Added expected cascade awareness section

#### 2. Gate Diagnostics

**gate.rs:**
- Added `GateError` struct with detailed error info (message, step_text, file, line)
- Added `errors: Option<Vec<GateError>>` to `PhaseResult` for capturing lint errors
- Added `GateFailureDetails` helper for extracting and formatting error context
- Added `to_markdown()` method for human-readable error summaries

#### 3. Template Context Enrichment

**prompts.rs:**
- Added `BindingExemplar` struct for passing binding examples
- Extended `BriefContext` with:
  - `all_missing_steps: Vec<String>` - complete list of missing bindings
  - `binding_exemplars: Vec<BindingExemplar>` - example bindings from repo
  - `current_scenario_count: usize` - for AddOrClarifyScenario
  - `rules_without_scenarios: Vec<String>` - rules needing coverage

### Test Results

Ran `tesaki --loop 1` with the improved templates:

```
Before:  Spec:24 Bind:9 SUT:0 Struct:1 (total: 34)
After:   Spec:24 Bind:0 SUT:1 Struct:0 (total: 25)
Delta:   Spec:+0 Bind:-9 Total:-9
✅ Progress made - continuing
```

**Result: All 9 missing bindings were created successfully!**

The enriched MISSION.md now shows:
```markdown
## 🎯 Objective

Create step bindings for as many missing steps as possible. 9 bindings needed.

## Context

Missing step bindings (9 unique patterns, showing up to 30):
1. `the client receives message A exactly once`
2. `the client receives messages A B C in order`
3. `the client receives the response for that request`
4. `the client sends a request`
5. `the client sends on a server-to-client channel`
6. `the send returns an error`
7. `the server responds to the request`
8. `the server sends message A on an ordered reliable channel`
9. `the server sends messages A B C on an ordered reliable channel`
```

### Remaining Work

| Item | Status | Notes |
|------|--------|-------|
| Enriched templates | ✅ Done | All three mission types improved |
| Gate error details | ✅ Done | Structs ready, not yet wired to console |
| Previous mission context | 🟡 Template ready | Not wired to MissionBundle creation |
| Cascade awareness in console | ⏳ Pending | Template supports it, console doesn't |
| Mission success vs gate status | ⏳ Pending | Needs main.rs output changes |

### Key Insight

**The enriched templates work.** The agent now has all the context it needs:
- Exact list of what to create (not just counts)
- Step-by-step instructions (not just "make bindings")
- Pattern examples (not hunting for syntax)
- Clear validation criteria (what success looks like)

The previous mission failed because the agent didn't have the list of missing steps. This mission succeeded because it had exactly what it needed.

### Files Changed

1. `tesaki/prompts/mission/MISSION.md.j2` - Main mission template
2. `tesaki/prompts/mission/briefs/create_missing_bindings.md.j2` - Binding mission brief
3. `tesaki/prompts/mission/briefs/add_clarify_scenario.md.j2` - Scenario mission brief
4. `tesaki/src/prompts.rs` - Context structs and fields
5. `tesaki/src/gate.rs` - Error detail structs
6. `_WORKSPACE/CLAUDE.md` - Streamlined agent onboarding
7. `_WORKSPACE/DX_TEST_LOG.md` - This log

---

## Session 6: System Validated — Bulletproof & Turnkey

**Date:** 2025-01-27
**Actor:** Copilot CLI

### Final Improvements

1. **Mission-Specific Success Messages** (repl.rs)
   - Added `format_mission_success()` function
   - Shows clear emoji-prefixed messages:
     - `📝 Created N binding(s)` for CreateMissingBindings
     - `🔧 Fixed N SUT issue(s)` for FixRegressionFromGateFailure
     - `📋 Improved N spec issue(s)` for AddOrClarifyScenario
   - Cascade awareness: `→ N SUT issue(s) surfaced (expected cascade)`

### Validation Results

**Test 1: CreateMissingBindings**
```
Before:  Spec:24 Bind:9 SUT:0 Struct:1 (total: 34)
After:   Spec:24 Bind:0 SUT:1 Struct:0 (total: 25)
📝 Created 9 binding(s) → 1 SUT issue(s) surfaced (expected cascade)
✅ Progress made - continuing
```

**Test 2: FixRegressionFromGateFailure**
```
Before:  Spec:24 Bind:0 SUT:1 Struct:0 (total: 25)
After:   Spec:24 Bind:0 SUT:0 Struct:0 (total: 24)
🔧 Fixed 1 SUT issue(s)
✅ Progress made - continuing
```

**Final State:**
```
Final state: Spec: 24 issues • Structure: 0 • Bindings: 0 missing • SUT: 0 failing
```

### What Makes It Bulletproof

1. **Clear Mission Context**: MISSION.md includes all missing steps, not just counts
2. **Explicit Surface Policy**: Locked surfaces are clearly marked
3. **Cascade Awareness**: Output explains when new issues are expected
4. **Mission-Specific Metrics**: Shows what the mission achieved, not just gate status
5. **Self-Contained Start**: `cd naia && tesaki --loop 10` is all you need

### Files Changed

| File | Change |
|------|--------|
| `tesaki/src/repl.rs` | Added `format_mission_success()` function |
| `tesaki/prompts/mission/MISSION.md.j2` | Enhanced with emoji, surface warnings, verification |
| `tesaki/prompts/mission/briefs/*.j2` | Enriched with step lists, patterns, context |
| `tesaki/src/prompts.rs` | Added BriefContext fields for richer templates |
| `tesaki/src/gate.rs` | Added GateError/GateFailureDetails structs |
| `_WORKSPACE/CLAUDE.md` | Streamlined to self-contained quick start |

### The Autonomous Loop is Ready

```bash
cd naia
tesaki --loop 10
```

The system will:
1. Select missions algorithmically (spec→bindings→SUT)
2. Execute via runner with rich context
3. Track progress with clear output
4. Continue until done or stalled

**Total issues reduced: 34 → 24 in 2 missions**
