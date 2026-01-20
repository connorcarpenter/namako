mod test_utils;

use namako::{
    StatsWriter as _, World as _,
    given, then, when,
};

use test_utils::{World, WorldMut, WorldRef};

#[given("ok")]
#[when("ok")]
fn ok(mut ctx: WorldMut) -> Result<(), &'static str> {
    let _ = ctx.world();
    Ok(())
}

#[then("ok")]
fn then_ok(ctx: WorldRef) -> Result<(), &'static str> {
    let _ = ctx.world();
    Ok(())
}

#[given("error")]
#[when("error")]
fn error(mut ctx: WorldMut) -> Result<(), &'static str> {
    let _ = ctx.world();
    Err("error")
}

#[then("error")]
fn then_error(ctx: WorldRef) -> Result<(), &'static str> {
    let _ = ctx.world();
    Err("error")
}

#[tokio::test]
async fn fails() {
    let writer =
        World::namako().with_default_cli().run("tests/features/result").await;

    assert_eq!(writer.passed_steps(), 3);
    assert_eq!(writer.skipped_steps(), 0);
    assert_eq!(writer.failed_steps(), 3);
    assert_eq!(writer.parsing_errors(), 0);
}
