# Namako

**Spec-Driven Development testing framework for Rust projects**

| Tool | Purpose |
|------|---------|
| **Namako** | BDD testing framework + CLI (measures truth) |
| **Tesaki** | AI task orchestrator — see [standalone repo](https://github.com/connorcarpenter/tesaki) |

---

## AI Coding Agent? Start Here

**Single entrypoint:** [`_AGENTS/AGENT_GUIDE.md`](./_AGENTS/AGENT_GUIDE.md)

This file contains everything you need to understand and work on this codebase efficiently.

---

## Quick Start (Human Users)

```bash
# Install namako CLI (from namako/)
cargo install --path cli --force

# Install Tesaki (from standalone tesaki repo)
cargo install --git https://github.com/connorcarpenter/tesaki.git --force

# Configure a target repo
cd <target-repo>
mkdir -p .tesaki
cat > .tesaki/config.toml << 'EOF'
specs_dir = "test/specs"
adapter_cmd = "cargo run --manifest-path test/npa/Cargo.toml --"
agent = "copilot"
max_retries = 0
max_cert_updates = 3
EOF

# Run autonomous loop
tesaki --loop 10
```

## Repository Structure

```
namako/
├── engine/           # namako_engine core library
├── cli/              # namako_cli binary
├── benches/          # Benchmarks
├── skill-tool/       # Skill tooling
├── codegen/          # Step macros (#[given], #[when], #[then])
├── _AGENTS/          # AI agent documentation (START HERE)
└── _WORKSPACE/       # Operational docs & archives
```

## Commands

### Namako CLI
```bash
namako lint    # Parse specs, resolve bindings
namako gate    # Full CI: lint → run → verify
namako status  # JSON status packet
namako review  # Work backlog packet
namako update  # Reinstall binary when behind origin HEAD
namako verify  # Check a run report against sources and baseline
namako update-cert  # Refresh the certification baseline
namako explain # Scenario fidelity packet
namako stub    # Placeholder scenarios for orphan bindings
```

### Tesaki CLI
```bash
tesaki              # Interactive REPL
tesaki --loop N     # Run N missions autonomously
tesaki diagnose ID  # Debug a specific mission
```

## Testing

```bash
cargo test              # All tests
cargo test -p namako_cli
```

## Documentation Map

| File | Purpose |
|------|---------|
| `_AGENTS/AGENT_GUIDE.md` | **Single entrypoint for AI agents** |
| `AGENTS.md` | Seat entrypoint and spec line |
| `docs/EVIDENCE.md`, `docs/IMPACT.md` | Behavior specs |

Historical/archived docs are in `_WORKSPACE/ARCHIVE/`.

---

*Namako: "sea cucumber" in Japanese — methodical, thorough, spec-driven.*
