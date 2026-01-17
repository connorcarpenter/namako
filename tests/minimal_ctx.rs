use namako::{
    codegen::{AssertOutcome, Assertable, StepContext},
    given,
};

// Context wrappers - pub(crate) to match World visibility
struct WorldMut<'a>(&'a mut World);

#[derive(Clone, Copy)]
struct WorldRef<'a>(&'a World);

impl<'a> WorldMut<'a> {
    fn new(world: &'a mut World) -> Self {
        Self(world)
    }
}

impl<'a> WorldRef<'a> {
    fn new(world: &'a World) -> Self {
        Self(world)
    }
}

impl<'a> StepContext for WorldMut<'a> {
    type World = World;
}

impl<'a> StepContext for WorldRef<'a> {
    type World = World;
}

impl Assertable for World {
    type Ctx<'a> = WorldRef<'a> where Self: 'a;

    fn assert_then<T, F>(&mut self, mut f: F) -> T
    where
        F: FnMut(&Self::Ctx<'_>) -> AssertOutcome<T>,
    {
        let ctx = WorldRef(self);
        match f(&ctx) {
            AssertOutcome::Passed(v) => v,
            AssertOutcome::Pending => panic!("Pending not supported"),
            AssertOutcome::Failed(msg) => panic!("{msg}"),
        }
    }
}

#[given("test")]
fn step(_: WorldMut) {}

#[derive(Clone, Copy, Debug, Default, namako::World)]
#[world(mut_ctx = WorldMut<'a>, ref_ctx = WorldRef<'a>)]
struct World;
