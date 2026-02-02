# CLAUDE.md — Quick Start for AI Agents

This file is a **15-second onramp**. All detailed information lives in the authoritative docs.

---

## The One Command You Need

```bash
cd naia
tesaki --loop 10   # Run 10 autonomous missions
```

That's it. The system will:
1. Select the next task algorithmically (no LLM for decisions)
2. Execute it via the configured runner (Copilot/Claude)
3. Check if progress was made
4. Continue until done or stalled

---

## Before Any Work

1. **Read `CURRENT_STATUS.md`** — Check MODE, Active FSM, Current Objective
2. **Read `GOLD_PLAN.md`** — Understand layer boundaries (§2.3–§2.7)
3. **Obey `SYSTEM.md`** — Hard constraints (no git ops, repo hygiene)

---

## Quick Commands

| Task | Command |
|------|---------|
| Run autonomous loop | `tesaki --loop 10` |
| Interactive REPL | `tesaki` (then `loop 10`) |
| Check gates | `namako gate --adapter-cmd "..." --specs-dir test/specs` |
| See status | `namako status --adapter-cmd "..." --json` |

---

## Session Discipline

- **Run gates exactly as `CURRENT_STATUS.md` specifies**
- **End every session** by updating `OUTPUT.md` + `CURRENT_STATUS.md`
- **No git operations** — Connor handles all commits

---

## Layer Confusion Warning

Do not confuse the two FSMs:

| FSM | When It Applies |
|-----|-----------------|
| **Bootstrap Loop** | MODE=BOOTSTRAP — building Namako/Tesaki toolchain |
| **Tesaki Product FSM** | MODE=CONSUMPTION — using the toolchain to build Naia |

Check `CURRENT_STATUS.md` to see which applies now.

---

## Quick Reference

| Question | Where to Look |
|----------|---------------|
| What mode am I in? | `CURRENT_STATUS.md` → Header → MODE |
| What gates should I run? | `CURRENT_STATUS.md` → Gates Snapshot |
| What's the current objective? | `CURRENT_STATUS.md` → Current Objective |
| What are the hash/schema contracts? | `GOLD_PLAN.md` → Part 7 |

---

*End of CLAUDE.md*
