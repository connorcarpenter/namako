use std::{fs, io, time::Duration};

use namako::{StatsWriter as _, World, gherkin::Step, given, then, when};

use tempfile::TempDir;
use tokio::time;

#[derive(Debug, World)]
#[world(init = Self::new)]
pub struct MyWorld {
    foo: i32,
    dir: TempDir,
}

impl MyWorld {
    fn new() -> io::Result<Self> {
        Ok(Self { foo: 0, dir: TempDir::new()? })
    }
}

#[given("non-regex")]
fn test_non_regex_sync(w: &mut MyWorld) {
    w.foo += 1;
}

#[given("non-regex async")]
async fn test_non_regex_async(w: &mut MyWorld, #[step] ctx: &Step) {
    time::sleep(Duration::new(1, 0)).await;

    assert_eq!(ctx.value, "non-regex async");

    w.foo += 1;
}

#[given("{word} is not {word}")]
fn test_foo_is_not_bar(_w: &mut MyWorld, _foo: String, _bar: String) {}

#[given("{word} is {int}")]
#[when(r"{word} is {int}")]
async fn test_regex_async(
    w: &mut MyWorld,
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

#[given("{word} is sync {int}")]
fn test_regex_sync_slice(w: &mut MyWorld, step: &Step, matches: &[String]) {
    assert_eq!(matches[0], "foo");
    assert_eq!(matches[1].parse::<usize>().unwrap(), 0);
    assert_eq!(step.value, "foo is sync 0");

    w.foo += 1;
}

#[when("I write \"{word}\" to '{word}'")]
fn test_return_result_write(
    w: &mut MyWorld,
    what: String,
    filename: String,
) -> io::Result<()> {
    let mut path = w.dir.path().to_path_buf();
    path.push(filename);
    fs::write(path, what)
}

#[then("the file {string} should contain {string}")]
fn test_return_result_read(
    w: &MyWorld,
    filename: String,
    what: String,
) -> io::Result<()> {
    // Can't mutate, but we don't need to actually read from immutable world struct
    // The previous code read mutable world to get 'dir', we need that immutable or via interior mutability.
    // 'dir' is TemporaryDirectory which is not Copy.
    // Let's assume MyWorld definition allows immutable access OR we change MyWorld for this test.

    // BUT WAIT, MyWorld is defined in this file. Let's check MyWorld definition.
    let mut path = w.dir.path().to_path_buf();
    path.push(filename);

    assert_eq!(what, fs::read_to_string(path)?);

    Ok(())
}

#[then("{string} contains {string}")]
fn test_return_result_read_slice(
    w: &MyWorld,
    inputs: &[String],
) -> io::Result<()> {
    let mut path = w.dir.path().to_path_buf();
    path.push(inputs[0].clone());

    assert_eq!(inputs[1], fs::read_to_string(path)?);

    Ok(())
}

#[tokio::main]
async fn main() {
    let features = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/features");

    let writer = MyWorld::namako()
        .max_concurrent_scenarios(None)
        .fail_on_skipped()
        .run(features)
        .await;

    assert_eq!(writer.failed_steps(), 1);
}
