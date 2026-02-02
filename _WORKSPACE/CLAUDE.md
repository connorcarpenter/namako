# CLAUDE.md — Agent Quick Start

**This file is self-contained. You do NOT need to read other docs to start working.**

---

## Run the Autonomous Loop

```bash
cd naia
tesaki --loop 10
```

The system will:
1. Select tasks algorithmically (no LLM for decisions)
2. Execute via runner (Copilot/Claude/Codex)
3. Track progress (before/after issue counts)
4. Continue until done or stalled (3× no-progress)

---

## Hard Constraints

1. **NO git operations** — Connor handles all commits
2. **Edit both repos freely** — `naia/` and `namako/` are both in scope
3. **Use repo-prefixed paths** — always `naia/path` or `namako/path`, never ambiguous

---

## When You're Done

Update `OUTPUT.md` with what you accomplished.

---

## If You Need More Context

| Need | File |
|------|------|
| Current mode, gates, paths | `CURRENT_STATUS.md` |
| Forbidden actions | `SYSTEM.md` |
| Full system spec (reference) | `GOLD_PLAN.md` |
| DX notes / previous findings | `DX_TEST_LOG.md` |

**But for 90% of sessions, you don't need these. Just run the command above.**

---

## What the System Does

Namako/Tesaki is a **spec-driven development loop**:
- `.feature` files are the source of truth
- `namako` computes what work remains (missing bindings, failing tests)
- `tesaki` dispatches one mission at a time to a coding agent
- Gates verify progress after each mission

You are the coding agent. Tesaki will tell you exactly what to do.

---

*End of CLAUDE.md*
