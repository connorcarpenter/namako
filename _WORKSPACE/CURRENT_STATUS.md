# CURRENT_STATUS.md — GOLD_PLAN v1 Dashboard

Generated: 2026-01-18 (Final Update by Opus)
Based on: Namako v2 "Autonomy" Sprint (Loop Enablement) + Failure Targeting

---

## 1. Snapshot Facts

| Item | Value |
|------|-------|
| Naia HEAD | `caa1c377` (modified) |
| Namako HEAD | `57e4290` (modified) |
| Pipeline Status | **GREEN** (Lint + Run + Verify all pass) |
| Active Scenarios | **23 executable** |
| Promotion Candidates | **8** (from 00_common.feature, all need new bindings) |

### Namako Integration Layout (`naia/test/`)
```
naia/test/
├── harness/              # naia_test_harness lib
├── npa/                  # naia_npa adapter binary (renamed from npap)
├── specs/
│   ├── features/         # 16 feature files (canonical contracts)
│   │   ├── 00_common.feature             # 8 @Deferred scenarios
│   │   ├── 01_connection_lifecycle.feature # 14 active scenarios
│   │   ├── smoke.feature                 # 9 active scenarios
│   │   └── [02-14]...feature             # Pruned (0 scenarios)
│   ├── scripts/
│   │   ├── namako_ci.sh                  # V1 CI Gate
│   │   ├── determinism_check.sh          # V2 Determinism Gate
│   │   ├── tesaki_next.sh                # V2 Autonomy Loop Stub
│   │   └── tesaki_loop.sh                # V2 NEXT_TASK.md Generator
│   └── certification.json                # Certified baseline (UPDATED)
└── tests/                # naia_tests step bindings lib
```

---

## 2. GOLD_PLAN v1+v2 DoD Gate Status

### Pipeline Execution (`bash test/specs/scripts/namako_ci.sh`)

| Step | Result | Notes |
|------|--------|-------|
| Lint | ✅ PASS | Resolved 23 scenarios, all steps bound |
| Run  | ✅ PASS | All 23 scenarios passed execution |
| Verify | ✅ PASS | Baseline matches current state |

**Stability:** CI passed twice consecutively. Determinism check passes.

### DoD Criterion Status (per GOLD_PLAN Part 11 + Autonomy Spec)

| Criterion | Status | Notes |
|-----------|--------|-------|
| Resolution works | ✅ Ready | All 23 scenarios resolve correctly |
| Plan-driven execution works | ✅ Ready | Adapter executes by binding_id only |
| Certification works | ✅ Ready | Baseline updated and verified |
| CI gate works | ✅ Ready | `namako_ci.sh` operational |
| **Determinism works** | ✅ Ready | `bytes(run1) == bytes(run2)` verified |
| **Autonomy Packet Loop** | ✅ Ready | `status`→`review`→`explain` implemented |
| **Tesaki Loop** | ✅ Ready | `tesaki next` generates `NEXT_TASK.md` |
| **@Deferred Filtering** | ✅ Ready | Engine excludes @Deferred from plan |
| **Failure Targeting** | ✅ Ready | `last_run_failures` field added to status JSON |

---

## 3. Current Identity (Certified)

| Field | Hash |
|-------|------|
| `feature_fingerprint_hash` | `eb508b39800dd89c2c9b28a6473cebbf09a7e2640b87adab6087f30f6c13bc1d` |
| `step_registry_hash` | `7d1522b771ced917aa3b70513131b382d4954710b89757ed54c59b7a4b310d33` |
| `resolved_plan_hash` | `396479fc690b89ba0bbfcf8df57120f020f1e012b248ce63972e5ad932069ba1` |

**Recommended Action:** `DONE`

---

## 4. Feature File Status

### Active Execution Coverage

| Feature File | Executable Scenarios | Status |
|--------------|---------------------|--------|
| smoke.feature | 9 | ✅ Working |
| 01_connection_lifecycle.feature | **14** | ✅ Working |
| 00_common.feature | 0 | 8 @Deferred (promotion candidates) |
| 02_transport.feature | 0 | Pruned |
| 03_messaging.feature | 0 | Pruned |
| 04_time_ticks_commands.feature | 0 | Pruned |
| 05_observability_metrics.feature | 0 | Pruned |
| 06_entity_scopes.feature | 0 | Pruned |
| 07_entity_replication.feature | 0 | Pruned |
| 08_entity_ownership.feature | 0 | Pruned |
| 09_entity_publication.feature | 0 | Pruned |
| 10_entity_delegation.feature | 0 | Pruned |
| 11_entity_authority.feature | 0 | Pruned |
| 12_server_events_api.feature | 0 | Pruned |
| 13_client_events_api.feature | 0 | Pruned |
| 14_world_integration.feature | 0 | Pruned |
| **TOTAL** | **23** | 8 promotion candidates |

---

## 5. Session Changes Summary

### Namako Changes (Uncommitted)
- **cli/src/status.rs**: Added failure targeting (+167 lines)
  - New `FailureRecord` struct with scenario_key, scenario_name, failure_kind, summary
  - New `last_run_failures` field in `StatusOutput`
  - `load_run_failures()` function to extract failures from run_report.json
  - Helper functions: `extract_scenario_name()`, `classify_failure()`, `truncate_summary()`
  - Unit tests for all new helper functions

### Naia Changes (Uncommitted)
- **certification.json**: Updated to current baseline
- **scripts/*.sh**: Updated `npap` → `npa` paths

---

## 6. Promotion Candidates (All Blocked)

From `namako review`, all 8 candidates have `reuse_score: 0`:

1. Multiple commands for same tick apply in receipt order
2. Same-tick scope operations resolve deterministically
3. Identical inputs produce identical outputs
4. Duplicate replication messages do not panic
5. Malformed inbound packet is dropped without panic
6. API misuse returns Err not panic
7. Protocol mismatch produces ProtocolMismatch rejection
8. Reconnecting client receives fresh entity spawns

**Blocker:** All candidates require implementing new step bindings (no reuse).

Per TODO.md §5.2: "If all candidates require net-new bindings (reuse_score=0), stop loop and record what was achieved."

---

## 7. Artifacts

| Artifact | Path |
|----------|------|
| Status JSON | `target/namako_artifacts/tesaki/status.json` |
| Review JSON | `target/namako_artifacts/tesaki/review.json` |
| NEXT_TASK.md | `target/namako_artifacts/tesaki/NEXT_TASK.md` |
| Run Report | `target/namako_artifacts/run_report.json` |
| Resolved Plan | `target/namako_artifacts/resolved_plan.json` |

---

## 8. Next Steps

To promote scenarios from the @Deferred backlog:

1. Choose a promotion candidate from the list above
2. Implement the required step bindings in `naia/test/tests/src/steps/`
3. Remove the `@Deferred` tag from the scenario
4. Run `bash scripts/namako_ci.sh` until green
5. Run `namako update-cert` to update baseline
6. Repeat

---

*End of CURRENT_STATUS.md*
