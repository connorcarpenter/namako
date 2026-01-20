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
runner = "mock"
max_retries = 2
max_cert_updates = 3
max_runtime_seconds = 600
max_files_changed = 10
```

### 3. Run tesaki

```bash
# From anywhere in your repo (config discovery works from any subdirectory)
tesaki run

# Or run with explicit flags (flags override config)
tesaki run -s test/specs -a "cargo run -p npa --" --runner mock

# See the resolved config
tesaki config print
```

## Commands

### `tesaki run`

The autonomous development loop (v1.7). Creates a mission bundle, invokes the runner, and validates results.

```bash
tesaki run [OPTIONS]
```

Options:
- `-s, --spec-root <PATH>` — Path to specs directory (uses config if omitted)
- `-a, --adapter <CMD>` — Adapter command (uses config if omitted)
- `--runner <RUNNER>` — Runner backend: `mock`, `cmd`, or `claude`
- `--runner-cmd <CMD>` — Command template for cmd runner
- `--max-retries <N>` — Maximum retry attempts (default: 2)
- `--max-cert-updates <N>` — Maximum update-cert operations (default: 3)
- `--max-runtime-seconds <N>` — Maximum runtime per mission (default: 600)
- `--max-files-changed <N>` — Maximum files runner may change (default: 10)

### `tesaki next`

Generate the next task based on current Namako state. Outputs `NEXT_TASK.md`.

```bash
tesaki next [OPTIONS]
```

### `tesaki config print`

Print the resolved configuration and where it was found.

```bash
tesaki config print
```

## Configuration Discovery

When `-s` or `-a` flags are omitted, Tesaki searches for configuration in this order:

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
runner = "mock"                     # mock, cmd, or claude
runner_cmd = "my-agent {mission_dir}" # For cmd runner
max_retries = 2                     # Retry attempts
max_cert_updates = 3                # Update-cert limit
max_runtime_seconds = 600           # Runtime budget
max_files_changed = 10              # File change limit
```

### Flag Override Rules

CLI flags always override config file values. Partial overrides are supported:

```bash
# Use config for everything except runner
tesaki run --runner claude

# Use config for adapter, override specs_dir
tesaki run -s ../other/specs
```

## Alternative: Direct Cargo Run

If you prefer not to use the shim:

```bash
cd /path/to/namako
cargo run -p tesaki -- run -s ../naia/test/specs -a "..."
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
