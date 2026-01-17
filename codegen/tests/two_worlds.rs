use namako::{
    StatsWriter as _, World,
    codegen::{AssertOutcome, Assertable, StepContext},
    gherkin::Step, given, then, when, writer,
};

#[derive(Debug, Default, World)]
#[world(mut_ctx = FirstWorldMut<'a>, ref_ctx = FirstWorldRef<'a>)]
pub struct FirstWorld {
    foo: i32,
}

#[derive(Debug, Default, World)]
#[world(mut_ctx = SecondWorldMut<'a>, ref_ctx = SecondWorldRef<'a>)]
pub struct SecondWorld {
    foo: i32,
}

// Context wrapper types for FirstWorld
pub struct FirstWorldMut<'a>(&'a mut FirstWorld);

#[derive(Clone, Copy)]
pub struct FirstWorldRef<'a>(&'a FirstWorld);

impl<'a> FirstWorldMut<'a> {
    fn new(world: &'a mut FirstWorld) -> Self { Self(world) }
}
impl<'a> FirstWorldRef<'a> {
    fn new(world: &'a FirstWorld) -> Self { Self(world) }
    fn world(&self) -> &FirstWorld { self.0 }
}
impl<'a> StepContext for FirstWorldMut<'a> { type World = FirstWorld; }
impl<'a> StepContext for FirstWorldRef<'a> { type World = FirstWorld; }

impl Assertable for FirstWorld {
    type Ctx<'a> = FirstWorldRef<'a> where Self: 'a;
    fn assert_then<T, F>(&mut self, mut f: F) -> T
    where F: FnMut(&Self::Ctx<'_>) -> AssertOutcome<T> {
        match f(&FirstWorldRef(self)) {
            AssertOutcome::Passed(v) => v,
            AssertOutcome::Pending => panic!("Pending not supported"),
            AssertOutcome::Failed(msg) => panic!("{msg}"),
        }
    }
}

// Context wrapper types for SecondWorld
pub struct SecondWorldMut<'a>(&'a mut SecondWorld);

#[derive(Clone, Copy)]
pub struct SecondWorldRef<'a>(&'a SecondWorld);

impl<'a> SecondWorldMut<'a> {
    fn new(world: &'a mut SecondWorld) -> Self { Self(world) }
}
impl<'a> SecondWorldRef<'a> {
    fn new(world: &'a SecondWorld) -> Self { Self(world) }
    fn world(&self) -> &SecondWorld { self.0 }
}
impl<'a> StepContext for SecondWorldMut<'a> { type World = SecondWorld; }
impl<'a> StepContext for SecondWorldRef<'a> { type World = SecondWorld; }

impl Assertable for SecondWorld {
    type Ctx<'a> = SecondWorldRef<'a> where Self: 'a;
    fn assert_then<T, F>(&mut self, mut f: F) -> T
    where F: FnMut(&Self::Ctx<'_>) -> AssertOutcome<T> {
        match f(&SecondWorldRef(self)) {
            AssertOutcome::Passed(v) => v,
            AssertOutcome::Pending => panic!("Pending not supported"),
            AssertOutcome::Failed(msg) => panic!("{msg}"),
        }
    }
}

#[given("{word} is sync {int}")]
fn test_regex_sync_slice_first(w: FirstWorldMut, step: &Step, matches: &[String]) {
    assert_eq!(matches[0], "foo");
    assert_eq!(matches[1].parse::<usize>().unwrap(), 0);
    assert_eq!(step.value, "foo is sync 0");

    w.0.foo += 1;
}

#[given("{word} is {int}")]
#[when("{word} is {int}")]
async fn test_regex_async_first(
    w: FirstWorldMut,
    step: String,
    #[step] ctx: &Step,
    num: usize,
) {
    assert_eq!(step, "foo");
    assert_eq!(num, 0);
    assert_eq!(ctx.value, "foo is 0");

    w.0.foo += 1;
}

#[given("{word} is not {word}")]
fn test_foo_is_not_bar_first(_w: FirstWorldMut, _foo: String, _bar: String) {}

#[when("I write \"{word}\" to '{word}'")]
fn test_write_first(_w: FirstWorldMut, _what: String, _filename: String) {}

#[then("the file {string} should contain {string}")]
fn test_read_first(w: FirstWorldRef, _inputs: &[String]) {
    let _ = w.world();
}

#[then("{string} contains {string}")]
fn test_read_slice_first(w: FirstWorldRef, _inputs: &[String]) {
    let _ = w.world();
}

#[given("{word} is {int}")]
#[when("{word} is {int}")]
async fn test_regex_async_second(
    w: SecondWorldMut,
    step: String,
    #[step] _ctx: &Step,
    num: usize,
) {
    assert_eq!(step, "foo");
    assert_eq!(num, 0);

    w.0.foo += 1;
}

#[given("{word} is sync {int}")]
fn test_regex_sync_slice_second(w: SecondWorldMut, step: &Step, matches: &[String]) {
    assert_eq!(matches[0], "foo");
    assert_eq!(matches[1].parse::<usize>().unwrap(), 0);
    assert_eq!(step.value, "foo is sync 0");

    w.0.foo += 1;
}

#[given("{word} is not {word}")]
fn test_foo_is_not_bar_second(_w: SecondWorldMut, _foo: String, _bar: String) {}

#[when("I write \"{word}\" to '{word}'")]
fn test_write_second(_w: SecondWorldMut, _what: String, _filename: String) {}

#[then("the file {string} should contain {string}")]
fn test_read_second(w: SecondWorldRef, _inputs: &[String]) {
    let _ = w.world();
}

#[then("{string} contains {string}")]
fn test_read_slice_second(w: SecondWorldRef, _inputs: &[String]) {
    let _ = w.world();
}


#[tokio::main]
async fn main() {
    let writer = FirstWorld::namako()
        .max_concurrent_scenarios(None)
        .with_writer(writer::Libtest::or_basic())
        .run("./tests/features")
        .await;

    assert_eq!(writer.failed_steps(), 0);

    let writer = SecondWorld::namako()
        .max_concurrent_scenarios(None)
        .with_writer(writer::Libtest::or_basic())
        .run("./tests/features")
        .await;

    assert_eq!(writer.failed_steps(), 0);
}
