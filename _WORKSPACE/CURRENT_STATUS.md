# CURRENT_STATUS.md — Namako/Tesaki Tooling Status

**Last Updated:** 2026-02-04
**Mode:** CONSUMPTION
**Scope:** Namako/Tesaki toolchain only (not Naia product development)

---

## TL;DR

- Tool quality roadmap in `namako/_WORKSPACE/TODO.md` is complete.
- Expected cascades are handled as success (EXPECTED_CASCADE) after AddOrClarifyScenario.
- Pre-gate build is skipped for Spec-only missions (configurable).
- Policy violations are reported (no hard enforcement).
- AddOrClarifyScenario budgets to exactly one executable scenario by default.

## Start Here (New Agent)

1) Read `namako/_WORKSPACE/RUNBOOK.md` for the turnkey loop checklist.
2) Skim `namako/_WORKSPACE/TODO.md` to see what’s done and what remains in scope.
3) Optional deep spec: `namako/_WORKSPACE/GOLD_PLAN.md` (historical system design).
4) Historical artifacts: `namako/_WORKSPACE/ARCHIVE/`.

## How to Validate This Repo

From `namako/`:

```bash
cargo test
```

Targeted alternatives:

```bash
cargo test -p tesaki
cargo test -p namako-codegen
```

## Notes / Known Quirks

- Codegen example features include one intentional failing scenario; tests still pass.
- Expected cascade after AddOrClarifyScenario is labeled EXPECTED_CASCADE (not STOP).

## Doc Cleanup Recommendations

- Keep `RUNBOOK.md` and `TODO.md` for day-to-day work.
- If you want a lean workspace, you can delete `ARCHIVE/` and/or `GOLD_PLAN.md` once you’re comfortable; they are useful for historical context but not required for routine work.
