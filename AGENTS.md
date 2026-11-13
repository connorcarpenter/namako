# namako

Specs: `docs/EVIDENCE.md` (run-evidence judge), `docs/IMPACT.md` (test selection), `README.md` (CLI verbs and layout).

namako is a BDD framework: `engine/` (library, linked by consumers as `namako_engine`), `codegen/` (step macros), `cli/` (the `namako` binary), `benches/`, `skill-tool/`.
Trunk is `main`; land on `dev`. Owner and integrator: Nash.

Lints (nydus-svi, 2026-11-10): the Kingdom shared Rust lint set (ambry `standards/RUST_LINTS.md`) lives in the root `Cargo.toml` `[workspace.lints]` block and every member inherits it with `[lints] workspace = true`; `clippy.toml` keeps the justified `HashableRegex` exemption and allows unwrap/expect in tests. First rung denies `dbg_macro`, `todo`, `unimplemented`; second rung (nydus-5fo) denies `unsafe_code`, each required `unsafe` item carrying a scoped `#[expect(unsafe_code, reason = ...)]` and a `// SAFETY:` comment; third rung (nydus-tev) denies `unwrap_used` and `expect_used` in non-test code, with tests exempt through `clippy.toml`; fourth rung (nydus-u4o) denies `allow_attributes_without_reason`, so every suppression is a `#[expect(lint, reason = ...)]`. Add a rung by editing the workspace block only; never a blanket `allow`, never lint levels in gate argv.
