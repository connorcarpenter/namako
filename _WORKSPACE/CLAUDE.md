# CLAUDE.md — Agent Quick Start

**This file is self-contained. Read IMPL_PLAN_v_1_9.md for current work.**

---

## Current State (2026-02-02)

- **Mode:** CONSUMPTION (toolchain complete, now improving it)
- **Tesaki:** v1.8 working, autonomous loop functional
- **Next:** Implement v1.9 improvements per `IMPL_PLAN_v_1_9.md`

---

## Quick Commands

```bash
# Run autonomous spec-driven development
cd naia && tesaki --loop 10

# Build and test Tesaki
cd namako && cargo build -p tesaki && cargo test -p tesaki

# Run Namako gate manually
cd naia && namako gate -s test/specs -a "cargo run --manifest-path test/npa/Cargo.toml --"
```

---

## Hard Constraints

1. **NO git operations** — Connor handles all commits
2. **Edit both repos freely** — `naia/` and `namako/` are both in scope
3. **Use repo-prefixed paths** — always `naia/path` or `namako/path`

---

## Key Files for v1.9 Implementation

| File | Purpose |
|------|---------|
| `IMPL_PLAN_v_1_9.md` | **THE PLAN** — 5 sprints, 18 work items |
| `tesaki/src/mission_type.rs` | Add `recommended_model()` |
| `tesaki/src/base_runner.rs` | Parse token usage from stderr |
| `tesaki/src/repl.rs` | Display token stats, session summary |
| `tesaki/src/config.rs` | Model override config |

---

## What the System Does

**Namako** = deterministic truth (parses specs, runs gates, produces packets)
**Tesaki** = orchestrator (selects missions, invokes runners, tracks progress)
**Runner** = coding agent (Copilot/Claude/Codex executes missions)

The loop: `compute state → select mission → execute → validate → repeat`

---

*End of CLAUDE.md*
