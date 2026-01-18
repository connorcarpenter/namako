# CURRENT_STATUS.md — GOLD_PLAN v1 Dashboard

Generated: 2026-01-18 (Updated)
Based on: Namako v2 "Autonomy" Sprint (Loop Enablement)

---

## 1. Snapshot Facts

| Item | Value |
|------|-------|
| Naia HEAD | `178785deb3f683af97689cd0f06c3dd2c6167201` |
| Namako HEAD | `f38adcbf2ffcb1b1a9ae0177a77299aaafe01a9a` |
| Pipeline Status | ✅ GREEN (Lint + Run + Verify + Determinism) |
| Active Scenarios | 20 executable |

### Namako Integration Layout (`naia/test/`)
```
naia/test/
├── harness/              # naia_test_harness lib (with TestWorldRef::server_observed)
├── npap/                 # naia_npap adapter binary
├── specs/
│   ├── features/         # 16 feature files (canonical contracts)
│   │   ├── 00_common.feature             # 10 deferred scenarios
│   │   ├── 01_connection_lifecycle.feature # 11 active scenarios
│   │   ├── smoke.feature                 # 9 active scenarios
│   │   └── [02-14]...feature             # Pruned (0 scenarios)
│   ├── scripts/
│   │   ├── namako_ci.sh                  # V1 CI Gate
│   │   ├── determinism_check.sh          # V2 Determinism Gate (NEW)
│   │   └── tesaki_next.sh                # V2 Autonomy Loop Stub (NEW)
│   └── certification.json                # Certified baseline (v1 maintained)
└── tests/                # naia_tests step bindings lib (with 22 active bindings)
```

---

## 2. GOLD_PLAN v1+v2 DoD Gate Status

### Pipeline Execution (`bash test/specs/scripts/namako_ci.sh`)

| Step | Result | Notes |
|------|--------|-------|
| Lint | ✅ PASS | Resolved 20 scenarios, 82 steps |
| Run  | ✅ PASS | All 20 scenarios passed execution |
| Verify | ✅ PASS | Baseline matches current execution |

**Stability:** Confirmed by consecutive passes.

### DoD Criterion Status (per GOLD_PLAN Part 11 + Autonomy Spec)

| Criterion | Status | Notes |
|-----------|--------|-------|
| Resolution works | ✅ Ready | All 20 scenarios resolve correctly |
| Plan-driven execution works | ✅ Ready | `TODO.md` drove 2→20 expansion |
| Certification works | ✅ Ready | `certification.json` tracking state |
| CI gate works | ✅ Ready | `namako_ci.sh` guarding merges |
| **Determinism works** | ✅ Ready | `bytes(run1) == bytes(run2)` verified |
| **Autonomy Packet Loop** | ✅ Ready | `status`→`review`→`explain` implemented |
| **Tesaki Stub** | ✅ Ready | `tesaki_next.sh` generates `NEXT_ACTION` |

---

## 3. Feature File Status

### Active Execution Coverage

| Feature File | Executable Scenarios | Status |
|--------------|---------------------|--------|
| smoke.feature | 9 | ✅ Certified |
| 01_connection_lifecycle.feature | 11 | ✅ Certified (Batch 1-2) |
| 00_common.feature | 0 | 10 Deferred |
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
| **TOTAL** | **20** | **10** (Visible deferred) |

### Binding Modules (`naia/test/tests/src/steps/`)

| Module | Bindings | Status |
|--------|----------|--------|
| `smoke.rs` | 6 | Core vertical slice |
| `connection.rs` | 16 | Lifecycle & Event observation |
| `_abi_proofs.rs` | 6 | Infrastructure self-test |

---

## 4. Known Failures / Instability

**None.**
CI is green. Determinism check passes.

---

## 5. Next Steps

### Phase 4: Step Binding Implementation — 🔄 IN PROGRESS

- [x] Initial vertical slice (smoke.feature)
- [x] Batch 1: Connection lifecycle (3 scenarios)
- [x] Batch 2: Event ordering (6 scenarios)
- [x] Batch 3+: Expansion to 20 scenarios
- [ ] Continue expansion to `00_common.feature` (Next 10)
- [ ] Restore/Unprune `12_server_events_api.feature`

### Phase 5: CI Integration — ✅ COMPLETE

- `namako_ci.sh` is reliable.
- `certification.json` is maintained.

### Phase 6: Autonomous Loop Enablement — ✅ COMPLETE

- [x] Implement `namako status`, `review`, `explain`
- [x] Ensure strict byte-level determinism
- [x] Create `determinism_check.sh`
- [x] Create `tesaki_next.sh` (Autonomy Loop Stub)
- [ ] **First Tesaki-Driven Iteration:** Use the scripts to drive the next batch of scenarios.

---

*End of CURRENT_STATUS.md*
