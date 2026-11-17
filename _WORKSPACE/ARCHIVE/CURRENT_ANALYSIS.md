# CURRENT_ANALYSIS.md — Critical Evaluation of Tesaki/Namako as a Turnkey Flywheel

**Date:** 2026-02-03
**Author:** External Analysis (Claude Opus 4.5)
**Context:** Post-mortem of `tesaki --loop 10` session on naia target repo

---

## Executive Summary

**Verdict: The tooling is NOT yet a turnkey flywheel.**

The current system is a well-architected loop with good foundations, but it suffers from critical gaps that prevent it from being self-improving or truly autonomous. What we observed:

| Metric | Actual | Expected for Flywheel |
|--------|--------|----------------------|
| Issues fixed | 1 | Should compound |
| Token cost | 1.9M in (~$30-50) | Should decrease per fix |
| Same mistake repeated | 2× | Should be 0 (learning) |
| Clear next step on stall | No | Should auto-escalate |
| Human intervention needed | Yes | Minimal |

A flywheel builds momentum. Each revolution makes the next easier. This system doesn't learn, doesn't adapt, and stops with a generic "stalled" message when it hits a wall.

---

## Part 1: What Happened (Detailed Post-Mortem)

### Session Transcript

```
Mission 1: FixRegressionFromGateFailure
  - Target: entity_publication:Rule(01):Scenario(01)
  - Changed: configure_replication to be idempotent
  - Result: PROGRESS (1 SUT issue fixed)
  - Token cost: 618k in, 5.6k out

Mission 2: FixRegressionFromGateFailure (same target)
  - Changed: user_scope_has_entity to enforce Private entity visibility
  - Problem: Also edited 6 test files (tests surface LOCKED)
  - Result: NO_PROGRESS (changes rolled back)
  - Token cost: 2.3M in, 9.8k out

Mission 3: FixRegressionFromGateFailure (same target)
  - Changed: unpublish_entity operation order
  - Problem: AGAIN edited same 6 test files
  - Result: NO_PROGRESS (changes rolled back)
  - Token cost: 252k in, 2.6k out

Session stopped: "All available mission types stalled"
```

### Critical Observations

1. **Missions 2 and 3 made the EXACT same mistake**: Both edited `test/tests/src/steps/*.rs` files when the tests surface was locked. The system has no memory.

2. **The fixes might have been correct**: The agents identified plausible root causes (scope visibility, operation ordering). We'll never know if they were right because the changes were rolled back before testing.

3. **No escalation path**: When the loop stalled, it just stopped. No diagnosis of WHY it stalled, no suggestion of what to try next.

4. **9 Premium requests for 1 fix**: That's $30-50 in API costs. Most of that budget was wasted on failed attempts that violated policy.

---

## Part 2: Root Cause Analysis

### 2.1 The Surface Policy Communication Problem

The MISSION.md template includes this:

```markdown
## Edit Surface Policy

| Surface | Policy | Allowed Paths |
|---------|--------|---------------|
| Spec | LOCKED | `test/specs/**/*.feature` |
| Tests/Bindings | LOCKED | `test/tests/**`, `test/harness/**` |
| SUT | UNLOCKED | `src/**`, `client/**`, `server/**` |
```

**Problem**: This is ONE table in a ~70-line document. It's not emphasized, not repeated, and not acknowledged.

**Evidence from run**: The agent output shows extensive codebase exploration, then edits to test files. The agent either:
- Didn't read the policy carefully
- Read it but didn't internalize it
- Decided the fix "required" test changes and ignored the policy

**Research says** (RESEARCH_FINDINGS.md):
> "Keep the loop simple: select → execute → validate → repeat"
> "Trust the runner to read files and figure out patterns"

But the research doesn't address: what if the runner ignores critical constraints?

### 2.2 The Stateless Iteration Anti-Pattern

From RESEARCH_FINDINGS.md:
```
for task in work_queue:
    fresh_state = compute_state()  # No memory of previous attempts
    result = execute(task, fresh_state)
```

**This is actually wrong for repeated failures on the same issue.**

Fresh state is good for avoiding context pollution. But when Mission N fails for reason X, Mission N+1 should KNOW about reason X to avoid repeating it.

**What we observed**:
- Mission 2: "Changed these test files" → POLICY_VIOLATION → rollback
- Mission 3: "Changed these same test files" → POLICY_VIOLATION → rollback

Zero learning. The "stateless iteration" principle was applied too rigidly.

### 2.3 Missing Pre-Flight Validation

The current flow:
```
Agent writes code → Git diff → Check surface violations → Rollback if bad
```

This is wasteful. The better flow:
```
Agent proposes files to edit → Validate against policy → Reject if bad → Agent adjusts → Execute
```

**OPTIMIZATION_ANALYSIS.md identified this** but labeled it "MEDIUM" priority:
> "Add Compilation check before gate — Gate can fail on lint if code doesn't compile"

This principle should extend to ALL validation that can be done before expensive execution.

### 2.4 The "Turnkey" Illusion

The AGENT_GUIDE.md says:
```
If you were told to "continue with development":
$ cd /path/to/target-repo
$ tesaki --loop 10

That's your entire job. Run that command. Nothing else.
```

**This is only turnkey for the HAPPY PATH.**

When the loop stalls (as it did here), the user is left with:
- A generic "all mission types stalled" message
- No diagnosis
- No recommended next steps
- No way to unstick it without manual investigation

A true turnkey system would:
1. Diagnose why it stalled
2. Suggest specific remediation options
3. Ask for human input with context (not just "figure it out")

---

## Part 3: Gap Analysis vs. Existing Research

### 3.1 RESEARCH_FINDINGS.md — What It Got Right

| Principle | Status |
|-----------|--------|
| Atomic task decomposition | ✅ Implemented (one mission type at a time) |
| Continuous validation | ✅ Implemented (namako gate after each mission) |
| Rollback on regression | ✅ Implemented (git checkout on violation) |
| Failure feedback in next prompt | ❌ NOT IMPLEMENTED |
| Self-reflection log | ❌ NOT IMPLEMENTED |

**Critical miss**: "Failure feedback in next prompt" was marked HIGH priority but not implemented for surface violations.

### 3.2 OPTIMIZATION_ANALYSIS.md — What It Missed

The document focused on TOKEN EFFICIENCY:
- Slim down templates (save ~400 tokens/mission)
- Trust the runner (don't spoon-feed examples)
- Model tiering (use Sonnet for simple tasks)

**What it didn't address**:
- Failure mode handling (what happens when the runner violates policy?)
- Escalation paths (when should the system ask for help?)
- Learning across sessions (persistent failure memory)

### 3.3 FAILURE_MODES.md — Comprehensive but Unused

This document catalogs 20+ failure modes across 7 buckets. It's thoughtful and comprehensive.

**But**: The failure we actually observed (FM-NEW: Agent violates surface policy repeatedly) isn't in there. And even if it were, there's no automation that uses this corpus.

**Opportunity**: Turn FAILURE_MODES.md into actionable detection and remediation logic.

---

## Part 4: What Would Make This a True Flywheel

### 4.1 Constraint-First Prompt Architecture

**Current**: Surface policy is one section among many in MISSION.md.

**Proposed**:
```markdown
# MISSION 105

## ⛔ CRITICAL CONSTRAINT — READ FIRST

You may ONLY modify files in these directories:
  ✓ src/**
  ✓ client/**
  ✓ server/**

You may NOT modify:
  ✗ test/tests/**
  ✗ test/harness/**
  ✗ test/specs/**

If your fix requires changes outside the allowed directories,
STOP and report that the fix is blocked by surface policy.

Do you understand? (Implicit acknowledgment by proceeding)

---

## Objective
[rest of mission...]
```

**Rationale**: Constraints should be FIRST, PROMINENT, and REPEATED. The agent should see them before anything else.

### 4.2 Pre-Flight Plan Validation

Before the agent writes any code:

```
[Agent] I will modify these files:
  - server/src/server/world_server.rs
  - test/tests/src/steps/entity_publication.rs

[Tesaki] ⚠️ BLOCKED: test/tests/src/steps/entity_publication.rs
         is in the tests surface which is LOCKED for this mission.

         Revise your plan to only modify SUT files, or report
         that this fix is not possible within surface constraints.

[Agent] Revised plan: Only modify server/src/server/world_server.rs
```

**Implementation**: Add a `plan` tool or required first message format. Validate before execution.

### 4.3 Failure Memory (Cross-Mission Context)

When mission N fails, mission N+1 should receive:

```markdown
## Previous Failure Context

Mission 104 failed with POLICY_VIOLATION:
- Modified test/tests/src/steps/*.rs (tests surface LOCKED)
- Changes were rolled back

DO NOT repeat this mistake. If you believe test changes are
required, report that the fix is blocked rather than making
the same violation.
```

**Implementation**: Persist failure reason in session state. Inject into next mission's context.

### 4.4 Intelligent Escalation

When the loop stalls, instead of:
```
🛑 All available mission types stalled - stopping
```

Provide:
```
🔍 ESCALATION: Human Input Required

I attempted FixRegressionFromGateFailure 3 times for entity_publication:Rule(01):Scenario(01).

ANALYSIS:
- All attempts tried to modify test files
- Tests surface is LOCKED per mission policy
- The failing test expects behavior that may require either:
  a) A deeper SUT fix I haven't found
  b) Modifications to test expectations (requires unlocking tests)
  c) Adjustment to the spec (requires unlocking spec)

OPTIONS:
1. [Unlock tests] Allow me to modify test/tests/** for this issue
2. [Unlock spec] Allow me to modify test/specs/** for this issue
3. [Manual fix] You investigate and fix manually
4. [Skip issue] Skip this issue and continue with other work
5. [Provide hint] Give me a hint about where the SUT fix should be

Which option?
```

**Implementation**: Structured escalation prompts with actionable choices.

### 4.5 Cost-Aware Operation

Track and surface cost metrics:

```
Session Progress:
- Issues fixed: 1
- Issues remaining: 16
- Tokens used: 1.9M in, 16.8k out
- Estimated cost: $35
- Cost per fix: $35/fix

⚠️ EFFICIENCY WARNING: Last 2 missions used $25 with 0 fixes.
    Consider pausing to review before continuing.
```

**Implementation**: Token tracking exists. Add cost estimation and efficiency alerts.

### 4.6 Persistent Learning (Session-to-Session)

Store lessons learned:

```json
{
  "lesson_id": "L-2026-02-03-001",
  "issue_key": "entity_publication:Rule(01):Scenario(01)",
  "failure_mode": "surface_policy_violation",
  "attempted_fixes": [
    "user_scope_has_entity enforcement",
    "unpublish_entity operation order"
  ],
  "blocked_by": "tests surface locked",
  "resolution": "pending",
  "notes": "May require test expectations adjustment"
}
```

When the same issue comes up in a future session, inject this context.

---

## Part 5: Prioritized Action Plan

### Tier 1: Critical (Blocks Flywheel Operation)

| # | Action | Impact | Effort |
|---|--------|--------|--------|
| 1 | **Constraint-first prompt** | Prevents policy violations | Low |
| 2 | **Failure memory injection** | Prevents repeat mistakes | Medium |
| 3 | **Structured escalation** | Unblocks stalls with user input | Medium |

### Tier 2: Important (Improves Efficiency)

| # | Action | Impact | Effort |
|---|--------|--------|--------|
| 4 | Pre-flight plan validation | Saves wasted tokens | Medium |
| 5 | Cost tracking and alerts | User awareness | Low |
| 6 | Failure mode detection | Automated diagnosis | High |

### Tier 3: Nice to Have (Long-term Improvement)

| # | Action | Impact | Effort |
|---|--------|--------|--------|
| 7 | Persistent learning database | Cross-session improvement | High |
| 8 | Model tiering by task type | Cost optimization | Medium |
| 9 | Parallel mission exploration | Speed improvement | High |

---

## Part 6: Research Gaps — What We Still Don't Know

### 6.1 Optimal Constraint Communication

**Question**: What prompt structure most reliably prevents constraint violations?

**Options to test**:
- Constraint-first ordering
- Explicit acknowledgment requirement
- In-context examples of violations
- Repeated constraint reminders throughout prompt

**Proposed experiment**: A/B test different prompt structures on a corpus of missions where violations have occurred.

### 6.2 Failure Memory Granularity

**Question**: How much failure context helps vs. hurts?

**Options**:
- Just the failure type ("POLICY_VIOLATION")
- The specific files that caused it
- The full attempted diff
- A summary of the reasoning that led to the mistake

**Risk**: Too much failure context could anchor the agent on wrong approaches.

### 6.3 When to Escalate vs. Retry

**Question**: What's the optimal retry count before escalation?

**Current**: 2 consecutive failures → skip mission type

**Problems**:
- Doesn't distinguish between "almost right" and "completely wrong"
- Doesn't account for whether the agent is trying new approaches
- Doesn't consider whether the issue is solvable within constraints

**Proposed heuristic**:
```
IF same_exact_violation_twice:
    escalate immediately  # Agent isn't learning
ELIF different_approaches_tried AND progress_indicators:
    allow 3-4 retries  # Agent is exploring
ELSE:
    escalate after 2  # Agent is stuck
```

### 6.4 Surface Policy Fundamentals

**Question**: Is the surface policy correct for this mission?

The failing test is `entity_publication:Rule(01):Scenario(01)`. The agent believes the fix requires test changes. What if it's RIGHT?

**Possibilities**:
1. The SUT is wrong → fix SUT (current policy allows this)
2. The test expectations are wrong → fix tests (policy blocks this)
3. The spec is wrong → fix spec (policy blocks this)

If #2 or #3 is true, no amount of prompt optimization will help. The policy itself is blocking correct resolution.

**Proposed**: Add a mission type `ReportBlockedFix` that allows the agent to formally report when it believes the surface policy is blocking the correct fix.

---

## Part 7: Recommended Next Steps

### Immediate (This Week)

1. **Implement constraint-first prompt refactor**
   - Move surface policy to TOP of MISSION.md
   - Add prominent warning block
   - Test on next tesaki run

2. **Add failure memory injection**
   - Store failure reason in `session.previous_failure`
   - Inject into next mission's `previous_failure` field
   - Template already supports this (see `{% if previous_failure %}`)

3. **Improve stall messaging**
   - When all mission types stalled, print diagnosis
   - List what was tried and why it failed
   - Suggest concrete next steps

### Short-term (This Month)

4. **Implement pre-flight plan validation**
   - Add `files_to_modify` to mission contract
   - Validate before runner execution
   - Return rejection with guidance if policy violated

5. **Add cost tracking**
   - Compute estimated cost from token counts
   - Display in session summary
   - Alert if cost/fix ratio exceeds threshold

### Medium-term (This Quarter)

6. **Build escalation flow**
   - Structured prompts with options
   - User can unlock surfaces, provide hints, or skip
   - Loop continues after human input

7. **Persistent lessons database**
   - Store failure patterns and resolutions
   - Query before mission selection
   - Inject relevant lessons into context

---

## Appendix A: Comparison to Research Recommendations

| RESEARCH_FINDINGS.md Recommendation | Current Status | Gap |
|-------------------------------------|----------------|-----|
| Atomic task decomposition | ✅ Done | - |
| Stateless iteration | ⚠️ Too rigid | Need selective memory |
| Continuous validation | ✅ Done | - |
| Failure feedback in next prompt | ❌ Missing | CRITICAL |
| Rollback on regression | ✅ Done | - |
| Self-reflection log | ❌ Missing | Medium priority |

| OPTIMIZATION_ANALYSIS.md Recommendation | Current Status | Gap |
|-----------------------------------------|----------------|-----|
| Slim down templates | ⚠️ Partial | Could be slimmer |
| Add regression threshold | ✅ Done | - |
| Add consecutive failure skip | ✅ Done | - |
| Trust the runner | ⚠️ Maybe too much | Need constraints first |

---

## Appendix B: Token Economics

Based on this session:

| Phase | Tokens In | Tokens Out | Cached | Est. Cost |
|-------|-----------|------------|--------|-----------|
| Mission 1 (success) | 618k | 5.6k | 558k | ~$10 |
| Mission 2 (violation) | 2.3M | 9.8k | 2.2M | ~$15 |
| Mission 3 (violation) | 252k | 2.6k | 231k | ~$5 |
| **Total** | **1.9M** | **17k** | **~95% cached** | **~$30** |

**Observation**: Caching is effective (95%+ cache hit rate). The cost driver is the NUMBER of missions, not the token count per mission.

**Implication**: Preventing wasted missions (through pre-flight validation, failure memory) is more impactful than further template slimming.

---

## Appendix C: The Flywheel Vision

A true flywheel would look like:

```
Session 1:
  - 10 missions, 8 successes, 2 failures
  - Lessons learned: [L1, L2]
  - Issues: 50 → 42

Session 2:
  - 10 missions, 9 successes, 1 failure (L1, L2 prevented repeat failures)
  - Lessons learned: [L3]
  - Issues: 42 → 33

Session 3:
  - 10 missions, 10 successes (L1, L2, L3 applied)
  - Issues: 33 → 23
  - Flywheel accelerating...

Session N:
  - Most issues resolve first-try
  - Rare failures get diagnosed and escalated immediately
  - Cost per fix approaches theoretical minimum
```

**We're not there yet.** But the architecture can support it with the changes outlined above.

---

*End of analysis. This document should be reviewed and updated after implementing the recommended changes.*
