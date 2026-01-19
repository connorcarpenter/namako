# CLAUDE.md — Agent Discipline for Namako/Naia Development

This file provides high-level discipline for AI agents (Claude, Opus) working on this project.

---

## 1. First Steps — Always

Before any work:

1. **Read** `_WORKSPACE/CURRENT_STATUS.md` — Check MODE, Active FSM, Current Objective
2. **Read** `_WORKSPACE/GOLD_PLAN.md` — Understand layer boundaries (§2.3–§2.6)
3. **Obey** the current MODE's allowed/forbidden edit surfaces

---

## 2. Mode Discipline

| Mode | Behavior |
|------|----------|
| `BOOTSTRAP` | Building the toolchain. Do NOT edit Naia product core. |
| `CONSUMPTION` | Using the toolchain. Product core edits allowed via Tesaki loop only. |

**Check CURRENT_STATUS.md for the current MODE before making any edits.**

---

## 3. Bootstrap Behavior (MODE=BOOTSTRAP)

When MODE=BOOTSTRAP:

- Follow the **Bootstrap Loop** (GOLD_PLAN §2.3.1), NOT the Tesaki Product FSM
- **Allowed edits:**
  - `namako/**` (CLI, Tesaki, engine)
  - `naia/test/**` (harness, tests, adapter, specs, scripts)
  - `_WORKSPACE/**` (docs)
- **Forbidden edits:**
  - `naia/client/**`, `naia/server/**`, `naia/shared/**`, `naia/adapters/**`
  - Any Naia crate outside `test/`
- **Violation handling:** Revert and record incident in `_WORKSPACE/OUTPUT.md`

---

## 4. Session Discipline

### Non-negotiables
- **No commits** — Do not commit, rebase, merge, push, pull, or switch branches
- **Minimal diffs** — Keep changes small and scoped
- **Run gates** — Verify work with gate scripts before stopping
- **End with docs** — Always update `_WORKSPACE/OUTPUT.md` and `_WORKSPACE/CURRENT_STATUS.md`

### Preferences
- Prefer minimal additions over refactors
- Prefer editing existing files over creating new ones
- Don't expand scope beyond the immediate task

---

## 5. Gate Commands

```bash
# Primary CI gate (lint + run + verify)
bash naia/test/specs/scripts/namako_ci.sh

# Determinism check
bash naia/test/specs/scripts/determinism_check.sh

# Tesaki unit tests
cargo test -p tesaki

# Build CLI
cargo build -p namako-cli
```

---

## 6. Artifact Locations

| Artifact | Path |
|----------|------|
| Status JSON | `target/namako_artifacts/tesaki/status.json` |
| Review JSON | `target/namako_artifacts/tesaki/review.json` |
| NEXT_TASK.md | `target/namako_artifacts/tesaki/NEXT_TASK.md` |
| Run Report | `target/namako_artifacts/run_report.json` |
| Resolved Plan | `target/namako_artifacts/resolved_plan.json` |
| Certification | `naia/test/specs/certification.json` |

---

## 7. Interpreting Namako Outputs

### `namako status --json`
Shows current identity vs baseline. Key fields:
- `recommended_action`: What to do next (RUN, UPDATE_CERT, DONE, etc.)
- `identity_drift`: Which hashes changed
- `last_run_failures`: Scenarios that failed (if any)

### `namako review`
Shows promotion candidates and binding bundles. Key fields:
- `promotion_candidates`: Scenarios ready for promotion
- `suggested_binding_bundle`: Steps that need bindings

### `namako explain --scenario-key <key>`
Shows scenario details for fidelity review. Includes:
- Contract context
- Step → binding resolution
- Implementation hashes

---

## 8. Quick Reference

| Question | Answer |
|----------|--------|
| What mode am I in? | Check `CURRENT_STATUS.md` → Header → MODE |
| Can I edit Naia core? | Only if MODE=CONSUMPTION |
| What should I do next? | Check `CURRENT_STATUS.md` → Next 3 Actions |
| How do I verify my work? | Run gate commands (§5) |
| Where do I document changes? | `_WORKSPACE/OUTPUT.md` |

---

*End of CLAUDE.md*
