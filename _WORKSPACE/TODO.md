# TODO - Namako/Tesaki

## Current Focus: v1.9 Implementation

**See `IMPL_PLAN_v_1_9.md` for the full plan.**

### Sprint 1: Token Feedback (Foundation)
- [ ] Parse token usage from runner stderr
- [ ] Display token usage after each mission
- [ ] Session end summary with token breakdown

### Sprint 2: Model Tiering
- [ ] Add `recommended_model()` to MissionType
- [ ] Implement model escalation on failure
- [ ] Config override for model preferences
- [ ] Wire model selection into runner invocation

### Sprint 3: Quality Context
- [ ] Smart context injection (similar bindings, specific errors)
- [ ] Revise templates for quality over quantity
- [ ] A/B test token usage before/after

### Sprint 4: System Reliability
- [ ] Pre-gate compilation check
- [ ] Structured error parsing
- [ ] Graceful failure handling
- [ ] State recovery on startup

### Sprint 5: Momentum & Polish
- [ ] Smart stall detection (error signature tracking)
- [ ] Mission type skip on repeated failure
- [ ] Mission history command
- [ ] Debug mode and explain-failure command

---

## Completed (v1.8)

- [x] Tesaki v1.8 REPL with autonomous loop
- [x] Copilot CLI support (planner + runner)
- [x] Naia bindings: 35 → 0 missing
- [x] Clean build (0 warnings, 132 tests)
- [x] Headless mode (`tesaki --loop N`)
- [x] Algorithmic task selection (no LLM)
- [x] Progress tracking with deltas
- [x] Batched bindings in mission context
- [x] Mission-specific success messages
