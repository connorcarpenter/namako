# Namako - AI Coding Instructions

Namako is a native Rust BDD testing framework supporting async tests (via `tokio`), Gherkin feature files, and compile-time step wiring.

## Big Picture Architecture

- **Core (`src/`)**: Implements the runner, event loop, parser (Gherkin), and reporting logic.
- **Codegen (`codegen/`)**: Procedural macros (`#[given]`, `#[when]`, `#[then]`, `#[derive(World)]`, `#[derive(Parameter)]`) for auto-wiring step definitions.
- **World Trait**: The central state container for test scenarios. Each scenario gets a fresh `World` instance.
- **Runner**: Orchestrates test execution. Configurable via `World::namako()`. Supports lifecycle hooks (`before`, `after`) and parallel execution.
- **Dependencies**: Heavily relies on `tokio` for async runtime, `gherkin` crate for parsing, and `inventory` for collecting step definitions from disparate modules.

## Critical Workflows

### Running Tests
- Use `cargo test` to run all tests, including Namako integration tests.
- Namako tests are typically defined in `tests/*.rs` files (integration tests) which configure a `World` and run against `.feature` files in `tests/features/`.

### Creating New Tests
1.  **Feature File**: Create `tests/features/my_feature.feature` using Gherkin syntax.
2.  **Step Definitions**: Create/update a `tests/*.rs` file (e.g., `tests/my_test.rs`) or a module within it.
3.  **Implement Steps**:
    ```rust
    // Async step with capture
    #[given("I have {int} cucumbers")]
    async fn init_cucumbers(world: &mut MyWorld, count: usize) {
        world.cucumbers = count;
    }
    
    // Sync step using Cucumber Expression
    #[when("I eat {int} cucumbers")]
    fn eat_cucumbers(world: &mut MyWorld, count: usize) {
        world.cucumbers -= count;
    }
    ```
4.  **Wire up Runner**:
    ```rust
    #[tokio::main]
    async fn main() {
        MyWorld::namako()
            .run_and_exit("tests/features/my_feature.feature")
            .await;
    }
    ```

## Conventions & Patterns

- **World Derivation**: Always use `#[derive(World)]` for the state struct to reduce boilerplate.
    ```rust
    #[derive(Debug, World)]
    #[world(init = Self::new)] // Optional custom init
    pub struct MyWorld { ... }
    ```
- **Step Attributes**: Use `given/when/then` attributes with string literals for matching.

- **Parameter Derivation**: Use `#[derive(Parameter)]` for custom types captured in steps.
    ```rust
    #[derive(Parameter, FromStr)]
    #[param(regex = "on|off")]
    enum Switch { On, Off }
    ```
- **Async First**: The framework is designed for `async`. Use `#[tokio::main]` for the test entry point.
- **Context Injection**: Use `#[step] ctx: &Step` in function arguments to access raw step data.

## Integration & Dependencies

- **Gherkin**: Steps match `.feature` files parsed by the `gherkin` crate.
- **CLI**: The standard runner allows integrating `clap` for custom CLI arguments via `.with_cli(opts)`.
- **Writers**: Use `.with_writer(writer::Libtest::or_basic())` for standard output formats (JSON, etc.).

## Key Files
- `src/lib.rs`: Exports the public API and `World` trait.
- `codegen/src/lib.rs`: Macros implementation.
- `tests/cli.rs`: Example of full runner configuration with CLI.
- `tests/features/`: Directory containing Gherkin specifications.
