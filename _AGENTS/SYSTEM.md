# Namako + Tesaki — AI Coding Agent Instructions

## Overview

This repository contains two tools for Spec-Driven Development (SDD):

1. **Namako** - BDD testing framework + CLI (measures truth)
2. **Tesaki** - AI task orchestrator (drives development)

## Quick Start

```bash
# Run tesaki autonomous loop on a target repo
cd <target-repo>  # e.g., naia
tesaki
> loop 10   # Run up to 10 missions

# Or run single commands
namako lint --adapter-cmd "<adapter>" --specs-dir <specs>
namako gate --adapter-cmd "<adapter>" --specs-dir <specs>
```

## Repository Structure

```
namako/
├── src/           # Namako core library
├── cli/           # namako-cli binary
├── codegen/       # Step macros (#[given], #[when], #[then])
├── tesaki/        # Tesaki AI orchestrator
│   └── src/
│       ├── main.rs           # CLI + REPL entrypoint
│       ├── repl.rs           # Interactive session
│       ├── chat_planner.rs   # LLM planner interface
│       ├── copilot_agent.rs  # GitHub Copilot CLI backend
│       ├── claude_code_agent.rs
│       ├── codex_agent.rs
│       ├── mission.rs        # Mission bundle management
│       ├── repo_state.rs     # Computed state from packets
│       └── issue_classifier.rs
└── _WORKSPACE/    # Documentation
```

## Namako CLI Commands

| Command | Description |
|---------|-------------|
| `namako lint` | Parse specs, resolve bindings → `resolved_plan.json` |
| `namako gate` | Full CI: lint → run → verify |
| `namako status` | JSON status packet |
| `namako review` | Work backlog packet |
| `namako explain` | Scenario traceability |
| `namako verify` | Compare current vs baseline certification |
| `namako update-cert` | Update baseline certification |

## Tesaki Commands

| Command | Description |
|---------|-------------|
| `tesaki` | Start interactive REPL |
| `tesaki run` | Execute single mission cycle |

### REPL Commands
- `loop N` - Run N missions autonomously
- `propose a mission` - Ask planner for next mission
- `run it` - Execute proposed mission
- `exit` - Quit REPL

## Configuration

Target repos need `.tesaki/config.toml`:

```toml
specs_dir = "test/specs"
adapter_cmd = "cargo run --manifest-path test/npa/Cargo.toml --"
agent = "copilot"        # primary agent for runner + planner
max_retries = 0          # Recommended: don't retry
max_cert_updates = 3
max_runtime_seconds = 600
```

## Runner Backends

| Backend | Command Pattern |
|---------|-----------------|
| `copilot` | `copilot -p @{mission}/MISSION.md --allow-all --add-dir {cwd}` |
| `claude` | `claude -p {mission}/MISSION.md --allowedTools all` |
| `codex` | `codex -q --approval-mode full-auto -f {mission}/MISSION.md` |
| `mock` | Always succeeds (for testing) |
| `cmd` | Custom command via `runner_cmd` config |

## Step Binding Pattern

```rust
use cucumber::{given, when, then};

#[given("a client connection")]
async fn client_connection(world: &mut MyWorld) {
    world.client.connect();
}

#[when("data is sent")]
async fn send_data(world: &mut MyWorld) {
    world.client.send(data);
}

#[then("the server receives it")]
async fn server_receives(world: &mut MyWorld) {
    assert!(world.server.received());
}
```

## Key Insights from Testing

1. **`max_retries = 0`** - Fresh context beats stale retries
2. **Small missions** - Focus on one task at a time
3. **Progress-based success** - Partial progress is still success
4. **Spec is truth** - Namako measures, doesn't opine

## Editing Namako/Tesaki Code

```bash
# Build
cargo build -p namako-cli -p tesaki

# Test
cargo test -p namako-cli   # 29 tests
cargo test -p tesaki       # 132 tests

# Run from source
cargo run -p tesaki -- --help
cargo run -p namako-cli -- --help
```

## Files to Read First

1. `_WORKSPACE/CURRENT_STATUS.md` - Current reality snapshot
2. `_WORKSPACE/TODO.md` - Step-by-step implementation plan
3. `_WORKSPACE/GOLD_PLAN.md` - Canonical architecture + constraints
4. `tesaki/src/main.rs` - CLI entrypoint
5. `tesaki/src/repl.rs` - Interactive session logic
