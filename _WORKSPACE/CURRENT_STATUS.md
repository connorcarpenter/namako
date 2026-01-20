# CURRENT_STATUS.md — Comprehensive Implementation Status

**Last Updated:** 2026-01-20
**MODE:** BOOTSTRAP (v1.5 COMPLETE, ready for CONSUMPTION transition)

---

## Executive Summary

**Namako v1 is FUNCTIONALLY COMPLETE.** All core components specified in GOLD_PLAN.md Parts 1–10 are implemented and operational.

**Namako v1.5 EXPLICIT ID TAGS is COMPLETE.** Implementation of stable, refactor-safe scenario identity system finished. Ready for production use.

| Milestone | Status |
|-----------|--------|
| Namako v1 Core | ✅ COMPLETE |
| NPA v1 Protocol | ✅ COMPLETE |
| Tesaki Task Orchestrator | ✅ COMPLETE |
| CI Gates | ✅ ALL GREEN |
| Bootstrap Exit Criteria | ✅ ALL SATISFIED |
| **Namako v1.5 Explicit ID Tags** | ✅ **COMPLETE** |

---

## 1. Gates Snapshot

### Commands

```bash
# Primary CI gate (lint → run → verify)
bash naia/test/specs/scripts/namako_ci.sh

# Determinism check (runs twice, compares bytes)
bash naia/test/specs/scripts/determinism_check.sh

# Tesaki orchestrator (from namako/ directory)
cargo run -p tesaki -- next \
  -s ../naia/test/specs \
  -a "cargo run --manifest-path ../naia/test/npa/Cargo.toml --" \
  --max-cert-updates 3
```

### Latest Results (2026-01-20)

| Gate | Status | Notes |
|------|--------|-------|
| `namako lint` | ✅ PASS | 31 scenarios, 134 steps resolved |
| `cargo test -p namako-cli` | ✅ PASS | 21 unit tests pass |
| `cargo test -p tesaki` | ✅ PASS | 5 unit tests pass (includes stub exclusion) |
| `cargo build -p namako-cli` | ✅ PASS | All warnings are cosmetic |
| Stub exclusion | ✅ VERIFIED | 0 promotion candidates (5 stubs excluded) |

### Scenario Counts

| Metric | Count |
|--------|-------|
| Executable scenarios | **31** |
| @Deferred scenarios | **5** (all are @Stub hygiene scenarios) |
| Promotion candidates | **0** (@Stub scenarios excluded) |
| Feature files | **17** |
| Total lines in specs | **2,111** |

---

## 2. V1 Implementation Status (Per GOLD_PLAN.md)

### Part 3: Crate Architecture — ✅ COMPLETE

| Crate | Location | Status |
|-------|----------|--------|
| `namako` (lib) | `namako/src/` | ✅ Engine, parser, npap, runner |
| `namako_codegen` (proc-macro) | `namako/codegen/` | ✅ Step macros, registry |
| `namako-cli` (bin) | `namako/cli/` | ✅ All v1 commands |
| `naia_test_harness` (lib) | `naia/test/harness/` | ✅ Scenario, World |
| `naia_tests` (lib) | `naia/test/tests/` | ✅ Step bindings |
| `naia_npa` (bin) | `naia/test/npa/` | ✅ NPA adapter |

### Part 4: Step Macro UX — ✅ COMPLETE

| Requirement | Status |
|-------------|--------|
| One macro + one string | ✅ Implemented |
| Generated binding IDs (§4.2) | ✅ `kind+expr_norm\|namako-binding-id-v1\|blake3-256-lowerhex` |
| Context-first ABI (§4.4) | ✅ `&mut CtxMut` for Given/When, `&CtxRef` for Then |
| Signature validation | ✅ Captures arity, docstring, datatable |
| Collision detection | ✅ Hard error on duplicate binding IDs |

### Part 5: Namako v1 CLI Commands — ✅ COMPLETE

| Command | Description | Status |
|---------|-------------|--------|
| `namako lint` | Resolve features → `resolved_plan.json` | ✅ |
| `namako verify` | Recompute hashes, compare to baseline | ✅ |
| `namako update-cert` | Manual baseline update with refusal rules | ✅ |
| `namako status` | FSM state + identity hashes (JSON output) | ✅ |
| `namako review` | Work backlog packet (promotion candidates) | ✅ |
| `namako explain` | Scenario fidelity packet | ✅ |
| `namako stub` | Generate @Deferred stubs for orphan bindings | ✅ |

### Part 5.5: Namako v1.5 Enhancements — ✅ COMPLETE

| Feature | Description | Status |
|---------|-------------|--------|
| Explicit ID tags | `@Feature(name)`, `@Rule_nn`, `@Scenario_nn` | ✅ |
| Orphan detection | Hard error on unused bindings | ✅ |
| Enhanced `review` | 5 sections per GOLD_PLAN §10.5.3 | ✅ |
| Fidelity packets | `explain` with binding_expression/source_location | ✅ |
| JSON status | All required fields per §10.5.5 | ✅ |
| Rich diffs | Human-readable identity status | ✅ |

### Part 6: NPA v1 Protocol — ✅ COMPLETE

| Requirement | Status |
|-------------|--------|
| `npap_version = 1` | ✅ |
| `hash_contract_version` | ✅ `namako-v1-json+blake3-256` |
| `adapter manifest` | ✅ Semantic registry JSON |
| `adapter run --plan --out` | ✅ Plan-driven execution |
| Dispatch by `binding_id` only | ✅ No text matching |
| Freshness check | ✅ Rejects stale plans |
| `impl_hash` scheme | ✅ `token-fingerprint-v1\|blake3-256-lowerhex` |
| Resolved plan schema | ✅ Per §6.4.1 |
| Run report schema | ✅ Per §6.4.2 |
| Scenario key derivation | ✅ `Feature:Rule_nn:Scenario_nn` format (explicit ID tags) |

### Part 7: Hashing & Identity — ✅ COMPLETE

| Requirement | Status | Implementation |
|-------------|--------|----------------|
| Hash contract version | ✅ | `namako-v1-json+blake3-256` |
| String normalization (§7.0.2) | ✅ | NFC + `\n` newlines |
| Canonical JSON (§7.0.3) | ✅ | Sorted keys, explicit nulls |
| BLAKE3-256 lowerhex | ✅ | 64-char hex output |
| Self-hash exclusion (§7.0.5) | ✅ | Omit only own hash field |
| `feature_fingerprint_hash` | ✅ | Simpler v1 fingerprint |
| `step_registry_hash` | ✅ | Sorted by binding_id |
| `resolved_plan_hash` | ✅ | |
| Certification identity tuple | ✅ | `{ identity, metadata }` |

### Part 9: Tesaki AI Driver — ✅ COMPLETE

| Requirement | Status |
|-------------|--------|
| Consumes Namako packets | ✅ status, review, explain |
| Generates NEXT_TASK.md | ✅ Deterministic output |
| `--max-cert-updates` governance | ✅ 0 = manual, N = autonomous |
| Audit log for update-cert | ✅ `update_cert_log.jsonl` |
| Mode-aware CORE blocker filtering | ✅ BOOTSTRAP skips CORE |
| Blocker classification | ✅ HARNESS_ONLY, CORE, EXTERNAL, UNKNOWN |

### Part 10: Spec-Driven Development Loop — ✅ COMPLETE

| Step | Description | Status |
|------|-------------|--------|
| Requirements capture | Human input | ✅ (manual) |
| Convert to .feature | Normative spec | ✅ |
| Scenario integrity loop | `namako lint` | ✅ |
| Binding faithfulness loop | lint → run → verify | ✅ |
| Implement system | Iterate until green | ✅ |

---

## 3. V1.5 Features Status (AI-Enablement — COMPLETE)

All v1.5 features have been implemented per GOLD_PLAN.md §10.5.

| Feature | Section | Status |
|---------|---------|--------|
| Explicit ID tags (@Feature/@Rule_nn/@Scenario_nn) | §10.5.1 | ✅ **COMPLETE** |
| Orphan binding hard error + `namako stub` | §10.5.2 | ✅ **COMPLETE** |
| `namako review` coverage enhancements | §10.5.3 | ✅ **COMPLETE** |
| Scenario fidelity packets (`namako explain`) | §10.5.4 | ✅ **COMPLETE** |
| Machine-readable process state (`namako status --json`) | §10.5.5 | ✅ **COMPLETE** |
| Rich `namako status` diffs | §10.5.6 | ✅ **COMPLETE** |

---

## 4. V2+ Features Status (Deferred Per GOLD_PLAN.md Part 11)

V2+ features remain **DEFERRED** — not blocking v1.5 or CONSUMPTION mode.

| Feature | Section | Status |
|---------|---------|--------|
| FeatureAstNorm (full AST hashing) | §11.1 | ⏳ Deferred |
| CBOR encoding profiles | §11.7 | ⏳ Deferred |
| Conformance fixtures | §11.8 | ⏳ Deferred |
| `resolution_semantics_id` | §11.9 | ⏳ Deferred |
| Stronger `impl_hash` schemes | §11.11 | ⏳ Deferred |
| `bindings_used_hash` | §11.12 | ⏳ Deferred |
| Multi-language support | §11.13 | ⏳ Deferred |
| Adapter SDKs (JS/TS, Python, etc.) | §11.14 | ⏳ Deferred |
| Cross-language hashing | §11.15 | ⏳ Deferred |
| Adapter certification tooling | §11.16 | ⏳ Deferred |

---

## 5. Bootstrap Exit Criteria (§2.5) — ✅ ALL SATISFIED

| Criterion | Status | Evidence |
|-----------|--------|----------|
| Tesaki end-to-end | ✅ | `tesaki next` produces `NEXT_TASK.md` deterministically |
| Namako packets deterministic | ✅ | `status --json`, `review`, `explain` all produce stable outputs |
| Tesaki selects promotion candidates | ✅ | `reuse_score` computed for @Deferred scenarios |
| Tesaki generates binding bundles | ✅ | `suggested_binding_bundle` in review output |
| Tesaki generates explain packets | ✅ | `explain` command outputs scenario details |
| Tesaki stops safely when blocked | ✅ | Returns `DONE` when no work available |
| Scenario fidelity workflow exists | ✅ | `namako explain` implemented |
| CI green | ✅ | All gates pass |

---

## 6. Completed Work History (Reconstructed from Git)

### Recent Commits (namako repo)

| Commit | Description |
|--------|-------------|
| `d8faace` | Mission 001 checkpoint (Named trait refactoring for ProtocolMismatch) |
| `d26e76e` | CONSUMPTION mode documentation updates |
| `a1accdf` | CLAUDE.md, SYSTEM.md SSoT policy, blocker classification, mode-aware filtering |
| `7c6e580` | Create CLAUDE.md for agent discipline |
| `7a1d71e` | Add suggested binding bundle computation for promotion candidates |
| `719569a` | Fix scenario counts and promotion candidates accuracy |
| `97a83c6` | Implement failure targeting with run report integration |
| `57e4290` | Add Tesaki task orchestrator with initial implementation |
| `55d7bf8` | Exclude deferred scenarios from executable plan |
| `f38adcb` | Remove CONVERSION_PLAN.md, update CURRENT_STATUS.md |
| `2965192` | Fix integration details in documentation |
| `1859785` | Consolidate context wrapper types into test_utils |
| `301b303` | Refactor context wrapper types |
| `59155ca` | Enhance context wrapper types with world access methods |
| `d2bb973` | Refactor context handling in codegen and tests |
| `ec0cb62` | Update GOLD_PLAN.md for context-first ABI |
| `9b59edb` | Implement polling loop for Then steps with ExpectCtx wrapping |

### Recent Commits (naia repo)

| Commit | Description |
|--------|-------------|
| `59efef07` | Refactor message handling and protocol management |
| `5e4bf6d9` | Implement trace event system for deterministic operation ordering |
| `726d8f36` | Add operation result tracking for error handling |
| `caa1c377` | Rename `npap` to `npa`, add implementation files |
| `baf48c49` | Add auth-required event ordering scenarios |
| `55a5e444` | Add determinism enforcement scripts |
| `178785de` | Refactor feature specifications, enhance connection tracking |
| `5288a9e5` | Comprehensive specification refactoring |
| `7399e6a3` | Add connection lifecycle specification |

---

## 7. Current Identity (Pending Certification)

**Note:** Identity hashes changed due to `source_symbol` field addition. Run `namako update-cert` to establish new baseline.

| Field | Hash |
|-------|------|
| `hash_contract_version` | `namako-v1-json+blake3-256` |
| `feature_fingerprint_hash` | `bba84b749b4419895b8c48e4450e498c03673a124eedd7d93fbca167eda696be` |
| `step_registry_hash` | `35cebf1fb7ae941e2ae4cf0838cdff1bfd85eb323c59b7bb554a0554330741c7` |
| `resolved_plan_hash` | `7de72320d341274742d561a6d93cfaef7ca27fc1bcbd7a5b4b8ab21394b7b454` |

---

## 8. Artifacts

| Artifact | Path |
|----------|------|
| Certification | `naia/test/specs/certification.json` |
| Resolved Plan | `naia/target/namako_artifacts/resolved_plan.json` |
| Run Report | `naia/target/namako_artifacts/run_report.json` |
| Status JSON | `naia/test/specs/target/namako_artifacts/tesaki/status.json` |
| Review JSON | `naia/test/specs/target/namako_artifacts/tesaki/review.json` |
| NEXT_TASK.md | `naia/test/specs/target/namako_artifacts/tesaki/NEXT_TASK.md` |

---

## 9. Transition Readiness

### Current Phase: Ready for CONSUMPTION

**v1.5 AI-enablement features are COMPLETE:**

| Sprint | Focus | Status |
|--------|-------|--------|
| Sprint 1 | Explicit ID tags (@Feature/@Rule_nn/@Scenario_nn) | ✅ COMPLETE |
| Sprint 2 | Orphan binding enforcement + `namako stub` | ✅ COMPLETE |
| Sprint 3 | Enhanced `namako review` packets | ✅ COMPLETE |
| Sprint 4 | Enhanced `namako explain` + `status` | ✅ COMPLETE |

### CONSUMPTION Transition Steps

To begin CONSUMPTION mode:
1. ✅ Verify `@Stub` exclusion from promotion candidates — **DONE** (2026-01-20)
2. [ ] Run `namako update-cert` to establish baseline (hashes changed)
3. [ ] Update this file: `MODE: CONSUMPTION`
4. [ ] Select first CORE work item (per GOLD_PLAN §2.7 First CONSUMPTION Mission Template)
5. [ ] Drive through Tesaki Product FSM

### Current @Deferred Status

**5 scenarios are currently @Deferred** — all are `@Deferred @Stub` scenarios in `_orphan_stubs.feature`.

These are **hygiene-only stubs** generated by `namako stub` for orphan bindings (step bindings that exist in the adapter but aren't used by any executable scenario). They:
- Exist to satisfy the orphan binding hard error policy (v1.5)
- Are tagged `@Stub` and MUST NOT be promoted or selected as work by Tesaki
- Represent bindings that may become useful when new scenarios are written

All production @Deferred scenarios have been:
- Promoted (bindings implemented, tests passing), OR
- Removed (determined to be out of scope for v1)

---

*End of CURRENT_STATUS.md*
