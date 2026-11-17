# Optional engine features: first adoption failure

Measured on Lifetop, 2026-10-21 03:15 UTC, through the ordinary Clerc impact
runner. Original candidate c3361483c51c5b4de9b5b6967823e7ead597bd4e has tree
40eb95d9f706b0dda5aa86bd86240c9e308fa204; published DEV and MAIN are
00d52e0f7e33d3f7b1501e75b71fab819c7a2a1f. No sibling repositories are inputs.
The adoption changes only the policy and its documentation; the failing
engine source is unchanged from that published base.

```sh
clerc impact run --base 00d52e0f7e33d3f7b1501e75b71fab819c7a2a1f \
  --head c3361483c51c5b4de9b5b6967823e7ead597bd4e --suite namako \
  --up-to 2 --out <new-directory>
```

The run ended 1 at R0. Its all-feature leg was:

```sh
cargo clippy --locked -p namako_engine --all-targets --all-features \
  --keep-going -- -D warnings
```

Native job f6bb12959c1f1 ended 101 on Lifetop after 60,705 ms. Both native
streams are complete (stdout 0 bytes, stderr 5,986 bytes). Signed terminal
receipt SHA256: 009a8a469aeda37de79c700139369c8d215548d83954f776e82c52a0d6b9ef29.
Its original package-scope key is
535ab179b5c44bff9ddcca05c34239db60f0b8b50083fec9fbbfe8018beaa7e2.

All eight errors, repeated for the library and its unit-test build:

- `engine/src/tracing.rs:245,268,271`: deprecated `UnboundedReceiver::try_next`.
- `engine/src/tracing.rs:99,114`: `clippy::absolute_paths` on the formatting
  layer type and constructor.
- `engine/src/tracing.rs:472`: `clippy::unnecessary_trailing_comma`.
- `engine/src/runner/basic.rs:388,589`: unfulfilled `too_many_arguments`
  expectations when tracing is enabled.

Default-feature workspace strict Clippy passed (native f8efb058e2598),
and the owned-file formatter checked 63 files with zero failures. They do
not cover the optional-feature failure. R1 and R2 were not climbed; no
runtime-test coverage or landing admission is claimed. The policy retains
the failing feature route and every strict lint.

The exact toolchain was rustc 1.96.0 (ac68faa20 2026-05-25), Cargo program
SHA256 be8b35bd9df5d8d1321a240e1801402521574d387a891a7cdbe86f1f2433119a,
Cargo config aa7c0c4700fec50b03e1c6410a53ed50cca6dd14eb893d750ecd1bbe9225408f,
and environment CARGO_INCREMENTAL=0, RUSTC_WRAPPER=sccache (digest
6f32b39c7ec444e052dce986b8bef97cc6eb704f75e83cc27ee01db3f5361c3d).

Raw evidence is retained at
`connor@lifetop:/home/connor/Work/specops/.agents/halden-home/evidence/zgf/namako-policy-current/`:
`impact-run.log`, `run/run.json`, `run/0/1.log`,
`native-original/f6bb12959c1f1.{show.json,stderr.txt}` and
`attempt-bindings.sha256`. The original caller and native jobs have ended.
No original binding or result was edited and no failed job was replayed.
