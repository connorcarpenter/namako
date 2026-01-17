# CURRENT_STATUS.md — GOLD_PLAN v1 Dashboard

Generated: 2026-01-17
Based on: Empirical inspection, not speculation

---

## 1. Snapshot Facts

| Item | Value |
|------|-------|
| Naia HEAD | `acf566c7a3c05d8ff96093a9d7936a7aa248db26` |
| Namako HEAD | `301b303981977833be9b569f925b157efeb7a30c` |
| Naia working tree | Modified: `.idea/naia.iml`, 25+ untracked files in `test/specs/` |
| Namako working tree | Clean (2 untracked in `_WORKSPACE/`) |

### Namako Integration Layout (`naia/test/`)
```
naia/test/
├── harness/              # naia_test_harness lib
├── npap/                 # naia_npap adapter binary
├── specs/
│   ├── contracts/        # 15 legacy spec.md files
│   ├── features/
│   │   ├── connection/
│   │   │   └── connection_lifecycle.feature   (3 scenarios)
│   │   └── smoke/
│   │       └── namako_smoke.feature           (2 scenarios)
│   ├── scripts/
│   │   └── namako_ci.sh  # Canonical pipeline script
│   ├── certification.json
│   ├── resolved_plan.json
│   └── run_report.json
└── tests/                # naia_tests step bindings lib
```

---

## 2. GOLD_PLAN v1 DoD Gate Status

### Pipeline Execution (`bash test/specs/scripts/namako_ci.sh`)

| Step | Result |
|------|--------|
| Lint | ✅ PASS |
| Run  | ✅ PASS |
| Verify | ❌ FAIL (3 mismatches) |

**Verify Output:**
```
✗ IDENTITY MISMATCHES (3):

  1. BASELINE DRIFT: feature_fingerprint_hash
     Baseline: 62d7960ec9c8f67e...
     Current:  56675e063b466934...

  2. BASELINE DRIFT: step_registry_hash
     Baseline: 43e7b9bf7a65cfbf...
     Current:  4364ddceba538a8a...

  3. BASELINE DRIFT: resolved_plan_hash
     Baseline: f7d5f355ee9db866...
     Current:  0404f57bf06fe9cd...
```

**Root Cause:** The baseline `certification.json` was locked before the `connection_lifecycle.feature` scenarios were added. Features and bindings have changed since baseline was written.

### Plan-Driven Adapter + Stale-Plan Refusal

| Check | Status | Evidence |
|-------|--------|----------|
| Adapter is plan-driven (dispatch by `binding_id` only) | ✅ YES | [run.rs#L38-L65](../../../naia/test/npap/src/run.rs) — `build_dispatch_table` maps `binding_id` → handler |
| Stale-plan refusal | ✅ YES | [run.rs#L135-L144](../../../naia/test/npap/src/run.rs) — compares `plan.header.step_registry_hash` vs current manifest |

**Manifest Command:**
```
cargo run --manifest-path naia/test/npap/Cargo.toml -- manifest
```
```json
{
  "npap_version": 1,
  "hash_contract_version": "namako-v1-json+blake3-256",
  "binding_id_scheme": "kind+expr_norm|namako-binding-id-v1|blake3-256-lowerhex",
  "impl_hash_scheme": "token-fingerprint-v1|blake3-256-lowerhex",
  "step_registry_hash": "4364ddceba538a8a869b2628b6b90c0eb7bce8d9275e7ad9adc310bd73e5ab17"
}
```

---

## 3. Identity Rules In Effect

| Rule | Location | Implementation |
|------|----------|----------------|
| **scenario_key derivation** | [npap.rs#L317-L320](../../src/npap.rs) | `normalize_path(rel) + ":L" + line` |
| **binding_id derivation** | [npap.rs#L227-L230](../../src/npap.rs) | `blake3("namako-binding-id-v1\|" + kind + "\|" + expr_norm)` |
| **THEN-step execution model** | [world.rs#L71-L92](../../../naia/test/tests/src/world.rs) | **Polling** — `assert_then()` loops with `scenario.until(500.ticks())` calling step repeatedly until `AssertOutcome::Passed` or timeout |

### THEN-step Detail
Steps return `AssertOutcome<T>`:
- `Passed(val)` → success, stop polling
- `Pending` → not yet, keep polling
- `Failed(msg)` → hard failure, panic immediately

Example: [connection.rs#L207-L216](../../../naia/test/tests/src/steps/connection.rs)

---

## 4. Contract Conversion Progress

### Summary
| Metric | Count |
|--------|-------|
| Total contracts | 15 |
| Feature files | 2 |
| Total scenarios | 5 |

### Conversion Table

| Contract | Feature Path | Scenarios | Baseline Updated | Notes |
|----------|--------------|-----------|------------------|-------|
| 00_common | — | 0 | N/A | Shared definitions only |
| **01_connection_lifecycle** | `features/connection/connection_lifecycle.feature` | 3 | N (stale) | Slice 1 in progress |
| 02_transport | — | 0 | — | Not started |
| 03_messaging | — | 0 | — | Not started |
| 04_time_ticks_commands | — | 0 | — | Not started |
| 05_observability_metrics | — | 0 | — | Not started |
| 06_entity_scopes | — | 0 | — | Not started |
| 07_entity_replication | — | 0 | — | Not started |
| 08_entity_ownership | — | 0 | — | Not started |
| 09_entity_publication | — | 0 | — | Not started |
| 10_entity_delegation | — | 0 | — | Not started |
| 11_entity_authority | — | 0 | — | Not started |
| 12_server_events_api | — | 0 | — | Not started |
| 13_client_events_api | — | 0 | — | Not started |
| 14_world_integration | — | 0 | — | Not started |
| **(smoke test)** | `features/smoke/namako_smoke.feature` | 2 | N (stale) | Foundational test |

---

## 5. Known Failures / Instability

### Current Failure: `namako verify` baseline mismatch

**Failing Command:**
```bash
cd naia/test/specs && bash scripts/namako_ci.sh
```

**Exit Code:** 3 (Verify failed)

**Relevant Log Lines:**
```
[1/3] Running lint...
✓ Lint passed

[2/3] Running adapter execution...
✓ Run passed

[3/3] Running verify...
✗ IDENTITY MISMATCHES (3):
  1. BASELINE DRIFT: feature_fingerprint_hash
  2. BASELINE DRIFT: step_registry_hash
  3. BASELINE DRIFT: resolved_plan_hash
❌ Verify failed (baseline mismatch)
```

**Resolution Required:**
Run `namako update-cert` to update baseline certification after confirming current scenarios and bindings are correct.

---

## 6. v1 DoD Checklist

| Criterion | Status | Notes |
|-----------|--------|-------|
| Resolution works (`namako lint`) | ✅ PASS | All 5 scenarios resolve cleanly |
| Plan-driven execution works (`namako run`) | ✅ PASS | All scenarios pass |
| Certification artifact exists | ✅ YES | `test/specs/certification.json` |
| CI gate works (`namako verify`) | ❌ FAIL | Baseline stale |
| Manual update works (`namako update-cert`) | ⚠️ UNTESTED | Needs explicit test |
| Adapter is non-autonomous (dispatch by binding_id) | ✅ YES | Proven by code inspection |
| Stale plans rejected | ✅ YES | Proven by code inspection |

---

## 7. Next Steps

1. **Fix baseline drift** — Run `namako update-cert` to lock new baseline
2. **Verify CI gate** — Confirm `namako_ci.sh` exits 0 after baseline update
3. **Continue slice conversion** — Proceed with `01_connection_lifecycle` Slice 2
4. **Document v1 completion** — Once all DoD gates pass, mark v1 as complete

---

*End of CURRENT_STATUS.md*
