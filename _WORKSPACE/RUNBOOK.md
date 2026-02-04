# RUNBOOK.md — Turnkey Loop Checklist

**Last Updated:** 2026-02-04
**Scope:** Tesaki + Namako toolchain usage (spec repo only)

---

## Turnkey Loop Checklist

### 1) Config (Required)
Create `.tesaki/config.toml` in your target repo:

```toml
specs_dir = "test/specs"
adapter_cmd = "cargo run --manifest-path test/npa/Cargo.toml --"

# Optional
agent = "copilot"          # runner + planner
max_retries = 0
max_cert_updates = 3
max_runtime_seconds = 600
max_files_changed = 10

# Pre-gate build (optional)
# pre_gate_build = "cargo check -p my-test-harness"
# pre_gate_build_mode = "auto"   # auto | always | never
```

### 2) One-Command Loop
From the repo root:

```bash
tesaki --loop 10
```

Or interactive:

```bash
tesaki
> loop 10
```

### 3) Expected Cascade (Normal)
If you add scenarios, lint can fail due to missing bindings. This is expected.
Tesaki will report:

```
Expected cascade: missing bindings created.
```

The next mission should be `CreateMissingBindings`.

### 4) Stop Criteria (When It’s Over)
Tesaki will stop when:
- `DONE`: All gates pass and no issues remain
- `NO_PROGRESS`: Runner made no changes or evidence didn’t improve
- `GATE_FAILED`: Lint/run/verify failed after retries
- `HUMAN_REQUIRED`: Workspace dirty or manual approval needed
- `BUDGET`: Runtime or attempt limits hit

### 5) Pre-Gate Build Behavior
- **auto**: skipped for spec-only missions (Tests/SUT locked)
- **always**: run build check every mission
- **never**: skip build check entirely

### 6) Validate This Repo (Tooling)
From `namako/`:

```bash
cargo test
```

Or target a package:

```bash
cargo test -p tesaki
cargo test -p namako-codegen
```

---

*This runbook reflects current tool behavior. Update alongside tool changes.*
