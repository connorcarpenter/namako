use std::{convert::Infallible, str::FromStr};

use namako::{
    Parameter, StatsWriter as _, World,
    codegen::StepContext,
    given, then,
};

// Context wrapper types
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

#[derive(Debug, Parameter, PartialEq)]
#[param(name = "param", regex = "'([^']*)'|(\\d+)")]
enum Param {
    Int(u64),
    Quoted(String),
}

impl FromStr for Param {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(s.parse::<u64>()
            .map_or_else(|_| Self::Quoted(s.to_owned()), Param::Int))
    }
}

#[given("regex: int: {param}")]
#[given("expr: int: {param}")]
fn assert_int(mut ctx: WMut, v: Param) {
    let _ = ctx.world();
    assert_eq!(v, Param::Int(42));
}

#[given("regex: quoted: {param}")]
#[given("expr: quoted: {param}")]
fn assert_quoted(mut ctx: WMut, v: Param) {
    let _ = ctx.world();
    assert_eq!(v, Param::Quoted("inner".to_owned()));
}

#[then("params verified")]
fn then_verified(ctx: WRef) {
    let _ = ctx.world();
}

#[derive(Clone, Copy, Debug, Default, World)]
#[world(mut_ctx = WMut<'a>, ref_ctx = WRef<'a>)]
struct W;

#[tokio::test]
async fn passes() {
    let writer = W::namako()
        .with_default_cli()
        .run("tests/features/from_str_and_parameter")
        .await;

    assert_eq!(writer.passed_steps(), 4);
    assert_eq!(writer.skipped_steps(), 0);
    assert_eq!(writer.failed_steps(), 0);
    assert_eq!(writer.parsing_errors(), 0);
}
