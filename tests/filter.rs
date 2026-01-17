use std::{fmt, io};

use namako::{
    codegen::{AssertOutcome, Assertable, StepContext},
    StatsWriter, World as _, WriterExt as _, given, then, when, writer,
};

// =============================================================================
// Context Wrapper Types (required for context-first ABI)
// =============================================================================

/// Mutable context wrapper for Given/When steps.
struct WorldMut<'a>(&'a mut World);

/// Immutable context wrapper for Then steps.
#[derive(Clone, Copy)]
struct WorldRef<'a>(&'a World);

impl<'a> WorldMut<'a> {
    fn new(world: &'a mut World) -> Self {
        Self(world)
    }
}

impl<'a> WorldRef<'a> {
    fn new(world: &'a World) -> Self {
        Self(world)
    }
}

impl<'a> StepContext for WorldMut<'a> {
    type World = World;
}

impl<'a> StepContext for WorldRef<'a> {
    type World = World;
}

impl Assertable for World {
    type Ctx<'a> = WorldRef<'a> where Self: 'a;

    fn assert_then<T, F>(&mut self, mut f: F) -> T
    where
        F: FnMut(&Self::Ctx<'_>) -> AssertOutcome<T>,
    {
        let ctx = WorldRef(self);
        match f(&ctx) {
            AssertOutcome::Passed(v) => v,
            AssertOutcome::Pending => {
                panic!("Then step returned Pending, but this World does not support polling")
            }
            AssertOutcome::Failed(msg) => {
                panic!("Then step failed: {msg}")
            }
        }
    }
}

// =============================================================================
// Step Functions
// =============================================================================

#[given("{int} < 10")]
#[when("{int} < 10")]
fn step(_: WorldMut, num: usize) {
    assert!(num < 10, "not filtered");
}

#[then("{int} < 10")]
fn then_step(_: WorldRef, num: usize) {
    assert!(num < 10, "not filtered");
}

#[tokio::test]
async fn by_examples() {
    let mut output = Output::default();
    let writer = World::namako()
        .with_writer(
            writer::Basic::new(&mut output, writer::Coloring::Auto, 0)
                .summarized(),
        )
        .with_default_cli()
        .filter_run("tests/features/filter", |_, _, sc| {
            // Omit `Examples` rows containing numbers less than 10.
            (sc.name == "by examples")
                && (sc.examples.first().is_some_and(|example| {
                    example.table.as_ref().is_some_and(|table| {
                        table.rows.get(1).is_some_and(|cols| {
                            cols.iter().all(|v| {
                                v.parse::<usize>().is_ok_and(|num| num < 10)
                            })
                        })
                    })
                }))
        })
        .await;

    if writer.execution_has_failed() {
        panic!("some steps failed:\n{output}");
    }
}

#[derive(Clone, Copy, Debug, Default, namako::World)]
#[world(mut_ctx = WorldMut<'a>, ref_ctx = WorldRef<'a>)]
struct World;

/// Deterministic output of [`writer::Basic`].
#[derive(Clone, Debug, Default)]
struct Output(Vec<u8>);

impl io::Write for Output {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl fmt::Display for Output {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let o = String::from_utf8(self.0.clone())
            .unwrap_or_else(|e| panic!("`Output` is not a string: {e}"));
        write!(f, "{o}")
    }
}
