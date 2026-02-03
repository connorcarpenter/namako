# TODO.md — PATH_FORWARD Implementation Plan

**Last Updated:** 2026-02-03  
**Canonical vision:** `namako/_WORKSPACE/PATH_FORWARD.md`  
**Purpose:** Step-by-step execution plan to implement PATH_FORWARD in code.

## Notes

- This TODO is the actionable plan. PATH_FORWARD is the vision and rationale.
- Some code comments still reference an older TODO. Treat those as legacy.

## Phase 0 — Baseline & Verification

- [ ] Confirm current behavior in code:
  - AddOrClarifyScenario progress uses `spec_issues` (not scenario count).
  - No mission chaining AddOrClarifyScenario → CreateMissingBindings.
  - No rule-count invariant gate.
  - No 2-scenario coverage heuristic in DONE criteria.
- [ ] Record a short baseline summary in CURRENT_STATUS if new facts emerge.

## Phase 1 — Executability Fix (Mission Chaining)

- [ ] Add scenario counts per rule to RepoState (feature parsing).
  - Target: `namako/tesaki/src/repo_state.rs`
  - Data: `scenarios_per_rule: HashMap<String, usize>`
- [ ] Update AddOrClarifyScenario progress evaluation:
  - Success if scenario count increases in target feature.
  - Do **not** require spec_issues to decrease on this mission.
  - Target: `namako/tesaki/src/main.rs` (progress evaluation)
- [ ] Implement mission chaining:
  - After AddOrClarifyScenario success, enqueue CreateMissingBindings.
  - Evaluate spec_issues only after bindings are created.
  - Target: `namako/tesaki/src/repl.rs` or mission scheduler
- [ ] Add tests for scenario counting + chaining behavior.

## Phase 2 — Rule-Count Invariant (Scope Creep Gate)

- [ ] Count rules before/after AddOrClarifyScenario.
  - Parse feature file and count `Rule:` entries.
  - Store pre-mission count in mission bundle or state.
- [ ] Add deterministic gate:
  - Reject mission if `rules_after > rules_before`.
  - Revert and retry with stricter prompt constraints.
- [ ] Update AddOrClarifyScenario brief to include:
  - “Do NOT add new Rule blocks”
  - Validation warning that rule count increase will be rejected.
- [ ] Add tests for rule-count invariant rejection.

## Phase 3 — Coverage Heuristic (DONE Criteria)

- [ ] Implement coverage heuristic:
  - A rule is “adequately covered” if it has ≥ 2 executable scenarios.
  - A feature is “complete” if all rules are adequately covered.
- [ ] Update DONE criteria in the loop to include coverage heuristic.
- [ ] Update mission selection to target rules with < 2 scenarios.
- [ ] Add tests for heuristic and DONE criteria.

## Phase 4 — LLM Coverage Assessment (Optional)

- [ ] Add “AssessSpecCoverage” flow for ambiguous cases:
  - Trigger only when rules with 1 scenario or > 4 scenarios exist.
- [ ] Implement 3-judge self-consistency assessment:
  - Same model, 3 samples, majority vote.
  - Locked rubric with 5 criteria, pass at ≥ 4.0/5.0.
- [ ] Store assessment results and summary gaps.

## Phase 5 — Documentation & Cleanup

- [ ] Update `namako/_WORKSPACE/CURRENT_STATUS.md` after each phase.
- [ ] Update `namako/_WORKSPACE/GOLD_PLAN.md` references to removed docs.
- [ ] Update `namako/_AGENTS/SYSTEM.md` “Files to Read First”.
- [ ] When all phases are complete, delete `namako/_WORKSPACE/PATH_FORWARD.md`.

