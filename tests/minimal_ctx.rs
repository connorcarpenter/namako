mod test_utils;

use namako::{given, then};

#[allow(unused_imports)]
use test_utils::World; // Required for step macro expansion
use test_utils::{WorldMut, WorldRef};

#[given("test")]
fn step(mut ctx: WorldMut) {
    let _ = ctx.world(); // Verify context access works
}

#[then("verified")]
fn then_step(ctx: WorldRef) {
    let _ = ctx.world(); // Verify ref context access works
}
