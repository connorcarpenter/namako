# TODO - Namako/Tesaki

## Completed (2026-02-02)

- [x] Tesaki v1.8 REPL with autonomous loop
- [x] Copilot CLI support (planner + runner)
- [x] Naia bindings: 35 → 0 missing
- [x] Clean build (0 warnings, 132 tests)
- [x] Documentation updated
- [x] **TURNKEY WORKFLOW** - Gate auto-cert + persistent artifacts
  - `namako gate --auto-cert` flag for self-healing on baseline drift
  - `namako gate --artifacts-dir` option (defaults to specs_dir)
  - Tesaki calls gate with `--auto-cert` during startup
  - No manual intervention needed for stale baselines
- [x] **HEADLESS MODE** - `tesaki --loop N` runs without REPL
- [x] **ALGORITHMIC SELECTION** - No LLM for task selection, deterministic
- [x] **PROGRESS TRACKING** - Before/after issue counts, continues on progress
- [x] **BATCHED BINDINGS** - All missing steps in one mission context

## Future Improvements (Nice-to-Have)

- [ ] Model selection in config.toml
- [ ] SUT implementation loop (tests exist, need impl)
