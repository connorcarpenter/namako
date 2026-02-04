# AI Coding Agent Guide — Namako/Tesaki

**Last Updated:** 2026-02-03
**Status:** Stable, all implementation gaps closed

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

**Only continue reading below if you are working ON the namako/tesaki toolchain itself (not a target repo).**

---

## What This Codebase Is

**Namako + Tesaki** is a Spec-Driven Development (SDD) toolchain:

| Component | Role |
|-----------|------|
| **Namako** | BDD testing framework + CLI. Parses `.feature` files, runs tests, verifies baselines. |
| **Tesaki** | AI task orchestrator. Selects missions, invokes AI agents, validates results. |

The tools work together in a loop:
```
select mission → execute (via AI runner) → validate (via namako gate) → repeat
```

---

## Repository Layout

```
namako/
├── src/                    # Namako core library (parsing, execution)
├── cli/                    # namako-cli binary
├── codegen/                # Step macros (#[given], #[when], #[then])
├── tesaki/                 # Tesaki AI orchestrator
│   ├── src/
│   │   ├── main.rs         # CLI + REPL entrypoint
│   │   ├── repl.rs         # Interactive session + autonomous loop
│   │   ├── mission_selector.rs  # Algorithmic mission selection
│   │   ├── mission_type.rs      # Mission enum + briefs
│   │   ├── repo_state.rs        # Computed state from packets
│   │   ├── base_runner.rs       # Runner abstraction + surface checks
│   │   ├── config.rs            # Config discovery + parsing
│   │   └── prompts.rs           # Template rendering
│   └── prompts/            # Jinja2 mission templates
│       └── mission/MISSION.md.j2
├── _AGENTS/                # This directory (AI agent docs)
└── _WORKSPACE/             # Operational docs
    ├── RUNBOOK.md          # How to run the loop
    └── ARCHIVE/            # Historical docs (GOLD_PLAN, etc.)
```

---

## Key Concepts

### Mission Types
Tesaki selects from these mission types based on evidence:

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
cargo build -p tesaki -p namako-cli

# Test (222 tests)
cargo test -p tesaki

# Run from source
cargo run -p tesaki -- --help
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
# Tesaki
tesaki              # Interactive REPL
tesaki --loop 10    # 10 autonomous missions
tesaki diagnose M-xxx  # Debug specific mission

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

### Adding a New Mission Type
1. Add variant to `MissionType` enum in `tesaki/src/mission_type.rs`
2. Implement `name()`, `target_label()`, `default_surface_policy()`, `generate_brief()`
3. Update `mission_selector.rs` to select it based on evidence
4. Add test in the corresponding test module

### Adding Configuration Options
1. Add field to `Config` struct in `tesaki/src/config.rs`
2. Add to `ResolvedConfig` struct
3. Pass through in `resolve_config()`
4. Wire into usage site (e.g., `repl.rs`)
5. Add test for parsing

---

## Quality Guardrails

### Spec Quality Gate
After `AddOrClarifyScenario`, these checks run:

| Rule | Blocks |
|------|--------|
| `NO_PLACEHOLDER_STEPS` | Generic steps like "Given a test scenario" |
| `DOMAIN_NOUN_REQUIRED` | Scenarios unrelated to parent Rule |
| `NO_ORPHAN_STUBS` | Stub markers outside `_orphan_stubs.feature` |

Violations trigger rollback and `NO_PROGRESS`.

### Surface Policy Enforcement
If a runner edits files outside its allowed surface patterns:
1. Changes are rolled back (`git checkout -- .`)
2. Mission marked as `POLICY_VIOLATION`
3. Session stops

---

## Important Files to Know

| File | What It Does |
|------|--------------|
| `tesaki/src/main.rs` | CLI parsing, `run_run()` loop |
| `tesaki/src/repl.rs` | REPL commands, `run_autonomous_loop()` |
| `tesaki/src/mission_selector.rs` | `select_mission_type()` algorithm |
| `tesaki/src/mission_type.rs` | Mission enum, briefs, policies |
| `tesaki/src/repo_state.rs` | `RepoState::compute()` from packets |
| `tesaki/src/base_runner.rs` | `check_surface_violations()` |
| `tesaki/src/config.rs` | Config discovery and parsing |
| `tesaki/src/spec_quality.rs` | Spec quality rules |
| `tesaki/prompts/mission/MISSION.md.j2` | Mission brief template |

---

## Recent Changes (2026-02-03)

All implementation gaps from IMPL_PLAN.md are now closed:

1. **Surface lock enforcement** — Violations trigger rollback + `POLICY_VIOLATION`
2. **Draft/Promote missions** — `DraftSpecScenarios` and `PromoteScenariosToExecutable` now selectable
3. **`tesaki diagnose` command** — Debug missions by ID
4. **`quality_gates_enabled` config** — Toggle spec quality checks
5. **Selection evidence in briefs** — Mission briefs show why they were selected

---

## Don't

- Don't modify code without reading it first
- Don't add features beyond what's requested
- Don't create new files unless necessary
- Don't retry failed missions in the same session (fresh context is better)
- Don't push to remote without explicit request

---

## Getting Help

- Run `cargo test -p tesaki` to verify changes
- Check `tesaki/src/*/tests` modules for patterns
- Historical design context: `_WORKSPACE/ARCHIVE/GOLD_PLAN.md`

---

## Autonomous Flywheel Features (v2.0)

**New in v2.0:** Tesaki now has self-improving autonomous capabilities that learn from failures and provide actionable guidance when stuck.

### 1. Constraint-First Prompt Architecture

**What it does:** Surfaces constraints are now displayed FIRST in every mission prompt, before the objective, making policy violations harder to trigger.

**How it works:**
- Every MISSION.md starts with a prominent `⚠️ CRITICAL CONSTRAINTS` block
- Clear ✅ ALLOWED vs ❌ FORBIDDEN file lists
- Explicit STOP directive if locked files are needed

**For agents:** You'll see constraints at the top of every mission. Read them carefully before planning changes.

### 2. Failure Memory

**What it does:** When a mission fails due to policy violation, the NEXT mission sees what went wrong and what NOT to try again.

**How it works:**
- Surface violations are captured with violated files and surface names
- Next mission gets a `⚠️ Previous Mission Failed` section
- Explicitly lists which files are LOCKED and caused the failure
- Provides guidance on alternative approaches

**For agents:** If you see a "Previous Mission Failed" section, pay close attention. It tells you exactly what NOT to do.

### 3. Persistent Lessons Database

**What it does:** Learns from failures across SESSIONS. If an issue has been attempted before (even weeks ago), you'll see what was tried and what blocked progress.

**How it works:**
- Lessons stored in `.tesaki/lessons.json`
- Each lesson tracks: failure mode, approaches tried, what blocked progress
- Lessons are injected into mission context if targeting the same issue
- Marked as resolved when issue is finally fixed

**For agents:** You may see a `📚 Previous Attempts on This Issue` section. These are from PAST sessions, not just the current one. Avoid repeating these approaches.

### 4. Intelligent Escalation

**What it does:** When the loop stalls, provides clear options instead of just stopping.

**How it works:**
- Detects escalation types: policy blocking, repeated failure, no progress
- Generates actionable options: unlock surface, skip issue, provide hint
- Displays human-readable message with numbered choices

**For agents:** N/A - this is for human operators when the loop gets stuck.

### 5. Cost Tracking & Efficiency Alerts

**What it does:** Tracks estimated cost in USD and calculates efficiency metrics (cost per issue resolved).

**How it works:**
- Estimates cost based on token usage and model pricing
- Calculates cost per issue resolved
- Rates efficiency: Excellent (<$5), Good ($5-15), Poor ($15-30), Critical (>$30)
- Displays warnings if efficiency is poor

**For agents:** N/A - this is session-level reporting visible in summaries.

### 6. Stall Diagnosis Reports

**What it does:** When stopping, explains exactly WHY and WHAT to try next.

**How it works:**
- Analyzes session state, failure history, and stop reason
- Generates detailed report: "What Happened", "Why It Stalled", "What To Try"
- Saves to `.tesaki/last_stall_diagnosis.md`

**For agents:** If you're asked to resume after a stall, check `.tesaki/last_stall_diagnosis.md` for context.

### Configuration

All flywheel features can be configured in `.tesaki/config.toml`:

```toml
# Enable/disable features
enable_failure_memory = true       # Default: true
enable_lessons = true               # Default: true
enable_cost_tracking = true         # Default: true

# Tuning parameters
cost_alert_threshold_usd = 20.0    # Default: 20.0
max_consecutive_failures = 2       # Default: 2
```

---

## For Human Operators: Responding to Escalations

When Tesaki escalates (e.g., "Surface policy blocking progress"), you have options:

1. **Unlock a surface:** If the fix genuinely requires editing locked files
   - Example: `tesaki --unlock-spec` or modify `config.toml`

2. **Skip the issue:** If it's a known-hard problem
   - Add to skip list or mark as requiring manual intervention

3. **Provide a hint:** If the agent is missing context
   - Add notes to `.tesaki/hints.md` or session context

4. **Review lessons:** Check `.tesaki/lessons.json` to see what's been tried before

**Pro tip:** Most escalations are due to surface policy being too strict. If the loop stalls repeatedly on the same issue with policy violations, consider unlocking the necessary surface temporarily.
