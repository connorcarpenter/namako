mod test_utils;

use namako_engine::{
    World as _,
    runner, given, then, writer::summarize::Stats,
};

use test_utils::{World, WorldMut, WorldRef};

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
