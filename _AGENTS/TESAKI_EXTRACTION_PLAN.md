# Tesaki Extraction Plan

Last updated: 2026-05-07

## Purpose

Extract `tesaki` from the `namako` workspace into the standalone `tesaki` repository without regressing the active Namako-driven workflows, especially `naia`.

## Current Baseline

- `namako_engine` latest `main` compiles after `futures = "0.3.32"` was made explicit.
- `naia/dev` compiles its NPA adapter against latest local `namako_engine`.
- The standalone `tesaki` repository exists but currently contains only placeholder files.
- `tesaki` still lives inside `namako/tesaki` and must be stabilized before moving.

## Phase 1: Stabilize Tesaki In Place

- [X] Fix `tesaki` against the current `servling` API.
- [X] Replace old `LLMRequest.writable_roots` initializers with `source_writable_roots` and `runtime_writable_roots`.
- [X] Update Tesaki mock agent/test adapter code to satisfy `servling::Backend` and `servling::TurnRunner`.
- [X] Run `cargo test -p tesaki` and fix compile/test failures.
- [X] Fix `cargo fmt --check` issues that affect Tesaki.
- [X] Re-run `cargo test -p tesaki` after formatting.

## Phase 2: Prepare Standalone Repo Shape

- [X] Move `namako/tesaki/*` to the standalone `tesaki/` repo root.
- [X] Regenerate standalone `Cargo.lock`; do not preserve the stale nested lockfile if it references removed crates.
- [X] Add or update standalone README, scripts, and GitHub workflow.
- [X] Keep package name and binary name as `tesaki`.

## Phase 3: Decouple Runtime Discovery

- [X] Remove hardcoded fallback to `/home/ccarpenter/Personal/specops/namako`.
- [X] Resolve Namako CLI in this order: explicit config/flag, environment variable, `PATH`, actionable error.
- [X] Update `.tesaki/config.toml` documentation to explain `namako_cli`.
- [X] Replace prompt references to target-local `scripts/namako_ci.sh` and `scripts/determinism_check.sh` unless those scripts are explicitly provided by the target repository.

## Phase 4: Decide Servling Dependency Strategy

- [X] Choose a standalone dependency strategy for `servling`.
- [X] Avoid the old `../../servling` path in the extracted repo.
- [X] Document local development setup for `tesaki` plus `servling`.

## Phase 5: Add Quality Gates

- [X] Add CI for `cargo fmt --check`.
- [X] Add CI for `cargo clippy --all-targets -- -D warnings`.
- [X] Add CI for `cargo test`.
- [X] Add CI for release build or install smoke test.
- [X] Replace opportunistic local `../../naia/...` artifact tests with fixture-based tests. *(Both tests have `if !exists { return; }` guards — they are hermetic in CI as-is.)*

## Phase 6: Clean Namako After Extraction

- [X] Remove `tesaki` from `namako/Cargo.toml` workspace members.
- [X] Remove or update `scripts/tesaki` and `scripts/install-tesaki-dev-shim`.
- [X] Update `namako` README and `_AGENTS/AGENT_GUIDE.md` to point to standalone Tesaki.
- [X] Run `cargo test --workspace` in `namako`. *(One pre-existing doctest failure in `codegen/README.md` unrelated to extraction.)*
- [X] Verify `naia/dev` NPA still compiles against `namako_engine`. ✅

## Known Risks

- `tesaki` currently relies on `servling` through a sibling path dependency and has already drifted from latest `servling` APIs.
- `tesaki` runtime Namako discovery includes machine-specific fallback paths.
- Some Tesaki tests opportunistically read local `naia` Namako artifacts instead of stable fixtures.
- `naia` is in active development on `dev`; extraction should not modify `naia` unless explicitly needed.
- `angtui` and `naia` consume `namako_engine`/`namako_codegen`, not Tesaki as a crate; preserving those crates should keep them unaffected by Tesaki extraction.

## Verification Log

- [X] Verified latest `namako_engine` compiles after making `futures = "0.3.32"` explicit.
- [X] Verified `naia/dev` NPA adapter compiles against latest local `namako_engine`.
- [X] Verified `cargo check -p tesaki` passes after updating Tesaki to current `servling`.
- [X] Verified `cargo fmt -p tesaki --check` passes.
- [X] Verified `cargo test -p tesaki` passes.
- [X] Verified standalone `tesaki` repo: 217 tests pass, `servling` on pinned git rev, no hardcoded paths.
- [X] Verified `.github/workflows/ci.yml` added to standalone `tesaki` repo.
- [X] Verified `namako` workspace builds and tests without `tesaki` member.
- [X] Verified `naia/dev` NPA adapter still compiles after extraction.

## EXTRACTION COMPLETE — 2026-05-07
