mod test_utils;

use std::{convert::Infallible, str::FromStr};

use namako_engine::{
    Parameter, StatsWriter as _, World as _,
    given, then,
};

use test_utils::{World, WorldMut, WorldRef};

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
fn assert_int(mut ctx: WorldMut, v: Param) {
    let _ = ctx.world();
    assert_eq!(v, Param::Int(42));
}

#[given("regex: quoted: {param}")]
#[given("expr: quoted: {param}")]
fn assert_quoted(mut ctx: WorldMut, v: Param) {
    let _ = ctx.world();
    assert_eq!(v, Param::Quoted("inner".to_owned()));
}

#[then("params verified")]
fn then_verified(ctx: WorldRef) {
    let _ = ctx.world();
}

#[tokio::test]
async fn passes() {
    let writer = World::namako()
        .with_default_cli()
        .run("tests/features/from_str_and_parameter")
        .await;

    assert_eq!(writer.passed_steps(), 4);
    assert_eq!(writer.skipped_steps(), 0);
    assert_eq!(writer.failed_steps(), 0);
    assert_eq!(writer.parsing_errors(), 0);
}
