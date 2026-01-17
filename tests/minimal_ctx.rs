use namako::{
    codegen::StepContext,
    given, then,
};

// Context wrappers - pub(crate) to match World visibility
struct WorldMut<'a>(&'a mut World);

#[derive(Clone, Copy)]
struct WorldRef<'a>(&'a World);

impl<'a> WorldMut<'a> {
    fn new(world: &'a mut World) -> Self {
        Self(world)
    }
    fn world(&mut self) -> &mut World {
        self.0
    }
}

impl<'a> WorldRef<'a> {
    fn new(world: &'a World) -> Self {
        Self(world)
    }
    fn world(&self) -> &World {
        self.0
    }
}

impl<'a> StepContext for WorldMut<'a> {
    type World = World;
}

impl<'a> StepContext for WorldRef<'a> {
    type World = World;
}

#[given("test")]
fn step(mut ctx: WorldMut) {
    let _ = ctx.world(); // Verify context access works
}

#[then("verified")]
fn then_step(ctx: WorldRef) {
    let _ = ctx.world(); // Verify ref context access works
}

#[derive(Clone, Copy, Debug, Default, namako::World)]
#[world(mut_ctx = WorldMut<'a>, ref_ctx = WorldRef<'a>)]
struct World;
