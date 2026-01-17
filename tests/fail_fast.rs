use namako::{
    World as _,
    codegen::StepContext,
    runner, given, then, writer::summarize::Stats,
};

// Context wrapper types
struct WorldMut<'a>(&'a mut World);

#[derive(Clone, Copy)]
struct WorldRef<'a>(&'a World);

impl<'a> WorldMut<'a> {
    fn new(world: &'a mut World) -> Self { Self(world) }
    fn world(&mut self) -> &mut World { self.0 }
}
impl<'a> WorldRef<'a> {
    fn new(world: &'a World) -> Self { Self(world) }
    fn world(&self) -> &World { self.0 }
}
impl<'a> StepContext for WorldMut<'a> { type World = World; }
impl<'a> StepContext for WorldRef<'a> { type World = World; }

#[derive(Clone, Copy, Debug, Default, namako::World)]
#[world(mut_ctx = WorldMut<'a>, ref_ctx = WorldRef<'a>)]
struct World;

#[given("setup")]
fn given_setup(mut ctx: WorldMut) {
    let _ = ctx.world();
}

#[then("step panics")]
fn step_panics(ctx: WorldRef) {
    let _ = ctx.world(); // Verify context access
    panic!("this is a panic message");
}

#[then("nothing happens")]
fn nothing_happens(ctx: WorldRef) {
    let _ = ctx.world(); // Verify context access
}

#[tokio::test]
async fn correct_stats() {
    for (feat, (p_sc, f_sc, _r_sc, p_st, f_st, _r_st)) in [
        ("no_retry", (0, 1, 0, 0, 1, 0)),
    ] {
        let writer = World::namako()
            .with_runner(
                runner::Basic::default()
                    .steps(World::collection())
                    .max_concurrent_scenarios(1)
                    .fail_fast(),
            )
            .with_default_cli()
            .run(format!("tests/features/fail_fast/{feat}.feature"))
            .await;

        assert_eq!(
            *writer.scenarios_stats(),
            Stats { passed: p_sc, skipped: 0, failed: f_sc },
            "Wrong `Stats` for `Scenario`s in `{feat}`",
        );
        assert_eq!(
            *writer.steps_stats(),
            Stats { passed: p_st, skipped: 0, failed: f_st },
            "Wrong `Stats` for `Step`s in `{feat}`",
        );
    }
}
