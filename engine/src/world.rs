use std::fmt::Display;
#[cfg(feature = "macros")]
use std::{fmt::Debug, path::Path};

#[cfg(feature = "macros")]
use crate::{
    codegen::{self, StepConstructor as _, WorldInventory},
    namako::DefaultNamako,
    step,
};

/// Represents a shared user-defined state for a [Namako] run.
/// It lives on per-[scenario][0] basis.
///
/// This crate doesn't provide out-of-box solution for managing state shared
/// across [scenarios][0], because we want some friction there to avoid tests
/// being dependent on each other. If your workflow needs a way to share state
/// between [scenarios][0] (ex. database connection pool), we recommend using
/// a [`std::sync::LazyLock`] or organize it other way via [shared state][1].
///
/// [0]: https://cucumber.io/docs/gherkin/reference#descriptions
/// [1]: https://doc.rust-lang.org/book/ch16-03-shared-state.html
/// [Namako]: https://cucumber.io
pub trait World: Sized + 'static {
    /// Error of creating a new [`World`] instance.
    type Error: Display;

    /// Mutable context type for Given/When steps.
    ///
    /// This context provides mutable access to test state and MUST only
    /// expose mutation operations (no assertions/expects).
    type MutCtx<'a>
    where
        Self: 'a;

    /// Context type for Then steps (read/assertion API).
    ///
    /// This context provides read/assertion access and MUST only expose
    /// assertion/expect operations (no mutation API exposed to step authors).
    type RefCtx<'a>
    where
        Self: 'a;

    /// Creates a new [`World`] instance.
    fn new() -> impl Future<Output = Result<Self, Self::Error>>;

    /// Creates a mutable context for Given/When steps.
    ///
    /// The returned context provides mutable access and MUST only expose
    /// mutation operations. This enforces capability separation at compile time.
    fn ctx_mut(&mut self) -> Self::MutCtx<'_>;

    /// Creates a read-only context for Then steps.
    ///
    /// The returned context provides read/assertion access and MUST only expose
    /// assertion/expect operations (no mutation API). Note: takes `&mut self`
    /// because expect operations may need to tick/advance simulation internally.
    fn ctx_ref(&mut self) -> Self::RefCtx<'_>;

    /// Execute a Then-step assertion with polling.
    ///
    /// The default implementation polls with retry on `Pending`:
    /// - Calls `f()` with a fresh `RefCtx`
    /// - On `Passed(v)`: returns `v`
    /// - On `Pending`: sleeps briefly, retries (up to 100 attempts)
    /// - On `Failed(msg)`: panics with the message
    ///
    /// Override this method for custom polling semantics (e.g., tick-based
    /// simulation where you call `scenario.tick()` between attempts).
    ///
    /// # Panics
    ///
    /// Panics if the assertion fails or times out after 100 attempts.
    fn assert_then<T, F>(&mut self, mut f: F) -> T
    where
        F: FnMut(&Self::RefCtx<'_>) -> codegen::AssertOutcome<T>,
    {
        const MAX_ATTEMPTS: usize = 100;
        const POLL_INTERVAL_MS: u64 = 10;

        for _attempt in 0..MAX_ATTEMPTS {
            let ctx = self.ctx_ref();
            match f(&ctx) {
                codegen::AssertOutcome::Passed(v) => return v,
                codegen::AssertOutcome::Pending => {
                    std::thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));
                }
                codegen::AssertOutcome::Failed(msg) => panic!("{msg}"),
            }
        }
        panic!(
            "Then assertion timed out after {} attempts ({}ms total)",
            MAX_ATTEMPTS,
            MAX_ATTEMPTS as u64 * POLL_INTERVAL_MS
        );
    }

    #[cfg(feature = "macros")]
    /// Returns runner for tests with auto-wired steps marked by [`given`],
    /// [`when`] and [`then`] attributes.
    #[must_use]
    fn collection() -> step::Collection<Self>
    where
        Self: Debug + WorldInventory,
    {
        let mut out = step::Collection::new();

        for given in inventory::iter::<Self::Given> {
            let (loc, regex, fun) = given.inner();
            out = out.given(Some(loc), regex(), fun);
        }

        for when in inventory::iter::<Self::When> {
            let (loc, regex, fun) = when.inner();
            out = out.when(Some(loc), regex(), fun);
        }

        for then in inventory::iter::<Self::Then> {
            let (loc, regex, fun) = then.inner();
            out = out.then(Some(loc), regex(), fun);
        }

        out
    }

    #[cfg(feature = "macros")]
    /// Returns default [`Namako`] with all the auto-wired [`Step`]s.
    #[must_use]
    fn namako<I: AsRef<Path>>() -> DefaultNamako<Self, I>
    where
        Self: Debug + WorldInventory,
    {
        crate::Namako::new().steps(Self::collection())
    }

    #[cfg(feature = "macros")]
    /// Runs [`Namako`].
    ///
    /// [`Feature`]s sourced by [`Parser`] are fed into [`Runner`] where the
    /// later produces events handled by [`Writer`].
    ///
    /// # Panics
    ///
    /// If encountered errors while parsing [`Feature`]s or at least one
    /// [`Step`] panicked.
    ///
    /// [`Feature`]: gherkin::Feature
    fn run<I: AsRef<Path>>(input: I) -> impl Future<Output = ()>
    where
        Self: Debug + WorldInventory,
    {
        Self::namako().run_and_exit(input)
    }

    #[cfg(feature = "macros")]
    /// Runs [`Namako`] with [`Scenario`]s filter.
    ///
    /// [`Feature`]s sourced by [`Parser`] are fed into [`Runner`] where the
    /// later produces events handled by [`Writer`].
    ///
    /// # Panics
    ///
    /// If encountered errors while parsing [`Feature`]s or at least one
    /// [`Step`] panicked.
    ///
    /// [`Feature`]: gherkin::Feature
    /// [`Scenario`]: gherkin::Scenario
    /// [`Step`]: gherkin::Step
    fn filter_run<I, F>(input: I, filter: F) -> impl Future<Output = ()>
    where
        Self: Debug + WorldInventory,
        I: AsRef<Path>,
        F: Fn(&gherkin::Feature, Option<&gherkin::Rule>, &gherkin::Scenario) -> bool + 'static,
    {
        Self::namako().filter_run_and_exit(input, filter)
    }
}
