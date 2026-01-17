use std::{env, panic::AssertUnwindSafe};

use clap::Parser;
use namako::{
    World as _, cli,
    codegen::{AssertOutcome, Assertable, StepContext},
    given, then,
};
use futures::FutureExt as _;
use serial_test::{parallel, serial};

#[derive(cli::Args)]
struct CustomCli {
    #[command(subcommand)]
    command: Option<SubCommand>,
}

#[derive(clap::Subcommand)]
enum SubCommand {
    Smoke(Smoke),
}

#[derive(cli::Args)]
struct Smoke {
    #[arg(long)]
    report_name: String,
}

// Context wrapper types
struct WorldMut<'a>(&'a mut World);

#[derive(Clone, Copy)]
struct WorldRef<'a>(&'a World);

impl<'a> WorldMut<'a> {
    fn new(world: &'a mut World) -> Self { Self(world) }
    fn world(&mut self) -> &mut World { self.0 }
}
impl<'a> WorldRef<'a> {
    fn new(world: &'a World) -> Self { Self(world) }
    fn world(&self) -> &World { self.0 }
}
impl<'a> StepContext for WorldMut<'a> { type World = World; }
impl<'a> StepContext for WorldRef<'a> { type World = World; }

impl Assertable for World {
    type Ctx<'a> = WorldRef<'a> where Self: 'a;
    fn assert_then<T, F>(&mut self, mut f: F) -> T
    where F: FnMut(&Self::Ctx<'_>) -> AssertOutcome<T> {
        match f(&WorldRef(self)) {
            AssertOutcome::Passed(v) => v,
            AssertOutcome::Pending => panic!("Pending not supported"),
            AssertOutcome::Failed(msg) => panic!("{msg}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, namako::World)]
#[world(mut_ctx = WorldMut<'a>, ref_ctx = WorldRef<'a>)]
struct World;

#[given("an invalid step")]
fn invalid_step(mut ctx: WorldMut) {
    let _ = ctx.world();
    assert!(false);
}

#[then("cli verified")]
fn then_verified(ctx: WorldRef) {
    let _ = ctx.world();
}

// This test uses a subcommand with the global option `--tags` to filter on two
// failing tests and verifies that the error output contains 2 failing steps.
#[tokio::test]
#[parallel]
async fn tags_option_filters_all_scenarios_with_subcommand() {
    let cli = cli::Opts::<_, _, _, CustomCli>::try_parse_from([
        "test",
        "smoke",
        r#"--report-name="smoke.report""#,
        "--tags=@all",
    ])
    .expect("Invalid command line");

    let res =
        World::namako().with_cli(cli).run_and_exit("tests/features/cli");

    let err =
        AssertUnwindSafe(res).catch_unwind().await.expect_err("should err");
    let err = err.downcast_ref::<String>().unwrap();

    assert_eq!(err, "2 steps failed");
}

// This test uses a subcommand with the global option `--tags` to filter on one
// failing test and verifies that the error output contains 1 failing step.
#[tokio::test]
#[parallel]
async fn tags_option_filters_scenario1_with_subcommand() {
    let cli = cli::Opts::<_, _, _, CustomCli>::try_parse_from([
        "test",
        "smoke",
        r#"--report-name="smoke.report""#,
        "--tags=@scenario-1",
    ])
    .expect("Invalid command line");

    let res =
        World::namako().with_cli(cli).run_and_exit("tests/features/cli");

    let err =
        AssertUnwindSafe(res).catch_unwind().await.expect_err("should err");
    let err = err.downcast_ref::<String>().unwrap();

    assert_eq!(err, "1 step failed");
}

// This test verifies that the global option `--tags` is still available without
// subcommands and that the error output contains 1 failing step.
#[tokio::test]
#[parallel]
async fn tags_option_filters_scenario1_no_subcommand() {
    let cli = cli::Opts::<_, _, _, CustomCli>::try_parse_from([
        "test",
        "--tags=@scenario-1",
    ])
    .expect("Invalid command line");

    let res =
        World::namako().with_cli(cli).run_and_exit("tests/features/cli");

    let err =
        AssertUnwindSafe(res).catch_unwind().await.expect_err("should err");
    let err = err.downcast_ref::<String>().unwrap();

    assert_eq!(err, "1 step failed");
}

// This test verifies that the `NAMAKO_FILTER_TAGS` env var filters apply and
// that the error output contains 1 failing step.
#[tokio::test]
#[serial]
async fn tags_option_filters_scenario1_via_env() {
    unsafe {
        env::set_var("NAMAKO_FILTER_TAGS", "@scenario-1");
    }

    let cli = cli::Opts::<_, _, _, CustomCli>::try_parse_from(["test"])
        .expect("Invalid command line");

    let res =
        World::namako().with_cli(cli).run_and_exit("tests/features/cli");

    let err =
        AssertUnwindSafe(res).catch_unwind().await.expect_err("should err");
    let err = err.downcast_ref::<String>().unwrap();

    assert_eq!(err, "1 step failed");

    unsafe {
        env::remove_var("NAMAKO_FILTER_TAGS");
    }
}
