use namako::{
    StatsWriter as _, World,
    codegen::{AssertOutcome, Assertable, StepContext},
    given, then, when,
};

// Context wrapper types for context-first ABI
struct WMut<'a>(&'a mut W);

#[derive(Clone, Copy)]
struct WRef<'a>(&'a W);

impl<'a> WMut<'a> {
    fn new(world: &'a mut W) -> Self { Self(world) }
}

impl<'a> WRef<'a> {
    fn new(world: &'a W) -> Self { Self(world) }
}

impl<'a> StepContext for WMut<'a> { type World = W; }
impl<'a> StepContext for WRef<'a> { type World = W; }

impl Assertable for W {
    type Ctx<'a> = WRef<'a> where Self: 'a;
    fn assert_then<T, F>(&mut self, mut f: F) -> T
    where F: FnMut(&Self::Ctx<'_>) -> AssertOutcome<T> {
        match f(&WRef(self)) {
            AssertOutcome::Passed(v) => v,
            AssertOutcome::Pending => panic!("Pending not supported"),
            AssertOutcome::Failed(msg) => panic!("{msg}"),
        }
    }
}

#[given("ok")]
#[when("ok")]
fn ok(_: WMut) -> Result<(), &'static str> {
    Ok(())
}

#[then("ok")]
fn then_ok(_: WRef) -> Result<(), &'static str> {
    Ok(())
}

#[given("error")]
#[when("error")]
fn error(_: WMut) -> Result<(), &'static str> {
    Err("error")
}

#[then("error")]
fn then_error(_: WRef) -> Result<(), &'static str> {
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
