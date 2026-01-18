# CURRENT_STATUS.md — GOLD_PLAN v1 Dashboard

Generated: 2026-01-17
Based on: Empirical inspection

---

## 1. Snapshot Facts

| Item | Value |
|------|-------|
| Naia HEAD | `acf566c7a3c05d8ff96093a9d7936a7aa248db26` |
| Namako HEAD | `301b303981977833be9b569f925b157efeb7a30c` |
| Naia working tree | Modified details in `test/specs/` |
| Feature Files | 16 files (renamed to match spec convention) |

### Namako Integration Layout (`naia/test/`)
```
naia/test/
├── harness/              # naia_test_harness lib
├── npap/                 # naia_npap adapter binary
├── specs/
│   ├── contracts/        # 15 legacy spec.md files
│   ├── features/         # 16 matching feature files
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
- **Parse Status**: ✅ Gherkin syntax valid (All files read successfully)
- **Binding Status**: ❌ Missing bindings (~900 steps undefined)

---

## 3. Contract Conversion Progress

### Summary
| Metric | Count |
|--------|-------|
| Total contracts | 15 |
| Feature files | 15 (plus smoke) |
| Conversion Status | **100% Files Created** |

### Conversion Table

| Contract | Feature File | Status | Notes |
|----------|--------------|--------|-------|
| 00_common | `00_common.feature` | ✅ Created | Structure verified |
| 01_connection_lifecycle | `01_connection_lifecycle.feature` | ✅ Created | Structure verified |
| 02_transport | `02_transport.feature` | ✅ Created | Structure verified |
| 03_messaging | `03_messaging.feature` | ✅ Created | Structure verified |
| 04_time_ticks_commands | `04_time_ticks_commands.feature` | ✅ Created | Structure verified |
| 05_observability_metrics | `05_observability_metrics.feature` | ✅ Created | Structure verified |
| 06_entity_scopes | `06_entity_scopes.feature` | ✅ Created | Structure verified |
| 07_entity_replication | `07_entity_replication.feature` | ✅ Created | Structure verified |
| 08_entity_ownership | `08_entity_ownership.feature` | ✅ Created | Structure verified |
| 09_entity_publication | `09_entity_publication.feature` | ✅ Created | Structure verified |
| 10_entity_delegation | `10_entity_delegation.feature` | ✅ Created | Structure verified |
| 11_entity_authority | `11_entity_authority.feature` | ✅ Created | Structure verified |
| 12_server_events_api | `12_server_events_api.feature` | ✅ Created | Structure verified |
| 13_client_events_api | `13_client_events_api.feature` | ✅ Created | Structure verified |
| 14_world_integration | `14_world_integration.feature` | ✅ Created | Structure verified |

---

## 4. Known Failures / Instability

### Current Failure: Missing Step Bindings

**Failing Command:** `namako lint`

**Cause:**
Feature files define Gherkin steps (Given/When/Then) that do not yet have corresponding Rust implementation in the `naia_npap` adapter.

**Example Error:**
```
Missing step: no binding found for Given "a Naia client test environment is initialized"
```

---

## 5. Next Steps

1.  **Phase 3 Completion**:
    -   Address the filename typo: `01_entity_llifecycle.feature` -> `01_connection_lifecycle.feature` (User requested NO moves/renames, so leaving as is for now).
    -   Final review of Ambiguities sections (Completed: None found).
2.  **Phase 4: Implementation (Step Bindings)**:
    -   Implement Rust step definitions in `naia/test/tests/`.
    -   Map Gherkin steps to Naia test harness calls.
    -   Iteratively run `namako lint` -> `namako run` to verify implementation.

---

*End of CURRENT_STATUS.md*
