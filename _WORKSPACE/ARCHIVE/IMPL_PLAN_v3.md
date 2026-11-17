# IMPL_PLAN_v3.md — The Perfect Autonomous Flywheel

**Version:** 3.0
**Date:** 2026-02-04
**Goal:** Transform Tesaki into a **truly autonomous, self-improving, turnkey flywheel** that runs to completion without human intervention except for genuinely unsolvable problems.

---

## Executive Summary

### The Problem with v2.0

Tesaki v2.0 has impressive **architecture** but zero **integration**:

| Feature | Structure | Integration |
|---------|-----------|-------------|
| Lessons Database | ✅ Complete | ❌ Never loaded/saved |
| Escalation Detection | ✅ Complete | ❌ Never called |
| Stall Diagnosis | ✅ Complete | ❌ Never generated |
| Plan Validation | ✅ Complete | ❌ Never invoked |
| Previous Lessons Context | ✅ Defined | ❌ Always None |

**Result:** v2.0 is a beautifully architected system that doesn't actually use its advanced features.

### The v3.0 Vision

A **truly autonomous** flywheel that:

1. **Learns** — Records lessons on every failure, uses them to avoid repeating mistakes
2. **Adapts** — Adjusts mission selection based on failure patterns and success rates
3. **Self-Heals** — Recovers from common failures automatically before escalating
4. **Explains** — Generates comprehensive diagnostics when it genuinely can't proceed
5. **Improves** — Gets measurably better over time via tracked metrics
6. **Integrates** — All subsystems actually wired into the execution loop

---

## Architecture Overview

### Current Data Flow (v2.0 - Broken)

```
select_mission() → create_mission_bundle() → invoke_runner() → check_gate() → stop/continue
                                                    ↓
                                        lessons.rs (NEVER CALLED)
                                        diagnosis.rs (NEVER CALLED)
                                        escalation.rs (NEVER CALLED)
```

### Target Data Flow (v3.0 - Complete)

```
                    ┌─────────────────────────────────────────┐
                    │           LESSONS DATABASE              │
                    │    (loads on start, saves on exit)      │
                    └─────────────┬───────────────────────────┘
                                  │ find_lessons_for_target()
                                  ▼
select_mission() → create_mission_bundle() ─────────────────────────────────────┐
       ↑                   │                                                     │
       │                   │ previous_lessons populated                          │
       │                   ▼                                                     │
       │          invoke_runner() ──────────────────────────────────────────┐    │
       │                   │                                                 │    │
       │                   │ extract_proposed_files()                        │    │
       │                   ▼                                                 │    │
       │          validate_plan() ──┬── violations? ── early_warning ────────│────│
       │                   │        │                                        │    │
       │                   │        └── OK ──────────────────────────────────┤    │
       │                   ▼                                                 │    │
       │          check_gate() ──────┬── pass ── mark_lesson_resolved() ─────│────│
       │                   │         │                                       │    │
       │                   │         └── fail ── record_lesson() ────────────│────│
       │                   ▼                                                 │    │
       │          should_retry?() ───┬── yes ── adapt_approach() ────────────┘    │
       │                   │         │                                            │
       │                   │         └── no ── detect_escalation() ───────────────│
       │                   ▼                                                      │
       │          can_self_heal?() ──┬── yes ── apply_self_heal() ────────────────┘
       │                   │         │
       │                   │         └── no ── generate_diagnosis()
       │                   ▼                           │
       │          select_next_mission() ←──────────────┘
       │                   │
       └───────────────────┘
```

---

## Preamble: How to Read This Document

This is a **complete implementation guide** for an AI coding agent with **zero prior context**. Every task specifies:

- **WHERE**: Exact file paths
- **WHAT**: Precise changes needed (logic description, not code)
- **WHY**: Rationale for the change
- **HOW TO VERIFY**: Test criteria
- **DEPENDENCIES**: What must be done first

Follow phases IN ORDER. Each phase builds on previous ones.

**Critical Rule:** Do NOT skip to later phases. Each phase's tests must pass before proceeding.

---

## Phase 1: Wire the Lessons Database Into the Loop

**Goal:** Make lessons actually persist and load across sessions.

**Current State:** `lessons.rs` exists with `LessonsDatabase`, `Lesson`, `load()`, `save()` but is never called.

### Task 1.1: Load Lessons at Session Start

**File:** `tesaki/src/repl.rs`

**Where:** Find `run_autonomous_loop()` function, near the beginning where session setup happens.

**What:** 
1. After session initialization, call `LessonsDatabase::load()` with the spec_root path
2. Store the loaded database in a variable that persists through the loop
3. Handle errors gracefully (if load fails, start with empty database and log warning)

**Why:** Lessons must be available for injection into mission context.

**Verify:** Add test that confirms lessons file is loaded on loop start.

### Task 1.2: Save Lessons on Session End

**File:** `tesaki/src/repl.rs`

**Where:** Find all exit points from `run_autonomous_loop()` — search for `return`, `break`, early exits.

**What:**
1. Before each return/break, call `lessons_db.save()` with spec_root
2. Handle save errors (log but don't fail the session)

**Why:** Lessons must persist to disk for future sessions.

**Verify:** Add test that confirms lessons are saved on normal exit and error exit.

### Task 1.3: Record Lesson on Mission Failure

**File:** `tesaki/src/main.rs`

**Where:** Find `save_failure_record_full()` function and all places that call it.

**What:**
1. After saving failure record, also record a lesson:
   - Extract issue_key from mission target
   - Extract failure_mode from stop_reason
   - Extract blocked_by from violated_surface (if applicable)
   - Check if lesson for this issue_key already exists
   - If exists: add attempted_approach; if not: create new lesson
2. Pass the lessons database through from caller (will need to thread it through)

**Why:** Every failure should contribute to the knowledge base.

**Verify:** Add test that confirms lesson is created after policy violation failure.

### Task 1.4: Mark Lesson Resolved on Success

**File:** `tesaki/src/main.rs`

**Where:** Find the success path after a mission completes without errors — look for where issue count decreases.

**What:**
1. When a mission succeeds (reduces issue count for its target):
   - Find lessons for that target
   - Mark them as resolved with the approach that worked
   - Include brief description of successful approach

**Why:** Resolved lessons provide positive examples for future attempts.

**Verify:** Add test that confirms lesson is marked resolved after successful mission.

### Task 1.5: Thread LessonsDatabase Through the Call Chain

**File:** `tesaki/src/main.rs` and `tesaki/src/repl.rs`

**Where:** `run_run()` function signature and all callers.

**What:**
1. Add `lessons_db: &mut LessonsDatabase` parameter to `run_run()`
2. Update all call sites in `repl.rs` to pass the database
3. Ensure database is mutably borrowed only when needed

**Why:** The database must be accessible throughout the mission lifecycle.

**Verify:** Compilation succeeds, all existing tests pass.

### Task 1.6: Add Integration Tests for Lessons Persistence

**File:** `tesaki/src/lessons.rs` (test module)

**What:** Add tests that verify:
1. Empty database is created when no file exists
2. Lessons survive save → load cycle
3. Multiple lessons for same target are tracked
4. Resolved lessons are distinguished from unresolved

**Verify:** `cargo test -p tesaki -- lessons`

---

## Phase 2: Inject Lessons Into Mission Context

**Goal:** Make past lessons visible to the AI agent in mission prompts.

**Current State:** `MissionContext.previous_lessons` exists but is always `None`.

### Task 2.1: Create LessonContext Conversion Function

**File:** `tesaki/src/lessons.rs`

**What:**
1. Add function `lessons_to_context(lessons: &[&Lesson]) -> Vec<LessonContext>`
2. Convert each Lesson to LessonContext for template use
3. Only include unresolved lessons (skip resolved ones)
4. Limit to most recent N lessons (configurable, default 5)

**Why:** Clean separation between storage format and display format.

**Verify:** Add unit test for conversion.

### Task 2.2: Populate previous_lessons in create_mission_md

**File:** `tesaki/src/mission.rs`

**Where:** Find `create_mission_md()` function, around line 336 where `MissionContext` is created.

**What:**
1. Add `lessons_db: &LessonsDatabase` parameter to function
2. Before creating MissionContext:
   - Get target from mission_type.target_label()
   - If target is Some, call lessons_db.find_lessons_for_target()
   - Convert to LessonContext via lessons_to_context()
   - If non-empty, set previous_lessons to Some(contexts)
3. Update MissionContext creation to use populated previous_lessons

**Why:** Agents need to see what was tried before to avoid repeating mistakes.

**Verify:** Add test that confirms lessons appear in rendered MISSION.md.

### Task 2.3: Thread LessonsDatabase to create_mission_md

**File:** `tesaki/src/main.rs`

**Where:** All calls to `create_mission_md()` or `create_mission_bundle()`.

**What:**
1. Pass lessons_db reference through the call chain
2. Update function signatures as needed

**Verify:** Compilation succeeds.

### Task 2.4: Enhance Lesson Display in Template

**File:** `tesaki/prompts/mission/MISSION.md.j2`

**Where:** Find the `{% if previous_lessons %}` section.

**What:**
1. Add count of total attempts: "This issue has failed {{ previous_lessons | length }} times"
2. For each lesson, show:
   - Failure mode in bold
   - List of approaches tried with bullet points
   - If blocked_by exists, highlight it with warning emoji
3. Add strong directive: "Based on past failures, AVOID these approaches completely."
4. If blocked_by is "spec surface locked" or similar, add specific guidance

**Why:** Make the lesson information impossible to miss and actionable.

**Verify:** Render test with sample lessons confirms output.

### Task 2.5: Add Lesson Staleness Check

**File:** `tesaki/src/lessons.rs`

**What:**
1. Add `age_days()` method to Lesson based on created timestamp
2. In `lessons_to_context()`, optionally filter out lessons older than N days (configurable)
3. Add `is_stale()` method with configurable threshold (default: 30 days)

**Why:** Old lessons may no longer be relevant if codebase has changed.

**Verify:** Add unit tests for staleness calculation.

---

## Phase 3: Wire Escalation Into the Loop

**Goal:** When the loop truly can't proceed, present actionable options to humans.

**Current State:** `escalation.rs::detect_escalation()` exists but is never called.

### Task 3.1: Call detect_escalation on Loop Exit

**File:** `tesaki/src/repl.rs`

**Where:** Find all places where `run_autonomous_loop()` returns due to failure or stall.

**What:**
1. Before returning, call `detect_escalation()` with current session state
2. If escalation is detected (returns Some):
   - Call `format_escalation_message()` to get human-readable output
   - Print the escalation message to stdout
   - Save to `.tesaki/escalation_required.md`
3. Include escalation type in the return value so callers know

**Why:** Humans need clear guidance on what went wrong and what options exist.

**Verify:** Add test that confirms escalation message is printed on policy violation exit.

### Task 3.2: Handle Interactive Escalation

**File:** `tesaki/src/repl.rs`

**Where:** After printing escalation message in interactive mode (not --loop mode).

**What:**
1. If running in REPL mode (interactive):
   - Print numbered options from EscalationContext.suggested_options
   - Wait for user input (number or option id)
   - Apply the selected action:
     - "unlock_spec" → Modify session.intent.surface_overrides to unlock spec
     - "unlock_tests" → Modify session.intent.surface_overrides to unlock tests
     - "unlock_sut" → Modify session.intent.surface_overrides to unlock sut
     - "skip" → Add current target to session skip list (new field needed)
     - "hint" → Prompt for hint text, store in session context
   - After applying action, continue the loop instead of exiting

**Why:** Interactive users can resolve escalations and continue without restarting.

**Verify:** Manual test: trigger policy violation, see options, select one, verify loop continues.

### Task 3.3: Add Skip List to Session State

**File:** `tesaki/src/session.rs`

**Where:** `SessionState` struct.

**What:**
1. Add field: `skip_targets: Vec<String>`
2. Add method: `should_skip(target: &str) -> bool`

**Why:** Allows skipping known-hard issues to work on others.

**Verify:** Add unit test for skip list functionality.

### Task 3.4: Respect Skip List in Mission Selection

**File:** `tesaki/src/mission_selector.rs`

**Where:** `select_with_constraints()` function.

**What:**
1. Add skip_targets parameter
2. After selecting a mission type, check if its target is in skip list
3. If skipped, continue to next candidate mission
4. If all candidates are skipped, return special "AllSkipped" indicator

**Why:** Skip list must actually prevent selection of skipped targets.

**Verify:** Add test that confirms skipped targets are not selected.

### Task 3.5: Persist Escalation Decision

**File:** `tesaki/src/session.rs` or new file

**What:**
1. When user makes escalation choice, record it:
   - What was escalated
   - What option was chosen
   - Timestamp
2. Save to `.tesaki/escalation_history.json`
3. Load on session start to avoid re-asking same questions

**Why:** Decisions should persist to avoid repetitive prompting.

**Verify:** Add test for escalation history persistence.

---

## Phase 4: Wire Stall Diagnosis Into the Loop

**Goal:** When stopping, generate and save comprehensive diagnostic report.

**Current State:** `diagnosis.rs::StallDiagnosis::diagnose()` exists but is never called.

### Task 4.1: Generate Diagnosis on Every Non-Success Exit

**File:** `tesaki/src/repl.rs`

**Where:** All exit points from `run_autonomous_loop()` that are not success (DONE).

**What:**
1. Before returning, create StallDiagnosis via `diagnose()`:
   - Pass session state
   - Pass current repo state
   - Pass last stop reason
   - Pass last mission type and target
2. Call `format_report()` to get markdown
3. Print to stderr (brief summary)
4. Save full report to `.tesaki/last_stall_diagnosis.md`
5. Log the diagnosis save path

**Why:** Every stall should leave a diagnostic trail for humans to understand.

**Verify:** Add test that confirms diagnosis file is created on stall.

### Task 4.2: Enhance Diagnosis with Lessons Context

**File:** `tesaki/src/diagnosis.rs`

**Where:** `diagnose()` function.

**What:**
1. Add `lessons_db: &LessonsDatabase` parameter
2. Include lessons summary in diagnosis:
   - How many total lessons exist for this session
   - How many lessons exist for current target
   - What approaches have been tried (from lessons)
3. Add this to the "What Happened" section

**Why:** Diagnosis should include full learning context.

**Verify:** Add test with lessons present, verify they appear in report.

### Task 4.3: Add Actionable Commands to Diagnosis

**File:** `tesaki/src/diagnosis.rs`

**Where:** `generate_recommendations()` function.

**What:**
1. Make recommendations include specific commands where possible:
   - "Run: tesaki --unlock-spec --loop 5"
   - "Run: tesaki --skip-target 'feature:auth:login'"
   - "File: Add hint to .tesaki/hints/feature_auth_login.md"
2. Include actual CLI flag syntax
3. Include file paths that could be edited

**Why:** Copy-pasteable commands reduce friction.

**Verify:** Confirm recommendations include runnable commands.

### Task 4.4: Add Diagnosis to Session Summary

**File:** `tesaki/src/repl.rs`

**Where:** Session summary printing (search for "SESSION SUMMARY").

**What:**
1. If diagnosis was generated, include brief summary:
   - Primary blocking factor
   - Top recommendation
   - Path to full report
2. Keep it concise (3-4 lines max in summary)

**Why:** Humans should see diagnosis summary without reading full file.

**Verify:** Manual test: trigger stall, confirm summary includes diagnosis info.

---

## Phase 5: Wire Plan Validation Into Runner Flow

**Goal:** Catch policy violations BEFORE changes are written to disk.

**Current State:** `plan_validator.rs` exists but `validate_plan()` is never called.

### Task 5.1: Extract Proposed Files from Runner Output

**File:** `tesaki/src/main.rs`

**Where:** After runner completes but before changes are committed — find where runner output is captured.

**What:**
1. Call `extract_proposed_files()` on runner stdout/stderr
2. If extraction returns Some files:
   - Create ProposedPlan with those files
   - Call `validate_plan()` with current surface policy
   - If validation fails:
     - Log warning with violated files
     - Add to failure context (informational)
     - Continue (don't block — actual enforcement is in git diff)

**Why:** Early warning helps with debugging, even if not blocking.

**Verify:** Add test with runner output containing file mentions, confirm extraction works.

### Task 5.2: Add Pre-Flight Warning to Mission Output

**File:** `tesaki/src/main.rs`

**Where:** After plan validation runs.

**What:**
1. If validation found violations but we continue anyway:
   - Print yellow warning: "⚠️ Pre-flight check: Runner plans to modify locked files"
   - List the files that would violate
   - Note: "Will be caught during commit check if violations persist"

**Why:** Visibility into what the runner is attempting.

**Verify:** Manual test with known-violating runner output.

### Task 5.3: Make Plan Validation Optionally Blocking

**File:** `tesaki/src/config.rs`

**What:**
1. Add config option: `plan_validation_mode: Option<String>` (warn, block, disabled)
2. Default to "warn" (current behavior)
3. If "block", stop the mission immediately if pre-flight validation fails

**Why:** Some users may want stricter enforcement.

**Verify:** Add config test for new option.

### Task 5.4: Improve File Extraction Patterns

**File:** `tesaki/src/plan_validator.rs`

**Where:** `extract_proposed_files()` function.

**What:**
1. Add patterns for more runner formats:
   - Claude: "I'll modify", "Let me edit", "Making changes to"
   - Copilot: "Editing file:", "Creating file:"
   - Generic: Markdown file paths like `\`src/foo.rs\``
2. Handle multi-file mentions on same line
3. Normalize paths (remove leading ./, handle absolute paths)

**Why:** Better extraction = better pre-flight warnings.

**Verify:** Add tests for each new pattern.

---

## Phase 6: Adaptive Mission Selection

**Goal:** Select missions intelligently based on failure history and success patterns.

**Current State:** Mission selection is purely algorithmic based on current state, ignores history.

### Task 6.1: Track Mission Success Rates

**File:** `tesaki/src/session.rs`

**What:**
1. Add struct `MissionStats { attempts: u32, successes: u32, failures: u32, last_attempt: String }`
2. Add field to SessionState: `mission_stats: HashMap<String, MissionStats>`
3. Add methods: `record_attempt()`, `record_success()`, `record_failure()`, `success_rate()`

**Why:** Need data to make intelligent decisions.

**Verify:** Add unit tests for stats tracking.

### Task 6.2: Penalize Repeatedly Failing Missions

**File:** `tesaki/src/mission_selector.rs`

**Where:** `select_with_constraints()` function.

**What:**
1. Add `mission_stats` parameter
2. After identifying candidate missions:
   - Calculate failure rate for each
   - If failure rate > 70% and attempts > 3, lower priority
   - If failure rate = 100% and attempts > 5, skip entirely
3. Log when missions are skipped due to poor success rate

**Why:** Don't keep trying things that never work.

**Verify:** Add test: mission with 0% success rate is eventually skipped.

### Task 6.3: Prioritize High-Success Mission Types

**File:** `tesaki/src/mission_selector.rs`

**What:**
1. When multiple missions are equally valid:
   - Prefer mission types with higher historical success rate
   - Prefer missions targeting issues similar to past successes
2. Add tie-breaker logic to selection algorithm

**Why:** Do more of what works.

**Verify:** Add test: given equal candidates, higher success rate is chosen.

### Task 6.4: Implement Cooldown for Failing Targets

**File:** `tesaki/src/mission_selector.rs`

**What:**
1. After N consecutive failures on same target:
   - Add to cooldown list with timestamp
   - Don't select for M minutes (configurable)
   - After cooldown, allow retry once
   - If fails again after cooldown, increase cooldown duration (exponential backoff)

**Why:** Prevents tight failure loops, allows time for other work.

**Verify:** Add test for cooldown behavior.

### Task 6.5: Learn From Lessons in Mission Selection

**File:** `tesaki/src/mission_selector.rs`

**What:**
1. Add `lessons_db` parameter
2. When selecting mission for a target:
   - Check if lessons exist for that target
   - If lesson has blocked_by = "spec surface locked" and spec is still locked:
     - Skip this target unless surface policy has changed
   - Log when targets are skipped due to learned blockers

**Why:** Don't select missions we know will fail due to policy.

**Verify:** Add test: target with policy-blocking lesson is skipped.

---

## Phase 7: Self-Healing Behaviors

**Goal:** Automatically recover from common, fixable failures before escalating.

### Task 7.1: Define Self-Healing Strategies

**File:** `tesaki/src/self_heal.rs` (NEW FILE)

**What:**
1. Create module with enum `SelfHealStrategy`:
   - `RetryWithDifferentModel` — Try haiku instead of opus for simple tasks
   - `RetryWithHint` — Add hint from lessons to prompt
   - `RetryWithSmallerScope` — Focus on single file instead of multi-file
   - `UnlockTemporarily` — Auto-unlock surface for one attempt with auto-rollback
   - `SkipAndContinue` — Skip current target, work on others
2. Define `struct SelfHealContext` with decision factors
3. Define `fn suggest_self_heal(context: &SelfHealContext) -> Option<SelfHealStrategy>`

**Why:** Structured approach to automatic recovery.

**Verify:** Add unit tests for each strategy suggestion.

### Task 7.2: Implement Self-Heal Suggestion Logic

**File:** `tesaki/src/self_heal.rs`

**What:**
1. `suggest_self_heal()` logic:
   - If failure was timeout or token limit → suggest RetryWithDifferentModel
   - If failure was NoProgress and lessons show successful approach → suggest RetryWithHint
   - If failure was PolicyViolation and only 1 file violated → suggest UnlockTemporarily
   - If multiple targets available and one is stuck → suggest SkipAndContinue
2. Return None if no safe self-heal is available

**Why:** Automated decision-making for common cases.

**Verify:** Add tests for each decision path.

### Task 7.3: Apply Self-Heal in Loop

**File:** `tesaki/src/repl.rs`

**Where:** After mission failure, before escalation check.

**What:**
1. Call `suggest_self_heal()` with current context
2. If suggestion returned:
   - Log the self-heal attempt
   - Apply the strategy
   - Continue loop instead of escalating
   - Track self-heal attempts (max 3 per target)
3. If no suggestion or max attempts reached, proceed to escalation

**Why:** Automatic recovery before human involvement.

**Verify:** Add integration test for self-heal → retry → success path.

### Task 7.4: Implement UnlockTemporarily Strategy

**File:** `tesaki/src/self_heal.rs`

**What:**
1. When UnlockTemporarily is applied:
   - Save current surface policy
   - Temporarily unlock the blocked surface
   - Run exactly one mission
   - If mission fails, rollback any changes and restore original policy
   - If mission succeeds, keep changes but restore original policy

**Why:** Safe way to try unlocking without permanent policy change.

**Verify:** Add test: unlock → success → policy restored.

### Task 7.5: Track Self-Heal Metrics

**File:** `tesaki/src/session.rs`

**What:**
1. Add `self_heal_stats: HashMap<SelfHealStrategy, SelfHealStats>`
2. Track: attempts, successes, failures per strategy
3. Include in session summary

**Why:** Learn which self-heal strategies work.

**Verify:** Add test for self-heal stats tracking.

---

## Phase 8: Comprehensive Metrics and Telemetry

**Goal:** Capture metrics that enable continuous improvement.

### Task 8.1: Define Metrics Schema

**File:** `tesaki/src/metrics.rs` (NEW FILE)

**What:**
1. Create struct `SessionMetrics`:
   - `session_id: String`
   - `start_time: String`
   - `end_time: String`
   - `total_missions: u32`
   - `successful_missions: u32`
   - `failed_missions: u32`
   - `issues_at_start: usize`
   - `issues_at_end: usize`
   - `estimated_cost_usd: f64`
   - `self_heals_attempted: u32`
   - `self_heals_successful: u32`
   - `escalations_triggered: u32`
   - `lessons_created: u32`
   - `lessons_used: u32`
2. Create methods to populate from session state

**Why:** Structured data for analysis.

**Verify:** Add unit tests for metrics calculation.

### Task 8.2: Save Metrics on Session End

**File:** `tesaki/src/repl.rs`

**Where:** Session end, after summary.

**What:**
1. Create SessionMetrics from session state
2. Save to `.tesaki/metrics/session_<timestamp>.json`
3. Also append to `.tesaki/metrics/history.jsonl` (one line per session)

**Why:** Historical data for trend analysis.

**Verify:** Add test that confirms metrics file is created.

### Task 8.3: Compute Aggregate Metrics

**File:** `tesaki/src/metrics.rs`

**What:**
1. Add function `load_metrics_history(spec_root: &Path) -> Vec<SessionMetrics>`
2. Add function `compute_aggregates(history: &[SessionMetrics]) -> AggregateMetrics`:
   - Average issues resolved per session
   - Average cost per issue (trend over time)
   - Most effective mission types
   - Common failure patterns
   - Self-heal success rates by strategy

**Why:** Understand long-term trends.

**Verify:** Add test with sample history, verify aggregates.

### Task 8.4: Add metrics Command

**File:** `tesaki/src/main.rs`

**What:**
1. Add CLI command: `tesaki metrics`
2. Load metrics history
3. Compute and display aggregates
4. Show trends (improving? degrading?)
5. Highlight actionable insights

**Why:** Users can see how the system is performing.

**Verify:** Manual test with sample metrics files.

### Task 8.5: Efficiency-Based Alerts

**File:** `tesaki/src/repl.rs`

**Where:** During session, after each mission.

**What:**
1. Calculate rolling efficiency (cost per issue over last N missions)
2. If efficiency degrades significantly (>3x worse than average):
   - Print warning
   - Log the degradation
   - Optionally pause and ask user to continue
3. Track efficiency trend in metrics

**Why:** Early warning when system is wasting resources.

**Verify:** Add test for efficiency warning trigger.

---

## Phase 9: Hint System

**Goal:** Allow humans to provide targeted hints that persist and are used automatically.

### Task 9.1: Define Hint Schema

**File:** `tesaki/src/hints.rs` (NEW FILE)

**What:**
1. Create struct `Hint`:
   - `id: String`
   - `created: String`
   - `target_pattern: String` (regex or glob for matching issue keys)
   - `hint_text: String`
   - `expires: Option<String>` (optional expiration date)
   - `used_count: u32`
   - `last_used: Option<String>`
2. Create struct `HintsDatabase`
3. Implement load/save to `.tesaki/hints.json`

**Why:** Structured hint storage.

**Verify:** Add unit tests for hint CRUD.

### Task 9.2: Load and Match Hints

**File:** `tesaki/src/hints.rs`

**What:**
1. Add function `find_hints_for_target(db: &HintsDatabase, target: &str) -> Vec<&Hint>`
2. Match using target_pattern as regex/glob
3. Filter out expired hints
4. Sort by relevance (most recent, most used)

**Why:** Hints need to be discoverable.

**Verify:** Add tests for matching logic.

### Task 9.3: Inject Hints Into Mission Context

**File:** `tesaki/src/prompts.rs`

**What:**
1. Add `hints: Option<Vec<String>>` to MissionContext
2. Add template section for hints (similar to lessons)

**File:** `tesaki/src/mission.rs`

**What:**
1. Load hints database
2. Find matching hints
3. Populate MissionContext.hints

**Why:** Hints should appear in mission prompts.

**Verify:** Add test that confirms hints appear in MISSION.md.

### Task 9.4: Add hint CLI Command

**File:** `tesaki/src/main.rs`

**What:**
1. Add command: `tesaki hint add --target "pattern" --text "hint"`
2. Add command: `tesaki hint list`
3. Add command: `tesaki hint remove <id>`
4. Add command: `tesaki hint edit <id>`

**Why:** Users can manage hints without editing JSON.

**Verify:** Manual test of hint commands.

### Task 9.5: Auto-Generate Hints from Lessons

**File:** `tesaki/src/hints.rs`

**What:**
1. Add function `suggest_hints_from_lessons(lessons: &LessonsDatabase) -> Vec<HintSuggestion>`
2. For lessons with repeated failures:
   - Suggest hint based on what was tried
   - Include "Avoid: <approaches_tried>"
3. For lessons with blocked_by:
   - Suggest hint about the blocker

**Why:** Automate hint creation from learned knowledge.

**Verify:** Add tests for hint suggestion logic.

---

## Phase 10: Quality Verification Loop

**Goal:** Verify that fixes actually work before considering them complete.

### Task 10.1: Add Post-Fix Verification Phase

**File:** `tesaki/src/main.rs`

**Where:** After a mission succeeds (gate passes).

**What:**
1. Don't immediately consider mission complete
2. Run a verification check:
   - Re-run the gate
   - Confirm the specific issue is actually resolved
   - Check for regressions in related scenarios
3. If verification fails, mark mission as partial success

**Why:** Gate passing doesn't guarantee issue is fixed.

**Verify:** Add test: fix that passes gate but doesn't resolve issue is caught.

### Task 10.2: Track Issue Provenance

**File:** `tesaki/src/repo_state.rs`

**What:**
1. Add `issue_id: String` to issue structs
2. Track which specific issues were targeted by mission
3. After mission, check if those specific issues are gone

**Why:** Can't verify fix without knowing what was targeted.

**Verify:** Add test for issue tracking.

### Task 10.3: Add Regression Detection

**File:** `tesaki/src/main.rs`

**Where:** After any mission completes (success or failure).

**What:**
1. Compare issues before vs after
2. If new issues appeared that weren't there before:
   - Mark as regression
   - Log detailed regression info
   - Include in failure context for next mission
3. Distinguish between "expected cascade" and "unexpected regression"

**Why:** Changes shouldn't make things worse.

**Verify:** Add test: mission that introduces new failure is caught.

### Task 10.4: Implement Rollback on Regression

**File:** `tesaki/src/main.rs`

**What:**
1. If regression detected and not expected cascade:
   - Rollback the changes (git checkout or similar)
   - Record the regression in lessons
   - Continue with next mission
2. Add config option: `rollback_on_regression: bool` (default true)

**Why:** Don't leave codebase worse than before.

**Verify:** Add test: regression → rollback → issues back to pre-mission state.

---

## Phase 11: Integration Test Suite

**Goal:** Ensure all subsystems work together correctly.

### Task 11.1: Create Integration Test Harness

**File:** `tesaki/tests/integration_autonomous.rs` (NEW FILE)

**What:**
1. Create test harness with:
   - Temp directory with mock spec
   - Mock runner that produces predictable output
   - Methods to trigger various scenarios
2. Helper functions for setup/teardown

**Why:** Need controlled environment for complex tests.

**Verify:** Basic harness runs successfully.

### Task 11.2: Test Lessons Flow End-to-End

**What:**
1. Test scenario:
   - Mission 1: Fails with policy violation
   - Verify: Lesson created with violation details
   - Mission 2: Targeting same issue
   - Verify: Lesson appears in MISSION.md
   - Mission 2: Succeeds
   - Verify: Lesson marked resolved
   - Session 2: New session
   - Verify: Lesson loaded from disk

**Verify:** Full lessons lifecycle works.

### Task 11.3: Test Escalation Flow End-to-End

**What:**
1. Test scenario:
   - Multiple failures on same target
   - Verify: Escalation detected
   - Verify: Escalation message includes correct options
   - Apply escalation action (unlock surface)
   - Verify: Loop continues with unlocked surface

**Verify:** Full escalation lifecycle works.

### Task 11.4: Test Self-Heal Flow End-to-End

**What:**
1. Test scenario:
   - Mission fails with recoverable error
   - Verify: Self-heal suggested
   - Self-heal applied
   - Retry mission
   - Verify: Success after self-heal

**Verify:** Full self-heal lifecycle works.

### Task 11.5: Test Metrics Collection End-to-End

**What:**
1. Test scenario:
   - Run 3 missions with mixed success/failure
   - Session ends
   - Verify: Metrics file created
   - Verify: Metrics contain correct counts
   - Load history, compute aggregates
   - Verify: Aggregates are reasonable

**Verify:** Full metrics lifecycle works.

---

## Phase 12: Documentation and Polish

**Goal:** Update all documentation, add user-facing improvements.

### Task 12.1: Update AGENT_GUIDE.md

**What:**
1. Document all new features (self-heal, hints, metrics)
2. Add troubleshooting section
3. Add configuration reference
4. Add example scenarios with expected behavior

**Verify:** Documentation is complete and accurate.

### Task 12.2: Update RUNBOOK.md

**What:**
1. Add metrics interpretation guide
2. Add hint management guide
3. Add self-heal behavior explanation
4. Add advanced configuration examples

**Verify:** Runbook covers all new features.

### Task 12.3: Update CLI Help

**File:** `tesaki/src/main.rs`

**What:**
1. Add help text for all new commands
2. Add examples in help output
3. Add version info including v3.0 features

**Verify:** `tesaki --help` is comprehensive.

### Task 12.4: Add Quick Start Guide

**File:** `tesaki/README.md`

**What:**
1. Add getting started section
2. Add common commands
3. Add configuration examples
4. Add link to detailed docs

**Verify:** New user can get started from README.

### Task 12.5: Archive IMPL_PLAN_v3.md

**What:**
1. After implementation complete, move to ARCHIVE/
2. Update CURRENT_STATUS.md to v3.0
3. Create CHANGELOG.md with v3.0 release notes

**Verify:** Documentation is organized and current.

---

## Verification Checklist

After completing all phases:

### Unit Tests
```bash
cargo test -p tesaki
# Should pass all existing + new tests
# Expected: 500+ tests
```

### Integration Tests
```bash
cargo test -p tesaki --test integration_autonomous
# Should pass all integration scenarios
```

### Manual Smoke Test
```bash
cd /path/to/target-repo
tesaki --loop 10
# Should show:
# 1. Lessons loaded at start
# 2. Lessons injected into missions
# 3. Self-heal attempts on recoverable failures
# 4. Escalation prompts on unrecoverable failures
# 5. Comprehensive diagnosis on stall
# 6. Metrics saved on exit
```

### Documentation Check
- [ ] AGENT_GUIDE.md updated
- [ ] RUNBOOK.md updated  
- [ ] README.md has quick start
- [ ] CLI help is complete
- [ ] All config options documented

---

## Appendix A: New Files to Create

1. `tesaki/src/self_heal.rs` — Self-healing strategies
2. `tesaki/src/metrics.rs` — Metrics collection and aggregation
3. `tesaki/src/hints.rs` — Hint system
4. `tesaki/tests/integration_autonomous.rs` — Integration tests

## Appendix B: Files Requiring Major Modification

1. `tesaki/src/repl.rs` — Wire lessons, escalation, diagnosis, self-heal into loop
2. `tesaki/src/main.rs` — Wire lessons, plan validation, verification into run_run
3. `tesaki/src/mission_selector.rs` — Adaptive selection based on history
4. `tesaki/src/mission.rs` — Inject lessons and hints into context
5. `tesaki/src/session.rs` — Add skip list, mission stats, self-heal stats

## Appendix C: Configuration Reference

```toml
# .tesaki/config.toml v3.0 options

# Existing options (v2.0)
enable_failure_memory = true
enable_lessons = true
enable_cost_tracking = true
cost_alert_threshold_usd = 20.0
max_consecutive_failures = 2

# New options (v3.0)
plan_validation_mode = "warn"           # warn, block, disabled
lesson_staleness_days = 30              # Ignore lessons older than this
max_self_heal_attempts = 3              # Per-target self-heal limit
auto_unlock_on_single_violation = false # Experimental: auto-unlock for simple cases
collect_metrics = true                  # Save session metrics
rollback_on_regression = true           # Auto-rollback if new issues appear
cooldown_base_minutes = 5               # Initial cooldown for failing targets
cooldown_max_minutes = 60               # Maximum cooldown duration
```

## Appendix D: Success Criteria

**v3.0 is complete when:**

1. ✅ Lessons persist across sessions and appear in mission prompts
2. ✅ Escalation prompts appear with actionable options
3. ✅ Self-healing recovers from common failures automatically
4. ✅ Metrics are collected and can be analyzed
5. ✅ Hints can be added and are used in missions
6. ✅ Adaptive selection avoids repeatedly failing missions
7. ✅ Regressions are detected and rolled back
8. ✅ All features are configurable
9. ✅ Integration tests verify end-to-end behavior
10. ✅ Documentation is complete and accurate

**The ultimate test:** Can tesaki run `--loop 100` on a moderately complex repo and:
- Resolve most issues without human intervention
- Learn from failures and avoid repeating them
- Self-heal from common problems
- Escalate only genuinely unsolvable problems
- Leave comprehensive diagnostics when it stops
- Produce metrics showing improvement over time

---

*End of Implementation Plan v3.0*

**This is the plan for the PERFECT autonomous SDD flywheel. Follow it completely.**
