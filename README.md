# Namako + Tesaki

**Spec-Driven Development toolchain for Rust projects**

| Tool | Purpose |
|------|---------|
| **Namako** | BDD testing framework + CLI (measures truth) |
| **Tesaki** | AI task orchestrator (drives autonomous development) |

---

## AI Coding Agent? Start Here

**Single entrypoint:** [`_AGENTS/AGENT_GUIDE.md`](./_AGENTS/AGENT_GUIDE.md)

This file contains everything you need to understand and work on this codebase efficiently.

---

## Quick Start (Human Users)

```bash
# Install dev shims (from namako/)
cargo install --path tesaki --force
cargo install --path cli --force

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
├── src/              # Namako core library
├── cli/              # namako_cli binary
├── codegen/          # Step macros (#[given], #[when], #[then])
├── tesaki/           # Tesaki AI orchestrator
│   ├── src/          # Rust source
│   └── prompts/      # Jinja2 mission templates
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
```

### Tesaki CLI
```bash
tesaki              # Interactive REPL
tesaki --loop N     # Run N missions autonomously
tesaki diagnose ID  # Debug a specific mission
```

## Testing

```bash
cargo test              # All tests (~222 tests)
cargo test -p tesaki    # Tesaki only
cargo test -p namako_cli
```

## Documentation Map

| File | Purpose |
|------|---------|
| `_AGENTS/AGENT_GUIDE.md` | **Single entrypoint for AI agents** |
| `_WORKSPACE/RUNBOOK.md` | Operational checklist for loop execution |
| `tesaki/README.md` | Tesaki configuration & dev setup |
| `tesaki/prompts/README.md` | Mission template authoring guide |

Historical/archived docs are in `_WORKSPACE/ARCHIVE/`.

---

*Namako: "sea cucumber" in Japanese — methodical, thorough, spec-driven.*
