# CURRENT_STATUS.md — GOLD_PLAN v1 Dashboard

Generated: 2026-01-17
Based on: Empirical inspection

---

## 1. Snapshot Facts

| Item | Value |
|------|-------|
| Naia HEAD | `acf566c7a3c05d8ff96093a9d7936a7aa248db26` |
| Namako HEAD | `301b303981977833be9b569f925b157efeb7a30c` |
| Naia working tree | Modified — feature files reformatted to canonical template |
| Feature Files | 16 files (15 contracts + smoke) |

### Namako Integration Layout (`naia/test/`)
```
naia/test/
├── harness/              # naia_test_harness lib
├── npap/                 # naia_npap adapter binary
├── specs/
│   ├── contracts/        # 15 legacy spec.md files (can be deprecated)
│   ├── features/         # 16 feature files (canonical contracts)
│   │   ├── 00_common.feature
│   │   ├── 01_connection_lifecycle.feature
│   │   ├── 02_transport.feature
│   │   ├── 03_messaging.feature
│   │   ├── 04_time_ticks_commands.feature
│   │   ├── 05_observability_metrics.feature
│   │   ├── 06_entity_scopes.feature
│   │   ├── 07_entity_replication.feature
│   │   ├── 08_entity_ownership.feature
│   │   ├── 09_entity_publication.feature
│   │   ├── 10_entity_delegation.feature
│   │   ├── 11_entity_authority.feature
│   │   ├── 12_server_events_api.feature
│   │   ├── 13_client_events_api.feature
│   │   ├── 14_world_integration.feature
│   │   └── smoke.feature
│   ├── scripts/
│   │   └── namako_ci.sh
│   └── ...
└── tests/                # naia_tests step bindings lib (needs update)
```

---

## 2. GOLD_PLAN v1 DoD Gate Status

### Pipeline Execution (`bash test/specs/scripts/namako_ci.sh`)

| Step | Result | Notes |
|------|--------|-------|
| Lint | ❌ FAIL | "Missing step" errors (Expected: step bindings not yet implemented) |
| Run  | ➖ SKIP | Blocked by Lint |
| Verify | ➖ SKIP | Blocked by Lint |

**Lint Output Summary:**
- **Parse Status**: ✅ Gherkin syntax valid (All 16 files parse correctly)
- **Binding Status**: ❌ ~892 steps missing bindings (expected until Phase 4)

### DoD Criterion Status (per GOLD_PLAN Part 11)

| Criterion | Status | Notes |
|-----------|--------|-------|
| Resolution works | 🔄 Blocked | Steps resolve but no bindings exist |
| Plan-driven execution works | ➖ Pending | Requires step bindings |
| Certification works | ➖ Pending | Requires passing run |
| CI gate works | ➖ Pending | Requires certification baseline |
| Manual update works | ➖ Pending | Requires passing run |
| Adapter is non-autonomous | ✅ Ready | naia_npap dispatches by binding ID |
| Stale plans rejected | ✅ Ready | Adapter freshness check implemented |

---

## 3. Feature File Status (Canonical Template Compliance)

### Phase 3 Complete: Format/Structure Cleanup ✅

All 15 contract feature files have been reformatted to the canonical template:

| Section | Status |
|---------|--------|
| Header banner | ✅ All files |
| NORMATIVE CONTRACT MIRROR (no legacy IDs) | ✅ All files |
| Feature + Rule blocks (Gherkin) | ✅ All files |
| DEFERRED TESTS section | ✅ All files |
| AMBIGUITIES section | ✅ All files |

**Legacy ID Removal**: All `[prefix-XX]` patterns removed from all files.

### Scenario & Deferred Item Counts

| Feature File | Executable Scenarios | Deferred Items |
|--------------|---------------------|----------------|
| 00_common.feature | 10 | 2 |
| 01_connection_lifecycle.feature | 39 | 3 |
| 02_transport.feature | 7 | 2 |
| 03_messaging.feature | 17 | 3 |
| 04_time_ticks_commands.feature | 14 | 2 |
| 05_observability_metrics.feature | 11 | 2 |
| 06_entity_scopes.feature | 11 | 1 |
| 07_entity_replication.feature | 12 | 2 |
| 08_entity_ownership.feature | 13 | 2 |
| 09_entity_publication.feature | 8 | 2 |
| 10_entity_delegation.feature | 12 | 2 |
| 11_entity_authority.feature | 28 | 2 |
| 12_server_events_api.feature | 29 | 2 |
| 13_client_events_api.feature | 27 | 2 |
| 14_world_integration.feature | 23 | 2 |
| smoke.feature | 2 | 0 |
| **TOTAL** | **263** | **31** |

### Top Missing Harness Capabilities (from DEFERRED TESTS)

| Capability | Occurrences |
|------------|-------------|
| Wire-level packet inspection | 2 |
| Network conditioner with precise latency control + metrics API | 2 |
| Time manipulation / clock injection | 2 |
| Memory profiling instrumentation | 2 |
| Precise timing control of concurrent operations | 2 |

---

## 4. Known Failures / Instability

### Current Failure: Missing Step Bindings

**Failing Command:** `namako lint`

**Cause:**
Feature files define 263 Gherkin scenarios with ~892 unique steps. The `naia_npap` adapter has not yet implemented step bindings for these.

**Example Error:**
```
Missing step: no binding found for Given "a Naia client test environment is initialized"
```

**This is expected.** Phase 4 (Step Binding Implementation) will resolve this.

---

## 5. Next Steps

### Phase 3: Contract Conversion — ✅ COMPLETE

- [x] All 15 contracts converted to `.feature` files
- [x] Canonical template applied to all files
- [x] Legacy IDs removed (`[connection-XX]`, `[messaging-XX]`, etc.)
- [x] NORMATIVE CONTRACT MIRROR reorganized under semantic headings
- [x] DEFERRED TESTS sections added to all files
- [x] AMBIGUITIES sections present in all files

### Phase 4: Step Binding Implementation — 🔄 NEXT

1. **Start with `smoke.feature`** (2 scenarios) as vertical slice
2. Implement core step bindings in `naia/test/tests/`:
   - `Given a server is running`
   - `When a client connects`
   - `Then the server has {int} connected client(s)`
3. Iterate: `namako lint` → `namako run` → fix → repeat
4. Expand to remaining 261 scenarios across 15 contract files

### Phase 5: CI Integration — ⏳ BLOCKED

- Requires Phase 4 completion (at least smoke scenarios passing)
- Generate baseline `certification.json`
- Enable `namako verify` in CI pipeline

---

*End of CURRENT_STATUS.md*
