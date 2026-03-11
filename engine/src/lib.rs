#![doc(
    html_logo_url = "https://avatars.githubusercontent.com/u/91469139?s=128",
    html_favicon_url = "https://avatars.githubusercontent.com/u/91469139?s=256"
)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(any(doc, test), doc = include_str!("../README.md"))]
#![cfg_attr(not(any(doc, test)), doc = env!("CARGO_PKG_NAME"))]
#![deny(nonstandard_style, rustdoc::all, trivial_casts, trivial_numeric_casts)]
#![forbid(non_ascii_idents, unsafe_code)]
#![warn(
    clippy::absolute_paths,
    clippy::allow_attributes,
    clippy::allow_attributes_without_reason,
    clippy::as_conversions,
    clippy::as_pointer_underscore,
    clippy::as_ptr_cast_mut,
    clippy::assertions_on_result_states,
    clippy::branches_sharing_code,
    clippy::cfg_not_test,
    clippy::clear_with_drain,
    clippy::clone_on_ref_ptr,
    clippy::coerce_container_to_any,
    clippy::collection_is_never_read,
    clippy::create_dir,
    clippy::dbg_macro,
    clippy::debug_assert_with_mut_call,
    clippy::decimal_literal_representation,
    clippy::default_union_representation,
    clippy::derive_partial_eq_without_eq,
    clippy::doc_include_without_cfg,
    clippy::empty_drop,
    clippy::empty_structs_with_brackets,
    clippy::equatable_if_let,
    clippy::empty_enum_variants_with_brackets,
    clippy::exit,
    clippy::expect_used,
    clippy::fallible_impl_from,
    clippy::filetype_is_file,
    clippy::float_cmp_const,
    clippy::fn_to_numeric_cast_any,
    clippy::get_unwrap,
    clippy::if_then_some_else_none,
    clippy::imprecise_flops,
    clippy::infinite_loop,
    clippy::iter_on_empty_collections,
    clippy::iter_on_single_items,
    clippy::iter_over_hash_type,
    clippy::iter_with_drain,
    clippy::large_include_file,
    clippy::large_stack_frames,
    clippy::let_underscore_untyped,
    clippy::literal_string_with_formatting_args,
    clippy::lossy_float_literal,
    clippy::map_err_ignore,
    clippy::map_with_unused_argument_over_ranges,
    clippy::mem_forget,
    clippy::missing_assert_message,
    clippy::missing_asserts_for_indexing,
    clippy::missing_const_for_fn,
    clippy::missing_docs_in_private_items,
    clippy::module_name_repetitions,
    clippy::multiple_inherent_impl,
    clippy::multiple_unsafe_ops_per_block,
    clippy::mutex_atomic,
    clippy::mutex_integer,
    clippy::needless_collect,
    clippy::needless_pass_by_ref_mut,
    clippy::needless_raw_strings,
    clippy::non_zero_suggestions,
    clippy::nonstandard_macro_braces,
    clippy::option_if_let_else,
    clippy::or_fun_call,
    clippy::panic_in_result_fn,
    clippy::partial_pub_fields,
    clippy::pathbuf_init_then_push,
    clippy::pedantic,
    clippy::precedence_bits,
    clippy::print_stderr,
    clippy::print_stdout,
    clippy::pub_without_shorthand,
    clippy::rc_buffer,
    clippy::rc_mutex,
    clippy::read_zero_byte_vec,
    clippy::redundant_clone,
    clippy::redundant_test_prefix,
    clippy::redundant_type_annotations,
    clippy::renamed_function_params,
    clippy::ref_patterns,
    clippy::rest_pat_in_fully_bound_structs,
    clippy::return_and_then,
    clippy::same_name_method,
    clippy::semicolon_inside_block,
    clippy::set_contains_or_insert,
    clippy::shadow_unrelated,
    clippy::significant_drop_in_scrutinee,
    clippy::significant_drop_tightening,
    clippy::single_option_map,
    clippy::str_to_string,
    clippy::string_add,
    clippy::string_lit_as_bytes,
    clippy::string_lit_chars_any,
    clippy::string_slice,
    clippy::suboptimal_flops,
    clippy::suspicious_operation_groupings,
    clippy::suspicious_xor_used_as_pow,
    clippy::tests_outside_test_module,
    clippy::todo,
    clippy::too_long_first_doc_paragraph,
    clippy::trailing_empty_array,
    clippy::transmute_undefined_repr,
    clippy::trivial_regex,
    clippy::try_err,
    clippy::undocumented_unsafe_blocks,
    clippy::unimplemented,
    clippy::uninhabited_references,
    clippy::unnecessary_safety_comment,
    clippy::unnecessary_safety_doc,
    clippy::unnecessary_self_imports,
    clippy::unnecessary_struct_initialization,
    clippy::unused_peekable,
    clippy::unused_result_ok,
    clippy::unused_trait_names,
    clippy::unwrap_in_result,
    clippy::unwrap_used,
    clippy::use_debug,
    clippy::use_self,
    clippy::useless_let_if_seq,
    clippy::verbose_file_reads,
    clippy::volatile_composites,
    clippy::while_float,
    clippy::wildcard_enum_match_arm,
    ambiguous_negative_literals,
    closure_returning_async_block,
    future_incompatible,
    impl_trait_redundant_captures,
    let_underscore_drop,
    macro_use_extern_crate,
    meta_variable_misuse,
    missing_debug_implementations,
    redundant_lifetimes,
    rust_2018_idioms,
    single_use_lifetimes,
    unit_bindings,
    unnameable_types,
    unreachable_pub,
    unstable_features,
    unused,
    variant_size_differences
)]

pub mod cli;
pub mod event;
pub mod feature;
pub(crate) mod future;
mod namako;
pub mod parser;
pub mod runner;
pub mod step;
pub mod tag;
pub mod writer;

#[cfg(feature = "macros")]
pub mod codegen;
#[cfg(feature = "npap")]
pub mod engine;
#[cfg(feature = "npap")]
pub mod id_tags;
#[cfg(feature = "npap")]
pub mod npap;
#[cfg(feature = "tracing")]
pub mod tracing;

// TODO: Remove once tests run without complains about it.
#[cfg(test)]
mod only_used_in_doc_tests_and_book {
    use rand as _;
    use tempfile as _;
    use tokio as _;
}

use std::fmt::Display;
#[cfg(feature = "macros")]
use std::{fmt::Debug, path::Path};

pub use gherkin;
#[cfg(feature = "macros")]
#[doc(inline)]
pub use namako_codegen::{Parameter, World, given, then, when};

#[cfg(feature = "macros")]
#[doc(inline)]
pub use self::codegen::Parameter;
#[cfg(feature = "macros")]
use self::{
    codegen::{StepConstructor as _, WorldInventory},
    namako::DefaultNamako,
};
#[doc(inline)]
pub use self::{
    event::Event,
    namako::Namako,
    parser::Parser,
    runner::{Runner, ScenarioType},
    step::Step,
    writer::{Arbitrary as ArbitraryWriter, Ext as WriterExt, Stats as StatsWriter, Writer},
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
        Namako::new().steps(Self::collection())
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
