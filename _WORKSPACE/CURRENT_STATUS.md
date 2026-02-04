# CURRENT_STATUS.md — Namako/Tesaki Tooling Status

**Last Updated:** 2026-02-03
**Status:** Stable — All implementation gaps closed
**Mode:** CONSUMPTION (tool is ready for use)

---

## TL;DR

The Tesaki v1.9 implementation is complete. All gaps from the implementation plan have been closed:

- Surface lock enforcement triggers automatic rollback on violations
- Draft/Promote mission types are now selectable by the mission selector
- `tesaki diagnose <mission_id>` command works
- `quality_gates_enabled` config flag is wired through
- Mission briefs include selection evidence

## For AI Agents: Start Here

**Single entrypoint:** [`../_AGENTS/AGENT_GUIDE.md`](../_AGENTS/AGENT_GUIDE.md)

That file contains everything needed to understand and work on this codebase.

## Quick Validation

```bash
cd namako/
cargo test -p tesaki    # 222 tests, all passing
```

## Documentation Map

| Document | Purpose |
|----------|---------|
| `_AGENTS/AGENT_GUIDE.md` | **Complete guide for AI coding agents** |
| `_WORKSPACE/RUNBOOK.md` | Turnkey loop execution checklist |
| `_WORKSPACE/ARCHIVE/` | Historical docs (GOLD_PLAN, old plans) |

## Recent Completions (2026-02-03)

| Feature | Status |
|---------|--------|
| Surface policy violation → rollback | ✅ Complete |
| `DraftSpecScenarios` mission type | ✅ Selectable |
| `PromoteScenariosToExecutable` mission type | ✅ Selectable |
| `tesaki diagnose` CLI command | ✅ Working |
| `quality_gates_enabled` config flag | ✅ Wired through |
| Selection evidence in MISSION.md | ✅ Rendering |

## Known Quirks

- Codegen example features include one intentional failing scenario; tests still pass
- Expected cascade after AddOrClarifyScenario is labeled EXPECTED_CASCADE (not STOP)
