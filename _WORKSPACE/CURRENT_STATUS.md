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

## 2. Gates Snapshot

### Commands

```bash
# CI gate (lint + run + verify)
bash naia/test/specs/scripts/namako_ci.sh

# Determinism check
bash naia/test/specs/scripts/determinism_check.sh

# Tesaki tests
cargo test -p tesaki

# Tesaki next (example, with --max-cert-updates for autonomous baseline updates)
cargo run -p tesaki -- next \
  -s ../naia \
  -a "cargo run --manifest-path ../naia/test/npa/Cargo.toml --" \
  --max-cert-updates 3
```

### Latest Results

| Gate | Status | Notes |
|------|--------|-------|
| `namako_ci.sh` | ✅ PASS | Lint, Run, Verify all green |
| `determinism_check.sh` | ✅ PASS | `bytes(run1) == bytes(run2)` |
| `cargo test -p tesaki` | ✅ PASS | 5 unit tests for token behavior |
| `cargo build -p namako-cli` | ✅ PASS | CLI compiles |

### Scenario Counts

| Metric | Count |
|--------|-------|
| Executable scenarios | **28** |
| @Deferred scenarios | **3** |
| Promotion candidates | **3** (blocked on Naia core gaps) |

---

## 3. Current Objective

**Harden toolchain governance to prevent Naia-core drift during bootstrap.**

---

## 4. Next 3 Actions

1. ✅ Finish doc hardening (GOLD_PLAN + CURRENT_STATUS + CLAUDE.md)
2. Simplify Tesaki update-cert governance (`--max-cert-updates` flag)
3. Run validation gates and update OUTPUT.md

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
| `feature_fingerprint_hash` | `eb508b39800dd89c2c9b28a6473cebbf09a7e2640b87adab6087f30f6c13bc1d` |
| `step_registry_hash` | `7d1522b771ced917aa3b70513131b382d4954710b89757ed54c59b7a4b310d33` |
| `resolved_plan_hash` | `396479fc690b89ba0bbfcf8df57120f020f1e012b248ce63972e5ad932069ba1` |

---

## 7. Blocked @Deferred Scenarios (for reference)

These 3 scenarios require Naia core/harness changes before promotion:

1. **Protocol mismatch produces ProtocolMismatch rejection** — needs protocol versioning in handshake
2. **Same-tick scope operations resolve deterministically** — needs trace sink in harness
3. **Multiple commands for same tick apply in receipt order** — needs trace sink in harness

**Status:** Remain @Deferred until MODE=CONSUMPTION.

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
