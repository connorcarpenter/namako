use namako::{
    StatsWriter as _, World,
    codegen::StepContext,
    given, then, when,
};

// Context wrapper types for context-first ABI
struct WMut<'a>(&'a mut W);

#[derive(Clone, Copy)]
struct WRef<'a>(&'a W);

impl<'a> WMut<'a> {
    fn new(world: &'a mut W) -> Self { Self(world) }
    fn world(&mut self) -> &mut W { self.0 }
}

impl<'a> WRef<'a> {
    fn new(world: &'a W) -> Self { Self(world) }
    fn world(&self) -> &W { self.0 }
}

impl<'a> StepContext for WMut<'a> { type World = W; }
impl<'a> StepContext for WRef<'a> { type World = W; }

#[given("ok")]
#[when("ok")]
fn ok(mut ctx: WMut) -> Result<(), &'static str> {
    let _ = ctx.world();
    Ok(())
}

#[then("ok")]
fn then_ok(ctx: WRef) -> Result<(), &'static str> {
    let _ = ctx.world();
    Ok(())
}

#[given("error")]
#[when("error")]
fn error(mut ctx: WMut) -> Result<(), &'static str> {
    let _ = ctx.world();
    Err("error")
}

#[then("error")]
fn then_error(ctx: WRef) -> Result<(), &'static str> {
    let _ = ctx.world();
    Err("error")
}

#[derive(Clone, Copy, Debug, Default, World)]
#[world(mut_ctx = WMut<'a>, ref_ctx = WRef<'a>)]
struct W;

#[tokio::test]
async fn fails() {
    let writer =
        W::namako().with_default_cli().run("tests/features/result").await;

    assert_eq!(writer.passed_steps(), 3);
    assert_eq!(writer.skipped_steps(), 0);
    assert_eq!(writer.failed_steps(), 3);
    assert_eq!(writer.parsing_errors(), 0);
}
