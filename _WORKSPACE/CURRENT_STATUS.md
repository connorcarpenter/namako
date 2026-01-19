# CURRENT_STATUS.md — Bootstrap Dashboard

---

## 1. Header

| Field | Value |
|-------|-------|
| Generated | 2026-01-19 |
| Naia HEAD | `caa1c377` (modified) |
| Namako HEAD | `57e4290` (modified) |
| **MODE** | **BOOTSTRAP** |
| **Active FSM** | **Bootstrap Loop** (per GOLD_PLAN §2.3.1) |

---

## Doc SSoT Policy

See `SYSTEM.md §0` for the authoritative Single Source of Truth policy. This file (`CURRENT_STATUS.md`) is the live operational dashboard. Do not duplicate content that belongs in `GOLD_PLAN.md` or `SYSTEM.md`.

---

## 2. Gates Snapshot

### Commands

```bash
# CI gate (lint + run + verify)
bash naia/test/specs/scripts/namako_ci.sh

# Determinism check
bash naia/test/specs/scripts/determinism_check.sh

# Tesaki tests
cargo test -p tesaki

# Tesaki next (from namako/ dir; see GOLD_PLAN §9.4 for governance)
# --max-cert-updates 0 = manual only (CI default)
# --max-cert-updates 3 = autonomous updates allowed (local dev)
cargo run -p tesaki -- next \
  -s ../naia/test/specs \
  -a "cargo run --manifest-path ../naia/test/npa/Cargo.toml --" \
  --max-cert-updates 3
```

### Latest Results

| Gate | Status | Notes |
|------|--------|-------|
| `namako_ci.sh` | ✅ PASS | Lint, Run, Verify all green |
| `determinism_check.sh` | ✅ PASS | `bytes(run1) == bytes(run2)` |
| `cargo test -p tesaki` | ✅ PASS | 4 unit tests for governance |
| `cargo build -p namako-cli` | ✅ PASS | CLI compiles |

### Scenario Counts

| Metric | Count |
|--------|-------|
| Executable scenarios | **30** |
| @Deferred scenarios | **1** (CORE blocker only) |
| Promotion candidates | **1** (blocked on CORE — needs Naia core changes) |

---

## 3. Current Objective

**Determinism/ordering @Deferred scenarios unblocked. Blocker classification and mode-aware filtering implemented.**

---

## 4. Next 3 Actions

1. Verify all gates pass: `namako_ci.sh`, `determinism_check.sh`, `cargo test -p tesaki`
2. Review OUTPUT.md for session summary
3. Consider MODE=CONSUMPTION transition once CORE work is ready

---

## 5. Guardrails

### BOOTSTRAP Allowed Edit Surface
- `namako/**` (Namako CLI, Tesaki, engine crates)
- `naia/test/**` (harness, tests, adapter `naia_npa`, specs, scripts)
- `_WORKSPACE/**` (docs)

### BOOTSTRAP Forbidden Edit Surface
- `naia/client/**`
- `naia/server/**`
- `naia/shared/**`
- `naia/adapters/**`
- Any Naia crate outside `test/`

### Violation Handling
If forbidden surface is edited: **revert immediately** and record incident in `OUTPUT.md`.

---

## 6. Current Identity (Certified)

| Field | Hash |
|-------|------|
| `feature_fingerprint_hash` | `97e690fc777dbffb19b8ef6cde452bb069f13c8aa392d004434f6c9856133323` |
| `step_registry_hash` | `fade7f96927fb05a993e3c7b90009ef9db942d449e78417546e90000711d4f35` |
| `resolved_plan_hash` | `45b3a375edeee6747b28095cda7a0db41ba288f6646be413dd30bc4c86c6983b` |

---

## 7. Blocked @Deferred Scenarios (for reference)

Only 1 scenario remains @Deferred (CORE blocker):

1. **Protocol mismatch produces ProtocolMismatch rejection** — @Blocker(CORE), needs protocol versioning in Naia handshake

**Previously @Deferred scenarios now executable:**
- ~~Same-tick scope operations resolve deterministically~~ — **PROMOTED** (trace sink implemented)
- ~~Multiple commands for same tick apply in receipt order~~ — **PROMOTED** (trace sink implemented)

**Status:** 1 scenario remains @Deferred until MODE=CONSUMPTION (CORE changes required).

---

## 8. Artifacts

| Artifact | Path |
|----------|------|
| Status JSON | `target/namako_artifacts/tesaki/status.json` |
| Review JSON | `target/namako_artifacts/tesaki/review.json` |
| NEXT_TASK.md | `target/namako_artifacts/tesaki/NEXT_TASK.md` |
| Run Report | `target/namako_artifacts/run_report.json` |
| Resolved Plan | `target/namako_artifacts/resolved_plan.json` |
| Certification | `naia/test/specs/certification.json` |

---

*End of CURRENT_STATUS.md*
