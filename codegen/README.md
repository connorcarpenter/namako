# namako_codegen

Procedural macros for Namako step bindings.

## Usage

```rust,ignore
use namako_codegen::{given, when, then};

#[given("a precondition")]
fn setup(mut ctx: WorldMut) {
    // ...
}

#[when("an action occurs")]
fn action(mut ctx: WorldMut) {
    // ...
}

#[then("an outcome is observed")]
fn verify(ctx: WorldRef) {
    assert!(ctx.world().result.is_ok());
}
```

`WorldMut` and `WorldRef` are the associated context types produced by
`#[derive(World)]` on your world struct. `given` and `when` steps receive
mutable context; `then` steps receive shared (read-only) context.

## Testing

```bash
cargo test -p namako_codegen
```
