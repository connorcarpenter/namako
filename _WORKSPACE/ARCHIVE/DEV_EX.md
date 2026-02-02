# DEV_EX.md — Tesaki Developer Experience (v1.8)

**Status:** Target UX / design spec (v1.8)
**Audience:** Developers using Tesaki + Namako to build a System Under Test (SUT), and maintainers implementing Tesaki
**Scope:** The end-to-end *interactive* developer experience driven by `tesaki` (session) and `tesaki run` (one mission cycle), powered by Namako packets, executed via one-shot Runner invocations.

---

## 0. What this is (in one paragraph)

Tesaki is your “personal Claude Code”—but with a real notion of repo state and a deterministic loop. You talk to Tesaki in natural language, Tesaki computes what’s true from Namako, selects the next smallest unit of work (a mission), dispatches a coding agent via a Runner (Claude Code / Codex CLI / etc.), validates via Namako again, and repeats until “done.” You can ask questions, redirect focus, or jump stages at any time; Tesaki adapts by recomputing state and continuing.

---

## 1. System boundaries (Tesaki vs Namako vs SUT vs Runner)

This boundary is non-negotiable.

### 1.1 Namako (base truth)
Namako is the reality tool. It inspects specs/tests/code and produces machine-readable packets.

**Namako responsibilities:**
- Parse specs (e.g., `.feature`)
- Resolve identity (feature/rule/scenario IDs)
- Analyze bindings coverage/quality
- Run gates (test execution + structured failures)
- Produce packets (`status`, `review`, `explain`, `gate`)

Namako does **not** orchestrate development. It measures truth.

### 1.2 Tesaki (driver/orchestrator)
Tesaki owns the development loop and developer experience.

**Tesaki responsibilities:**
- Start an interactive session (`tesaki`)
- Compute a `RepoState` from Namako packets
- Select the next mission (or answer questions without running a mission)
- Decide which edit surfaces are locked/unlocked for that mission
- Produce a mission bundle, invoke a Runner, collect outputs
- Re-run Namako gate, interpret results, decide to continue/stop
- Maintain a durable action record via mission bundles

Tesaki does **not** implement the SUT. It drives the work.

### 1.3 SUT (the system you’re building)
The SUT is your project (e.g., Naia). It is the thing that must satisfy the spec and pass gates.

### 1.4 Runner (one-shot executor)
A Runner is the controlled interface to a coding agent session (Claude Code, Codex CLI, etc.).

**Runner responsibilities:**
- Execute exactly one mission
- Apply edits within allowed surfaces
- Produce an attempt report + stop reason
- Never “keep going” beyond the mission boundary

Tesaki controls the loop. The Runner executes one mission.

---

## 2. The CLI experience (v1.8)

### 2.1 Primary commands

- `tesaki`
  Starts an **interactive session** (TTY). Natural language interface over a computed repo state. Tesaki continues running missions until done or blocked.

- `tesaki run`
  Runs **exactly one mission cycle** and exits (headless-friendly). Used for automation and “one step at a time” workflows.

Optional supporting commands (small surface area):
- `tesaki status` — short computed state summary (human + machine readable)
- `tesaki explain` — computed “what/why” view without running a mission
- `tesaki config` — config discovery + diagnostics

Everything else is expressed as *session intent* (Section 6), not a zoo of subcommands.

---

## 3. RepoState: truth computed from Namako packets

Tesaki’s source of truth is Namako packets, at minimum:

- `namako status --json`
  Inventory: features/rules/scenarios, IDs, step coverage, runnable scenarios, etc.

- `namako review --json`
  Quality signals: missing bindings, weak assertions, poor step hygiene, etc.

- `namako explain --json`
  Traceability: scenario_key ↔ steps ↔ bindings ↔ evidence; “why” explanations.

- `namako gate --json`
  Pass/fail gate and failure details (including runnable scenario failures).

Tesaki combines these into a single internal model:

- Spec issues (underspecified intent, missing cases)
- Structure issues (identity tags missing/invalid, parse/resolve errors)
- Test/binding issues (missing steps, low-quality Thens)
- SUT issues (tests exist and run, but fail)
- Global blockers (build/tooling breaks, environment constraints)
- Candidate task queue (derived from the above)

Tesaki never relies on “chat memory” to decide what’s next. It recomputes.

---

## 4. The 5-stage workflow (UX lens, not a rigid wizard)

Tesaki presents a familiar structure as a lens over task selection. You can jump at any time.

1) **Refine Spec**
2) **Structure Spec**
3) **Implement Tests & Bindings**
4) **Implement SUT Code**
5) **Finalize**

Stages are a *filter* and a default *edit-surface policy* (locks/unlocks). They are not the underlying engine.

### 4.1 Default edit-surface policy by stage

Tesaki controls three edit surfaces:

- **Spec surface** (e.g., `.feature` and other spec artifacts)
- **Test/bindings surface** (binding code, harness, test infra)
- **SUT surface** (implementation code for the system under test)

Each mission has an explicit surface policy: `LOCKED` or `UNLOCKED`.

Recommended defaults:

- **Refine Spec**
  - Spec: UNLOCKED
  - Tests/Bindings: LOCKED (unless explicitly requested)
  - SUT: LOCKED

- **Structure Spec**
  - Spec: UNLOCKED (structure/identity only)
  - Tests/Bindings: LOCKED (unless needed to keep bindings consistent with identity)
  - SUT: LOCKED

- **Implement Tests & Bindings**
  - Spec: LOCKED (unless explicitly requested)
  - Tests/Bindings: UNLOCKED
  - SUT: LOCKED

- **Implement SUT Code**
  - Spec: LOCKED
  - Tests/Bindings: LOCKED (unless explicitly requested to fix a bad test)
  - SUT: UNLOCKED

- **Finalize**
  - Spec: LOCKED (unless closing small doc gaps)
  - Tests/Bindings: LOCKED
  - SUT: LOCKED
  - Focus: verification, summaries, clean stopping point

You can override these in-session:
- “Spec locked; just do bindings.”
- “Unlock bindings too, this failure might be the test.”
- “Keep spec editable; I’m still shaping it.”

The point is not “limit files” or “limit LoC.” The point is **limit which surfaces can be edited**.

---

## 5. Missions: the atomic unit of progress

### 5.1 One mission at a time (always)
Even inside an interactive session, Tesaki dispatches exactly one mission per cycle:

1) Read packets → compute state
2) Select mission → write mission bundle
3) Run Runner (one-shot)
4) Re-run gate → update state
5) Continue or stop

This is what keeps the loop diagnosable and reviewable.

### 5.2 Mission Types vs Missions

#### Mission Type (reusable template)
A **Mission Type** is a named operation category that Tesaki understands. It encodes:
- the “shape” of the task
- required inputs
- allowed edit surfaces by default
- expected validation signals
- what Namako evidence should improve afterward

Mission Types are cross-project and form Tesaki’s “happy path toolkit.”

#### Mission (project-specific instance)
A **Mission** is a concrete application of a Mission Type to your repo at a specific moment:
- references specific scenario IDs / files / failures
- includes the exact constraints you set in-session
- produces a mission bundle with evidence and results

### 5.3 Canonical “happy path” Mission Types (v1.8)

**Spec refinement**
- `RefineFeatureIntent` — improve top-level feature intent comments (scope, non-goals, constraints)
- `AddOrClarifyScenario` — add/adjust scenarios to remove ambiguity or cover edges
- `ResolveAmbiguousRequirement` — turn “vibes” into falsifiable statements (may ask HUMAN_REQUIRED)

**Spec structure**
- `NormalizeIdentityTags` — ensure explicit Feature/Rule/Scenario IDs exist and are consistent
- `FixGherkinStructure` — repair malformed Gherkin, broken references, parse failures
- `ReconcileIdentityWithBindings` — update bindings references *only if necessary* to match identity changes

**Tests & bindings**
- `CreateMissingBindings` — create step bindings for runnable scenarios
- `StrengthenThenAssertions` — improve “Then” checks to be specific and stable
- `RefactorBindingsForClarity` — clean step reuse without collapsing meaning
- `StabilizeTestHarnessUsage` — fix harness misuse / flakiness patterns (still within tests surface)

**SUT implementation**
- `ImplementBehaviorForScenario` — implement missing behavior to satisfy a failing scenario
- `FixRegressionFromGateFailure` — diagnose and fix a new failure introduced recently
- `AlignImplementationToContract` — close gaps where SUT behavior contradicts spec

**Finalize**
- `SummarizeAndClose` — produce a short summary of what changed, what now passes, what remains
- `CleanupAfterSuccess` — ensure no leftover partial artifacts, ensure “done” is a clean stop

**Meta / inquiry (usually no Runner)**
- `ExplainState` — synthesize state and “why” from packets (Tesaki does this locally)
- `TriageFailures` — cluster gate failures into likely causes (also local unless deep changes needed)

These are templates. A real repo may add project-specific mission types later, but v1.8 should feel complete with the above.

---

## 6. Interactive session intents (natural language → constraints)

In a `tesaki` session, you speak naturally. Tesaki interprets your input as constraints on:

- stage lens (Refine/Structure/Bindings/SUT/Finalize)
- surface locks (spec/tests/SUT)
- what “done” means for this session (scope)
- which feature/scenario to focus
- whether to ask questions or act

Examples:

- “Focus on bindings only.”
  - Stage lens: Tests & bindings
  - Spec: LOCKED, SUT: LOCKED

- “Jump back to refining the spec; it’s unclear.”
  - Stage lens: Refine Spec
  - Spec: UNLOCKED, Tests/SUT: LOCKED

- “Tell me what’s failing and why.”
  - No mission; Tesaki runs `explain`-style synthesis

- “Don’t touch tests—fix the SUT.”
  - Stage lens: Implement SUT
  - Tests: LOCKED, SUT: UNLOCKED

- “Unlock spec too; I might change it as we go.”
  - Spec: UNLOCKED in subsequent missions unless you re-lock it

Tesaki should restate the interpreted constraint in one tight line before acting:
- “Got it: Stage = Bindings; Spec locked; SUT locked.”

---

## 7. Propagation semantics (the ripple effect)

A core v1.8 goal:

> If I or Tesaki edits spec/tests/SUT, the system automatically computes the downstream work until the repo is back to a clean, gated state.

This is not a special command. It is the default consequence of the loop:

- Spec edits → Namako packets change → new/changed scenarios appear → binding needs appear → SUT work appears → loop continues
- Binding edits → runnable set expands/changes → gate failures change → SUT work appears → loop continues
- SUT edits → failing scenarios shrink (or shift) → loop continues

Propagation is just: recompute state → pick next mission.

---

## 8. Mission bundle (durable record of what happened)

Each mission writes a bundle at:

`.tesaki/missions/<mission_id>/`

Recommended contents:

- `MISSION.md`
  The mission brief: objective, stage lens, surface locks, constraints, “why now,” and validation plan.

- `INPUTS/`
  Frozen inputs: relevant packets, excerpts, minimal context Tesaki used to select the mission.

- `RUNNER_OUTPUT/`
  Runner-produced artifacts:
  - `attempt_report.md` — what changed and why (human-readable)
  - `stop_reason.json` — DONE / HUMAN_REQUIRED / BLOCKED / FAILED (structured)
  - `transcript.txt` — optional; full I/O trace (only when enabled)

- `POST_GATE.json`
  The post-mission `namako gate --json` result (or a pointer to the stored packet).

This gives you a clean “what happened” trail without inventing extra sub-systems.

---

## 9. Validation: every mission must move the measured state

After every Runner invocation, Tesaki must run `namako gate --json` and interpret:

- Did the targeted issue improve?
- Did we introduce regressions?
- Are we done, blocked, or do we need a decision?

A mission is successful if:
- It achieved its declared objective, and
- The post-gate evidence matches the expected effect (even if other failures remain)

No mission should be “hand-wavy.” It must be falsifiable.

---

## 10. Stop reasons (explicit, machine-readable)

Every `tesaki run` ends with a stop reason. Interactive sessions surface the same reasons when they occur.

- `DONE`
  No eligible work remains within the current scope; gates are satisfactory.

- `HUMAN_REQUIRED`
  Tesaki needs a decision: ambiguity, tradeoff, or missing requirement.

- `BLOCKED`
  External dependency: environment/tooling/build broken in a way Tesaki cannot fix within allowed surfaces.

- `FAILED`
  Unexpected failure: Runner crash, tool error, inconsistent state.

Stop reasons must include:
- what was attempted
- what evidence triggered the stop (packet snippets/fields)
- what Tesaki recommends next

---

## 11. “Best dev experience” example (illustrative)

> Tesaki:
> Spec: 1 issue • Structure: 0 • Bindings: 4 missing • SUT: 2 failing • Verify: clean
> Proposed mission: `CreateMissingBindings` for Scenario `@Scenario(03)` “client connects”
> Surfaces: Spec LOCKED • Tests UNLOCKED • SUT LOCKED
> Validation: runnable count increases; missing bindings decreases; no new failures

> You: “Why is Scenario(03) missing bindings? I thought we had it.”

> Tesaki:
> Namako shows Scenario(03) has steps that don’t match any binding patterns. The earlier binding matches Scenario(02)’s wording, not this one.
> Options:
> 1) Update bindings to cover Scenario(03) (Bindings)
> 2) Reword Scenario(03) to match existing bindings (Spec)
> What do you want?

> You: “Update bindings. Keep spec locked.”

> Tesaki:
> Interpreted: Stage = Bindings; Spec LOCKED.
> Running one mission now.

(Then Tesaki runs exactly one mission via Runner, re-gates, and loops.)

---

## 12. Non-goals for v1.8

To keep v1.8 tight, we do not require:

- A large suite of stage-specific CLI subcommands (session intents cover it)
- Arbitrary numeric budgets like “max LoC changed” as a first-class concept
- Project implementation-plan concepts like “BOOTSTRAP vs CONSUMPTION” exposed to users
- Multi-runner consensus schemes
- Auto-commits, auto-branching, or repo hygiene decisions (developer-owned)

---

## 13. Glossary

- **Spec**: `.feature` files and other spec artifacts
- **Scenario**: a BDD scenario with stable explicit identity
- **Binding**: code that maps scenario steps to executable tests
- **SUT**: system under test (the implementation)
- **Packet**: Namako-produced JSON describing `status/review/explain/gate`
- **RepoState**: Tesaki’s computed internal view derived from packets
- **Mission Type**: reusable task template (cross-project)
- **Mission**: a project-specific instance of a mission type
- **Runner**: one-shot executor using a coding agent under the hood
- **Surface locks**: which edit surfaces are allowed for a mission

---

## 14. Summary

v1.8 is:

- `tesaki` starts an interactive, natural-language session
- `tesaki run` executes one mission cycle and exits
- Repo state is computed from Namako packets every cycle
- Progress happens one mission at a time, always re-validated by gate
- The 5-stage workflow is a flexible lens + default surface-lock policy
- Mission Types provide a standard “happy path toolkit”
- Mission bundles provide a durable record of what happened and why
- Boundaries are crisp: Namako measures truth, Tesaki drives, Runner executes, SUT evolves
