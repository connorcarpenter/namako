# GOLD_PLAN.md

## The Authoritative Specification System for Naia

---

## Table of Contents

1. [Executive Summary](#part-1-executive-summary)
2. [Architecture and Modes](#part-2-architecture-and-modes)
    - [2.3 Two FSMs: Bootstrap vs Product](#23-two-fsms-bootstrap-vs-product-normative)
    - [2.4 Modes: BOOTSTRAP vs CONSUMPTION](#24-modes-bootstrap-vs-consumption-normative)
    - [2.5 Bootstrap Exit Criteria](#25-bootstrap-exit-criteria-normative)
    - [2.6 Consumption Entry Criteria](#26-consumption-entry-criteria-normative)
    - [2.7 First CONSUMPTION Mission Template](#27-first-consumption-mission-template-normative)
3. [Canonical Repo & Crate Architecture](#part-3-canonical-repo--crate-architecture)
4. [Step Macro UX and Binding Identity](#part-4-step-macro-ux-and-binding-identity)
5. [Namako CLI Commands](#part-5-namako-cli-commands)
6. [NPA: Adapter Protocol](#part-6-npa-adapter-protocol)
    - [6.4.3 Scenario Key Derivation](#643-scenario-key-derivation-normative)
7. [Hashing & Identity Contract](#part-7-hashing--identity-contract)
    - [7.0 Hash & Encoding Contract — Single Source of Truth](#70-hash--encoding-contract--single-source-of-truth)
8. [Current Limitations (v2 Will Address)](#part-8-current-limitations-v2-will-address)
9. [Tesaki: The AI Driver Layer (No Inference in Namako)](#part-9-tesaki-the-ai-driver-layer-no-inference-in-namako)
10. [The Spec-Driven Development Loop](#part-10-the-spec-driven-development-loop)
    - [10.5 AI-Enablement Features](#part-105-ai-enablement-features)
    - [10.7 Runner Integration](#part-107-runner-integration--tesaki--coding-agent-integration)
    - [10.8 Interactive Developer Experience (v1.8)](#part-108-tesaki-v18--interactive-developer-experience)
11. [Namako v2 — Deferred Publish-Grade Features](#part-11-namako-v2--deferred-publish-grade-features)
    - [11.11 Multi-Language Support](#1111-multi-language-support-language-neutral-engine-language-specific-adapters)
    - [11.12 Adapter SDKs](#1112-adapter-sdks-v2)
    - [11.13 Cross-Language Hashing & Conformance](#1113-cross-language-hashing--conformance-v2)
    - [11.14 Adapter Certification Tooling](#1114-adapter-certification-tooling-v2)
12. [Definition of Done](#part-12-definition-of-done)
13. [Appendix: Concept Checklist](#appendix-concept-checklist)

---

## Part 1: Executive Summary

This document specifies **Namako**, the authoritative spec-driven development system for Naia. Namako is a fork of the `cucumber` crate, renamed and pruned for our purposes.

The core workflow is:

```
spec (.feature) → engine(resolve) → plan → adapter(execute) → evidence → verify
```

**Namako** provides:
- Strict resolution of `.feature` files to step bindings
- Plan-driven execution (adapter executes by binding ID only, no text matching)
- Hash-based integrity evidence
- CI-gated certification via `namako verify`
- Explicit identity tags (`@Feature`, `@Rule(nn)`, `@Scenario(nn)`) for refactor-safe scenario keys
- Rich AI-actionable packets (`review`, `explain`, `status --json`)
- Orphan binding detection with `namako stub` mitigation

**Namako v2** will add publish-grade hardening features (see Part 11).

### The Core Insight: Plan-Driven Integrity

Drift occurs when the linter thinks a step matches Binding A, but the runtime matches Binding B. **Namako eliminates this class of error entirely.**

The **Namako Engine** is the sole source of matching logic. It resolves every step into a **Resolved Execution Plan**. The project adapter is **structurally forbidden** from performing text matching; it simply executes the Binding IDs dictated by the Engine.

### Language-Neutral by Design

Namako is **language-agnostic**: the engine/CLI is a Rust tool, but adapters MAY be implemented in **any programming language** (Rust, JS/TS, Python, Go, C++, etc.). The adapter protocol (NPA) is the only cross-language integration boundary. Currently ships with Rust adapter support for Naia; v2 will expand to official SDKs and conformance tooling for other ecosystems.

### Tesaki (AI Driver) — Inference Lives Above Namako

Namako MUST be **inference-free** and **deterministic**. It produces plans, evidence, and verification results, plus machine-readable **packets** that describe what work remains.

**Tesaki** is the AI driver/orchestrator that:
- consumes Namako packets (`review`, `explain`, `status`)
- performs LLM inference to propose patches (spec edits, scenario promotion, bindings, harness, SUT implementation)
- re-runs Namako gates (`lint` → `run` → `verify`) until a milestone is achieved

This separation ensures reproducibility, auditability, and model/provider independence.

---

## Part 2: Architecture and Modes

### 2.1 Current Capabilities

| Capability | Status | Notes |
|------------|--------|-------|
| Explicit ID tags | ✅ | `@Feature(name)`, `@Rule(nn)`, `@Scenario(nn)` |
| Orphan binding hard error | ✅ | With `namako stub` mitigation |
| Rich AI packets | ✅ | `review`, `explain`, `status --json` |
| Runner integration | ✅ | `tesaki run` with Claude Code/Codex backends |
| Feature fingerprint hashing | ✅ | Content-based (cosmetic changes affect hash) |
| Canonical JSON encoding | ✅ | Deterministic serialization |
| Rust adapter (Naia) | ✅ | NPA protocol |

### v1.8 Planned Enhancements

| Feature | Description | Section |
|---------|-------------|---------|
| Interactive sessions | `tesaki` starts TTY session with natural language | §10.8.2 |
| RepoState model | Computed internal state from Namako packets | §10.8.3 |
| 5-stage workflow | Refine Spec → Structure → Tests → SUT → Finalize | §10.8.4 |
| Edit-surface policies | Explicit Spec/Tests/SUT locks per mission | §10.8.5 |
| Mission Types | Typed task templates (17+ types) | §10.8.6 |
| Session intents | Natural language → constraint interpretation | §10.8.7 |

### v2 Planned Enhancements

| Feature | Description |
|---------|-------------|
| FeatureAstNorm | Cosmetic-change-immune hashing |
| CBOR encoding | Cross-platform byte reproducibility |
| Multi-language SDKs | JS/TS, Python, Go adapter support |
| Conformance fixtures | Regression safety for adapters |

### Namako vs Tesaki (Normative)

- Namako MUST NOT trigger AI inference.
- Tesaki MUST NOT bypass Namako gates.
- Tesaki MAY be replaced (Claude Code wrapper, local runner, CI bot) without changing Namako semantics.

| Layer | Responsibility | Determinism |
|------|----------------|------------|
| Namako | parse/resolve/plan/verify + packet outputs | MUST be deterministic |
| NPA adapter | execute plan by binding_id + emit run_report | MUST be deterministic for same inputs |
| Tesaki | LLM-driven patch generation + loop control | NOT deterministic (by nature), but must be gated |

### 2.2 Design Principle: No Dead Ends

The current system is designed such that every v2 feature can be adopted incrementally via:
- Version bumps (`hash_contract_version`, `binding_id_scheme`)
- Additive schema changes
- Identity regeneration (via `update-cert`)

No current decision MUST be reversed to adopt v2.

### 2.3 Two FSMs: Bootstrap vs Product (Normative)

This specification defines two distinct finite state machines. **Do not confuse these layers.**

#### 2.3.1 Bootstrap Loop (Construction Process)

The loop that Opus/Claude follows **while building the Namako/Tesaki toolchain**. This is NOT the Product FSM.

**Bootstrap Loop States:**
1. Read CURRENT_STATUS.md + GOLD_PLAN.md
2. Identify next toolchain gap (Namako CLI, Tesaki driver, harness, adapter)
3. Implement minimal fix
4. Run gates (`namako gate`, `namako gate --determinism`, `cargo test`)
5. Update docs (CURRENT_STATUS.md, TODO.md)
6. Stop (human reviews diff)

**Scope:** Building the spec-driven development infrastructure itself.

#### 2.3.2 Tesaki Product FSM (End-State Process)

The FSM that Tesaki will run autonomously to drive `spec → scenario → bindings → implementation`. This is the target workflow described in Part 9 and Part 10.

**Product FSM States:** AuthorContract → ReviewBacklog → PromoteScenarios → Resolve → BindSteps → Execute → Verify → BlessBaseline → MilestoneComplete (per §9.2)

**Scope:** Using the completed toolchain to develop the Naia product.

### 2.4 Modes: BOOTSTRAP vs CONSUMPTION (Normative)

A single mode flag governs what edits are permitted:

| Mode | Description |
|------|-------------|
| `BOOTSTRAP` | Default. Building the toolchain. Product core is off-limits. |
| `CONSUMPTION` | Using the toolchain to build the product. Product core edits allowed via Tesaki loop. |

**Current mode MUST be recorded in `CURRENT_STATUS.md`.**

#### 2.4.1 BOOTSTRAP Allowed Edit Surface

- `namako/**` (Namako CLI, Tesaki, engine crates)
- `naia/test/**` (harness, tests, adapter crate `naia_npa`, specs, scripts)
- Docs under `_WORKSPACE/**`

#### 2.4.2 BOOTSTRAP Forbidden Edit Surface

Naia product core (anything outside `naia/test/**`):
- `naia/client/**`
- `naia/server/**`
- `naia/shared/**`
- `naia/adapters/**`
- Any other Naia crate not under `test/`

**Violation handling:** Revert and record incident in CURRENT_STATUS.md.

#### 2.4.3 CONSUMPTION Allowed Edit Surface

All BOOTSTRAP surfaces, **plus** Naia product core — but ONLY when driven by a selected contract/scenario via the Tesaki Product FSM.

### 2.5 Bootstrap Exit Criteria (Normative)

The system transitions from `MODE=BOOTSTRAP` to `MODE=CONSUMPTION` only when ALL of the following are satisfied:

1. **Tesaki end-to-end:** `tesaki run` runs and produces a deterministic mission bundle
2. **Namako packets deterministic:** `status --json`, `review`, `explain` all produce stable, usable outputs
3. **Tesaki autonomous capabilities:**
   - Can select promotion candidates when `reuse_score > 0`
   - Can implement bindings for suggested bundles
   - Can run gates (lint → run → verify)
   - Can update baseline (with governance)
   - Stops safely when blocked (no infinite loops)
4. **Scenario fidelity workflow exists:** Full fidelity via `namako explain`
5. **CI green:** All gates pass (`namako gate`, `namako gate --determinism`, `cargo test -p tesaki`)

### 2.6 Consumption Entry Criteria (Normative)

**Rule:** Naia core work begins ONLY after `CURRENT_STATUS.md` declares `MODE = CONSUMPTION`.

Until that declaration:
- All autonomous loops MUST stay within BOOTSTRAP allowed surfaces
- Any drift into Naia core is a violation requiring revert

### 2.7 First CONSUMPTION Mission Template (Normative)

When transitioning from BOOTSTRAP to CONSUMPTION, the first mission SHOULD follow this template:

1. **Select ONE CORE blocker** from the promotion candidates
2. **Define the minimal observable contract:**
   - What MUST become observable (client events, server state)
   - What MUST NOT happen (panic, ambiguous errors)
   - Minimal implementation boundaries (no unrelated refactors)
3. **Drive end-to-end through the Tesaki Product FSM:**
   - Promote the @Deferred scenario (remove tag)
   - Implement missing step bindings
   - Implement minimal Naia core changes
   - Run gates until green
   - Update baseline with governance
4. **Keep scope minimal:** One scenario, one mission, no scope creep

This template ensures controlled, incremental expansion of the certified surface.

---

## Part 3: Canonical Repo & Crate Architecture

### 3.1 Namako Repo (fork of `cucumber`, renamed/pruned)

The Namako repo MUST contain exactly these crates:

#### 3.1.1 `namako` (lib)
The core engine/runtime library. Contains:
- Resolution logic
- Artifact schemas (or re-exports)
- Hashing utilities (or re-exports)
- Verification logic

Dependencies:
- `gherkin` (Gherkin parser)
- `cucumber_expressions` (expression matching)
- `namako_codegen` (proc-macros)

#### 3.1.2 `namako_codegen` (proc-macro)
Formerly `cucumber_codegen`. Owns:
- Step macros (`#[given(...)]`, `#[when(...)]`, `#[then(...)]`)
- Registry generation

#### 3.1.3 `namako_cli` (bin)
Provides the CLI commands:
- `manifest` — Debug: prints adapter semantic registry + hashes
- `lint` — Resolve features, generate resolved_plan, fail on strict errors
- `run` — Lint + execute plan via adapter + produce run report + validate integrity
- `verify` — CI gate: candidate identity == baseline identity
- `update-cert` — Manual: writes baseline cert (MUST refuse unless prerequisites satisfied)
- `status` — Diff identity vs metadata (plus JSON output)

### 3.2 Naia Repo (project integrating Namako)

The Naia repo MUST contain exactly these crates:

#### 3.2.1 `naia_test_harness` (lib)
This is a rename of existing `naia_test` (the test harness).
- Implements the Namako "World" type used by step bindings
- Encapsulates `naia_test_harness::Scenario` (1 server + N clients using local transport)
- Concurrency-immune by design (local channels, not sockets)

#### 3.2.2 `naia_tests` (lib)
Contains all step binding functions.
- Step functions use Namako step macros from `namako_codegen`
- Depends on `naia_test_harness` to construct World and drive scenarios

#### 3.2.3 `naia_npa` (bin)
The NPA adapter binary for Naia.
- Links `naia_tests` so all bindings/registry/dispatch are present
- Implements:
  - `naia_npa manifest` — prints registry JSON
  - `naia_npa run --plan ... --out ...` — executes resolved plan by `binding_id` only, emits `run_report`

### 3.3 File Locations (Normative)

| Artifact | Location |
|----------|----------|
| `.feature` files | `test/specs/features/**/*.feature` (Naia repo) |
| Baseline certification | `test/specs/certification.json` (Naia repo) |
| Artifacts directory | `target/namako_artifacts/` (Naia repo, or as configured) |

---

## Part 4: Step Macro UX and Binding Identity

### 4.1 UX Requirement: One Macro + One String (Hard Requirement)

Step functions MUST be declared using exactly:

```rust
#[given("...")]
fn some_given_step(mut ctx: TestWorldMut) { ... }

#[when("...")]
fn some_when_step(mut ctx: TestWorldMut) { ... }

#[then("...")]
fn some_then_step(ctx: TestWorldRef) { ... }
```

Each macro takes **exactly one string argument**.
- No additional attributes
- No additional metadata
- No embedded IDs in strings
- No optional parameters

**Step Function Signatures (Context-First ABI):**

The ABI enforces **capability separation** at compile time through context types:

- **Given/When steps**: First parameter MUST be `ctx: &mut CtxMut` (mutable reference to context)
- **Then steps**: First parameter MUST be `ctx: &CtxRef` (immutable reference to read-only context)

The context types MUST implement the `StepContext` trait to link back to their World type.

```rust
// Given/When: mutation context only (no assertions)
#[given("a user named {string} with role {string}")]
fn user_with_role(ctx: &mut TestWorldMut, username: String, role: String) { ... }

// Then: read-only context only (no mutations)
#[then("the user count is {int}")]
fn check_user_count(ctx: &TestWorldRef, expected: usize) { ... }

// Captures + DocString
#[when("the server receives config")]
fn server_config(ctx: &mut TestWorldMut, config_doc: Option<String>) { ... }

// Captures + DataTable
#[then("the following users exist")]
fn users_exist(ctx: &TestWorldRef, users_table: Option<Vec<Vec<String>>>) { ... }
```

The function signature determines `signature.captures_arity`, `signature.accepts_docstring`, and `signature.accepts_datatable` in the manifest (see §4.4 for the normative ABI definition).

### 4.2 Generated Binding ID (Normative)

User code MUST NOT contain explicit binding IDs. The system MUST ALWAYS generate `binding_id` deterministically from:
- `effective_kind` (Given/When/Then)
- `expression_string` (the literal string inside the macro)

#### 4.2.1 Binding ID Scheme (Normative)

Define `expr_norm` as the macro string normalized by:
1. Unicode normalization to NFC
2. Newline normalization to `\n`

> **Note:** Do not add other normalizations (e.g., whitespace collapsing). Keep it simple.

Define:
```
binding_id = blake3_256_lowerhex( "namako-binding-id|" + kind + "|" + expr_norm )
```

The semantic registry MUST include:
```
binding_id_scheme = "kind+expr_norm|namako-binding-id|blake3-256-lowerhex"
```

`binding_id_scheme` MUST be included in the `step_registry_hash` computation.

> **v2 Note:** The binding-id scheme is chosen specifically because it is **portable across languages and tooling**. Any adapter in any language can compute the same `binding_id` from the same `(kind, expression_string)` pair using the documented algorithm and BLAKE3.

#### 4.2.2 Collision Rule (Normative)

If two bindings in a single project produce the same `(kind, expr_norm)`:
- That is a **hard error** (registry construction MUST fail).
- Rationale: identity collision creates operational ambiguity.

### 4.3 Dispatch Rule (Normative)

The adapter MUST:
- Execute steps **only by binding_id** using a direct lookup/dispatch table
- NOT perform text matching or regex at runtime
- Treat `step_text` as metadata only

### 4.4 Binding ABI (Normative)

This section defines how `namako_codegen` derives signature metadata from the step function signature. This is the authoritative definition for signature enforcement in §5.3.

#### 4.4.0 Context-First ABI (Capability Separation)

The ABI enforces **capability separation at compile time** through typed context parameters:

| Step Kind | Required First Parameter | Capabilities |
|-----------|--------------------------|--------------|
| Given | `ctx: &mut CtxMut` (mutable reference) | Mutation operations ONLY |
| When | `ctx: &mut CtxMut` (mutable reference) | Mutation operations ONLY |
| Then | `ctx: &CtxRef` (immutable reference) | Read/assertion operations ONLY |

**Rationale:** This design makes it structurally impossible to:
- Call mutation operations from a Then step (read-only context has no mutation methods)
- Call assertion operations from a Given/When step (mutable context has no assertion methods)

**Context Types:**
- `CtxMut` (e.g., `TestWorldMut<'a>`) — wraps mutable access to the World, exposes ONLY mutation APIs
- `CtxRef` (e.g., `TestWorldRef<'a>`) — wraps read-only access to the World, exposes ONLY assertion APIs

Both context types MUST implement `namako::codegen::StepContext` to provide the `World` type association.

**Trampoline Generation:**

The adapter still receives `&mut World` internally. The `namako_codegen` macro generates a trampoline that:
1. Receives `&mut World` from the adapter runtime
2. Constructs the appropriate context:
   - Given/When: `let mut ctx = world.ctx_mut();`
   - Then: `let ctx = world.ctx_ref();`
3. Calls the user function with the context as a reference (`&mut ctx` or `&ctx`)

#### 4.4.1 Required First Parameter

Every step function MUST have a context reference as its first parameter:
- **Given/When**: `ctx: &mut CtxMut` where `CtxMut: StepContext`
- **Then**: `ctx: &CtxRef` where `CtxRef: StepContext`

The World type is derived from the context type via the `StepContext::World` associated type.

#### 4.4.2 Captures Mapping

- **`signature.captures_arity`** equals the number of capture parameters after the context parameter, **excluding** any optional DocString/DataTable parameters.
- All captures are passed as `String` currently (typed capture conversion is deferred to v2).
- Captures appear in the function signature in the same order as their corresponding `{...}` placeholders in the expression string.

**Example:**
```rust
#[given("a {string} named {string}")]
fn example(mut ctx: TestWorldMut, type_name: String, entity_name: String) { ... }
// captures_arity = 2
```

#### 4.4.3 DocString Support

- If the binding accepts a DocString, it MUST include an `Option<String>` parameter (or a `DocString` wrapper type) after all capture parameters.
- `signature.accepts_docstring = true` if this parameter is present; `false` otherwise.
- If a step does NOT include a DocString at runtime, the adapter passes `None` / `null`.

#### 4.4.4 DataTable Support

- If the binding accepts a DataTable, it MUST include an `Option<Vec<Vec<String>>>` parameter (or a `DataTable` wrapper type) after all capture parameters and after any DocString parameter.
- `signature.accepts_datatable = true` if this parameter is present; `false` otherwise.
- If a step does NOT include a DataTable at runtime, the adapter passes `None` / `null`.

#### 4.4.5 Parameter Order (Normative)

When both DocString and DataTable are supported, the parameter order MUST be:
1. Context parameter (`ctx: &mut CtxMut` or `ctx: &CtxRef`)
2. Capture parameters (in expression order)
3. DocString parameter (if present)
4. DataTable parameter (if present)

This fixed order ensures deterministic signature reflection by `namako_codegen`.

#### 4.4.6 Signature Constraints

- Exactly **zero or one** DocString parameter allowed per binding.
- Exactly **zero or one** DataTable parameter allowed per binding.
- Ambiguous signatures (e.g., multiple `Option<String>` parameters that could be DocString or captures) MUST be rejected by `namako_codegen` at compile time.
- **Given/When with read-only context (`CtxRef`)** → MUST be rejected at compile time.
- **Then with mutable context (`CtxMut`)** → MUST be rejected at compile time.

> **Note:** The Binding ABI is what `namako_codegen` uses to compute the `signature.*` fields in the adapter manifest.

---

## Part 5: Namako CLI Commands

### 5.1 Current Scope: What is IN

The system MUST include:

| Capability | Description |
|------------|-------------|
| Gherkin parsing | Parse `.feature` files via `gherkin` crate |
| Step resolution | Resolve steps to bindings via `cucumber_expressions` |
| Strict resolution errors | Missing steps (0 matches) → hard error |
| | Ambiguity (>1 match) → hard error |
| | Signature mismatch → hard error |
| Resolved plan artifact | `resolved_plan.json` |
| Run report artifact | `run_report.json` |
| Certification artifact | `certification.json` (baseline + candidate concept) |
| Deterministic identity tuple | See §7 |
| CI gate | `namako verify` (strict identity compare) |
| Manual baseline update | `namako update-cert` (only when explicitly invoked + prerequisites satisfied) |

### 5.2 Deferred to v2

| Deferred Feature | Rationale |
|------------------|-----------|
| Full FeatureAstNorm hashing | Simpler fingerprint is sufficient for current use |
| CBOR canonical encoding profiles | Currently uses canonical JSON; v2 may migrate |
| Malicious adapter defense | Out of scope (trusted adapter assumption; v2 adds conformance tooling) |
| Conformance fixtures with canonical bytes | Deferred to v2 |
| `resolution_semantics_id` | Deferred to v2; currently uses simpler versioning |
| Multi-language adapter SDKs | Currently Rust only; v2 adds JS/TS, Python, Go |

### 5.3 CLI Commands (Normative)

#### `namako manifest`
**Purpose:** Debug. Prints adapter semantic registry + hashes.

#### `namako lint`
**Purpose:** Resolve features + generate resolved_plan + fail on strict errors.

**Behavior:**
1. Parse all `.feature` files
2. Fetch adapter manifest (semantic registry)
3. Resolve each step to exactly one binding
4. Validate signatures (captures arity, docstring/datatable expectations)
5. Generate `resolved_plan.json`
6. Exit 0 on success, non-zero on any error

**Strict Errors:**
- Missing step (0 matches)
- Ambiguous step (>1 match)
- Signature mismatch (see below)

**Signature Mismatch Definition (Normative):**

A signature mismatch occurs when the step's requirements do not match the binding's declared capabilities. The binding's signature metadata is derived from the function signature per the Binding ABI (§4.4):

| Check | Rule |
|-------|------|
| **Captures arity** | The number of captures produced by matching the expression to the step text MUST equal `signature.captures_arity` (per §4.4.2) |
| **DocString requirement** | If the step includes a DocString, the binding MUST declare `accepts_docstring = true` (per §4.4.3) |
| **DataTable requirement** | If the step includes a DataTable, the binding MUST declare `accepts_datatable = true` (per §4.4.4) |

**Handling absent DocString/DataTable:**
- If a step does NOT include a DocString, the binding MAY declare `accepts_docstring = true` or `false` (binding receives `null`)
- If a step does NOT include a DataTable, the binding MAY declare `accepts_datatable = true` or `false` (binding receives `null`)
- The adapter MUST pass `null` for absent DocString/DataTable regardless of binding declaration

> **KISS (current):** Captures are always strings. Typed capture conversion is deferred to v2.

#### `namako run`
**Purpose:** Lint + execute plan via adapter + produce run report + validate integrity.

**Behavior:**
1. Execute `lint` (fail if lint fails)
2. Invoke adapter: `adapter run --plan <resolved_plan.json> --out <run_report.json>`
3. Validate run report integrity (see §7.4)
4. Exit 0 on success, non-zero on any failure

> **Note:** `namako run` MUST execute the plan produced by the current `namako lint` resolution step (i.e., current engine semantics). Subsequently, `namako verify` will independently recompute and confirm that the resolved plan matches current sources.

#### `namako verify`
**Purpose:** CI gate. Candidate identity MUST equal baseline identity. Verify is the **authority** — it recomputes hashes from current sources.

**Behavior:**
1. Ensure a `run_report.json` exists
2. **Recompute** all authority hashes from current sources (see §7.4.1):
   - `feature_fingerprint_hash` from current `.feature` files
   - `step_registry_hash` from current adapter manifest
   - `resolved_plan_hash` from freshly recomputed resolved plan (not on-disk file)
3. Validate that run report header hashes match recomputed values; fail with `STALE OR DRIFTED ARTIFACT` if any mismatch (see §7.4.3)
4. Validate per-step integrity (binding IDs, payload hashes, impl hashes per §7.4.2)
5. Compare candidate identity to baseline identity with strict equality
6. Exit 0 if all checks pass, non-zero on any mismatch

**Prerequisite:** A successful `namako run` MUST have completed.

#### `namako update-cert`
**Purpose:** Manual action. Overwrites baseline certification with current candidate.

**Behavior:**
1. MUST refuse to write baseline unless:
   - `namako lint` passes with zero errors
   - `namako run` completes and all scenarios are `Passed`
2. If prerequisites satisfied, write `certification.json`

**Rationale:** Certification is never updated implicitly.

#### `namako status`
**Purpose:** If present, clearly diff identity vs metadata.

**Behavior:**
- Show identity fields that differ (blocking)
- Show metadata fields that differ (informational)

---

## Part 6: NPA: Adapter Protocol

### 6.0 Language Neutrality (Normative)

NPA is **language-neutral**. Adapters MAY be implemented in any programming language as long as they:
- Implement the `manifest` and `run` commands per this specification
- Obey all schema and invariant requirements
- Dispatch by `binding_id` only (no runtime text matching)

The Namako Engine/CLI MUST treat the adapter as an **external executable** invoked via the configured `adapter_cmd`. The engine MUST NOT depend on project language runtimes.

### 6.1 Versioning

All artifacts MUST include:
- `npap_version` — Protocol version (use `1`)
- `hash_contract_version` — Identifies encoding + hashing rules (currently: `"namako-json+blake3-256"`)

### 6.2 Command: `adapter manifest`

The adapter MUST implement:
```
naia_npa manifest
```

Returns the **semantic step registry** as JSON.

#### 6.2.1 Semantic Step Registry (Normative)

**Per Binding:**

| Field | Type | Description |
|-------|------|-------------|
| `binding_id` | string | Generated per §4.2 |
| `kind` | string | `"Given"`, `"When"`, or `"Then"` |
| `expression` | string | The cucumber expression string |
| `signature.captures_arity` | u32 | Number of captures expected |
| `signature.accepts_docstring` | bool | Whether binding accepts docstring |
| `signature.accepts_datatable` | bool | Whether binding accepts datatable |
| `impl_hash` | string | Drift signal (see §6.2.2) |

**Registry Header:**

| Field | Type | Description |
|-------|------|-------------|
| `npap_version` | u32 | Protocol version |
| `hash_contract_version` | string | Encoding + hashing rules |
| `binding_id_scheme` | string | Per §4.2.1 |
| `impl_hash_scheme` | string | Per §6.2.2 |
| `step_registry_hash` | string | Hash of the semantic registry |

**Registry Ordering and Hashing (Normative):**

The `step_registry_hash` MUST be computed as follows:
1. Construct a registry object containing:
   - `npap_version`
   - `hash_contract_version`
   - `binding_id_scheme`
   - `impl_hash_scheme`
   - `bindings`: an array of all binding entries
2. The `bindings` array MUST be sorted by `binding_id` (lexicographic ascending) before hashing
3. Apply `canonical_json_encode()` per §7.0.3
4. Compute: `step_registry_hash = blake3_256_lowerhex( canonical_json_encode( registry_without_step_registry_hash ) )`

The manifest JSON emission MUST use the same sorted order for bindings.

> **Rationale:** Sorting by `binding_id` ensures that discovery order (e.g., from proc macros) does not affect the hash. This makes registry identity deterministic across builds.

#### 6.2.2 `impl_hash` (Requirements)

`impl_hash` MUST change when the binding implementation changes. It serves as a drift signal to detect when implementation code has been modified.

**Scheme (Normative):**

The manifest header MUST include:
```
impl_hash_scheme = "token-fingerprint|blake3-256-lowerhex"
```

**Computation (Normative):**

The proc macro MUST compute `impl_hash` as follows:
1. Extract the token stream of the binding function body (excluding the function signature and attributes)
2. Normalize the token stream:
   - UTF-8 encoding
   - Unicode NFC normalization
   - Newlines normalized to `\n`
   - Whitespace collapsed to single spaces between tokens
   - Comments MUST be excluded
   - Absolute file paths MUST NOT appear in the fingerprint (use relative or omit)
3. Compute: `impl_hash = blake3_256_lowerhex( normalized_token_fingerprint )`

**Determinism Guarantee:**

The `impl_hash` MUST be deterministic across builds on the same codebase:
- Same source code → same `impl_hash`
- Different build directory paths MUST NOT affect the hash
- Reformatting (whitespace/newlines) MAY affect the hash (acceptable; v2 may strengthen)

> **Rationale:** Token-based fingerprinting avoids the pitfalls of raw source hashing (path sensitivity, comment drift) while remaining implementable in a proc macro.

> **v2 Note:** Stronger schemes may capture dependency signals or use AST-based normalization (see §10.9).

### 6.3 Command: `adapter run`

The adapter MUST implement:
```
naia_npa run --plan <resolved_plan.json> --out <run_report.json>
```

#### 6.3.1 Runtime Rules (Normative)

The adapter:
1. MUST refuse to run if plan's `step_registry_hash` does not match current manifest hash
2. MUST refuse to run if any `binding_id` in plan does not exist in registry
3. MUST execute steps **by binding_id dispatch only** (no text matching)
4. MUST treat `step_text` as non-executable metadata
5. MUST compute `executed_payload_hash` using the same rules as `planned_payload_hash`
6. MUST emit `executed_impl_hash` (from semantic registry entry of invoked binding)

#### 6.3.2 Freshness Check (Normative)

Before execution, the adapter MUST verify:
- `plan.header.step_registry_hash == current_manifest.step_registry_hash`
- All `binding_id`s in plan exist in registry
- Signatures match (arity, docstring, datatable expectations)

If any check fails, the adapter MUST refuse to execute and exit non-zero.

### 6.4 Artifact Schemas

#### 6.4.1 Resolved Plan (`resolved_plan.json`)

```json
{
  "header": {
    "npap_version": 1,
    "hash_contract_version": "namako-json+blake3-256",
    "feature_fingerprint_hash": "...",
    "step_registry_hash": "...",
    "resolved_plan_hash": "..."
  },
  "scenarios": {
    "<scenario_key>": {
      "steps": [
        {
          "effective_kind": "Given",
          "step_text": "server is running",
          "binding_id": "abc123...",
          "captures": [],
          "docstring": null,
          "datatable": null,
          "payload_hash": "..."
        }
      ]
    }
  }
}
```

> **Note (Normative):** For hashed objects (including resolved plan steps), optional fields such as `docstring` and `datatable` MUST be explicitly present. Absence MUST be encoded as `null`, not omitted.

**Scenario Key:** Use a deterministic key derived from explicit identity tags per §6.4.3 (e.g., `connection_lifecycle:Rule(01):Scenario(03)`).

#### 6.4.2 Run Report (`run_report.json`)

```json
{
  "header": {
    "npap_version": 1,
    "hash_contract_version": "namako-json+blake3-256",
    "feature_fingerprint_hash": "...",
    "step_registry_hash": "...",
    "resolved_plan_hash": "..."
  },
  "scenarios": [
    {
      "scenario_key": "<scenario_key>",
      "status": "Passed",
      "steps": [
        {
          "planned_binding_id": "abc123...",
          "executed_binding_id": "abc123...",
          "planned_payload_hash": "...",
          "executed_payload_hash": "...",
          "executed_impl_hash": "...",
          "status": "Passed"
        }
      ]
    }
  ]
}
```

**Ordering:**
- Scenarios: ordered by `scenario_key` (lexicographic)
- Steps: in plan order
- Object keys: sorted (for determinism)

**Header Echo:** The run report MUST echo the plan header fields exactly.

#### 6.4.3 Scenario Key Derivation (Normative)

The `scenario_key` MUST be globally unique within a project and MUST be derived deterministically from explicit identity tags.

**Derivation Rule (Explicit IDs):**

Scenario keys are derived from explicit identity tags (`@Feature(name)`, `@Rule(nn)`, `@Scenario(nn)`):

```
scenario_key = FeatureName + ":" + "Rule(" + nn + ")" + ":" + "Scenario(" + mm + ")"
```

For scenarios directly under a Feature (no Rule):
```
scenario_key = FeatureName + ":" + "Scenario(" + mm + ")"
```

**Examples:**
```
connection_lifecycle:Rule(01):Scenario(03)
namako_smoke_test:Scenario(01)
```

**Scenario Outline Extension:**

For Scenario Outlines with Examples tables, each example row generates a distinct scenario. The key MUST include the example identifier:

```
scenario_key = FeatureName + ":" + "Rule(" + nn + ")" + ":" + "Scenario(" + mm + ")" + ":E" + eid
```

Where `eid` is:
- The value from the `EID` column if present
- Otherwise, the 0-based row index within the Examples block

**Example:**
```
auth:Rule(02):Scenario(01):Evalid_token
auth:Rule(02):Scenario(01):E0
```

**Collision Detection (Normative):**

If two scenarios in a project compute the same `scenario_key`:
- Lint MUST emit a **hard error**: `SCENARIO KEY COLLISION: <key>`
- This indicates duplicate `(Feature, Rule(nn), Scenario(nn))` tuples or a bug in key derivation

**Required Tags (Normative per §10.5.1):**
- Features MUST have `@Feature(name)` tag
- Rules MUST have `@Rule(nn)` tag
- Scenarios MUST have `@Scenario(nn)` tag

### 6.5 Execution Payload Contract (Normative)

> **See §7.0 for authoritative encoding and hashing rules.**

The **Execution Payload** for each step consists of:
- `effective_kind`
- `binding_id`
- `captures` (array of strings)
- `docstring` (normalized string or null)
- `datatable` (normalized cells or null)
- `step_text` (exact AST string)

**Normalization Rules (per §7.0.2):**
- DocStrings: line endings normalized to `\n`
- DataTables: exact cell strings from AST, Unicode NFC
- Strings: Unicode normalized to NFC
- **Optional fields in hashed objects:** MUST be explicitly present; absence MUST be encoded as `null` (not omitted)

**Payload Hash (per §7.0.3 and §7.0.6):**
```
payload_hash = blake3_256_lowerhex( canonical_json_encode( ExecutionPayload ) )
```

---

## Part 7: Hashing & Identity Contract

### 7.0 Hash & Encoding Contract — Single Source of Truth

This section is the **authoritative reference** for all hashing and encoding rules in the current system. All other sections MUST defer to these rules. Implementers MUST follow this section exactly to achieve deterministic, reproducible hashes.

#### 7.0.1 Hash Domain Constraints (Normative)

Hashed objects MUST contain **only** the following types:
- **Strings** (UTF-8)
- **Booleans** (`true` / `false`)
- **Integers** (signed or unsigned; represented without decimal points)
- **Arrays** (ordered lists)
- **Objects** (with string keys only)
- **Null** (for explicitly absent optional fields)

**Forbidden in hashed objects:**
- **Floats are forbidden.** All numeric values in hashed objects MUST be integers.
- Timestamps, durations, file paths, and platform-specific information MUST be placed in metadata sections only (not hashed), unless explicitly normalized to strings per this specification.

#### 7.0.2 String Normalization (Normative)

Before hashing (and before canonical JSON encoding), all strings MUST be normalized as follows:
1. **Encoding:** UTF-8
2. **Unicode normalization:** NFC (Canonical Decomposition, followed by Canonical Composition)
3. **Newline normalization:** All newline sequences (`\r\n`, `\r`) MUST be converted to `\n`

This applies to:
- Expression strings in bindings
- Step text
- DocStrings
- DataTable cell values
- Any other string content in hashed objects

#### 7.0.3 Canonical JSON Rules (Normative)

All hashed objects MUST be encoded using the following canonical JSON rules:

| Rule | Specification |
|------|---------------|
| **Object key ordering** | Keys MUST be sorted lexicographically (by Unicode code point) |
| **Array ordering** | Arrays MUST preserve their semantic order (unless a specific sort is defined) |
| **No comments** | JSON MUST NOT contain comments |
| **No trailing commas** | JSON MUST NOT contain trailing commas |
| **Encoding** | UTF-8 |
| **Minimal escaping** | Only required JSON escapes (`\"`, `\\`, control characters) |
| **Integers only** | All numbers MUST be integers (floats are forbidden per §7.0.1) |
| **No leading zeros** | Integer representation MUST NOT have leading zeros (except `0` itself) |
| **Null for absent optionals** | Optional fields in hashed objects MUST be present; absence MUST be encoded as `null` |

**Definition:** `canonical_json_encode(object)` means: apply string normalization (§7.0.2), then encode the object under these rules.

#### 7.0.4 Sorting Rules for Hashed Collections (Normative)

When hashing collections, the following sort orders MUST be applied:

| Collection | Sort Key | Order |
|------------|----------|-------|
| Semantic registry bindings | `binding_id` | Lexicographic ascending |
| Run report scenarios | `scenario_key` | Lexicographic ascending |
| Steps within a scenario | Plan order | Preserve plan sequence |
| Feature fingerprint files | Relative path | Lexicographic ascending |

Any other sets or maps in hashed objects MUST specify their sort key in their schema definition.

#### 7.0.5 Self-Hash Exclusion Rule (Normative)

When computing an object's own hash:
- **Omit only** the field that will store that object's own hash
- **Do NOT omit** other hash fields that are inputs to the object's identity

**Example:**
- When computing `resolved_plan_hash`, omit only `header.resolved_plan_hash`
- Do NOT omit `header.step_registry_hash` or `header.feature_fingerprint_hash` (these are inputs)

#### 7.0.6 Hash Algorithm (Normative)

The system uses **BLAKE3-256** for all hash computations:
- Output: 256-bit hash
- Encoding: lowercase hexadecimal string (64 characters)
- Notation: `blake3_256_lowerhex(...)`

### 7.1 Hash Contract Versioning (Normative)

> **See §7.0 for the complete hash and encoding contract.**

The system MUST define:
```
hash_contract_version = "namako-json+blake3-256"
```

This identifies:
- Canonical JSON encoding (per §7.0.3)
- BLAKE3-256 hash algorithm (per §7.0.6)

This version string MUST be included in every hashed artifact header.

### 7.2 Self-Hash Exclusion Rule (Normative)

> **See §7.0.5 for the authoritative definition of the self-hash exclusion rule.**

When hashing an object:
- Omit **only** the field that stores that object's own hash
- Do NOT omit other hash fields that are part of the object's identity

**Example:**
- When computing `resolved_plan_hash`, omit `header.resolved_plan_hash`
- Do NOT omit `header.step_registry_hash` (it's input, not output)

### 7.3 Identity Tuple (Normative)

The certification artifact (`certification.json`) contains `{ identity, metadata }`.

**Identity (strictly compared by `verify`):**

| Field | Description |
|-------|-------------|
| `hash_contract_version` | Encoding + hashing rules |
| `feature_fingerprint_hash` | Hash of feature content (current: simpler than FeatureAstNorm) |
| `step_registry_hash` | Hash of semantic step registry |
| `resolved_plan_hash` | Hash of resolved execution plan |

**Metadata (recorded, not compared for pass/fail):**

| Field | Description |
|-------|-------------|
| `engine_version` | Namako version |
| `adapter_build_info` | Optional: adapter version/build |
| `cargo_lock_hash` | Optional: for reproducibility |
| `rustc_version` | Optional: for reproducibility |

#### 7.3.1 Feature Fingerprint Hash

> **Normalization and encoding rules per §7.0.**

Compute a simpler feature fingerprint:
```
feature_fingerprint_hash = blake3_256_lowerhex(
  canonical_json_encode( FeatureFingerprint )
)
```

Where `FeatureFingerprint` includes:
- All feature file paths (sorted lexicographically per §7.0.4)
- For each file: hash of UTF-8 content after:
  - Unicode normalization to NFC (per §7.0.2)
  - Newline normalization to `\n` (per §7.0.2)

> **Note:** v2 adopts full FeatureAstNorm for stability under cosmetic edits.

### 7.4 Verification Rules (Normative)

`namako verify` MUST perform the following checks. The verify command is the **authority**; it does not trust echoed header values but recomputes them from current sources.

#### 7.4.1 Recompute Authority Inputs (Normative)

During `namako verify`, the CLI MUST recompute all authority values from current sources:
1. `feature_fingerprint_hash` — from the current `.feature` files on disk
2. `step_registry_hash` — from the current adapter manifest (per §6.2.1 and §7.0)
3. `resolved_plan_hash` — from a **freshly recomputed resolved plan** produced by resolving current `.feature` files against the current adapter manifest using current engine semantics (per §7.0)
4. For each step in the plan: `planned_payload_hash` — from the ExecutionPayload definition (per §6.5 and §7.0)

> **Critical:** Verify MUST NOT treat the on-disk `resolved_plan.json` as authoritative. It MAY compare the recomputed plan to the on-disk plan to detect stale artifacts, but the recomputed plan is the source of truth.

#### 7.4.2 Validate Run Report Integrity (Normative)

`namako verify` MUST assert:
1. **Header hashes match recomputed values:**
   - Run report `feature_fingerprint_hash` == recomputed from current `.feature` files
   - Run report `step_registry_hash` == recomputed from current adapter manifest
   - Run report `resolved_plan_hash` == recomputed from freshly resolved plan (per §7.4.1)
2. **Per-step integrity:**
   - For every step: `planned_binding_id == executed_binding_id`
   - For every step: `planned_payload_hash == executed_payload_hash`
   - For every step: `executed_impl_hash` == current manifest's `impl_hash` for that `binding_id`
3. **Protocol version match:**
   - `hash_contract_version` and `npap_version` match expected values

#### 7.4.3 Stale Artifact Detection (Normative)

If any recomputed value differs from the run report header value, `namako verify` MUST:
- **Fail immediately** with exit code non-zero
- **Emit a clear diagnostic:** `STALE OR DRIFTED ARTIFACT: <field_name> does not match current source`
- Identify which artifact is stale (features, registry, or plan)

**Specific stale cases:**
- `STALE OR DRIFTED ARTIFACT: feature_fingerprint_hash does not match current .feature files`
- `STALE OR DRIFTED ARTIFACT: step_registry_hash does not match current adapter manifest`
- `STALE OR DRIFTED ARTIFACT: resolved_plan does not match current resolution` — emitted when the on-disk `resolved_plan.json` (or its header hash) does not match the freshly recomputed plan hash

This ensures that old run reports cannot pass verification if the underlying sources have changed.

#### 7.4.4 Compare Candidate to Baseline Identity (Normative)

After integrity validation passes, `namako verify` MUST:
1. Compare candidate identity to baseline identity (`certification.json`)
2. Perform strict field-by-field equality on all identity fields
3. Any mismatch → hard failure with exit code non-zero

### 7.5 Canonical JSON Encoding (Normative)

> **See §7.0.3 for the authoritative canonical JSON encoding rules.**

Use canonical JSON:
- Object keys: sorted lexicographically
- No trailing commas
- No comments
- UTF-8 encoding
- **For hashed objects:** Optional fields MUST be present; absence MUST be encoded as `null`
- **For non-hashed metadata only:** Optional fields MAY be omitted when absent
- Numbers: integers only in hashed objects (no floats); no leading zeros
- Strings: minimal escaping (only required escapes)

---

## Part 8: Current Limitations (v2 Will Address)

The following limitations are accepted for current use; v2 will address them for publish-grade hardening:

### 8.1 Expression-Based Binding IDs

The generated `binding_id` ties identity to expression strings.
- Editing an expression string changes its `binding_id`
- This is treated as identity drift requiring `update-cert`
- v2 may adopt stable explicit IDs for publish-grade stability

### 8.2 Feature Fingerprint Hashing

Current implementation uses feature fingerprint (content hash) rather than FeatureAstNorm.
- Cosmetic edits (whitespace, comments) may change hash
- v2 may adopt full FeatureAstNorm for cosmetic-change immunity

---

## Part 9: Tesaki: The AI Driver Layer (No Inference in Namako)

### 9.1 Tesaki's Inputs/Outputs (Normative)

**Inputs:**
- `.feature` files
- Adapter manifest
- `run_report.json`
- `certification.json`
- Namako packets (from `review`, `explain`, `status`)

**Outputs:**
- Repo patches only (no commits)
- Logs / CURRENT_STATUS.md

### 9.2 The Development FSM (Normative)

| State | Entry Command(s) | Success Condition (Exit) | Failure → Allowed Transitions |
|-------|------------------|--------------------------|-------------------------------|
| AuthorContract | (human input) | Developer confirms behavior description | — |
| ReviewBacklog | `namako review` | Packet parsed, work items extracted | Parse error → AuthorContract |
| PromoteScenarios | (Tesaki edits .feature) | New scenarios added to .feature | — |
| Resolve | `namako lint` | Exit 0 | Lint errors → PromoteScenarios or BindSteps |
| BindSteps | (Tesaki writes bindings) | All steps have bindings | — |
| Execute | `namako run` | Exit 0 | Run errors → BindSteps or Implement |
| Explain/Fidelity | `namako explain` | Binding/test matches rule spirit | Fidelity gap → BindSteps |
| Verify | `namako verify` | Exit 0 | Verify drift → Execute or BindSteps |
| BlessBaseline | `namako update-cert` | Human-approved baseline written | — |
| MilestoneComplete | — | Slice certified, CI green | — |

### 9.3 Tesaki's Non-Negotiable Rules (Normative)

1. **Tesaki MUST NOT modify certification/baseline to "fix" failures.**
2. **Tesaki MUST treat Namako outputs as authority** (no guessing which steps ran).
3. **Tesaki MUST operate slice-first:** promote small batches, keep CI green.

### 9.4 Tesaki Update-Cert Governance (Normative)

Baseline updates require controlled governance. The `tesaki` CLI provides this via:

**Command-Line Control:**
```bash
tesaki run --max-cert-updates N
```

| `--max-cert-updates` | Behavior |
|----------------------|----------|
| `0` | Manual only — Tesaki will NOT run `update-cert` (CI default) |
| `1..999` | Autonomous — Tesaki MAY run `update-cert` up to N times per session (local dev) |

**Audit Log:**

When Tesaki runs `update-cert`, it MUST append an entry to `update_cert_log.jsonl`:

```json
{"timestamp_utc": "...", "reason": "...", "identity_before": {...}, "identity_after": {...}}
```

**Recommended Defaults:**
- **CI:** `--max-cert-updates 0` (require manual approval for baseline changes)
- **Local BOOTSTRAP dev:** `--max-cert-updates 3` (allow autonomous updates when gates pass)

**Prerequisite for Autonomous Update:**

Tesaki MAY run `update-cert` autonomously only when ALL of the following are true:
1. `--max-cert-updates N > 0` and update count < N for this session
2. All gates pass (lint, run, verify would succeed with new baseline)
3. No scenario failures in the run report
4. No git operations are performed (no commits)

### 9.5 Autonomy Today vs v2

- Today, Tesaki drives the loop by consuming Namako packets (`review`/`explain`/`status`) and gating every change via `namako gate`.
- v2 may add publish-grade hardening (conformance, multi-language SDKs, stronger hashing) without changing the loop’s core control boundaries.

---

## Part 10: The Spec-Driven Development Loop

### 10.1 Core Principle

**Namako is the authority.** Tesaki (the AI driver) does not "guess correctness." It repeatedly:
```
run → classify → minimal edit → rerun
```
until all gates are satisfied.

### 10.2 The Tight Loop (Slice-Based Workflow)

Work in **small slices** (typically one `Rule` or a small set of scenarios). Do not expand scope until the current slice is certified.

#### Step 1: Requirements Capture

**Goal:** Convert an idea into a testable behavioral contract.

**Exit condition:** Developer confirms the behavior description is correct and complete.

#### Step 2: Convert to Normative Spec (.feature)

**Goal:** The `.feature` file becomes the single normative spec surface.

**Tesaki actions:**
- Convert requirements into `.feature` file
- Put rationale into Gherkin comments (`# ...`)
- The `.feature` is now normative source

#### Step 3: Scenario Integrity Loop

**Goal:** Ensure `.feature` is structurally valid and unambiguous.

**Tesaki loop:**
1. Run: `namako lint`
2. If lint fails: fix and iterate

**Exit condition:** `namako lint` passes with no errors.

#### Step 4: Binding/Test Faithfulness Loop

**Goal:** Ensure scenarios are faithfully bound and executable.

**Tesaki loop:**
1. Run: `namako lint`
2. Run: `namako run`
3. Run: `namako verify`

**On failure:**
- Resolution/signature fails → fix bindings
- Execution faithfulness fails → fix adapter/bindings
- Verify fails → produce diff, explain drift, await developer approval before `update-cert`

**Exit condition:** All three pass.

#### Step 5: Implement the System

**Goal:** Implement/modify system under test until scenarios pass.

**Tesaki loop:**
- Make minimal implementation changes
- Re-run `namako lint` → `namako run` until green

**Exit condition:** All scenarios pass.

### 10.3 Existing Markdown Specs

This project has existing Markdown docs describing Naia behavior.
- Those docs are **source material only**
- `.feature` becomes **normative source**
- Markdown may be archived or deleted after conversion

---

## Part 10.5: AI-Enablement Features

These features enable robust AI-driven autonomous development. They maximize Tesaki's effectiveness while minimizing implementation complexity.

### AI-Enablement Feature Set (Normative)

| Feature | Description |
|---------|-------------|
| Explicit ID tags | `@Feature(name)`, `@Rule(nn)`, `@Scenario(nn)` — identity survives refactoring |
| Orphan binding hard error | Prevents dead code accumulation; `namako stub` mitigation |
| `namako review` | Rich work packets for Tesaki task selection |
| `namako explain` | Scenario fidelity packets for AI-assisted review |
| `namako status --json` | Machine-readable process state for FSM automation |
| Rich status diffs | Human-readable identity status for debugging |

### 10.5.1 Explicit Identity Tags (Normative)

Structural identity tags for refactor-safe scenario keys:

| Tag | Applies To | Format | Example |
|-----|-----------|--------|---------|
| `@Feature(name)` | Feature | Alphanumeric snake_case | `@Feature(connection_lifecycle)` |
| `@Rule(nn)` | Rule | Numeric index (1-based) | `@Rule(01)`, `@Rule(02)` |
| `@Scenario(nn)` | Scenario | Numeric index (1-based) | `@Scenario(01)`, `@Scenario(02)` |
| `EID` column | Examples row | String identifier | `EID: auth_success` |

**Invariants:**
- Features MUST have `@Feature(name)` tag
- Rules MUST have `@Rule(nn)` tag
- Scenarios MUST have `@Scenario(nn)` tag
- Example rows SHOULD have `EID` column (MUST for v2)

**Derivation Rules:**
- `scenario_key` = `Feature + ":" + Rule(nn) + ":" + Scenario(nn)` (or `+ ":E" + EID` for outline rows)
- Collision detection: duplicate `(Feature, Rule(nn), Scenario(nn))` tuples → hard error

### 10.5.2 Orphan Bindings as Hard Error (Normative)

Orphan binding policy:

- Binding in registry not used by any scenario → **hard error** in `namako lint`
- Rationale: Prevents dead code accumulation, keeps registry minimal

**Mitigation Tool:**

`namako stub` generates placeholder usage for orphan bindings:

```bash
# List orphan bindings
namako lint --show-orphans

# Generate stub scenario for an orphan
namako stub --binding <binding_id> --feature <path.feature>

# Auto-stub all orphans (creates _orphan_stubs.feature)
namako stub --all
```

**Stub Output Example:**
```gherkin
# Auto-generated by namako stub — delete after implementing real scenarios
@Deferred @Stub
Scenario: Stub for orphan binding abc123
  Given <original step text from binding>
```

### 10.5.3 Enhanced Work Packets (`namako review`) (Normative)

`namako review` outputs AI-actionable packets:

**Required Packet Sections:**
1. **Coverage Summary:**
   - Feature/Rule → executable scenario counts
   - Rules with 0 scenario coverage
   - @Deferred scenario count per feature

2. **Deferred Items:**
   - Full list of `@Deferred` scenarios with:
     - scenario_key
     - scenario_name
     - feature_path
     - rule_name
     - blocker classification (HARNESS_ONLY, CORE, EXTERNAL, UNKNOWN)

3. **Promotion Candidates:**
   - Ranked by `reuse_score` (step binding reuse)
   - `suggested_binding_bundle` for missing steps
   - `new_step_texts_estimate` count

4. **Missing Bindings Worklist:**
   - For top N promotion candidates
   - Step texts that need new bindings
   - Suggested expression patterns

5. **Harness Gaps:**
   - Normalized capability descriptions
   - Count of blocked scenarios per gap

**Determinism Requirement:** Output MUST be stable (sorted keys, stable ordering).

### 10.5.4 Scenario Fidelity Packets (`namako explain`) (Normative)

`namako explain --scenario-key <key>` outputs a fidelity packet:

**Packet Contents:**
```json
{
  "scenario_key": "...",
  "scenario_name": "...",
  "feature_path": "...",
  "rule_name": "...",
  "rule_description": "...",
  "steps": [
    {
      "step_kind": "Given",
      "step_text": "...",
      "binding_id": "...",
      "binding_expression": "...",
      "impl_hash": "...",
      "source_location": "path/to/file.rs:123"
    }
  ],
  "related_tags": ["@smoke", "@connection"],
  "contract_excerpt": "# Rule: Connection Events\n\nWhen a client..."
}
```

**Why it matters:**
- Enables AI review: "Does this binding actually test what the rule describes?"
- Provides implementation context without reading entire codebase

### 10.5.5 Machine-Readable Process State (`namako status --json`) (Normative)

**Promoted from §11.6.**

`namako status --json` enables FSM automation:

**Required Output Fields:**
```json
{
  "recommended_next_action": "RUN_LINT | FIX_RUN | NEEDS_UPDATE_CERT_APPROVAL | DONE",
  "lint_status": "pass | fail | stale",
  "run_status": "pass | fail | stale | not_run",
  "verify_status": "pass | fail | stale | not_run",
  "drift": {
    "kind": "NONE | FEATURE | REGISTRY | PLAN | BASELINE",
    "details": [
      {"field": "...", "baseline": "...", "current": "..."}
    ]
  },
  "last_run_failures": [
    {
      "scenario_key": "...",
      "scenario_name": "...",
      "failure_kind": "step_failed | panic | timeout"
    }
  ],
  "identity": {
    "current": { ... },
    "baseline": { ... }
  },
  "metadata": {
    "timestamp": "...",
    "namako_version": "..."
  }
}
```

**Determinism Requirement:** Output MUST be stable for same inputs.

### 10.5.6 Rich Status Diffs (Normative)

**Promoted from §11.10.**

`namako status` includes human-readable diff output:

**Text Output (default):**
```
=== Identity Status ===
✗ feature_fingerprint_hash: DRIFTED
  Baseline: abc123...
  Current:  def456...

✓ step_registry_hash: MATCH
✓ resolved_plan_hash: MATCH

=== Recommended Action ===
NEEDS_UPDATE_CERT_APPROVAL

=== Recent Failures ===
1. features/connection.feature:L42 — Server connection timeout
   Step 3: Then the client receives a disconnect event
   Error: assertion failed: expected Disconnect, got None
```

**Why it matters:**
- Faster developer debugging
- Clearer CI failure diagnostics

### AI-Enablement Definition of Done

The AI-enablement features are complete when:

| Criterion | Description |
|-----------|-------------|
| **Explicit IDs enforced** | All features require @Feature/@Rule(nn)/@Scenario(nn); scenario_key uses ID-based format |
| **Orphan → hard error** | `namako lint` fails on orphan bindings; `namako stub` generates placeholders |
| **Review packets enhanced** | All 5 packet sections present in `namako review` output |
| **Explain packets complete** | `namako explain` outputs full fidelity packets |
| **Status JSON complete** | All required fields present in `namako status --json` |
| **Rich diffs implemented** | Text diff output shows clear diagnostics |
| **Migration complete** | All existing `.feature` files have explicit ID tags |

---

## Part 10.7: Runner Integration — Tesaki ↔ Coding-Agent Integration

**Runner integration is BOOTSTRAP work.** This section defines how Tesaki orchestrates an external coding agent (runner) to autonomously drive spec-driven development. This is the **Product Loop** we are building.

### 10.7.0 Non-Negotiable Invariants (Normative)

These invariants MUST be maintained in this design and implementation:

1. **Tesaki is the ONLY orchestrator / state machine** in the Product Loop.
2. **Namako is measurement + packet emission** (gate + status/review/explain). Tesaki consumes packets; it does not re-derive them.
3. **Runner is an untrusted executor** (Claude Code or Codex CLI). Runner never decides "what next", never advances Tesaki state.
4. **Assume context bleed is always possible.** Design around it via Mission Bundles + post-run gates; do not introduce "fresh session support" flags.
5. **Mission Bundle is the unit of work** and the contract boundary between Tesaki and the runner.
6. **Single-command UX target:** user runs `tesaki run` repeatedly. Tesaki runs Namako internally for measurement.

### 10.7.1 Layers and Roles (Bootstrap vs Product vs Runner)

Three layers exist and MUST remain strictly separated:

| Layer | Name | Description |
|-------|------|-------------|
| **Layer A** | BOOTSTRAP | Connor + Opus building Namako/Tesaki (this project phase) |
| **Layer B** | PRODUCT LOOP | Tesaki + Namako driving Naia development (the workflow we are building) |
| **Layer C** | RUNNER | External coding agent process (Claude Code or Codex CLI session) |

**Critical Invariant:** Layer C (Runner) MUST NOT orchestrate Layer B (Product Loop). The runner receives a mission, executes it, and returns. It does not decide what happens next.

### 10.7.2 Ownership Table (What Lives Where)

**Tesaki owns:**
- Task selection from Namako packets
- Mission lifecycle + stop conditions
- Invoking runner (Claude Code or Codex CLI integration)
- Post-run validation by running `namako gate --json`
- Governance enforcement (cert update limits, retries, halts)

**Namako owns:**
- `namako gate` (lint → run → verify; determinism option)
- `status --json`, `review`, `explain` packets
- Certification baseline format + update-cert refusal rules
- Deterministic artifact emission and schemas

**Runner owns:**
- Apply edits to satisfy mission prompt
- Optional local iteration to converge
- Emit an "attempt report" to OUTPUT (not authoritative for success)

**Explicitly NOT owned by runner:**
- Selecting next task
- Calling Tesaki for orchestration (runner does not invoke Tesaki)
- Performing baseline updates unless Tesaki explicitly requests (and Tesaki still validates)

### 10.7.3 "Tesaki Consumes Packets Only" Invariant (Anti-Drift)

**Normative Statement:**
- Tesaki MUST NOT replicate Namako logic by re-parsing or re-inferring semantics from raw artifacts.
- Tesaki reads structured packets (`status --json`, `review`, `explain`, `gate --json`) and makes decisions based on their fields.
- If Tesaki needs a new signal (e.g., "which scenarios are flaky"), it MUST be added to Namako packet outputs, not inferred by Tesaki.

This prevents architectural drift where Tesaki becomes a shadow implementation of Namako.

### 10.7.4 Runner Integration Model: Runner Lives Inside Tesaki

**Connor preference:** Avoid "another CLI". Runner is NOT a separate CLI tool.

**Model Definition:**
- Runner is an internal Tesaki abstraction (Rust trait/module).
- Claude Code and Codex CLI are the primary concrete backend implementations.
- Swapping runners is done by specifying `--runner claude` or `--runner codex` (or implementing additional backends inside Tesaki, e.g., local LLM, mock runner for testing).

**No separate runner CLI:** Users run `tesaki run`, which internally manages runner invocation. The runner process receives a mission bundle and returns; it has no external CLI of its own.

### 10.7.5 Mission Bundle Contract (Filesystem Contract Boundary)

Tesaki creates a deterministic Mission Bundle directory for each mission:

```
.tesaki/missions/<mission_id>/
├── NEXT_TASK.md          # Single prompt payload for the runner
├── INPUTS/               # Namako packet snapshots relevant to mission
│   ├── status.json       # namako status --json output
│   ├── review.json       # namako review output
│   ├── explain.json      # namako explain output (if relevant)
│   └── gate.json         # namako gate --json output (pre-mission state)
├── POLICY.md             # Rules: no commits, no orchestration, scope limits
├── EXPECTED.md           # Explicit postconditions Tesaki will check
└── OUTPUT/               # Runner writes here; Tesaki writes gate results
    ├── attempt_report.md # Runner's self-reported attempt summary
    ├── transcript.txt    # Optional: runner session transcript
    └── gate_result.json  # Tesaki writes post-run gate output
```

**Why this design:**
- Reduces reliance on runner context (mission is self-contained)
- Enables replay/debug (mission bundle captures full state)
- Clear contract boundary (filesystem, not API)
- Runner cannot corrupt Tesaki state (writes only to OUTPUT/)

### 10.7.6 Mission Size Budgets (Prevent Giant Wandering Missions)

Tesaki enforces budgets to prevent runaway missions. These are defaults; each can be overridden in `.tesaki/config.toml`.

**Configuration file location:** `.tesaki/config.toml` in the target repository (e.g., naia/.tesaki/config.toml). Tesaki discovers this file by walking up from the current directory.

**Example `.tesaki/config.toml`:**
```toml
# Required
specs_dir = "test/specs"
adapter_cmd = "cargo run --manifest-path test/npa/Cargo.toml --"

# Optional overrides
runner = "mock"           # mock, cmd, or claude
max_retries = 2
max_cert_updates = 3
max_runtime_seconds = 600
max_files_changed = 10
```

| Budget | Default | Description |
|--------|---------|-------------|
| `max_files_changed` | 10 | Maximum files the runner may modify per mission |
| `max_scenarios_promoted` | 3 | Maximum scenarios promoted from @Deferred per mission |
| `max_runtime_seconds` | 600 | Maximum wall-clock runtime per mission (10 minutes) |
| `max_retries` | 2 | Maximum retry attempts on runner failure |
| `max_cert_updates` | 3 | Maximum baseline updates per session (existing, §9.4) |

**Enforcement:**
- Tesaki checks budgets before and after runner execution.
- Budget exceeded → mission fails with `BUDGET_EXCEEDED` stop condition.
- Budgets are part of the product safety model.

### 10.7.7 Success Criteria, Next-Step Selection, and Stop Conditions

**Success is defined by Tesaki, not the runner:**
- Runner's attempt report is informational, not authoritative.
- Tesaki runs `namako gate --json` after runner exits.
- Only Tesaki transitions state based on gate output and expected postconditions.

**Next step selection (deterministic):**
1. Tesaki re-reads Namako packets (status/review/explain/gate) after mission completes.
2. Tesaki applies task selection rules (priority, reuse_score, etc.).
3. Tesaki generates next mission bundle or enters stop condition.

**Stop conditions (explicit, deterministic):**

| Condition | Description |
|-----------|-------------|
| `DONE` | No eligible tasks remain (all scenarios passing, no promotion candidates) |
| `BLOCKED` | Only blocked items remain (e.g., all require HARNESS_ONLY or EXTERNAL work) |
| `HUMAN_REQUIRED` | Human intervention needed (e.g., baseline update approval required, ambiguous requirements) |
| `ENVIRONMENT_ERROR` | Gate invocation failed, adapter crash, filesystem errors |
| `BUDGET` | Runtime/attempt/file limits reached |

Tesaki MUST emit a structured stop reason when halting.

### 10.7.8 Runner Failure Modes (Deterministic Handling)

| Failure Mode | Tesaki Response |
|--------------|-----------------|
| Runner exits non-zero | Retry once (if under `max_retries`), then stop with `RUNNER_FAILED` |
| Runner produces no diff / no meaningful changes | Retry once, then stop with `NO_PROGRESS` |
| Runner produces malformed/missing attempt report | Log warning, proceed with gate check (attempt report is not authoritative) |
| Gate fails before verify (lint/run issues) | Retry once, then stop with `GATE_FAILED` |
| Runner exceeds runtime budget | Kill runner process, stop with `BUDGET` |

**On failure:** Tesaki preserves the mission bundle at `.tesaki/failed/<mission_id>/` for inspection.

### 10.7.9 Canonical UX Flow (Single Source of Truth)

This is the canonical flow for `tesaki run`:

```
0. (One-time setup) Create .tesaki/config.toml in target repo

1. User runs `tesaki run` (with zero flags when config exists)

2. Tesaki measures via Namako packets:
   - namako status --json
   - namako review
   - namako gate --json

3. Tesaki selects next task from packets (or enters stop condition)

4. Tesaki generates Mission Bundle:
   - Create .tesaki/missions/<mission_id>/
   - Write NEXT_TASK.md, INPUTS/, POLICY.md, EXPECTED.md

5. Tesaki invokes runner backend:
   - Pass mission bundle location
   - Runner executes in working tree
   - Runner writes to OUTPUT/

6. Tesaki validates:
   - Run `namako gate --json`
   - Compare against EXPECTED.md postconditions
   - Record gate_result.json

7. Tesaki transitions or stops:
   - If success: clean up mission bundle, loop to step 2
   - If failure: preserve mission bundle at .tesaki/failed/, emit stop reason
   - If budget reached: stop with explicit reason
```

**Development setup (one-time):**
```bash
# Install dev shim for tesaki command
./scripts/install-tesaki-dev-shim

# Create config in target repo
mkdir -p naia/.tesaki
cat > naia/.tesaki/config.toml << 'EOF'
specs_dir = "test/specs"
adapter_cmd = "cargo run --manifest-path test/npa/Cargo.toml --"
runner = "mock"
EOF
```

**User repeats `tesaki run`** to continue development. Each invocation is atomic and safe.

---

## Part 10.8: Tesaki v1.8 — Interactive Developer Experience

**v1.8 builds on v1.7 Runner Integration to deliver the full interactive developer experience.** This section specifies the UX layer that transforms Tesaki from a batch orchestrator into an interactive development companion.

### 10.8.0 Design Principles (Normative)

1. **Tesaki is "personal Claude Code" with repo state awareness.** Users talk naturally; Tesaki computes truth from Namako packets and dispatches missions.
2. **One mission at a time.** Even in interactive mode, Tesaki executes exactly one mission per cycle, then re-validates.
3. **Stages are a lens, not a wizard.** The 5-stage workflow filters task selection; users can jump stages at any time.
4. **Edit surfaces are explicit.** Every mission declares which surfaces (Spec/Tests/SUT) are locked or unlocked.
5. **Propagation is automatic.** Edits ripple through: spec changes → binding needs → SUT work. Tesaki recomputes and continues.

### 10.8.1 System Boundaries (Normative)

This boundary is non-negotiable and extends §10.7.1:

| Layer | Responsibility | Determinism |
|-------|----------------|-------------|
| **Namako** | Parse, resolve, measure, emit packets | MUST be deterministic |
| **Tesaki** | Orchestrate, compute RepoState, select missions, invoke runners | Session state is stateful; mission selection is deterministic |
| **Runner** | Execute one mission, apply edits, return | NOT deterministic (AI-driven) |
| **SUT** | The system being built | N/A |

**Tesaki owns:**
- Interactive session management
- RepoState computation from Namako packets
- Stage lens and edit-surface policy enforcement
- Mission type selection and brief generation
- Natural language intent interpretation

**Tesaki does NOT own:**
- Truth measurement (Namako only)
- Code generation (Runner only)
- Baseline updates without governance

### 10.8.2 CLI Commands (Normative)

#### Primary Commands

| Command | Description |
|---------|-------------|
| `tesaki` | Start an **interactive session** (TTY). Natural language interface over computed repo state. |
| `tesaki run` | Run **exactly one mission cycle** and exit (headless-friendly). Per §10.7. |

#### Supporting Commands

| Command | Description |
|---------|-------------|
| `tesaki status` | Short computed state summary (human + machine readable) |
| `tesaki explain` | Computed "what/why" view without running a mission |
| `tesaki config` | Config discovery + diagnostics |

Everything else is expressed as **session intent** (§10.8.6), not subcommands.

### 10.8.3 RepoState: Computed Truth from Namako Packets (Normative)

Tesaki's internal model is computed from Namako packets. It MUST NOT re-derive semantics by parsing raw artifacts.

#### Required Packet Inputs

| Packet | Source | Purpose |
|--------|--------|---------|
| `status.json` | `namako status --json` | Gate states, drift, failures |
| `review.json` | `namako review` | Coverage, candidates, missing bindings |
| `explain.json` | `namako explain` | Scenario fidelity details |
| `gate.json` | `namako gate --json` | Pass/fail gate with structured failures |

#### RepoState Model (Normative Schema)

```
RepoState {
  // Gate states
  lint_status: pass | fail | stale
  run_status: pass | fail | stale | not_run
  verify_status: pass | fail | stale | not_run

  // Issue categories (derived from packets)
  spec_issues: [SpecIssue]           // Underspecified intent, ambiguity
  structure_issues: [StructureIssue] // Missing identity tags, parse errors
  binding_issues: [BindingIssue]     // Missing steps, weak assertions
  sut_issues: [SutIssue]             // Tests exist but fail
  global_blockers: [Blocker]         // Build/tooling/environment

  // Candidate task queue
  candidate_tasks: [CandidateTask]   // Sorted by priority

  // Identity
  current_identity: Identity
  baseline_identity: Identity | null
  drift: DriftInfo | null
}
```

**Invariant:** RepoState MUST be recomputed after every mission. Tesaki never relies on "chat memory" for state.

### 10.8.4 The 5-Stage Workflow (Normative)

Stages are a **UX lens** over task selection with default **edit-surface policies**. Users can override at any time.

#### Stage Definitions

| Stage | Focus | Default Surfaces |
|-------|-------|------------------|
| **Refine Spec** | Improve feature intent, add/clarify scenarios | Spec: UNLOCKED, Tests: LOCKED, SUT: LOCKED |
| **Structure Spec** | Normalize identity tags, fix Gherkin structure | Spec: UNLOCKED, Tests: LOCKED, SUT: LOCKED |
| **Implement Tests** | Create bindings, strengthen assertions | Spec: LOCKED, Tests: UNLOCKED, SUT: LOCKED |
| **Implement SUT** | Make tests pass by implementing behavior | Spec: LOCKED, Tests: LOCKED, SUT: UNLOCKED |
| **Finalize** | Verification, summaries, clean stopping point | Spec: LOCKED, Tests: LOCKED, SUT: LOCKED |

#### Auto-Stage Detection (Normative)

Tesaki SHOULD infer the appropriate stage from RepoState:

1. **Refine Spec** — If `spec_issues` is non-empty
2. **Structure Spec** — If `structure_issues` is non-empty
3. **Implement Tests** — If `binding_issues` is non-empty
4. **Implement SUT** — If `sut_issues` is non-empty (tests exist but fail)
5. **Finalize** — If all gates pass and no issues remain

#### Stage Override

Users MAY override the detected stage via session intent:
- "Jump back to refining the spec"
- "Focus on bindings only"
- "Skip to SUT implementation"

### 10.8.5 Edit-Surface Policies (Normative)

Every mission MUST declare explicit surface policies.

#### Surface Definition

| Surface | Description | Example Paths |
|---------|-------------|---------------|
| **Spec** | Feature files and spec artifacts | `test/specs/**/*.feature` |
| **Tests/Bindings** | Step bindings, harness, test infrastructure | `test/tests/**`, `test/harness/**` |
| **SUT** | System under test implementation | `src/**`, `client/**`, `server/**` |

#### Policy States

| State | Meaning |
|-------|---------|
| `LOCKED` | Runner MUST NOT edit files in this surface |
| `UNLOCKED` | Runner MAY edit files in this surface |

#### Policy in Mission Bundle

The `POLICY.md` (or v1.8 `MISSION.md`) MUST include:

```markdown
## Edit Surfaces

| Surface | Policy | Paths |
|---------|--------|-------|
| Spec | LOCKED | `test/specs/**/*.feature` |
| Tests/Bindings | UNLOCKED | `test/tests/**`, `test/harness/**` |
| SUT | LOCKED | `src/**`, `client/**`, `server/**` |
```

### 10.8.6 Mission Types (Normative)

A **Mission Type** is a named operation template encoding:
- Required inputs and context
- Default edit-surface policy
- Expected validation signals
- What Namako evidence should improve afterward

#### Canonical Mission Types (v1.8)

**Spec Refinement:**

| Type | Stage | Description |
|------|-------|-------------|
| `RefineFeatureIntent` | Refine Spec | Improve feature intent comments (scope, non-goals) |
| `AddOrClarifyScenario` | Refine Spec | Add/adjust scenarios to cover edges |
| `ResolveAmbiguousRequirement` | Refine Spec | Turn "vibes" into falsifiable statements |

**Spec Structure:**

| Type | Stage | Description |
|------|-------|-------------|
| `NormalizeIdentityTags` | Structure Spec | Ensure @Feature/@Rule(nn)/@Scenario(nn) exist |
| `FixGherkinStructure` | Structure Spec | Repair malformed Gherkin, parse failures |

**Tests & Bindings:**

| Type | Stage | Description |
|------|-------|-------------|
| `CreateMissingBindings` | Implement Tests | Create step bindings for runnable scenarios |
| `StrengthenThenAssertions` | Implement Tests | Improve "Then" checks to be specific and stable |
| `RefactorBindingsForClarity` | Implement Tests | Clean step reuse without collapsing meaning |

**SUT Implementation:**

| Type | Stage | Description |
|------|-------|-------------|
| `ImplementBehaviorForScenario` | Implement SUT | Implement missing behavior to pass a scenario |
| `FixRegressionFromGateFailure` | Implement SUT | Diagnose and fix a new failure |

**Finalize:**

| Type | Stage | Description |
|------|-------|-------------|
| `SummarizeAndClose` | Finalize | Produce summary of changes, what passes, what remains |
| `CleanupAfterSuccess` | Finalize | Ensure no partial artifacts, clean stop |

**Meta (No Runner):**

| Type | Stage | Description |
|------|-------|-------------|
| `ExplainState` | N/A | Synthesize state from packets (Tesaki local) |
| `TriageFailures` | N/A | Cluster gate failures into likely causes |

#### Mission Type Selection (Normative)

Given a RepoState, Tesaki selects the mission type by:

1. Identify the highest-priority issue category
2. Select the mission type that addresses that category
3. Apply stage lens filtering if user has constrained stages
4. Generate mission brief with type-specific content

### 10.8.7 Session Intents (Normative)

In interactive mode, users speak naturally. Tesaki interprets input as constraints on:

- **Stage lens** — Which stage to operate in
- **Surface locks** — Override default locks
- **Scope** — What "done" means for this session
- **Focus** — Which feature/scenario to prioritize

#### Intent Examples

| User Input | Interpreted Constraints |
|------------|------------------------|
| "Focus on bindings only" | Stage = Implement Tests; Spec: LOCKED, SUT: LOCKED |
| "Jump back to refining the spec" | Stage = Refine Spec; Spec: UNLOCKED |
| "Tell me what's failing and why" | No mission; run ExplainState locally |
| "Don't touch tests—fix the SUT" | Stage = Implement SUT; Tests: LOCKED, SUT: UNLOCKED |
| "Unlock spec too" | Spec: UNLOCKED (persists until re-locked) |

#### Constraint Acknowledgment

Tesaki MUST restate interpreted constraints before acting:

```
> Got it: Stage = Implement Tests; Spec LOCKED; SUT LOCKED.
> Running mission: CreateMissingBindings for @Scenario(03)
```

### 10.8.8 Mission Bundle v1.8 Structure (Normative)

v1.8 updates the mission bundle structure from §10.7.5:

```
.tesaki/missions/<mission_id>/
├── MISSION.md              # Mission brief with type, stage, surfaces
├── INPUTS/                 # Frozen Namako packet snapshots
│   ├── status.json
│   ├── review.json
│   ├── explain.json        # If relevant to mission
│   ├── gate.json           # Pre-mission gate state
│   └── workspace.json
├── RUNNER_OUTPUT/          # Runner writes here
│   ├── attempt_report.md   # Runner's self-reported summary
│   ├── stop_reason.json    # Structured stop reason
│   └── transcript.txt      # Optional session trace
└── POST_GATE.json          # Post-mission gate result
```

#### MISSION.md Template (Normative)

```markdown
# Mission <id>

**Type:** <MissionType>
**Stage:** <Stage>
**Target:** <scenario_key or feature_path>

## Surfaces

| Surface | Policy |
|---------|--------|
| Spec | <LOCKED|UNLOCKED> |
| Tests/Bindings | <LOCKED|UNLOCKED> |
| SUT | <LOCKED|UNLOCKED> |

## Objective

<Type-specific objective description>

## Context

<Relevant excerpts from INPUTS/ packets>

## Validation

After runner exit:
1. `namako gate --json` must pass
2. <Type-specific expected evidence change>

## Budgets

| Limit | Value |
|-------|-------|
| Max files changed | <N> |
| Max runtime (seconds) | <N> |

---
*Generated by Tesaki v1.8*
```

### 10.8.9 Propagation Semantics (Normative)

A core v1.8 goal:

> If spec/tests/SUT is edited, Tesaki automatically computes downstream work until the repo is back to a clean, gated state.

This is NOT a special command. It is the **default consequence of the loop**:

1. **Spec edits** → Namako packets change → new scenarios appear → binding needs appear → SUT work appears → loop continues
2. **Binding edits** → runnable set changes → gate failures change → SUT work appears → loop continues
3. **SUT edits** → failing scenarios shrink → loop continues

**Propagation is just:** recompute RepoState → pick next mission → repeat until DONE.

### 10.8.10 Interactive Session Flow (Normative)

```
1. User runs `tesaki`

2. Tesaki computes RepoState:
   - namako status --json
   - namako review
   - namako gate --json

3. Tesaki displays summary:
   > Spec: 1 issue • Structure: 0 • Bindings: 4 missing • SUT: 2 failing
   > Stage: Implement Tests
   > Proposed: CreateMissingBindings for @Scenario(03)

4. User responds (natural language or command):
   - "Run it" → Execute proposed mission
   - "Why?" → Explain the selection
   - "Focus on SUT instead" → Change stage, re-propose
   - "Quit" → Exit session

5. If mission runs:
   - Create mission bundle
   - Invoke runner
   - Run namako gate --json
   - Update RepoState
   - Loop to step 3

6. Session continues until:
   - DONE (no eligible work)
   - BLOCKED (external dependency)
   - HUMAN_REQUIRED (decision needed)
   - User quits
```

### 10.8.11 Stop Reasons (Normative)

Every mission ends with a structured stop reason:

| Reason | Description |
|--------|-------------|
| `DONE` | No eligible work within scope; gates satisfactory |
| `HUMAN_REQUIRED` | Decision needed: ambiguity, tradeoff, missing requirement |
| `BLOCKED` | External dependency Tesaki cannot resolve |
| `FAILED` | Unexpected failure: runner crash, tool error |

Stop reasons MUST include:
- What was attempted
- What evidence triggered the stop
- What Tesaki recommends next

### 10.8.12 Non-Goals for v1.8 (Normative)

To keep v1.8 focused, we explicitly defer:

- Large suite of stage-specific CLI subcommands (session intents cover it)
- Arbitrary numeric budgets like "max LoC changed" as first-class concept
- Multi-runner consensus schemes
- Auto-commits, auto-branching (developer-owned)
- Multi-turn context tracking with undo/rollback
- Session persistence across restarts

---

## Part 11: Namako v2 — Deferred Publish-Grade Features

This section captures all hardening features not currently implemented but designed into the system for future adoption.

### 11.1 FeatureAstNorm (Full AST Normal Form Hashing)

**What it adds:**
- Parse `.feature` → Gherkin AST → Canonical internal model (`FeatureAstNorm`) → Hash
- Immune to cosmetic changes (whitespace, comments, blank lines)

**Why it matters:**
- Publish-grade stability: spec identity survives formatting changes
- Enables meaningful diff on semantic changes only

**Migration:**
- Bump `hash_contract_version`
- Regenerate `certification.json`
- All identity fields will change

**FeatureAstNorm Schema:**
- Feature: `feature_id`, `feature_tags` (sorted), `rules[]` (ordered)
- Rule: `rule_id`, `rule_tags` (sorted), `background_steps[]`, `scenarios[]` (ordered)
- Scenario: `scenario_id`, `scenario_tags` (sorted), `steps[]`
- Scenario Outline: same as Scenario, plus `examples` as `BTreeMap<EID, ExampleRowNorm>`
- Step: `effective_kind`, `step_text`, `docstring`, `datatable`

### 11.2 Explicit Identity Tags

**What it adds:**
- `@Feature(name)` on Features — explicit, refactor-stable feature identity
- `@Rule(nn)` on Rules — explicit rule identity
- `@Scenario(nn)` on Scenarios — explicit scenario identity
- `EID` column in Scenario Outline examples — explicit row identity

**Why it matters:**
- Identity survives file renames
- Identity survives scenario reordering
- Publish-grade stability for long-lived specs

**Migration:**
- Add required tags to all `.feature` files
- Bump `hash_contract_version`
- Regenerate certification

**Invariant (v2):** Every Feature, Rule, Scenario, and Example Row MUST have explicit identity.

### 11.3 Orphan Bindings as Hard Error

**What it adds:**
- Binding in registry not used by any scenario → hard error

**Why it matters:**
- Prevents dead code in test bindings
- Ensures registry is minimal and intentional

**Mitigation tool:**
- `namako stub --binding <id>` generates a minimal placeholder scenario

**Migration:**
- Run `namako stub` for any orphans
- Or delete unused bindings

### 11.4 Work Packets for Tesaki (`namako review`)

**What it adds:**
`namako review` outputs deterministic JSON packets that turn the spec corpus into an AI-executable backlog.

Minimum packet sections:
- **Coverage:** Feature/Rule → executable scenario counts + "rules with 0 coverage"
- **Deferred:** extracted DEFERRED TESTS items with source spans
- **Binding worklist:** missing/ambiguous step texts for a proposed promotion batch
- **Harness gaps:** normalized capability gaps + how many deferred items they block

**Why it matters:**
- Enables autonomous spec→scenario iteration
- Makes progress measurable and reproducible
- Prevents human "hunt and peck" task selection

> **Note:** review packets MUST be deterministic (stable ordering, stable JSON).

### 11.5 Scenario Fidelity Packets (`namako explain`)

**What it adds:**
`namako explain --scenario <selector>` emits a deterministic JSON bundle:
- contract context excerpt (Rule + relevant normative mirror headings)
- scenario steps
- resolved binding IDs + binding source spans + impl_hashes
- (optional) shallow helper/call-surface list

**Why it matters:**
- Enables AI-assisted "spirit of the spec" review: does the binding/test actually assert what the rule claims?

### 11.6 Machine-Readable Process State (`namako status --json`)

**What it adds:**
`namako status --json` emits the current development state:
- last lint/run/verify results
- current identity tuple vs baseline identity
- drift reason codes
- recommended next gate (pure rules)

**Why it matters:**
- Tesaki can drive the FSM without parsing console logs
- makes automation robust in CI/local runs

### 11.7 Canonical Byte Encoding (CBOR Profile)

**What it adds:**
- CBOR canonical encoding instead of JSON
- Strict schema enforcement
- Deterministic byte-for-byte output

**Why it matters:**
- True cross-implementation reproducibility
- Smaller artifact size
- More robust against encoding edge cases

**Migration:**
- Bump `hash_contract_version` to indicate CBOR
- Regenerate all artifacts
- Update all tooling to CBOR

### 11.8 Conformance Fixtures

**What it adds:**
- Fixture suite with:
  - Canonical input (structured)
  - Canonical encoded bytes
  - Expected hash outputs
- CI validates fixtures on all platforms

**Why it matters:**
- Proves cross-platform hash reproducibility
- Catches encoding bugs early

**Scope:**
- FeatureAstNorm
- SemanticStepRegistry
- ResolvedPlan

### 11.9 Resolution Semantics ID

**What it adds:**
- `resolution_semantics_id` field in identity tuple
- Stable string identifying: parsing + matching + kind inference + signature enforcement

**Why it matters:**
- Detects when resolution semantics change
- Enables controlled migration between resolution versions

**Initial value:** `"namako-resolution-v2"`

### 11.10 Rich `namako status` Diffs

**What it adds:**
- Detailed diff output showing:
  - Identity fields that changed (blocking)
  - Metadata fields that changed (informational)
  - Per-scenario/per-step breakdown

**Why it matters:**
- Developer UX for understanding drift
- Faster debugging

### 11.11 Stronger `impl_hash` Schemes

**What it adds:**
- Exclude comments from source fingerprint
- Exclude file paths from source fingerprint
- Capture dependency signals (imports, called functions)

**Why it matters:**
- `impl_hash` changes only when behavior changes
- Fewer false positives on cosmetic code changes

### 11.12 `bindings_used_hash`

**What it adds:**
- `bindings_used_hash` in identity tuple
- Computed from sorted list of unique binding IDs in resolved plan

**Why it matters:**
- Quick signal that binding set changed
- Enables fast-path verification

### 11.13 Multi-Language Support (Language-Neutral Engine, Language-Specific Adapters)

This section defines how Namako supports projects in **any programming language** (JS/TS, Python, Go, C++, JVM, .NET, etc.).

#### 11.13.1 Core Principle (Normative)

- The Namako Engine/CLI MUST remain a Rust tool.
- Any project integrates via an **external adapter executable** that implements NPA.
- The adapter protocol is the **only cross-language integration boundary**.

**Engine Constraints:**
- The engine MUST NOT depend on project language runtimes.
- The engine MUST invoke adapters via `adapter_cmd` (configured in `namako.toml`).
- The engine MUST validate adapter outputs against strict JSON schemas.

**Adapter Constraints:**
- The adapter MUST implement `manifest` and `run` commands.
- The adapter MUST dispatch by `binding_id` only (no runtime text matching).
- The adapter MUST emit artifacts conforming to NPA schemas.

#### 11.13.2 Universal "3-Piece" Project Pattern

Any language ecosystem SHOULD follow this pattern (equivalent to Naia's Rust structure):

| Component | Purpose | Naia Equivalent |
|-----------|---------|----------------|
| `<project>_test_harness` | World type + test helpers | `naia_test_harness` |
| `<project>_tests` | Step definitions (one keyword + one string per step) | `naia_tests` |
| `<project>_npap` | Adapter executable (`manifest` + `run`) | `naia_npa` |

**Language-Specific Examples:**

**JavaScript/TypeScript (Node.js):**
```
myproject-test-harness/   # npm package: World class, test utilities
myproject-tests/          # npm package: step definitions using decorators
myproject-namako/         # Node CLI: dist/myproject_namako.js
```

**Python:**
```
myproject_test_harness/   # Python package: World class, fixtures
myproject_tests/          # Python package: step definitions using decorators
myproject_namako/         # Python module: python -m myproject_namako
```

**Go:**
```
pkg/testharness/          # Go package: World struct, test helpers
pkg/tests/                # Go package: step definitions using struct tags or registration
cmd/myproject-namako/     # Go binary: ./bin/myproject-namako
```

**C++:**
```
src/test_harness/         # C++ library: World class, test utilities
src/tests/                # C++ library: step definitions via registration macros
src/myproject_namako/     # C++ binary: ./build/myproject_namako
```

#### 11.13.3 Adapter Command Configuration Examples

The `namako.toml` file configures the adapter command for each project:

```toml
# Rust (current Naia setup)
adapter_cmd = ["cargo", "run", "-q", "-p", "naia_npa", "--"]

# JavaScript/TypeScript (Node.js)
adapter_cmd = ["node", "dist/myproject_namako.js"]

# Python
adapter_cmd = ["python", "-m", "myproject_namako"]

# Go (compiled binary)
adapter_cmd = ["./bin/myproject-namako"]

# C++ (compiled binary)
adapter_cmd = ["./build/myproject_namako"]
```

> **Note:** These examples are v2 guidance. The current system ships with Rust adapter support only.

### 11.14 Adapter SDKs (v2)

**What it adds:**
- Official Namako SDKs for major ecosystems: JS/TS, Python, Go, JVM, .NET, C++

**Why it matters:**
- Without SDKs, each adapter author re-invents the protocol and risks subtle incompatibilities.
- SDKs ensure consistent UX and correct implementation across ecosystems.

**SDK Responsibilities (Normative):**

Each SDK MUST provide:

1. **Ergonomic Step Registration**
   - Functions/decorators/annotations consistent with: one keyword (Given/When/Then) + one string expression
   - Example (Python): `@given("a user named {string}")`
   - Example (JS/TS): `Given("a user named {string}", async (world, name) => { ... })`

2. **Deterministic Binding ID Generation**
   - Compute `binding_id` from `(kind, expression_string)` using the documented `binding_id_scheme`
   - MUST produce identical IDs to the Rust implementation for the same inputs

3. **Semantic Registry Export**
   - Emit JSON manifest matching NPA schema
   - Include `binding_id`, `kind`, `expression`, `signature`, `impl_hash`

4. **Plan Execution Harness**
   - Load `resolved_plan.json`
   - Dispatch steps by `binding_id` only (no text matching)
   - Invoke bindings with correct captures, docstrings, datatables

5. **Run Report Emission**
   - Emit `run_report.json` with canonical ordering
   - Include all required fields per NPA schema

**Migration:**
- SDK adoption is optional but recommended
- Projects MAY implement NPA directly without SDK

### 11.15 Cross-Language Hashing & Conformance (v2)

Cross-language hash reproducibility is critical. This section defines two strategies.

#### Strategy 1: Reference Hash Helper ("Hash Oracle") — Recommended First

**What it adds:**
- A portable helper tool: `namako_hash_cli` (or `namako_hashd` daemon)
- Built from Rust, distributed as a standalone binary
- Adapters call it to compute hashes

**Contract (Normative):**
- The helper MUST implement the current `hash_contract_version` exactly.
- The helper MUST be distributed with version alignment to the Namako CLI.
- Adapters MUST declare in their manifest whether they use the helper (`hash_mode: "oracle"`) or native hashing (`hash_mode: "native"`).

**Helper Commands:**
```bash
# Compute binding_id
namako_hash_cli binding-id --kind Given --expression "a user named {string}"
# Output: {"binding_id": "abc123..."}

# Compute step_registry_hash
namako_hash_cli registry-hash --input registry.json
# Output: {"step_registry_hash": "def456..."}

# Compute payload_hash
namako_hash_cli payload-hash --input payload.json
# Output: {"payload_hash": "ghi789..."}
```

**Why it matters:**
- Ensures identical hashes across ecosystems without re-implementing canonical encoding.
- Reduces SDK implementation burden.
- Single source of truth for hash computation.

#### Strategy 2: Native Hashing in SDKs — Later (Publish Polish)

**What it adds:**
- SDKs implement canonical encoding + hashing natively in each language.
- No external helper dependency.

**Requirements:**
- SDKs MUST pass all conformance fixtures.
- SDKs MUST document their canonical encoding implementation.

#### Conformance Fixtures (Normative)

The Namako repo MUST ship conformance fixtures for:

| Fixture Category | Purpose |
|-----------------|--------|
| `binding_id_scheme` | Verify `(kind, expression)` → `binding_id` |
| `registry_hash` | Verify semantic registry → `step_registry_hash` |
| `payload_hash` | Verify execution payload → `payload_hash` |
| `plan_hash` | Verify resolved plan → `resolved_plan_hash` |

**Fixture Format (Normative):**
```json
{
  "fixture_version": "1",
  "hash_contract_version": "namako-json+blake3-256",
  "cases": [
    {
      "name": "simple_given_step",
      "input": { "kind": "Given", "expression": "a user named {string}" },
      "expected_binding_id": "abc123..."
    }
  ]
}
```

**Validation:**
- Adapters/SDKs MUST be able to run the conformance suite.
- Any mismatch MUST cause the conformance check to fail.
- CI MUST validate fixtures on all supported platforms.

### 11.16 Adapter Certification Tooling (v2)

**What it adds:**
- A CLI command: `namako adapter-verify` (or `namako conformance`)
- Validates third-party adapters before they are trusted in CI.

**Checks Performed (Normative):**

| Check | Description |
|-------|-------------|
| **Schema Validation** | Manifest and run_report match NPA JSON schemas exactly |
| **Binding ID Correctness** | All `binding_id` values match expected computation from `(kind, expression)` |
| **Canonical Ordering** | Run report scenarios and steps are correctly ordered |
| **Hash Implementation** | All hashes match conformance fixture expectations |
| **Freshness Check** | Adapter correctly rejects stale plans |

**Output:**
- Clear pass/fail diagnostics per check.
- Detailed error messages for failures.

**Why it matters:**
- Ensures third-party adapters behave correctly.
- Catches protocol violations before they cause CI failures.
- Builds trust in the multi-language ecosystem.

**Usage:**
```bash
# Run adapter conformance suite
namako adapter-verify --adapter-cmd "node dist/myproject_namako.js"

# Run with specific fixtures
namako adapter-verify --adapter-cmd "./bin/myproject-namako" --fixtures path/to/fixtures/
```

---

## Part 12: Definition of Done

The system is live when:

| Criterion | Description |
|-----------|-------------|
| **Resolution works** | `namako lint` resolves all features with strict errors |
| **Plan-driven execution works** | `namako run` executes via adapter by binding ID only |
| **Certification works** | `certification.json` contains identity tuple |
| **CI gate works** | `namako verify` passes in CI |
| **Manual update works** | `namako update-cert` refuses unless prerequisites met |
| **Adapter is non-autonomous** | Adapter dispatches by binding ID, no text matching |
| **Stale plans rejected** | Adapter refuses mismatched `step_registry_hash` |

---

## Appendix: No-drop Checklist (v9 Concept Trace)

This appendix traces every major concept from `NORTH_STAR_PLAN_v9.md` and labels its status.

### Goals

| Concept | Status | Notes |
|---------|--------|-------|
| Goal 1: Spec Unambiguity | **✅ Implemented** | Operational ambiguity → hard error. `namako review` implements rich packets (§10.5.3) |
| Goal 2: Scenario Completeness | **✅ Implemented** (partial) | Structural completeness (resolve all steps). Deep coverage **DEFERRED** to v2 |
| Goal 3: Test Faithfulness | **✅ Implemented** | Plan-driven execution |
| Goal 4: Repeatable Perfection | **✅ Implemented** | `namako verify` in CI |
| Goal 5: Change Propagation | **✅ Implemented** | Hash-based identity |
| Goal 6: Audit-Grade Outputs | **✅ Implemented** (partial) | Artifacts produced. Conformance fixtures **DEFERRED** to v2 (§11.8) |

### Architecture

| Concept | Status | Notes |
|---------|--------|-------|
| Engine resolves, Adapter obeys | **✅ Implemented** | Core principle |
| Trust boundary | **✅ Implemented** | Trusted adapter assumption |
| Baseline vs Candidate | **✅ Implemented** | Core certification model |
| Shared hashing infrastructure | **✅ Implemented** | Canonical JSON; §7.0 is single source of truth |
| Hash & Encoding Contract | **✅ Implemented** | §7.0 — authoritative reference for all hashing/encoding |
| `namako_hash` crate | **DEFERRED** to v2 | Current implementation uses inline hashing; v2 may extract crate |

### Resolution & Plan

| Concept | Status | Notes |
|---------|--------|-------|
| Resolved Execution Plan | **✅ Implemented** | Core artifact |
| `resolved_plan_hash` | **✅ Implemented** | Core identity field |
| `scenario_key` derivation | **✅ Implemented** | §6.4.3 — explicit ID format (`Feature:Rule(nn):Scenario(nn)`) |
| Kind inference (And/But → effective) | **✅ Implemented** | Standard Gherkin semantics |
| Signature enforcement | **✅ Implemented** | Hard error on mismatch; fully defined in §5.3 |
| Strict ambiguity policy | **✅ Implemented** | >1 match → hard error |
| Orphan → hard error | **✅ Implemented** | Hard error + `namako stub` mitigation (§10.5.2) |
| Missing step → hard error | **✅ Implemented** | 0 matches → hard error |

### ID Scheme

| Concept | Status | Notes |
|---------|--------|-------|
| `@Feature(name)` feature identity | **✅ Implemented** | Required tag (§10.5.1) |
| `@Rule(nn)` rule identity | **✅ Implemented** | Required tag (§10.5.1) |
| `@Scenario(nn)` scenario identity | **✅ Implemented** | Required tag (§10.5.1) |
| `EID` example row identity | **✅ Implemented** (optional) | SHOULD have; MUST for v2 (§10.5.1) |
| Expression-based binding ID | **✅ Implemented** | §4.2 |

### Spec Surface

| Concept | Status | Notes |
|---------|--------|-------|
| FeatureAstNorm | **DEFERRED** to v2 | §11.1. Current system uses simpler fingerprint |
| `feature_ast_hash` | **DEFERRED** to v2 | Current system uses `feature_fingerprint_hash` |
| Rule-only scenarios | **DEFERRED** to v2 | Current system does not enforce |
| Background under Rule only | **DEFERRED** to v2 | Current system does not enforce |
| Durations excluded from hash | **✅ Implemented** | Durations are metadata only |

### Invariants (v9)

| Invariant | Status | Notes |
|-----------|--------|-------|
| 1: Structural Tag Integrity | **✅ Implemented** | Explicit ID scheme enforced (§10.5.1) |
| 2: Explicit Binding Identity | **✅ Implemented** | Generated binding ID |
| 3: Engine Supremacy | **✅ Implemented** | Core principle |
| 4: No Orphan Bindings | **✅ Implemented** | Hard error + `namako stub` (§10.5.2) |
| 5: Operational Determinism | **✅ Implemented** | Sorted keys, stable order |
| 6: Single-Kind Binding Functions | **✅ Implemented** | Each binding → one kind |
| 7: Collision-Free Execution | **✅ Implemented** | Per-scenario World |
| 8: Explicit Certification Workflow | **✅ Implemented** | `verify` checks, `update-cert` changes |

### NPA

| Concept | Status | Notes |
|---------|--------|-------|
| `adapter manifest` | **✅ Implemented** | Semantic registry |
| `adapter run --plan` | **✅ Implemented** | Plan-driven execution |
| Semantic vs Debug registry split | **✅ Implemented** (simplified) | Current implementation has semantic only; debug is optional |
| `impl_hash` | **✅ Implemented** | Token-fingerprint scheme (§6.2.2) |
| `impl_hash_scheme` | **✅ Implemented** | Explicit scheme versioning (§6.2.2) |
| Freshness check | **✅ Implemented** | Refuse stale plans |
| `executed_payload_hash` | **✅ Implemented** | Integrity evidence |
| `executed_impl_hash` | **✅ Implemented** | Drift signal |

### Certification

| Concept | Status | Notes |
|---------|--------|-------|
| Identity vs Metadata split | **✅ Implemented** | Core design |
| `hash_contract_version` | **✅ Implemented** | Versioned encoding |
| Verify recomputes authority hashes | **✅ Implemented** | §7.4.1 — verify is the authority, not echoed values |
| Stale artifact detection | **✅ Implemented** | §7.4.3 — clear diagnostic on drift |
| `resolution_semantics_id` | **DEFERRED** to v2 | §11.9 |
| `bindings_used_hash` | **DEFERRED** to v2 | §11.12 |
| Conformance fixtures | **DEFERRED** to v2 | §11.8 |

### CLI

| Concept | Status | Notes |
|---------|--------|-------|
| `namako manifest` | **✅ Implemented** | Debug command |
| `namako lint` | **✅ Implemented** | Core command |
| `namako run` | **✅ Implemented** | Core command |
| `namako verify` | **✅ Implemented** | CI gate |
| `namako update-cert` | **✅ Implemented** | Manual baseline update |
| `namako status` | **✅ Implemented** | Diff tool + JSON output (§10.5.5) |
| `namako review` | **✅ Implemented** | AI work packets (§10.5.3) |
| `namako stub` | **✅ Implemented** | Orphan binding mitigation (§10.5.2) |

### Workflows

| Concept | Status | Notes |
|---------|--------|-------|
| Tight loop (AI-assisted SDD) | **✅ Implemented** | §10 |
| Slice-based workflow | **✅ Implemented** | §10.2 |
| Requirements capture | **✅ Implemented** | Step 1 |
| Convert to .feature | **✅ Implemented** | Step 2 |
| Scenario integrity loop | **✅ Implemented** | Step 3 |
| Binding faithfulness loop | **✅ Implemented** | Step 4 |
| Implement system | **✅ Implemented** | Step 5 |

### Multi-Language Support (New)

| Concept | Status | Notes |
|---------|--------|-------|
| Language-neutral adapter protocol | **✅ Implemented** (conceptual) | NPA is language-neutral by design; current system ships Rust adapter only |
| Any-language adapter support | **DEFERRED** to v2 | §11.13 |
| Universal 3-piece project pattern | **DEFERRED** to v2 | §11.13.2 |
| Adapter SDKs (JS/TS, Python, Go, etc.) | **DEFERRED** to v2 | §11.14 |
| Cross-language hashing (hash oracle) | **DEFERRED** to v2 | §11.15 Strategy 1 |
| Cross-language hashing (native SDK) | **DEFERRED** to v2 | §11.15 Strategy 2 |
| Conformance fixtures for adapters | **DEFERRED** to v2 | §11.15 |
| Adapter certification tooling | **DEFERRED** to v2 | §11.16 |

### Dropped Concepts

| Concept | Status | Reason |
|---------|--------|--------|
| Malicious adapter defense | **DROPPED** | Out of scope; trusted adapter assumption (v2 adds conformance as mitigation) |
| Deep semantic coverage measurement | **DROPPED** | Non-goal; review-driven process only |
| Assertion meaningfulness measurement | **DROPPED** | Non-goal; out of scope |

---

*End of NAMAKO_PLAN_FINAL.md*
