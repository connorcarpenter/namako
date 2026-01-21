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

## Phase 0: v1.7 Validation & Transition (Current)

**Goal:** Verify v1.7 end-to-end, then transition to CONSUMPTION mode.

**Status:** Ready for testing.

### Steps

1. **Test `tesaki run` with mock runner**
   ```bash
   cd naia && tesaki run --runner mock
   ```

2. **Test `tesaki run` with Claude Code**
   ```bash
   cd naia && tesaki run --runner claude
   ```

3. **Validate mission bundle structure**
   - Check `.tesaki/missions/<id>/` contains expected files
   - Verify gate_result.json is written correctly

4. **Transition to CONSUMPTION mode**
   - Update CURRENT_STATUS.md: `MODE: CONSUMPTION`
   - Select first CORE scenario to validate full loop

### Exit Criteria

- [ ] `tesaki run` completes successfully with mock runner
- [ ] Mission bundle contains all required files
- [ ] Gate validation correctly classifies Pass/FailVerifyOnly/FailOther
- [ ] CONSUMPTION mode first mission completes

---

## Phase 1: RepoState Model (Foundation for v1.8)

**Goal:** Build the computed RepoState model that powers v1.8 features.

**Rationale:** Every v1.8 feature (stages, surface policies, mission types, intents) depends on having a rich, computed view of repository state. This is the foundation.

### Implementation

Create `tesaki/src/repo_state.rs`:

```rust
pub struct RepoState {
    // From namako status --json
    pub lint_status: GateStatus,
    pub run_status: GateStatus,
    pub verify_status: GateStatus,
    pub drift: Option<DriftInfo>,
    pub last_run_failures: Vec<FailureInfo>,

    // From namako review
    pub spec_issues: Vec<SpecIssue>,
    pub structure_issues: Vec<StructureIssue>,
    pub binding_issues: Vec<BindingIssue>,
    pub sut_issues: Vec<SutIssue>,
    pub global_blockers: Vec<Blocker>,

    // Computed task queue
    pub candidate_tasks: Vec<CandidateTask>,

    // Identity
    pub current_identity: Identity,
    pub baseline_identity: Option<Identity>,
}
```

### Deliverables

| File | Description |
|------|-------------|
| `tesaki/src/repo_state.rs` | RepoState struct + computation from packets |
| `tesaki/src/issue_classifier.rs` | Classify issues by category (spec/structure/binding/sut) |
| Tests | 10+ unit tests for state computation |

### Exit Criteria

- [ ] RepoState correctly computed from packets
- [ ] Issue classification matches DEV_EX §3 categories
- [ ] Candidate task queue derived from issues

---

## Phase 2: Edit-Surface Policies

**Goal:** Implement explicit Spec/Tests/SUT surface locks.

**Rationale:** v1.8 stages are fundamentally about controlling which surfaces can be edited. This must be in place before stages.

### Implementation

Create `tesaki/src/surface_policy.rs`:

```rust
#[derive(Clone, Debug)]
pub enum SurfaceLock {
    Locked,
    Unlocked,
}

#[derive(Clone, Debug)]
pub struct SurfacePolicy {
    pub spec: SurfaceLock,
    pub tests_bindings: SurfaceLock,
    pub sut: SurfaceLock,
}
```

### Default Policies by Stage

| Stage | Spec | Tests/Bindings | SUT |
|-------|------|----------------|-----|
| Refine Spec | UNLOCKED | LOCKED | LOCKED |
| Structure Spec | UNLOCKED (structure only) | LOCKED | LOCKED |
| Implement Tests | LOCKED | UNLOCKED | LOCKED |
| Implement SUT | LOCKED | LOCKED | UNLOCKED |
| Finalize | LOCKED | LOCKED | LOCKED |

### Mission Bundle Integration

Update `POLICY.md` generation to include surface policy:

```markdown
## Edit Surfaces

| Surface | Policy | Allowed Paths |
|---------|--------|---------------|
| Spec | LOCKED | `test/specs/**/*.feature` |
| Tests/Bindings | UNLOCKED | `test/tests/**`, `test/harness/**` |
| SUT | LOCKED | `src/**`, `client/**`, `server/**` |
```

### Exit Criteria

- [ ] SurfacePolicy struct implemented
- [ ] POLICY.md includes surface locks
- [ ] Runner can validate surface violations (optional: deferred if complex)

---

## Phase 3: Mission Types

**Goal:** Implement typed mission templates per DEV_EX §5.3.

**Rationale:** Mission types encode the "shape" of tasks: what inputs are needed, which surfaces are allowed, what validation signals indicate success.

### Core Mission Types (Priority 1)

| Type | Stage | Description |
|------|-------|-------------|
| `CreateMissingBindings` | Tests & Bindings | Create step bindings for runnable scenarios |
| `ImplementBehaviorForScenario` | SUT | Implement SUT to pass a failing scenario |
| `FixRegressionFromGateFailure` | SUT | Fix a gate failure |

### Spec Mission Types (Priority 2)

| Type | Stage | Description |
|------|-------|-------------|
| `RefineFeatureIntent` | Refine Spec | Improve feature intent comments |
| `AddOrClarifyScenario` | Refine Spec | Add/adjust scenarios |
| `NormalizeIdentityTags` | Structure Spec | Ensure @Feature/@Rule/@Scenario tags |

### Test Mission Types (Priority 2)

| Type | Stage | Description |
|------|-------|-------------|
| `StrengthenThenAssertions` | Tests & Bindings | Improve assertion quality |
| `RefactorBindingsForClarity` | Tests & Bindings | Clean step reuse |

### Meta Mission Types (Priority 3)

| Type | Stage | Description |
|------|-------|-------------|
| `ExplainState` | N/A | Synthesize state from packets (no runner) |
| `TriageFailures` | N/A | Cluster gate failures |

### Implementation

Create `tesaki/src/mission_type.rs`:

```rust
pub enum MissionType {
    // Core
    CreateMissingBindings { scenario_key: String },
    ImplementBehaviorForScenario { scenario_key: String },
    FixRegressionFromGateFailure { failure: FailureInfo },

    // Spec
    RefineFeatureIntent { feature_path: String },
    AddOrClarifyScenario { feature_path: String },
    NormalizeIdentityTags { feature_path: String },

    // Tests
    StrengthenThenAssertions { scenario_key: String },
    RefactorBindingsForClarity { binding_ids: Vec<String> },

    // Meta
    ExplainState,
    TriageFailures,
}

impl MissionType {
    pub fn default_surface_policy(&self) -> SurfacePolicy { ... }
    pub fn expected_evidence_change(&self) -> EvidenceChange { ... }
    pub fn generate_mission_brief(&self, state: &RepoState) -> String { ... }
}
```

### Exit Criteria

- [ ] All Priority 1 mission types implemented
- [ ] Mission type selection based on RepoState
- [ ] Mission brief generation per type

---

## Phase 4: 5-Stage Workflow

**Goal:** Implement the stage lens as a filter over task selection.

**Rationale:** With RepoState, surface policies, and mission types in place, stages become a thin coordination layer.

### Stage Definition

```rust
pub enum Stage {
    RefineSpec,
    StructureSpec,
    ImplementTests,
    ImplementSut,
    Finalize,
}

impl Stage {
    pub fn default_surface_policy(&self) -> SurfacePolicy { ... }
    pub fn applicable_mission_types(&self) -> Vec<MissionType> { ... }
    pub fn auto_advance_condition(&self, state: &RepoState) -> bool { ... }
}
```

### Auto-Stage Detection

Tesaki should infer the appropriate stage from RepoState:

1. **Refine Spec** — If spec has ambiguity or missing coverage
2. **Structure Spec** — If identity tags are missing
3. **Implement Tests** — If bindings are missing
4. **Implement SUT** — If tests exist but fail
5. **Finalize** — If all gates pass

### Exit Criteria

- [ ] Stage enum and default policies implemented
- [ ] Auto-stage detection from RepoState
- [ ] Stage displayed in mission bundle

---

## Phase 5: Interactive Sessions

**Goal:** Implement `tesaki` (no subcommand) for interactive TTY sessions.

**Rationale:** This is the capstone v1.8 feature. With all prior infrastructure, interactive sessions become "natural language over RepoState + mission dispatch."

### UX Flow

```
$ tesaki
> Reading repo state...
> Spec: 1 issue • Structure: 0 • Bindings: 4 missing • SUT: 2 failing

> Stage: Implement Tests
> Surfaces: Spec LOCKED • Tests UNLOCKED • SUT LOCKED
> Proposed: CreateMissingBindings for @Scenario(03)

You: Why is Scenario(03) missing bindings?

> Checking... The steps in Scenario(03) don't match existing binding patterns.
> Options:
>   1) Create new bindings (stay in Tests stage)
>   2) Reword scenario (switch to Refine Spec)

You: Create new bindings.

> Interpreted: Stage = Tests; Spec LOCKED.
> Running mission...
```

### Implementation Scope

| Component | Description |
|-----------|-------------|
| `tesaki/src/session.rs` | Session state management |
| `tesaki/src/intent_parser.rs` | Natural language → constraints |
| `tesaki/src/repl.rs` | TTY REPL loop |
| Integration | Connect to RepoState, missions, runners |

### Deferred (v1.9+)

- Multi-turn context tracking
- Undo/rollback capabilities
- Session persistence

### Exit Criteria

- [ ] `tesaki` starts interactive session
- [ ] User can view current state
- [ ] User can adjust stage/surface constraints
- [ ] User can trigger mission execution

---

## Phase 6: Mission Bundle v1.8 Updates

**Goal:** Align mission bundle structure with DEV_EX §8.

### Structure Changes

| v1.7 | v1.8 | Change |
|------|------|--------|
| `NEXT_TASK.md` | `MISSION.md` | Rename + add mission type metadata |
| `OUTPUT/` | `RUNNER_OUTPUT/` | Rename |
| `OUTPUT/gate_result.json` | `POST_GATE.json` | Move to root |
| N/A | `stop_reason.json` | Add structured stop reason |

### MISSION.md Content

```markdown
# Mission 001-create-bindings-abc123

**Type:** CreateMissingBindings
**Stage:** Implement Tests
**Target:** @Scenario(03) "client connects"

## Surfaces

| Surface | Policy |
|---------|--------|
| Spec | LOCKED |
| Tests | UNLOCKED |
| SUT | LOCKED |

## Objective

Create step bindings for the missing steps in Scenario(03).

## Missing Bindings

- `Given a server running on port {int}`
- `When the client connects`
- `Then the client receives a connection event`

## Validation

After runner exit:
1. `namako gate --json` must pass
2. Scenario(03) should be executable (not @Deferred)

---
*Generated by Tesaki v1.8*
```

### Exit Criteria

- [ ] Mission bundle structure matches DEV_EX §8
- [ ] MISSION.md includes mission type and surface policy
- [ ] POST_GATE.json at root level

---

## Immediate Actions

### For v1.7 Validation (Now)

1. [ ] Run `tesaki run --runner mock` on clean naia specs
2. [ ] Verify mission bundle creation
3. [ ] Test Claude Code runner integration
4. [ ] Document any issues found

### For v1.8 Phase 1 (Next)

1. [ ] Create `tesaki/src/repo_state.rs`
2. [ ] Define RepoState struct
3. [ ] Implement packet → RepoState computation
4. [ ] Add unit tests

---

## Success Metrics

### v1.7 Complete (Current Milestone)

| Metric | Target | Status |
|--------|--------|--------|
| `tesaki run` works | Yes | ✅ Ready for test |
| Mission bundle valid | Yes | ✅ Implemented |
| 54 tests pass | Yes | ✅ Complete |

### v1.8 Complete (Target Milestone)

| Metric | Target |
|--------|--------|
| Interactive session works | `tesaki` starts REPL |
| 5 stages implemented | All stage transitions work |
| 10+ mission types | Core + spec + test types |
| RepoState model | Correctly computed from packets |
| Surface policies | Enforced in mission bundles |

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
