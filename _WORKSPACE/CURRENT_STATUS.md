# CURRENT_STATUS.md — GOLD_PLAN v1 Dashboard

Generated: 2026-01-19 (Updated by Opus after Promotion Sprint)
Based on: Namako v2 "Autonomy" Sprint + Common Definitions Promotion

---

## 1. Snapshot Facts

| Item | Value |
|------|-------|
| Naia HEAD | `caa1c377` (modified) |
| Namako HEAD | `57e4290` (modified) |
| Pipeline Status | **GREEN** (Lint + Run + Verify all pass) |
| Active Scenarios | **28 executable** (+5 from previous) |
| Promotion Candidates | **3** (from 00_common.feature, remaining @Deferred) |

### Namako Integration Layout (`naia/test/`)
```
naia/test/
├── harness/              # naia_test_harness lib
├── npa/                  # naia_npa adapter binary (renamed from npap)
├── specs/
│   ├── features/         # 16 feature files (canonical contracts)
│   │   ├── 00_common.feature             # 5 active + 3 @Deferred scenarios
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
    └── src/steps/
        ├── smoke.rs                      # Basic smoke test bindings
        ├── connection.rs                 # Connection lifecycle bindings
        └── common.rs                     # NEW: Common contract bindings
```

---

## 2. GOLD_PLAN v1+v2 DoD Gate Status

### Pipeline Execution (`bash test/specs/scripts/namako_ci.sh`)

| Step | Result | Notes |
|------|--------|-------|
| Lint | ✅ PASS | Resolved 28 scenarios, 120 steps bound |
| Run  | ✅ PASS | All 28 scenarios passed execution |
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
| 00_common.feature | **5** | ✅ Working (+5 promoted) |
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
| **TOTAL** | **28** | 3 remaining promotion candidates |

---

## 5. Session Changes Summary (2026-01-19 Promotion Sprint)

### Naia Test Harness Changes
- **harness/src/harness/scenario.rs**: Added `OperationResult` struct and tracking API
  - `last_operation_result: Option<OperationResult>` field in Scenario
  - Methods: `record_ok()`, `record_err()`, `record_panic()`, `clear_operation_result()`
  - Enables common outcome assertions (panic/Err detection)

- **harness/src/harness/mod.rs**: Exported `OperationResult`
- **harness/src/lib.rs**: Exported `OperationResult`

### Naia Tests Changes
- **tests/src/steps/common.rs**: NEW - Common Definitions contract bindings (~600 lines)
  - **Given steps**: `a test scenario`, `a connected client`, `a connected client with replicated entities`, `a client that was previously connected`, `the client disconnected`, `a test scenario with deterministic time`, `a deterministic network input sequence`
  - **When steps**: `the client attempts an invalid API operation`, `the server receives a malformed packet`, `duplicate replication messages arrive`, `the client reconnects`, `the same API call sequence is executed twice`
  - **Then steps**: `the operation returns an Err result`, `no panic occurs`, `the packet is dropped`, `they are handled idempotently`, `it receives fresh entity spawns for all in-scope entities`, `no prior session state is retained`, `the event emission order is identical both times`, `the entity spawn order is identical both times`

- **tests/src/steps/mod.rs**: Added `pub mod common;`

### Feature File Changes
- **specs/features/00_common.feature**: Promoted 5 scenarios
  - "API misuse returns Err not panic" - ACTIVE
  - "Malformed inbound packet is dropped without panic" - ACTIVE
  - "Duplicate replication messages do not panic" - ACTIVE
  - "Reconnecting client receives fresh entity spawns" - ACTIVE
  - "Identical inputs produce identical outputs" - ACTIVE
  - Remaining @Deferred: 3 scenarios (protocol mismatch, per-tick determinism x2)

### Certification Changes
- **specs/certification.json**: Updated to new baseline (28 scenarios, 120 steps)

---

## 6. Remaining Promotion Candidates (3)

After the 2026-01-19 Promotion Sprint, 3 scenarios remain @Deferred:

1. **Protocol mismatch produces ProtocolMismatch rejection**
   - Requires: Protocol version mismatch setup, rejection handling
   - Complexity: Medium (needs protocol negotiation testing)

2. **Same-tick scope operations resolve deterministically**
   - Requires: Multiple scope operations in same tick, determinism verification
   - Complexity: Medium (needs concurrent operation testing)

3. **Multiple commands for same tick apply in receipt order**
   - Requires: Command ordering verification across multiple commands
   - Complexity: Medium (needs tick buffer message ordering testing)

**Status:** These 3 scenarios require more complex harness infrastructure for testing concurrent/ordered operations and protocol negotiation.

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
