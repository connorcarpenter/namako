# IMPL_PLAN.md — Turnkey Flywheel Implementation Plan

**Version:** 2.0
**Date:** 2026-02-03
**Goal:** Transform Tesaki/Namako into a true autonomous, self-improving, turnkey flywheel

---

## Preamble: What This Document Is

This is a **complete implementation guide** for an AI coding agent with **zero prior context**. Every task is specified with:
- **WHERE**: Exact file paths
- **WHAT**: Precise changes needed
- **WHY**: Rationale for the change
- **HOW TO VERIFY**: Test criteria

Follow the tasks IN ORDER. Each phase builds on the previous.

---

## Architecture Overview (Read This First)

### Repository Structure
```
namako/
├── tesaki/
│   ├── src/
│   │   ├── main.rs           # CLI entrypoint + run_run() orchestration
│   │   ├── repl.rs           # REPL + run_autonomous_loop()
│   │   ├── session.rs        # SessionState struct
│   │   ├── prompts.rs        # Template rendering + context structs
│   │   ├── mission_type.rs   # MissionType enum + brief generation
│   │   ├── mission_selector.rs # Algorithmic mission selection
│   │   ├── stop_reason.rs    # StopReason enum
│   │   ├── config.rs         # Configuration parsing
│   │   ├── base_runner.rs    # Surface violation checking
│   │   └── policy_violation.rs # Policy violation detection
│   └── prompts/
│       ├── mission/
│       │   ├── MISSION.md.j2  # Main mission template
│       │   ├── POLICY.md.j2   # Policy document template
│       │   └── briefs/        # Mission-specific brief templates
│       └── components/        # Reusable template components
└── _WORKSPACE/
    └── IMPL_PLAN.md          # This document
```

### Key Data Flow
```
run_autonomous_loop() in repl.rs
  → select_with_constraints() in mission_selector.rs
  → run_run() in main.rs
    → create_mission_bundle() renders MISSION.md using prompts.rs
    → invoke_runner() calls external AI agent
    → check_surface_violations() in base_runner.rs
    → update SessionState
  → repeat or stop
```

### Key Structs
- `SessionState` (session.rs): Persists across missions within a session
- `MissionContext` (prompts.rs): Context for rendering MISSION.md
- `PreviousFailureContext` (prompts.rs): Failure info for next mission
- `StopReason` (stop_reason.rs): Why a mission stopped

---

## Phase 1: Constraint-First Prompt Architecture

**Goal:** Make surface constraints impossible to miss.

### Task 1.1: Create Critical Constraints Template Component

**File:** `tesaki/prompts/components/critical_constraints.md.j2` (NEW FILE)

**Create a new template file** that renders a highly visible constraint block. This component will be included at the TOP of MISSION.md, before anything else.

**Template Requirements:**
1. Start with a prominent header using warning symbols
2. Clearly list ALLOWED directories with checkmark symbols
3. Clearly list FORBIDDEN directories with X symbols
4. Include a directive that if the fix requires forbidden files, the agent should STOP and report the constraint
5. The template receives `surface_policy` and `surface_definitions` as context variables
6. Use conditional logic: only show forbidden paths for LOCKED surfaces
7. End with a clear separator before the rest of the mission

**Context variables available:**
- `surface_policy.spec` (string: "LOCKED" or "UNLOCKED")
- `surface_policy.tests_bindings` (string: "LOCKED" or "UNLOCKED")
- `surface_policy.sut` (string: "LOCKED" or "UNLOCKED")
- `surface_definitions.spec.patterns` (array of glob strings)
- `surface_definitions.tests_bindings.patterns` (array of glob strings)
- `surface_definitions.sut.patterns` (array of glob strings)

### Task 1.2: Integrate Critical Constraints into MISSION.md

**File:** `tesaki/prompts/mission/MISSION.md.j2`

**Modify the template** to include the critical constraints component FIRST, before the mission ID header.

**Changes:**
1. Add an include statement at the very top (line 1) that pulls in `components/critical_constraints.md.j2`
2. The include should pass the `surface_policy` and `surface_definitions` context
3. Remove or simplify the existing "Edit Surface Policy" section (around line 34-50) since constraints are now at the top
4. Keep a minimal surface reference in the body for reference, but the prominent block is at the top

### Task 1.3: Register the New Template

**File:** `tesaki/src/prompts.rs`

**Modify the `create_environment()` function** to register the new template component.

**Changes:**
1. Add a new `env.add_template()` call for `"components/critical_constraints.md.j2"`
2. Use `include_str!("../prompts/components/critical_constraints.md.j2")` to embed it

### Task 1.4: Add Tests for Constraint Rendering

**File:** `tesaki/src/prompts.rs` (in the `#[cfg(test)] mod tests` section)

**Add unit tests** that verify:
1. When spec is LOCKED, the rendered output contains the spec patterns in the FORBIDDEN section
2. When tests are LOCKED, the rendered output contains the test patterns in the FORBIDDEN section
3. When a surface is UNLOCKED, its patterns appear in the ALLOWED section
4. The constraint block appears BEFORE the mission objective

**Verification:** `cargo test -p tesaki -- prompts`

---

## Phase 2: Failure Memory Injection

**Goal:** Prevent agents from repeating the same mistakes across missions.

### Task 2.1: Extend PreviousFailureContext for Surface Violations

**File:** `tesaki/src/prompts.rs`

**Modify the `PreviousFailureContext` struct** to include more detailed failure information.

**Add these fields:**
1. `violated_files: Option<Vec<String>>` — List of files that violated policy
2. `violated_surface: Option<String>` — Which surface was violated (spec/tests/sut)
3. `attempted_approach: Option<String>` — Brief description of what was tried

### Task 2.2: Create Failure Memory Persistence

**File:** `tesaki/src/session.rs`

**Modify `SessionState`** to track failure context more comprehensively.

**Add these fields:**
1. `failure_history: Vec<FailureRecord>` — History of failures this session
2. `current_target_failures: u32` — How many times current target has failed

**Create a new struct `FailureRecord`:**
- `mission_type: String`
- `target: Option<String>`
- `stop_reason: String`
- `violated_files: Vec<String>`
- `timestamp: String` (ISO 8601)

### Task 2.3: Capture Surface Violations into Failure Context

**File:** `tesaki/src/main.rs`

**Find the section around line 1872** where surface violations are detected (search for `"Surface policy violations detected"`).

**Modify this section to:**
1. Create a `PreviousFailureContext` with:
   - `mission_type` from current mission
   - `target` from current mission
   - `stop_reason` = "POLICY_VIOLATION"
   - `details` = formatted list of violated files
   - `violated_files` = the actual file list
   - `violated_surface` = which surface was violated
2. Store this context in `session.last_gate_failure`
3. Also append to `session.failure_history`

### Task 2.4: Inject Failure Context into Next Mission

**File:** `tesaki/src/main.rs`

**Find the `create_mission_bundle()` function** (or wherever `MissionContext` is constructed).

**Modify to:**
1. Check if `session.last_gate_failure` is `Some`
2. If so, include it in the `MissionContext.previous_failure` field
3. After successful inclusion, clear `session.last_gate_failure` (so it's not repeated)

### Task 2.5: Update MISSION.md Template for Failure Context

**File:** `tesaki/prompts/mission/MISSION.md.j2`

**Find the `{% if previous_failure %}` block** (around line 67-76).

**Enhance it to:**
1. Show the violated files prominently if `previous_failure.violated_files` exists
2. Include a strong directive: "DO NOT modify these files. They are in a LOCKED surface."
3. If the same target has failed multiple times (check `previous_failure.target` == current `target`), add an escalation note

### Task 2.6: Add Tests for Failure Memory

**File:** `tesaki/src/main.rs` (in test module) or new `tesaki/src/failure_memory_test.rs`

**Add tests that verify:**
1. Surface violation creates correct `PreviousFailureContext`
2. Context is injected into next mission's MISSION.md
3. Context is cleared after injection
4. Multiple failures for same target are tracked

**Verification:** `cargo test -p tesaki`

---

## Phase 3: Pre-Flight Plan Validation

**Goal:** Catch policy violations BEFORE expensive execution.

### Task 3.1: Define Plan Validation Protocol

**File:** `tesaki/src/plan_validator.rs` (NEW FILE)

**Create a new module** that validates proposed file changes against surface policy.

**Define these types:**
1. `struct ProposedPlan { files_to_modify: Vec<String> }`
2. `struct ValidationResult { valid: bool, violations: Vec<String>, guidance: String }`

**Implement function:**
- `validate_plan(plan: &ProposedPlan, spec_patterns: &[String], tests_patterns: &[String], sut_patterns: &[String], spec_locked: bool, tests_locked: bool, sut_locked: bool) -> ValidationResult`

This function should use the same glob matching logic as `check_surface_violations()` in `base_runner.rs`.

### Task 3.2: Add Plan Extraction from Runner Output

**File:** `tesaki/src/plan_validator.rs`

**Add a function** to extract proposed file changes from runner output before they're written.

**Strategy:** Many AI agents output their plan before executing. Look for patterns like:
- "I will modify these files:"
- "Files to change:"
- "Editing:"

**Implement:**
- `extract_proposed_files(runner_output: &str) -> Option<Vec<String>>`

This is a best-effort extraction. If no plan is found, return `None` and skip validation.

### Task 3.3: Integrate Plan Validation into Runner Flow

**File:** `tesaki/src/main.rs`

**Find where the runner is invoked** (search for `invoke_runner` or similar).

**Add a validation step:**
1. After runner completes but BEFORE checking git diff
2. If `extract_proposed_files()` returns Some, call `validate_plan()`
3. If validation fails, log the violations and include them in the failure context
4. This is informational for now (doesn't block) — actual enforcement remains in git diff check

### Task 3.4: Register the New Module

**File:** `tesaki/src/main.rs` (or `lib.rs`)

**Add:** `mod plan_validator;`

### Task 3.5: Add Tests for Plan Validation

**File:** `tesaki/src/plan_validator.rs`

**Add tests that verify:**
1. Valid plan passes validation
2. Plan with locked-surface files fails validation
3. File extraction handles various output formats
4. Guidance message is helpful

**Verification:** `cargo test -p tesaki -- plan_validator`

---

## Phase 4: Intelligent Escalation

**Goal:** When the loop stalls, provide actionable options instead of just stopping.

### Task 4.1: Define Escalation Types

**File:** `tesaki/src/escalation.rs` (NEW FILE)

**Create a new module** for escalation handling.

**Define enum `EscalationType`:**
- `SurfacePolicyBlocking` — Fix requires editing locked surface
- `RepeatedFailure` — Same issue failed multiple times with different approaches
- `NoProgressMultipleAttempts` — Tried but made no progress
- `UnknownBlocker` — Can't determine why it's stuck

**Define struct `EscalationContext`:**
- `escalation_type: EscalationType`
- `target: String`
- `attempts: u32`
- `tried_approaches: Vec<String>`
- `blocked_by: Option<String>` (e.g., "tests surface locked")
- `suggested_options: Vec<EscalationOption>`

**Define struct `EscalationOption`:**
- `id: String` (e.g., "unlock_tests")
- `label: String` (e.g., "Unlock tests surface")
- `description: String`

### Task 4.2: Implement Escalation Detection

**File:** `tesaki/src/escalation.rs`

**Implement function:**
- `detect_escalation(session: &SessionState, current_mission_type: &str, stop_reason: &StopReason) -> Option<EscalationContext>`

**Logic:**
1. If `stop_reason` is `PolicyViolation`, return `SurfacePolicyBlocking`
2. If same target has failed 2+ times with different approaches, return `RepeatedFailure`
3. If `NoProgress` for 3+ consecutive missions, return `NoProgressMultipleAttempts`
4. Populate `suggested_options` based on type

### Task 4.3: Create Escalation Prompt Template

**File:** `tesaki/prompts/escalation/blocked.md.j2` (NEW FILE)

**Create a template** that renders a helpful escalation message.

**Template should include:**
1. Clear header indicating human input is required
2. Summary of what was attempted
3. Analysis of why it's blocked
4. Numbered list of options
5. Request for user to choose an option

### Task 4.4: Register Escalation Template

**File:** `tesaki/src/prompts.rs`

**Modify `create_environment()`** to register the escalation template.

### Task 4.5: Add Escalation Rendering Function

**File:** `tesaki/src/prompts.rs`

**Add function:**
- `render_escalation_md(ctx: &EscalationContext) -> Result<String>`

### Task 4.6: Integrate Escalation into Autonomous Loop

**File:** `tesaki/src/repl.rs`

**Find the section** where the loop stops due to stall (search for `"All available mission types stalled"`).

**Replace the simple message with:**
1. Call `detect_escalation()` to get context
2. Call `render_escalation_md()` to get formatted output
3. Print the escalation prompt
4. If running interactively (REPL), wait for user input
5. If running headless (`--loop`), write escalation to a file and exit

### Task 4.7: Handle Escalation Options

**File:** `tesaki/src/repl.rs`

**For interactive mode**, add handling for escalation options:
1. "1" or "unlock_tests" → Update `session.intent.surface_overrides` to unlock tests
2. "2" or "unlock_spec" → Update to unlock spec
3. "3" or "skip" → Add current target to a skip list
4. "4" or "hint" → Prompt user for a hint, store in session context

### Task 4.8: Register the New Module

**File:** `tesaki/src/main.rs` (or `lib.rs`)

**Add:** `mod escalation;`

### Task 4.9: Add Tests for Escalation

**File:** `tesaki/src/escalation.rs`

**Add tests that verify:**
1. `PolicyViolation` triggers `SurfacePolicyBlocking`
2. Repeated failures trigger `RepeatedFailure`
3. Suggested options are populated correctly
4. Template renders valid markdown

**Verification:** `cargo test -p tesaki -- escalation`

---

## Phase 5: Cost Tracking and Efficiency Alerts

**Goal:** Make costs visible and alert on inefficiency.

### Task 5.1: Add Cost Estimation to Token Tracking

**File:** `tesaki/src/token_usage.rs`

**Add these fields to `SessionTokenStats`:**
- `estimated_cost_usd: f64`

**Add these constants** (or load from config):
- `OPUS_INPUT_COST_PER_1K: f64 = 0.015`
- `OPUS_OUTPUT_COST_PER_1K: f64 = 0.075`
- `SONNET_INPUT_COST_PER_1K: f64 = 0.003`
- `SONNET_OUTPUT_COST_PER_1K: f64 = 0.015`

**Add method:**
- `fn estimate_cost(&self) -> f64` — Calculate based on token counts and model used

### Task 5.2: Track Issues Resolved

**File:** `tesaki/src/session.rs`

**Add field to `SessionState`:**
- `issues_resolved: u32`

**Update in autonomous loop** after each successful mission.

### Task 5.3: Calculate Efficiency Metrics

**File:** `tesaki/src/token_usage.rs`

**Add method to `SessionTokenStats`:**
- `fn cost_per_issue(&self, issues_resolved: u32) -> Option<f64>`
- `fn efficiency_rating(&self, issues_resolved: u32) -> EfficiencyRating`

**Define enum `EfficiencyRating`:**
- `Excellent` — < $5/issue
- `Good` — $5-15/issue
- `Poor` — $15-30/issue
- `Critical` — > $30/issue

### Task 5.4: Add Efficiency Alerts to Session Summary

**File:** `tesaki/src/repl.rs`

**Find the session summary section** (search for `"SESSION SUMMARY"`).

**Add:**
1. Cost estimate line: "Estimated cost: $X.XX"
2. Cost per issue: "Cost per issue: $X.XX"
3. Efficiency rating: "Efficiency: Good" (color-coded if terminal supports it)
4. If rating is Poor or Critical, add warning message

### Task 5.5: Add Efficiency Check During Loop

**File:** `tesaki/src/repl.rs`

**In `run_autonomous_loop()`**, after each mission:
1. Calculate running efficiency
2. If last 2 missions used > $20 with 0 issues resolved, print warning
3. Don't stop automatically, but make the cost visible

### Task 5.6: Add Tests for Cost Tracking

**File:** `tesaki/src/token_usage.rs`

**Add tests that verify:**
1. Cost estimation is reasonably accurate for known inputs
2. Efficiency rating thresholds are correct
3. Cost per issue handles zero issues gracefully

**Verification:** `cargo test -p tesaki -- token_usage`

---

## Phase 6: Persistent Failure Learning (Session-to-Session)

**Goal:** Learn from failures across sessions.

### Task 6.1: Define Lessons Schema

**File:** `tesaki/src/lessons.rs` (NEW FILE)

**Create a new module** for persistent lessons.

**Define struct `Lesson`:**
- `id: String` (UUID or hash)
- `created: String` (ISO 8601 timestamp)
- `issue_key: String` (e.g., scenario key)
- `failure_mode: String`
- `attempted_approaches: Vec<String>`
- `blocked_by: Option<String>`
- `resolution: Option<String>` (filled when resolved)
- `notes: Option<String>`

**Define struct `LessonsDatabase`:**
- `version: u32`
- `lessons: Vec<Lesson>`

### Task 6.2: Implement Lessons File I/O

**File:** `tesaki/src/lessons.rs`

**Implement functions:**
- `load_lessons(spec_root: &Path) -> Result<LessonsDatabase>` — Load from `.tesaki/lessons.json`
- `save_lessons(spec_root: &Path, db: &LessonsDatabase) -> Result<()>`
- `add_lesson(db: &mut LessonsDatabase, lesson: Lesson)`
- `find_lessons_for_target(db: &LessonsDatabase, target: &str) -> Vec<&Lesson>`
- `mark_resolved(db: &mut LessonsDatabase, id: &str, resolution: &str)`

### Task 6.3: Record Lessons on Failure

**File:** `tesaki/src/main.rs`

**Find where failures are recorded** (surface violations, repeated failures, etc.).

**Add calls to:**
1. `load_lessons()`
2. Create new `Lesson` from failure context
3. Check if similar lesson already exists (same issue_key)
4. If exists, update `attempted_approaches`; if not, add new
5. `save_lessons()`

### Task 6.4: Inject Lessons into Mission Context

**File:** `tesaki/src/main.rs`

**Before creating mission bundle:**
1. Call `find_lessons_for_target()` with current target
2. If lessons exist, format them into a string
3. Add to `MissionContext` (new field needed)

### Task 6.5: Extend MissionContext for Lessons

**File:** `tesaki/src/prompts.rs`

**Add to `MissionContext`:**
- `previous_lessons: Option<Vec<LessonContext>>`

**Define `LessonContext`:**
- `failure_mode: String`
- `approaches_tried: Vec<String>`
- `blocked_by: Option<String>`

### Task 6.6: Update MISSION.md Template for Lessons

**File:** `tesaki/prompts/mission/MISSION.md.j2`

**Add a conditional section:**
```jinja
{% if previous_lessons %}
## Previous Attempts on This Issue
{% for lesson in previous_lessons %}
- Failure: {{ lesson.failure_mode }}
- Tried: {{ lesson.approaches_tried | join(", ") }}
{% if lesson.blocked_by %}- Blocked by: {{ lesson.blocked_by }}{% endif %}
{% endfor %}
DO NOT repeat these approaches.
{% endif %}
```

### Task 6.7: Mark Lessons Resolved on Success

**File:** `tesaki/src/main.rs`

**After a mission succeeds** (reduces issue count for its target):
1. Load lessons
2. Find lessons for that target
3. Mark them resolved with the successful approach
4. Save lessons

### Task 6.8: Register the New Module

**File:** `tesaki/src/main.rs` (or `lib.rs`)

**Add:** `mod lessons;`

### Task 6.9: Add Tests for Lessons

**File:** `tesaki/src/lessons.rs`

**Add tests that verify:**
1. Lessons file is created correctly
2. Lessons are found by target
3. Duplicate approaches are not re-added
4. Resolution marking works
5. File I/O handles missing file gracefully

**Verification:** `cargo test -p tesaki -- lessons`

---

## Phase 7: Stall Diagnosis Enhancement

**Goal:** When stopping, explain exactly why and what to try.

### Task 7.1: Create Diagnostic Report Type

**File:** `tesaki/src/diagnosis.rs` (NEW FILE)

**Create a new module** for stall diagnosis.

**Define struct `StallDiagnosis`:**
- `stop_reason: StopReason`
- `mission_type: String`
- `target: Option<String>`
- `attempts_made: u32`
- `issues_at_start: usize`
- `issues_at_end: usize`
- `approaches_tried: Vec<String>`
- `blocking_factors: Vec<String>`
- `recommended_actions: Vec<String>`

### Task 7.2: Implement Diagnosis Generation

**File:** `tesaki/src/diagnosis.rs`

**Implement function:**
- `diagnose_stall(session: &SessionState, state: &RepoState, last_stop: &StopReason) -> StallDiagnosis`

**Logic:**
1. Analyze `session.failure_history` for patterns
2. Check which surfaces are locked vs which the agent tried to edit
3. Check if issue count is unchanging
4. Generate specific recommendations based on findings

### Task 7.3: Create Diagnosis Template

**File:** `tesaki/prompts/diagnosis/stall_report.md.j2` (NEW FILE)

**Create a template** that renders a clear diagnosis report.

**Sections:**
1. "What Happened" — Summary of attempts
2. "Why It Stalled" — Blocking factors
3. "What To Try" — Specific recommendations with commands
4. "Technical Details" — For debugging

### Task 7.4: Register Diagnosis Template

**File:** `tesaki/src/prompts.rs`

**Modify `create_environment()`** to register the diagnosis template.

### Task 7.5: Integrate Diagnosis into Loop Exit

**File:** `tesaki/src/repl.rs`

**Find ALL places** where the loop exits (search for `break`, `return`, `"stopping"`, etc.).

**For each non-success exit:**
1. Generate diagnosis
2. Render and print diagnosis report
3. Write report to `.tesaki/last_stall_diagnosis.md`

### Task 7.6: Register the New Module

**File:** `tesaki/src/main.rs` (or `lib.rs`)

**Add:** `mod diagnosis;`

### Task 7.7: Add Tests for Diagnosis

**File:** `tesaki/src/diagnosis.rs`

**Add tests that verify:**
1. PolicyViolation produces correct blocking factors
2. Recommendations are actionable
3. Template renders correctly

**Verification:** `cargo test -p tesaki -- diagnosis`

---

## Phase 8: Configuration Enhancements

**Goal:** Make new features configurable.

### Task 8.1: Add New Config Options

**File:** `tesaki/src/config.rs`

**Add to `Config` struct:**
- `enable_failure_memory: Option<bool>` (default: true)
- `enable_lessons: Option<bool>` (default: true)
- `enable_cost_tracking: Option<bool>` (default: true)
- `cost_alert_threshold_usd: Option<f64>` (default: 20.0)
- `max_consecutive_failures: Option<u32>` (default: 2)

### Task 8.2: Wire Config to Features

**File:** `tesaki/src/repl.rs` and `tesaki/src/main.rs`

**Modify the autonomous loop** to check these config options before enabling features.

### Task 8.3: Document New Config Options

**File:** `tesaki/README.md` or inline comments in `config.rs`

**Document each new option** with description and default value.

### Task 8.4: Add Config Tests

**File:** `tesaki/src/config.rs`

**Add tests** that verify new options parse correctly and default appropriately.

**Verification:** `cargo test -p tesaki -- config`

---

## Phase 9: Integration Testing

**Goal:** Verify all components work together.

### Task 9.1: Create Integration Test Harness

**File:** `tesaki/tests/integration_flywheel.rs` (NEW FILE)

**Create integration tests** that simulate full session flows:
1. Setup: Create mock spec directory with known issues
2. Run: Execute autonomous loop with mock runner
3. Verify: Check that failure memory, escalation, etc. trigger correctly

### Task 9.2: Test Failure Memory Flow

**Scenario:**
1. Mission 1: Runner outputs plan to edit locked file
2. System: Detects violation, records failure context
3. Mission 2: Verify failure context is in MISSION.md
4. Verify: Context includes violated files

### Task 9.3: Test Escalation Flow

**Scenario:**
1. Same issue fails 2 times
2. System: Triggers escalation
3. Verify: Escalation message includes options
4. Verify: Options are actionable

### Task 9.4: Test Cost Tracking

**Scenario:**
1. Run 3 missions with known token counts
2. Verify: Cost estimates are within 10% of expected
3. Verify: Session summary includes cost metrics

### Task 9.5: Test Lessons Persistence

**Scenario:**
1. Session 1: Issue fails, lesson recorded
2. Session 2: Same issue, verify lesson is loaded and injected
3. Session 2: Issue succeeds, verify lesson marked resolved

**Verification:** `cargo test -p tesaki --test integration_flywheel`

---

## Phase 10: Documentation Updates

**Goal:** Keep documentation in sync with implementation.

### Task 10.1: Update AGENT_GUIDE.md

**File:** `namako/_AGENTS/AGENT_GUIDE.md`

**Add section** explaining the new flywheel features:
1. Failure memory — what it does, how it works
2. Escalation — when it triggers, what options mean
3. Lessons — how they persist, how to clear them

### Task 10.2: Update RUNBOOK.md

**File:** `namako/_WORKSPACE/RUNBOOK.md`

**Add:**
1. New config options with descriptions
2. How to interpret escalation prompts
3. How to read stall diagnosis reports

### Task 10.3: Update CURRENT_STATUS.md

**File:** `namako/_WORKSPACE/CURRENT_STATUS.md`

**Update to reflect:**
1. New version number (suggest: v2.0)
2. New features implemented
3. Link to this IMPL_PLAN.md for historical context

### Task 10.4: Archive This Plan

**After implementation is complete:**
1. Move `IMPL_PLAN.md` to `_WORKSPACE/ARCHIVE/IMPL_PLAN_v2_0.md`
2. Update status to "Implemented"

---

## Verification Checklist

After completing all phases, verify:

### Unit Tests
```bash
cargo test -p tesaki
# Should pass all existing + new tests
```

### Integration Tests
```bash
cargo test -p tesaki --test integration_flywheel
# Should pass all integration scenarios
```

### Manual Smoke Test
```bash
cd /path/to/target-repo
tesaki --loop 3
# Should show:
# 1. Constraint block at top of MISSION.md
# 2. Failure context when violations occur
# 3. Cost tracking in session summary
# 4. Diagnosis report on stall
```

### Documentation Check
- [ ] AGENT_GUIDE.md updated
- [ ] RUNBOOK.md updated
- [ ] CURRENT_STATUS.md updated
- [ ] Config options documented

---

## Appendix A: File Creation Checklist

New files to create:
1. `tesaki/prompts/components/critical_constraints.md.j2`
2. `tesaki/prompts/escalation/blocked.md.j2`
3. `tesaki/prompts/diagnosis/stall_report.md.j2`
4. `tesaki/src/plan_validator.rs`
5. `tesaki/src/escalation.rs`
6. `tesaki/src/lessons.rs`
7. `tesaki/src/diagnosis.rs`
8. `tesaki/tests/integration_flywheel.rs`

---

## Appendix B: Key Functions to Modify

Functions requiring significant modification:
1. `run_autonomous_loop()` in `repl.rs` — Add failure memory, escalation, diagnosis
2. `create_mission_bundle()` in `main.rs` — Inject failure context and lessons
3. `create_environment()` in `prompts.rs` — Register new templates
4. Session summary printing in `repl.rs` — Add cost metrics

---

## Appendix C: Testing Strategy

### Test Pyramid
- **Unit tests**: Each new module has internal tests
- **Integration tests**: `integration_flywheel.rs` tests full flows
- **Manual tests**: Smoke test against real target repo

### Mock Strategy
- Use mock runner that produces predictable output
- Use temp directories for lessons file testing
- Verify template rendering against expected strings

---

## Appendix D: Rollout Strategy

### Phase Order Rationale
1. **Constraint-First** — Highest impact, lowest risk
2. **Failure Memory** — Prevents immediate repeat mistakes
3. **Pre-Flight Validation** — Nice-to-have optimization
4. **Escalation** — Makes stalls actionable
5. **Cost Tracking** — Visibility without behavior change
6. **Lessons** — Long-term improvement
7. **Diagnosis** — Polish for user experience
8. **Config** — Allow tuning
9. **Integration Tests** — Confidence
10. **Docs** — Completeness

Each phase can be deployed independently. Earlier phases have no dependencies on later phases.

---

*End of Implementation Plan*
