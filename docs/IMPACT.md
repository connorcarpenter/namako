# Namako impact policy

The canonical selector and runner is `clerc impact`, using
`.clerc/impact.toml`. The fleet testing strategy is maintained in Clerc's
`docs/TESTING.md`. Fabro executes Cargo commands; Clerc validates each exact
content key and its result. A missing, changed or unreadable result remains
UNKNOWN.

Each package's check edges start at its declared owned path root, such as
`engine/`. Clerc derives the typed package fan-out from those roots. A source
change therefore selects both the package's tests and its reverse dependencies;
the three static floors alone are not sufficient qualification for Rust changes.

The policy declares 20 checks: 3 R0, 3 R1, 10 R2, 2 R3 and 2 off-path benchmark
targets. Cargo metadata exposes 18 test-bearing targets. Four are explicitly
unchecked because they contain no runtime tests: the `minimal_ctx` compile
fixture, `test_utils` helper, benchmark fixture library and `skill-tool`
binary. Strict all-target Clippy compiles them; they provide no independent
test coverage.

- R0: the maintained owned-file formatter, strict workspace Clippy and
  strict all-feature engine Clippy. All three are always selected.
- R1: engine logic with NPAP enabled, code generation logic, and CLI logic.
  The CLI route excludes the nine freshness tests that launch Git or wait
  on a real remote timeout.
- R2: the eight engine integration targets, with NPAP/libtest enabled,
  and both custom codegen harnesses. Each package keeps its own Cargo
  invocation. Custom harnesses receive no libtest-only flags.
- R3: the nine Git/remote freshness tests and the tracing test's real
  asynchronous timers. The pure freshness decisions remain in R1.
- Off path: both Criterion benchmark targets belong to the benchmark
  class owned by Nash. They are separate from landing coverage.

An implementer runs committed content through:

```sh
clerc impact run --base <full-base-sha> --head <full-head-sha> \
  --suite namako --up-to 2 --out <new-output-directory>
clerc plan-admin land-key --root . --commit <full-head-sha> -- \
  cargo clippy --locked --workspace --all-targets --keep-going -- -D warnings
```

The integrator uses the same runner with `--seam --up-to 2` to require
every declared R0-R2 check, then runs any impact-selected upper-rung
checks needed at that seam. `--seam` without a cap requires all declared
rungs. Zero-test output provides no coverage. The native result, full
logs, counts and exact key remain the evidence; metadata eligibility and
policy verification alone are not test passes.

`clerc impact verify-map` checks root coverage, Cargo target declarations,
rung membership and unique runners. No alternate selector or gate wrapper
is added. The nine R3 exclusions are explicit test symbols; their separate
R3 route exercises those same tests rather than discarding them.
