# NEXT_STEPS.md — Roadmap to Tesaki v1.8 Developer Experience

**Last Updated:** 2026-01-21
**Purpose:** Define the path from v1.7 (current implementation) to v1.8 (DEV_EX.md target UX)

---

## Executive Summary

**v1.7 Runner Integration is COMPLETE** per GOLD_PLAN.md §10.7. All core components are implemented: mission bundles, runner backends, gate classification, update-cert governance, retry logic, and config discovery.

**v1.8** is defined in DEV_EX.md and represents a significant UX evolution: interactive sessions, 5-stage workflow, edit-surface policies, typed mission types, and natural language session intents.

This document bridges the gap with a phased implementation plan.

---

## Gap Analysis: v1.7 → v1.8

### What v1.7 Has (GOLD_PLAN §10.7 — Complete)

| Component | Status | Location |
|-----------|--------|----------|
| Mission Bundle filesystem contract | ✅ | `tesaki/src/mission.rs` |
| Runner trait + backends (Mock, Claude, Codex) | ✅ | `tesaki/src/runner.rs`, `claude_code_runner.rs` |
| `tesaki run` single-mission command | ✅ | `tesaki/src/main.rs` |
| Stop conditions (DONE, BLOCKED, etc.) | ✅ | `tesaki/src/stop_reason.rs` |
| Gate outcome classification | ✅ | `tesaki/src/gate.rs` |
| Update-cert governance | ✅ | `tesaki/src/main.rs` |
| Config discovery (.tesaki/config.toml) | ✅ | `tesaki/src/config.rs` |
| Workspace tracking | ✅ | `tesaki/src/workspace.rs` |

### What v1.8 Requires (DEV_EX.md — Not Yet Implemented)

| Feature | DEV_EX Section | Gap Description |
|---------|----------------|-----------------|
| **Interactive sessions** | §2.1 | `tesaki` (no subcommand) starts TTY session with natural language |
| **RepoState model** | §3 | Rich computed model combining status/review/explain/gate packets |
| **5-stage workflow** | §4 | Refine Spec → Structure Spec → Tests & Bindings → SUT → Finalize |
| **Edit-surface policies** | §4.1 | Explicit Spec/Tests/SUT locks per stage |
| **Mission Types** | §5.2–5.3 | 17+ typed mission templates (RefineFeatureIntent, CreateMissingBindings, etc.) |
| **Session intents** | §6 | Natural language → constraint interpretation |
| **Propagation semantics** | §7 | Automatic "ripple effect" computation |
| **Updated mission bundle** | §8 | MISSION.md, RUNNER_OUTPUT/, POST_GATE.json structure |

---

## Phase 5: Interactive Sessions (v1.8)

**Goal:** Implement `tesaki` (no subcommand) as an interactive TTY REPL that feels like Claude Code, while enforcing: **the model can only “think + talk” and request allowlisted `namako`/`tesaki` commands**. Tesaki is the only process that executes commands or dispatches mission Runners.

**Key idea:** *Stateless LLM chat planner per turn* + *allowlisted CLI execution* + *explicit commit point (mission proposal → execute)*.

### Non-negotiable constraints (safety + simplicity)

1. The model never reads the repo directly.
2. The model never runs shell commands directly.
3. The only “world observations” the model receives are **stdout/stderr from allowlisted commands** that Tesaki executed.
4. The model can propose a mission, but Tesaki creates the mission bundle and only executes it when the user explicitly approves (or default-autopilot mode is enabled).

### REPL loop (high level)

Each user message triggers a small loop until the “turn” is complete:

1) Tesaki calls a **Chat Planner** (LLM) with:
   - user message
   - `SessionState` (small, structured)
   - recent command results (stdout/stderr for this turn)
2) Chat Planner returns a **ChatPlan JSON**:
   - `say` (text to print)
   - `run[]` (allowlisted `namako`/`tesaki` commands to execute next)
   - optional `mission_proposal` (typed mission candidate)
   - `done` (bool)
3) Tesaki:
   - prints `say`
   - executes `run[]` **only if commands are allowlisted**
   - appends command results and (if `done=false`) calls the Chat Planner again
4) If a mission is proposed:
   - Tesaki shows it as a **MISSION PROPOSAL** (type, target, surfaces, validation)
   - waits for user approval (`run it` / `execute`)
5) On approval:
   - Tesaki writes the mission bundle
   - Tesaki dispatches the configured **Runner** for exactly one mission
   - Tesaki runs `namako gate --json` and summarizes results
   - loop continues with updated RepoState

### SessionState (what persists between turns)

Persist only small, structured state (no giant transcript):

- `intent` (stage lens + surface locks + scope/focus)
- `last_packets_fingerprint` (detect when we need to refresh)
- `pending_mission_id` (or full proposal struct)
- `recent_missions` (IDs + stop reasons)
- optional `chat_summary` (short rolling summary; bounded)

### UX Flow (example)

```

$ tesaki

> Reading repo state...
> Spec: 1 issue • Structure: 0 • Bindings: 4 missing • SUT: 2 failing

> Stage: Implement Tests & Bindings
> Surfaces: Spec LOCKED • Tests UNLOCKED • SUT LOCKED
> Proposed: CreateMissingBindings for @Scenario(03)

You: Why is Scenario(03) missing bindings?

> Running: namako explain --json (target: Scenario(03))
> ...
> The steps in Scenario(03) don't match any binding patterns currently registered.
> Options:
>
> 1. Create new bindings (stay in Tests stage)
> 2. Reword scenario (unlock Spec)

You: Create new bindings.

> Interpreted: Stage = Tests; Spec LOCKED.
> MISSION PROPOSAL ready: CreateMissingBindings for @Scenario(03).
> Say "run it" to execute, or ask questions.

You: run it

> Executing mission via Runner...
> Post-gate: missing bindings decreased (4 → 1). New failures: none.
> Next proposal: CreateMissingBindings for @Scenario(05)

````

### Implementation Scope

| Component | Description |
|-----------|-------------|
| `tesaki/src/repl.rs` | TTY loop, input handling, printing, turn iteration |
| `tesaki/src/session.rs` | `SessionState` + persistence (in-memory for v1.8) |
| `tesaki/src/chat_plan.rs` | JSON schema structs (`ChatPlan`, `AllowedCommand`, `MissionProposal`) |
| `tesaki/src/chat_planner.rs` | Calls LLM backend (Claude Code / Codex) in *plan-only* mode; parses JSON |
| `tesaki/src/allowlist.rs` | Enforces only `namako` + `tesaki` commands; rejects everything else |
| Integration | Connect to RepoState computation, mission selection, bundle writer, and Runner dispatch |

### Deferred (v1.9+)

- Session persistence across process restarts (resume/continue)
- Richer multi-turn memory beyond bounded `chat_summary`
- Undo/rollback primitives
- UI niceties (search history, bookmarks, etc.)

### Exit Criteria

- [ ] `tesaki` starts interactive session and prints computed RepoState summary
- [ ] Chat Planner can only request allowlisted `namako`/`tesaki` commands (enforced)
- [ ] User can set stage + surface locks via natural language
- [ ] Tesaki can present a Mission Proposal, wait for approval, then dispatch Runner
- [ ] After execution, Tesaki re-runs `namako gate --json` and summarizes deltas


---

## Phase 6: Mission Bundle v1.8 Updates

**Goal:** Align mission bundle structure with DEV_EX §8, and support mission proposals coming from the interactive session.

### Structure Changes

| v1.7 | v1.8 | Change |
|------|------|--------|
| `NEXT_TASK.md` | `MISSION.md` | Rename + include mission type + surfaces + validation |
| `OUTPUT/` | `RUNNER_OUTPUT/` | Rename |
| `OUTPUT/gate_result.json` | `POST_GATE.json` | Move to root |
| N/A | `RUNNER_OUTPUT/stop_reason.json` | Add structured stop reason |
| N/A | `INPUTS/packets/*` | Store relevant Namako packets used to justify mission |

### MISSION.md Content

```markdown
# Mission 001-create-bindings-abc123

**Type:** CreateMissingBindings
**Stage:** Implement Tests & Bindings
**Target:** @Scenario(03) "client connects"

## Surfaces

| Surface | Policy |
|---------|--------|
| Spec | LOCKED |
| Tests | UNLOCKED |
| SUT | LOCKED |

## Objective

Create step bindings for the missing steps in Scenario(03).

## Inputs

- `INPUTS/packets/status.json`
- `INPUTS/packets/review.json`
- `INPUTS/packets/explain_scenario_03.json` (if produced)

## Validation

After runner exit:
1. Run `namako gate --json` and save as `POST_GATE.json`
2. Missing bindings count must decrease for Scenario(03)
3. No new gate failures introduced (unless explicitly acknowledged)

---
*Generated by Tesaki v1.8*
````

### Exit Criteria

* [ ] Mission bundle structure matches DEV_EX §8
* [ ] `MISSION.md` includes mission type + stage + surface policy + validation
* [ ] `POST_GATE.json` at root level
* [ ] `RUNNER_OUTPUT/stop_reason.json` always present

---

## Immediate Actions (updated)

### For v1.7 Validation (Now)

1. [ ] Run `tesaki run --runner mock` on a clean specs repo
2. [ ] Verify v1.7 mission bundle creation + post-gate storage
3. [ ] Verify Claude Code Runner integration still works
4. [ ] Capture any runner quirks that will affect Phase 5 planning mode

### For v1.8 Phase 1 (Next)

1. [ ] Create `tesaki/src/chat_plan.rs` with the ChatPlan JSON structs
2. [ ] Implement `tesaki/src/allowlist.rs` for `namako` + `tesaki` commands only
3. [ ] Implement `tesaki/src/chat_planner.rs` in “plan-only JSON” mode
4. [ ] Implement `tesaki/src/repl.rs` turn loop (print → plan → run commands → plan → done)
5. [ ] Integrate mission proposal → bundle writer → Runner dispatch → post-gate summary

---

## Success Metrics (updated)

### v1.7 Complete (Current Milestone)

| Metric               | Target              | Status           |
| -------------------- | ------------------- | ---------------- |
| `tesaki run` works   | Yes                 | ✅ Ready for test |
| Mission bundle valid | Yes                 | ✅ Implemented    |
| Runner integration   | Claude Code + Codex | ✅ Implemented    |

### v1.8 Complete (Target Milestone)

| Metric                    | Target                                                      |
| ------------------------- | ----------------------------------------------------------- |
| Interactive session works | `tesaki` starts REPL                                        |
| Plan-only chat            | LLM can only request allowlisted `namako`/`tesaki` commands |
| 5 stages implemented      | All stage transitions + surface locks work                  |
| Mission proposals         | Generated in-session and require approval to execute        |
| Mission execution         | `tesaki run` executes exactly one mission cycle             |
| Post-gate summaries       | Show deltas after every mission                             |

````

---

## Document Consistency Notes

### GOLD_PLAN.md

GOLD_PLAN.md §10.7 specifies v1.7 Runner Integration and is accurate for the current implementation. v1.8 features from DEV_EX.md would require a new §10.8 section in GOLD_PLAN.md if we want to maintain GOLD_PLAN as the authoritative spec.

**Recommendation:** After v1.8 is designed and validated, add GOLD_PLAN.md §10.8 with the full v1.8 specification.

### DEV_EX.md

DEV_EX.md is the design spec for v1.8 UX. It is internally consistent but represents a significant evolution from v1.7. No changes needed; it serves as the target.

### CURRENT_STATUS.md

Update after each phase completes to reflect:
- v1.7 → v1.8 progress
- MODE transitions
- Test counts

---

*End of NEXT_STEPS.md*
