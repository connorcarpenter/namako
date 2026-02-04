# Tesaki

AI-friendly task orchestrator for Namako spec-driven development.

Tesaki is a deterministic task generator that:
- Consumes Namako status and review packets
- Generates actionable next-task instructions
- Orchestrates autonomous coding agents (v1.7 runner integration)
- Enforces governance limits on update-cert operations

## Quick Start (Development)

### 1. Install the dev shim (one-time)

```bash
./scripts/install-tesaki-dev-shim
```

This creates a symlink in `~/.local/bin/tesaki` pointing to the local checkout.
After installation, you can run `tesaki` from anywhere.

### 2. Create a per-repo config

In your target repository (e.g., naia), create `.tesaki/config.toml`:

```toml
# .tesaki/config.toml
specs_dir = "test/specs"
adapter_cmd = "cargo run --manifest-path test/npa/Cargo.toml --"

# Optional
agent = "copilot" # primary agent for runner + planner
max_retries = 2
max_cert_updates = 3
max_runtime_seconds = 600
max_files_changed = 10
```

### 3. Run tesaki

```bash
# From anywhere in your repo (config discovery works from any subdirectory)
tesaki
```

Tesaki starts an interactive REPL. Use natural language prompts, and approve missions when prompted.

## Configuration Discovery

Tesaki searches for configuration in this order:

1. Look for `.tesaki/config.toml` in the current directory
2. Walk up parent directories until found
3. First match wins

Paths in `specs_dir` and `adapter_cmd` (e.g., `--manifest-path`) are resolved relative to the directory containing `.tesaki/`.

### Config Schema

```toml
# Required
specs_dir = "test/specs"           # Path to specs directory
adapter_cmd = "cargo run -p npa --" # Adapter command

# Optional
agent = "copilot"                   # mock, claude, codex, or copilot
max_retries = 2                     # Retry attempts
max_cert_updates = 3                # Update-cert limit
max_runtime_seconds = 600           # Runtime budget
max_files_changed = 10              # File change limit
```

Advanced overrides (runner/planner split, cmd runner, surface patterns) are supported but optional.

## Alternative: Direct Cargo Run

If you prefer not to use the shim:

```bash
cd /path/to/namako
cargo run -p tesaki
```

## Development

### Running Tests

```bash
cargo test -p tesaki
```

### Building

```bash
cargo build -p tesaki
```
