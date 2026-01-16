

//! [`Writer`]-wrapper for collecting a summary of execution.

use std::{borrow::Cow, collections::HashMap};

use derive_more::with_trait::Deref;
use itertools::Itertools as _;

use crate::{
    Event, World, Writer,
    cli::Colored,
    event::{self, Source},
    parser,
    writer::{self, out::Styles},
};

/// Execution statistics.
///
/// [`Step`]: gherkin::Step
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Stats {
    /// Number of passed [`Step`]s (or [`Scenario`]s).
    ///
    /// [`Scenario`]: gherkin::Scenario
    /// [`Step`]: gherkin::Step
    pub passed: usize,

    /// Number of skipped [`Step`]s (or [`Scenario`]s).
    ///
    /// [`Scenario`]: gherkin::Scenario
    /// [`Step`]: gherkin::Step
    pub skipped: usize,

    /// Number of failed [`Step`]s (or [`Scenario`]s).
    ///
    /// [`Scenario`]: gherkin::Scenario
    /// [`Step`]: gherkin::Step
    pub failed: usize,
}

impl Stats {
    /// Returns total number of [`Step`]s (or [`Scenario`]s), these [`Stats`]
    /// have been collected for.
    ///
    /// [`Scenario`]: gherkin::Scenario
    /// [`Step`]: gherkin::Step
    #[must_use]
    pub const fn total(&self) -> usize {
        self.passed + self.skipped + self.failed
    }
}

/// Alias for [`fn`] used to determine should [`Skipped`] test considered as
/// [`Failed`] or not.
///
/// [`Failed`]: event::Step::Failed
/// [`Skipped`]: event::Step::Skipped
pub type SkipFn =
    fn(&gherkin::Feature, Option<&gherkin::Rule>, &gherkin::Scenario) -> bool;

/// Indicator of a [`Failed`], [`Skipped`] or retried [`Scenario`].
///
/// [`Failed`]: event::Step::Failed
/// [`Scenario`]: gherkin::Scenario
/// [`Skipped`]: event::Step::Skipped
#[derive(Clone, Copy, Debug)]
enum Indicator {
    /// [`Failed`] [`Scenario`].
    ///
    /// [`Failed`]: event::Step::Failed
    /// [`Scenario`]: gherkin::Scenario
    Failed,

    /// [`Skipped`] [`Scenario`].
    ///
    /// [`Scenario`]: gherkin::Scenario
    /// [`Skipped`]: event::Step::Skipped
    Skipped,
}

/// Possible states of a [`Summarize`] [`Writer`].
#[derive(Clone, Copy, Debug)]
enum State {
    /// [`Finished`] event hasn't been encountered yet.
    ///
    /// [`Finished`]: event::Namako::Finished
    InProgress,

    /// [`Finished`] event was encountered, but summary hasn't been output yet.
    ///
    /// [`Finished`]: event::Namako::Finished
    FinishedButNotOutput,

    /// [`Finished`] event was encountered and summary was output.
    ///
    /// [`Finished`]: event::Namako::Finished
    FinishedAndOutput,
}

/// Wrapper for a [`Writer`] for outputting an execution summary (number of
/// executed features, scenarios, steps and parsing errors).
///
/// Underlying [`Writer`] has to be [`Summarizable`] and [`ArbitraryWriter`]
/// with `Value` accepting [`String`]. If your underlying [`ArbitraryWriter`]
/// operates with something like JSON (or any other type), you should implement
/// a [`Writer`] on [`Summarize`] by yourself, to provide the required summary
/// format.
///
/// [`ArbitraryWriter`]: writer::Arbitrary
#[derive(Clone, Debug, Deref)]
pub struct Summarize<Writer> {
    /// Original [`Writer`] to summarize output of.
    #[deref]
    writer: Writer,

    /// Number of started [`Feature`]s.
    ///
    /// [`Feature`]: gherkin::Feature
    features: usize,

    /// Number of started [`Rule`]s.
    ///
    /// [`Rule`]: gherkin::Rule
    rules: usize,

    /// [`Scenario`]s [`Stats`].
    ///
    /// [`Scenario`]: gherkin::Scenario
    scenarios: Stats,

    /// [`Step`]s [`Stats`].
    ///
    /// [`Step`]: gherkin::Step
    steps: Stats,

    /// Number of [`Parser`] errors.
    ///
    /// [`Parser`]: crate::Parser
    parsing_errors: usize,



    /// Current [`State`] of this [`Writer`].
    state: State,

    /// Handled [`Scenario`]s to collect [`Stats`].
    ///
    /// [`Scenario`]: gherkin::Scenario
    handled_scenarios: HandledScenarios,
}

/// [`HashMap`] for keeping track of handled [`Scenario`]s. Whole path with
/// [`Feature`] and [`Rule`] is used to avoid collisions in case [`Scenario`]s
/// themself look identical.
///
/// [`Feature`]: gherkin::Feature
/// [`Rule`]: gherkin::Rule
/// [`Scenario`]: gherkin::Scenario
type HandledScenarios = HashMap<
    (
        Source<gherkin::Feature>,
        Option<Source<gherkin::Rule>>,
        Source<gherkin::Scenario>,
    ),
    Indicator,
>;

impl<W, Wr> Writer<W> for Summarize<Wr>
where
    W: World,
    Wr: writer::Arbitrary<W, String> + Summarizable,
    Wr::Cli: Colored,
{
    type Cli = Wr::Cli;

    async fn handle_event(
        &mut self,
        event: parser::Result<Event<event::Namako<W>>>,
        cli: &Self::Cli,
    ) {
        use event::{Namako, Feature, Rule};

        // Once `Namako::Finished` is emitted, we just pass events through,
        // without collecting `Stats`.
        // This is done to avoid miscalculations if this `Writer` happens to be
        // wrapped by a `writer::Repeat` or similar.
        if matches!(self.state, State::InProgress) {
            match event.as_deref() {
                Err(_) => self.parsing_errors += 1,
                Ok(Namako::Feature(feat, ev)) => match ev {
                    Feature::Started => self.features += 1,
                    Feature::Rule(_, Rule::Started) => {
                        self.rules += 1;
                    }
                    Feature::Rule(rule, Rule::Scenario(sc, ev)) => {
                        self.handle_scenario(
                            feat.clone(),
                            Some(rule.clone()),
                            sc.clone(),
                            ev,
                        );
                    }
                    Feature::Scenario(sc, ev) => {
                        self.handle_scenario(
                            feat.clone(),
                            None,
                            sc.clone(),
                            ev,
                        );
                    }
                    Feature::Finished | Feature::Rule(..) => {}
                },
                Ok(Namako::Finished) => {
                    self.state = State::FinishedButNotOutput;
                }
                Ok(Namako::Started | Namako::ParsingFinished { .. }) => {}
            }
        }

        self.writer.handle_event(event, cli).await;

        if matches!(self.state, State::FinishedButNotOutput) {
            self.state = State::FinishedAndOutput;

            let mut styles = Styles::new();
            styles.apply_coloring(cli.coloring());
            self.writer.write(styles.summary(self)).await;
        }
    }
}

#[warn(clippy::missing_trait_methods)]
impl<W, Wr, Val> writer::Arbitrary<W, Val> for Summarize<Wr>
where
    W: World,
    Self: Writer<W>,
    Wr: writer::Arbitrary<W, Val>,
{
    async fn write(&mut self, val: Val) {
        self.writer.write(val).await;
    }
}

impl<W, Wr> writer::Stats<W> for Summarize<Wr>
where
    W: World,
    Self: Writer<W>,
{
    fn passed_steps(&self) -> usize {
        self.steps.passed
    }

    fn skipped_steps(&self) -> usize {
        self.steps.skipped
    }

    fn failed_steps(&self) -> usize {
        self.steps.failed
    }

    fn parsing_errors(&self) -> usize {
        self.parsing_errors
    }
}

#[warn(clippy::missing_trait_methods)]
impl<Wr: writer::Normalized> writer::Normalized for Summarize<Wr> {}

#[warn(clippy::missing_trait_methods)]
impl<Wr: writer::NonTransforming> writer::NonTransforming for Summarize<Wr> {}

impl<Writer> From<Writer> for Summarize<Writer> {
    fn from(writer: Writer) -> Self {
        Self {
            writer,
            features: 0,
            rules: 0,
            scenarios: Stats { passed: 0, skipped: 0, failed: 0 },
            steps: Stats { passed: 0, skipped: 0, failed: 0 },
            parsing_errors: 0,
            state: State::InProgress,
            handled_scenarios: HashMap::new(),
        }
    }
}

impl<Writer> Summarize<Writer> {
    /// Keeps track of [`Step`]'s [`Stats`].
    ///
    /// [`Step`]: gherkin::Step
    fn handle_step<W>(
        &mut self,
        feature: Source<gherkin::Feature>,
        rule: Option<Source<gherkin::Rule>>,
        scenario: Source<gherkin::Scenario>,
        step: &gherkin::Step,
        ev: &event::Step<W>,
    ) {
        use self::{
            Indicator::{Failed, Skipped},
            event::Step,
        };

        match ev {
            Step::Started => {}
            Step::Passed(..) => {
                self.steps.passed += 1;
                if scenario.steps.last().filter(|s| *s == step).is_some() {
                    _ = self
                        .handled_scenarios
                        .remove(&(feature, rule, scenario));
                }
            }
            Step::Skipped => {
                self.steps.skipped += 1;
                self.scenarios.skipped += 1;
                _ = self
                    .handled_scenarios
                    .insert((feature, rule, scenario), Skipped);
            }
            Step::Failed(_, _, _, _err) => {
                self.steps.failed += 1;
                self.scenarios.failed += 1;

                _ = self
                    .handled_scenarios
                    .insert((feature, rule, scenario), Failed);
            }
        }
    }

    /// Keeps track of [`Scenario`]'s [`Stats`].
    ///
    /// [`Scenario`]: gherkin::Scenario
    fn handle_scenario<W>(
        &mut self,
        feature: Source<gherkin::Feature>,
        rule: Option<Source<gherkin::Rule>>,
        scenario: Source<gherkin::Scenario>,
        ev: &event::Scenario<W>,
    ) {
        use event::Scenario;

        let path = (feature, rule, scenario);

        match ev {
            Scenario::Started
            | Scenario::Log(_) => {}
            Scenario::Background(st, ev) | Scenario::Step(st, ev) => {
                self.handle_step(path.0, path.1, path.2, st.as_ref(), ev);
            }
            Scenario::Finished => {
                if !self.handled_scenarios.contains_key(&path) {
                    self.scenarios.passed += 1;
                }
            }
        }
    }
}

impl<Writer> Summarize<Writer> {
    /// Wraps the given [`Writer`] into a new [`Summarize`]d one.
    #[must_use]
    pub fn new(writer: Writer) -> Self {
        Self::from(writer)
    }

    /// Returns the original [`Writer`], wrapped by this [`Summarize`]d one.
    #[must_use]
    pub const fn inner_writer(&self) -> &Writer {
        &self.writer
    }

    /// Returns collected [`Scenario`]s [`Stats`] of this [`Summarize`]d
    /// [`Writer`].
    ///
    /// [`Scenario`]: gherkin::Scenario
    #[must_use]
    pub const fn scenarios_stats(&self) -> &Stats {
        &self.scenarios
    }

    /// Returns collected [`Step`]s [`Stats`] of this [`Summarize`]d [`Writer`].
    ///
    /// [`Step`]: gherkin::Step
    #[must_use]
    pub const fn steps_stats(&self) -> &Stats {
        &self.steps
    }
}

/// Marker indicating that a [`Writer`] can be wrapped into a [`Summarize`].
///
/// Not any [`Writer`] can be wrapped into a [`Summarize`], as it may transform
/// events inside and the summary won't reflect outputted events correctly.
///
/// So, this trait ensures that a wrong [`Writer`]s pipeline cannot be build.
///
/// # Example
///
/// ```rust,compile_fail
/// # use namako::{writer, World, WriterExt as _};
/// #
/// # #[derive(Debug, Default, World)]
/// # struct MyWorld;
/// #
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// MyWorld::namako()
///     .with_writer(
///         // `Writer`s pipeline is constructed in a reversed order.
///         writer::Basic::stdout()
///             .fail_on_skipped() // Fails as `Summarize` will count skipped
///             .summarized()      // steps instead of failed.
///     )
///     .run_and_exit("tests/features/readme")
///     .await;
/// # }
/// ```
///
/// ```rust
/// # use std::panic::AssertUnwindSafe;
/// #
/// # use namako::{writer, World, WriterExt as _};
/// # use futures::FutureExt as _;
/// #
/// # #[derive(Debug, Default, World)]
/// # struct MyWorld;
/// #
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// # let fut = async {
/// MyWorld::namako()
///     .with_writer(
///         // `Writer`s pipeline is constructed in a reversed order.
///         writer::Basic::stdout() // And, finally, print them.
///             .summarized()       // Only then, count summary for them.
///             .fail_on_skipped(), // First, transform skipped steps to failed.
///     )
///     .run_and_exit("tests/features/readme")
///     .await;
/// # };
/// # let err = AssertUnwindSafe(fut)
/// #         .catch_unwind()
/// #         .await
/// #         .expect_err("should err");
/// # let err = err.downcast_ref::<String>().unwrap();
/// # assert_eq!(err, "1 step failed");
/// # }
/// ```
pub trait Summarizable {}

impl<T: writer::NonTransforming> Summarizable for T {}

#[expect( // related to summarization only
    clippy::multiple_inherent_impl,
    reason = "related to summarization only"
)]
impl Styles {
    /// Generates a formatted summary [`String`].
    #[must_use]
    pub fn summary<W>(&self, summary: &Summarize<W>) -> String {
        let features = self.maybe_plural("feature", summary.features);

        let rules = if summary.rules > 0 {
            format!("{}\n", self.maybe_plural("rule", summary.rules))
        } else {
            String::new()
        };

        let scenarios =
            self.maybe_plural("scenario", summary.scenarios.total());
        let scenarios_stats = self.format_stats(summary.scenarios);

        let steps = self.maybe_plural("step", summary.steps.total());
        let steps_stats = self.format_stats(summary.steps);

        let parsing_errors = if summary.parsing_errors > 0 {
            self.err(self.maybe_plural("parsing error", summary.parsing_errors))
        } else {
            "".into()
        };

        format!(
            "{summary}\n{features}\n{rules}{scenarios}{scenarios_stats}\n\
             {steps}{steps_stats}\n{parsing_errors}",
            summary = self.bold(self.header("[Summary]")),
        )
        .trim_end_matches('\n')
        .to_owned()
    }

    /// Formats [`Stats`] for a terminal output.
    #[must_use]
    pub fn format_stats(&self, stats: Stats) -> Cow<'static, str> {
        let formatted = [
            if stats.passed > 0 {
                self.bold(self.ok(format!("{} passed", stats.passed)))
            } else {
                "".into()
            },
            if stats.skipped > 0 {
                self.bold(self.skipped(format!("{} skipped", stats.skipped)))
            } else {
                "".into()
            },
            if stats.failed > 0 {
                self.bold(self.err(format!("{} failed", stats.failed)))
            } else {
                "".into()
            },
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .join(&self.bold(", "));

        if formatted.is_empty() {
            "".into()
        } else {
            self.bold(format!(
                " {}{formatted}{}",
                self.bold("("),
                self.bold(")"),
            ))
        }
    }

    /// Adds `s` to `singular` if the given `num` is not `1`.
    fn maybe_plural(
        &self,
        singular: impl Into<Cow<'static, str>>,
        num: usize,
    ) -> Cow<'static, str> {
        self.bold(format!(
            "{num} {}{}",
            singular.into(),
            if num == 1 { "" } else { "s" },
        ))
    }
}
