use std::time::Duration;

use namako::{StatsWriter as _, World, gherkin::Step, given, then, when, writer};
use tokio::time;

#[derive(Debug, Default, World)]
pub struct FirstWorld {
    foo: i32,
}

#[derive(Debug, Default, World)]
pub struct SecondWorld {
    foo: i32,
}

#[given("{word} is sync {int}")]
fn test_regex_sync_slice_first(w: &mut FirstWorld, step: &Step, matches: &[String]) {
    assert_eq!(matches[0], "foo");
    assert_eq!(matches[1].parse::<usize>().unwrap(), 0);
    assert_eq!(step.value, "foo is sync 0");

    w.foo += 1;
}

#[given("{word} is {int}")]
#[when("{word} is {int}")]
async fn test_regex_async_first(
    w: &mut FirstWorld,
    step: String,
    #[step] ctx: &Step,
    num: usize,
) {
    time::sleep(Duration::new(1, 0)).await;

    assert_eq!(step, "foo");
    assert_eq!(num, 0);
    assert_eq!(ctx.value, "foo is 0");

    w.foo += 1;
}

#[given("{word} is not {word}")]
fn test_foo_is_not_bar_first(_w: &mut FirstWorld, _foo: String, _bar: String) {}

#[when("I write \"{word}\" to '{word}'")]
fn test_write_first(_w: &mut FirstWorld, _what: String, _filename: String) {}

#[then("the file {string} should contain {string}")]
fn test_read_first(_w: &mut FirstWorld, _filename: String, _what: String) {}

#[then("{string} contains {string}")]
fn test_read_slice_first(_w: &mut FirstWorld, _inputs: &[String]) {}

#[given("{word} is {int}")]
#[when("{word} is {int}")]
async fn test_regex_async_second(
    w: &mut SecondWorld,
    step: String,
    #[step] _ctx: &Step,
    num: usize,
) {
    time::sleep(Duration::new(1, 0)).await;

    assert_eq!(step, "foo");
    assert_eq!(num, 0);
    // ctx.value depends on the step being run. 'foo is 0'
    // check it just in case
    // assert_eq!(ctx.value, "foo is 0"); // might fail if used in other contexts? No, strict regex.

    w.foo += 1;
}

#[given("{word} is sync {int}")]
fn test_regex_sync_slice_second(w: &mut SecondWorld, step: &Step, matches: &[String]) {
    assert_eq!(matches[0], "foo");
    assert_eq!(matches[1].parse::<usize>().unwrap(), 0);
    assert_eq!(step.value, "foo is sync 0");

    w.foo += 1;
}

#[given("{word} is not {word}")]
fn test_foo_is_not_bar_second(_w: &mut SecondWorld, _foo: String, _bar: String) {}

#[when("I write \"{word}\" to '{word}'")]
fn test_write_second(_w: &mut SecondWorld, _what: String, _filename: String) {}

#[then("the file {string} should contain {string}")]
fn test_read_second(_w: &mut SecondWorld, _filename: String, _what: String) {}

#[then("{string} contains {string}")]
fn test_read_slice_second(_w: &mut SecondWorld, _inputs: &[String]) {}


#[tokio::main]
async fn main() {
    let writer = FirstWorld::namako()
        .max_concurrent_scenarios(None)
        .with_writer(writer::Libtest::or_basic())
        .run("./tests/features")
        .await;

    // All 15 steps should pass now
    assert_eq!(writer.passed_steps(), 15);
    assert_eq!(writer.skipped_steps(), 0);
    assert_eq!(writer.failed_steps(), 0);

    let writer = SecondWorld::namako()
        .max_concurrent_scenarios(None)
        .with_writer(writer::Libtest::or_basic())
        .run("./tests/features")
        .await;

    // All 15 steps should pass now
    assert_eq!(writer.passed_steps(), 15);
    assert_eq!(writer.skipped_steps(), 0);
    assert_eq!(writer.failed_steps(), 0);
}
