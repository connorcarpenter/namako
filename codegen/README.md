# namako_codegen

Procedural macros for Namako step bindings.

## Usage

```rust
use namako_codegen::{given, when, then};

#[given("a precondition")]
async fn setup(world: &mut MyWorld) {
    // ...
}

#[when("an action occurs")]
async fn action(world: &mut MyWorld) {
    // ...
}

#[then("an outcome is observed")]
async fn verify(world: &mut MyWorld) {
    assert!(world.result.is_ok());
}
```

## Testing

```bash
cargo test -p namako_codegen
```
