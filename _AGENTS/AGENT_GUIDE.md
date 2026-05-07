# AI Coding Agent Guide — Namako

**Last Updated:** 2026-05-07
**Status:** Stable. Tesaki extracted to standalone repo.

---

## STOP — Read This First

**If you were told to "continue with development" on a target repo (like `naia`):**

```bash
cd /path/to/target-repo
tesaki --loop 10
```

**That's your entire job. Run that command. Nothing else.**

- Do NOT manually investigate issues
- Do NOT fix lint errors yourself
- Do NOT explore the codebase
- Do NOT read other documentation
- Do NOT run `namako` commands directly

**"Continue with development" = Run `tesaki --loop 10`. Period.**

Tesaki handles everything: detecting issues, selecting missions, executing fixes, validating results.

---

**Only continue reading below if you are working ON the namako toolchain itself (not a target repo).**

Note: Tesaki (the AI orchestrator) now lives in its own standalone repository. If you're working on Tesaki, go there instead.

---

## What This Codebase Is

**Namako** is a Spec-Driven Development (SDD) testing framework:

| Component | Role |
|-----------|------|
| **Namako** | BDD testing framework + CLI. Parses `.feature` files, runs tests, verifies baselines. |
| **Tesaki** | AI task orchestrator (standalone repo — drives autonomous development loops). |

The tools work together in a loop:
```
select mission → execute (via AI runner) → validate (via namako gate) → repeat
```

---

## Repository Layout

```
namako/
├── src/                    # Namako core library (parsing, execution)
├── cli/                    # namako_cli binary
├── codegen/                # Step macros (#[given], #[when], #[then])
├── engine/                 # namako_engine library
├── _AGENTS/                # This directory (AI agent docs)
└── _WORKSPACE/             # Operational docs
    ├── RUNBOOK.md          # How to run the loop
    └── ARCHIVE/            # Historical docs
```

---

## Key Concepts

### Mission Types
Tesaki (standalone) selects from these mission types based on evidence:

| Mission Type | When Selected | What It Does |
|--------------|---------------|--------------|
| `FixRegressionFromGateFailure` | `sut_issues` > 0 | Fix failing tests |
| `CreateMissingBindings` | `binding_issues` with scenario_key | Add step bindings |
| `NormalizeIdentityTags` | `structure_issues` with MissingIdentityTag | Add @Feature/@Rule/@Scenario tags |
| `DraftSpecScenarios` | Rule has 0 scenarios, no deferred | Draft new scenarios |
| `PromoteScenariosToExecutable` | Deferred scenarios exist | Promote @Deferred to executable |
| `AddOrClarifyScenario` | Partial coverage | Add scenarios to existing rules |
| `AssessSpecCoverage` | Only ambiguous issues | LLM judges coverage adequacy |

### Surface Policy
Each mission declares which surfaces can be edited:
- **Spec**: `.feature` files
- **Tests/Bindings**: Test harness code
- **SUT**: Implementation code

Surface violations trigger automatic rollback.

### Stop Reasons
| Reason | Meaning |
|--------|---------|
| `DONE` | All gates pass, no issues |
| `NO_PROGRESS` | No changes made |
| `GATE_FAILED` | Lint/run/verify failed |
| `POLICY_VIOLATION` | Edited locked surface |
| `BUDGET` | Limits exceeded |

---

## How to Work on This Codebase

### Build & Test
```bash
cd /path/to/namako

# Build
cargo build -p namako_cli

# Test
cargo test

# Run from source
cargo run -p namako_cli -- --help
```

### Configuration
Target repos need `.tesaki/config.toml`:
```toml
specs_dir = "test/specs"
adapter_cmd = "cargo run --manifest-path test/npa/Cargo.toml --"
agent = "copilot"
max_retries = 0
max_cert_updates = 3
quality_gates_enabled = true  # Spec quality checks
```

### CLI Commands
```bash
# Namako
namako lint --adapter-cmd "..." --specs-dir "..."
namako gate --adapter-cmd "..." --specs-dir "..."
namako status --json
namako review --json
```

---

## Code Patterns

### Step Bindings (Test Harness)
```rust
use cucumber::{given, when, then};

#[given("a client connection")]
async fn client_connection(world: &mut MyWorld) {
    world.client = Client::connect().await;
}

#[when("data is sent")]
async fn send_data(world: &mut MyWorld) {
    world.client.send(b"hello").await;
}

#[then("the server receives it")]
async fn server_receives(world: &mut MyWorld) {
    assert!(world.server.received());
}
```

---

## Quality Guardrails

### Surface Policy Enforcement
If a runner edits files outside its allowed surface patterns:
1. Changes are rolled back (`git checkout -- .`)
2. Mission marked as `POLICY_VIOLATION`
3. Session stops

---

## Don't

- Don't modify code without reading it first
- Don't add features beyond what's requested
- Don't create new files unless necessary
- Don't retry failed missions in the same session (fresh context is better)
- Don't push to remote without explicit request

---

## Getting Help

- Run `cargo test` to verify changes
- Historical design context: `_WORKSPACE/ARCHIVE/GOLD_PLAN.md`
