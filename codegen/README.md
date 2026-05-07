# namako_codegen

Procedural macros for Namako step bindings.

## Usage

Define a world type to hold shared scenario state, declare context wrappers for
mutable (`given`/`when`) and read-only (`then`) access, then write step functions:

```rust
use namako_engine::{World, given, when, then};

/// Holds state shared across all steps in a scenario.
#[derive(Debug, Default, World)]
#[world(mut_ctx = MyWorldMut<'a>, ref_ctx = MyWorldRef<'a>)]
pub struct MyWorld {
    pub value: i32,
}

/// Mutable context — given/when steps receive this.
pub struct MyWorldMut<'a>(&'a mut MyWorld);
impl<'a> MyWorldMut<'a> {
    fn new(world: &'a mut MyWorld) -> Self { Self(world) }
}

/// Read-only context — then steps receive this.
#[derive(Clone, Copy)]
pub struct MyWorldRef<'a>(&'a MyWorld);
impl<'a> MyWorldRef<'a> {
    fn new(world: &'a MyWorld) -> Self { Self(world) }
}

#[given("the value is {int}")]
fn set_value(ctx: MyWorldMut, n: i32) {
    ctx.0.value = n;
}

#[when("the value is doubled")]
fn double_value(ctx: MyWorldMut) {
    ctx.0.value *= 2;
}

#[then("the value is {int}")]
fn check_value(ctx: MyWorldRef, expected: i32) {
    assert_eq!(ctx.0.value, expected);
}
```

## Testing

```bash
cargo test -p namako_codegen
```
