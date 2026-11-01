# Namako

**Spec-Driven Development testing framework for Rust projects**

---

## AI Coding Agent? Start Here

Start at [`AGENTS.md`](./AGENTS.md), then [`_AGENTS/AGENT_GUIDE.md`](./_AGENTS/AGENT_GUIDE.md).

---

## Quick Start (Human Users)

```bash
# Install namako CLI (from namako/)
cargo install --locked --path cli

# Gate a target repo's specs
cd <target-repo>/test/specs
namako gate -a "<adapter command>"
```

## Removed / not wanted

- **Tesaki**: a second orchestrator; retired Nov 1, 2026 by Usher (King) on fleet doctrine; it was dormant since May. Its instructions, install steps and CLI section were removed from this repo. Fleet dispatch is Nydus, execution is Fabro, selection is Clerc. Restore from git history only if Connor says Tesaki is wanted.

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

## Testing

```bash
cargo test              # All tests
cargo test -p namako_cli
```

## Documentation Map

| File | Purpose |
|------|---------|
| `_AGENTS/AGENT_GUIDE.md` | Toolchain internals guide |
| `AGENTS.md` | Seat entrypoint and spec line |
| `docs/EVIDENCE.md`, `docs/IMPACT.md` | Behavior specs |

Historical/archived docs are in `_WORKSPACE/ARCHIVE/`.

---

*Namako: "sea cucumber" in Japanese — methodical, thorough, spec-driven.*
