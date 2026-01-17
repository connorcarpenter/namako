use namako::{StatsWriter as _, World, given, then, when};

#[given("ok")]
#[when("ok")]
fn ok(_: &mut W) -> Result<(), &'static str> {
    Ok(())
}

#[then("ok")]
fn then_ok(_: &W) -> Result<(), &'static str> {
    Ok(())
}

#[given("error")]
#[when("error")]
fn error(_: &mut W) -> Result<(), &'static str> {
    Err("error")
}

#[then("error")]
fn then_error(_: &W) -> Result<(), &'static str> {
    Err("error")
}

#[derive(Clone, Copy, Debug, Default, World)]
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
