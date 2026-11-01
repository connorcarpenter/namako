# AI Coding Agent Guide — Namako

**Last Updated:** 2026-05-07
**Status:** Stable. Tesaki retired Nov 1, 2026 (see README "Removed / not wanted").

---

## What This Codebase Is

**Namako** is a Spec-Driven Development (SDD) testing framework:

| Component | Role |
|-----------|------|
| **Namako** | BDD testing framework + CLI. Parses `.feature` files, runs tests, verifies baselines. |


## Repository Layout

```
namako/
├── src/                    # Namako core library (parsing, execution)
├── cli/                    # namako_cli binary
├── codegen/                # Step macros (#[given], #[when], #[then])
├── engine/                 # namako_engine library
├── _AGENTS/                # This directory (AI agent docs)
└── _WORKSPACE/             # Operational docs
    ├── RUNBOOK.md          # Operational runbook
    └── ARCHIVE/            # Historical docs
```

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


## Don't

- Don't modify code without reading it first
- Don't add features beyond what's requested
- Don't create new files unless necessary
- Don't push to remote without explicit request

---

## Getting Help

- Run `cargo test` to verify changes
- Historical design context: `_WORKSPACE/ARCHIVE/GOLD_PLAN.md`
