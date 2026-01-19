# CLAUDE.md — Quick Start for AI Agents

This file is a **15-second onramp**. All detailed information lives in the authoritative docs.

---

## Before Any Work

1. **Read `CURRENT_STATUS.md`** — Check MODE, Active FSM, Current Objective, Next Actions
2. **Read `GOLD_PLAN.md`** — Understand layer boundaries (§2.3–§2.7), normative rules
3. **Obey `SYSTEM.md`** — Hard constraints (no git ops, repo hygiene)

---

## Session Discipline

- **Run gates exactly as `CURRENT_STATUS.md` specifies** (commands live there, not here)
- **End every session** by updating `OUTPUT.md` + `CURRENT_STATUS.md`
- **No git operations** — Connor handles all commits

---

## Layer Confusion Warning

Do not confuse the two FSMs:

| FSM | When It Applies |
|-----|-----------------|
| **Bootstrap Loop** | MODE=BOOTSTRAP — building Namako/Tesaki toolchain |
| **Tesaki Product FSM** | MODE=CONSUMPTION — using the toolchain to build Naia |

The Bootstrap Loop is NOT the Tesaki Product FSM. Check `CURRENT_STATUS.md` to see which applies now.

---

## Quick Reference

| Question | Where to Look |
|----------|---------------|
| What mode am I in? | `CURRENT_STATUS.md` → Header → MODE |
| What gates should I run? | `CURRENT_STATUS.md` → Gates Snapshot |
| What's the current objective? | `CURRENT_STATUS.md` → Current Objective |
| What are the hash/schema contracts? | `GOLD_PLAN.md` → Part 7 |
| What edits are allowed? | `CURRENT_STATUS.md` → Guardrails |

---

*End of CLAUDE.md*
