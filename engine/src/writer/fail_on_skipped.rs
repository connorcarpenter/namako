

//! [`Writer`]-wrapper for transforming [`Skipped`] [`Step`]s into [`Failed`].
//!
//! [`Failed`]: event::Step::Failed
//! [`Skipped`]: event::Step::Skipped
//! [`Step`]: gherkin::Step

use derive_more::with_trait::Deref;

use crate::{
    Event, World, Writer,
    event::{self, Source},
    parser, writer,
};

/// [`Writer`]-wrapper for transforming [`Skipped`] [`Step`]s into [`Failed`].
///
/// [`Failed`]: event::Step::Failed
/// [`Skipped`]: event::Step::Skipped
/// [`Step`]: gherkin::Step
#[derive(Clone, Copy, Debug, Deref)]
pub struct FailOnSkipped<W, F = SkipFn> {
    /// Original [`Writer`] to pass transformed event into.
    #[deref]
    writer: W,

    /// [`Fn`] to determine whether [`Skipped`] test should be considered as
    /// [`Failed`] or not.
    ///
    /// [`Failed`]: event::Step::Failed
    /// [`Skipped`]: event::Step::Skipped
    should_fail: F,
}

/// Alias for a [`fn`] used to determine whether [`Skipped`] test should be
/// considered as [`Failed`] or not.
///
/// [`Failed`]: event::Step::Failed
/// [`Skipped`]: event::Step::Skipped
pub type SkipFn =
    fn(&gherkin::Feature, Option<&gherkin::Rule>, &gherkin::Scenario) -> bool;

impl<W, Wr, F> Writer<W> for FailOnSkipped<Wr, F>
where
    W: World,
    F: Fn(
        &gherkin::Feature,
        Option<&gherkin::Rule>,
        &gherkin::Scenario,
    ) -> bool,
    Wr: Writer<W>,
{
    type Cli = Wr::Cli;

    async fn handle_event(
        &mut self,
        event: parser::Result<Event<event::Namako<W>>>,
        cli: &Self::Cli,
    ) {
        use event::{
            Namako, Feature, Rule, Scenario, Step,
            StepError::NotFound,
        };

        let map_failed = |f: &Source<_>, r: &Option<_>, sc: &Source<_>| {
            if (self.should_fail)(f, r.as_deref(), sc) {
                Step::Failed(None, None, None, NotFound)
            } else {
                Step::Skipped
            }
        };
        let map_failed_bg =
            |f: Source<_>, r: Option<_>, sc: Source<_>, st: _| {
                let ev = map_failed(&f, &r, &sc);
                let ev = Scenario::Background(st, ev);
                Namako::scenario(f, r, sc, ev)
            };
        let map_failed_step =
            |f: Source<_>, r: Option<_>, sc: Source<_>, st: _| {
                let ev = map_failed(&f, &r, &sc);
                let ev = Scenario::Step(st, ev);
                Namako::scenario(f, r, sc, ev)
            };

        let event = event.map(|outer| {
            outer.map(|ev| match ev {
                Namako::Feature(
                    f,
                    Feature::Rule(
                        r,
                        Rule::Scenario(
                            sc,
                            Scenario::Background(st, Step::Skipped),
                        ),
                    ),
                ) => map_failed_bg(f, Some(r), sc, st),
                Namako::Feature(
                    f,
                    Feature::Scenario(
                        sc,
                        Scenario::Background(st, Step::Skipped),
                    ),
                ) => map_failed_bg(f, None, sc, st),
                Namako::Feature(
                    f,
                    Feature::Rule(
                        r,
                        Rule::Scenario(
                            sc,
                            Scenario::Step(st, Step::Skipped),
                        ),
                    ),
                ) => map_failed_step(f, Some(r), sc, st),
                Namako::Feature(
                    f,
                    Feature::Scenario(
                        sc,
                        Scenario::Step(st, Step::Skipped),
                    ),
                ) => map_failed_step(f, None, sc, st),
                Namako::Started
                | Namako::Feature(..)
                | Namako::ParsingFinished { .. }
                | Namako::Finished => ev,
            })
        });

        self.writer.handle_event(event, cli).await;
    }
}

#[warn(clippy::missing_trait_methods)]
impl<W, Wr, Val, F> writer::Arbitrary<W, Val> for FailOnSkipped<Wr, F>
where
    W: World,
    Self: Writer<W>,
    Wr: writer::Arbitrary<W, Val>,
{
    async fn write(&mut self, val: Val) {
        self.writer.write(val).await;
    }
}

#[warn(clippy::missing_trait_methods)]
impl<W, Wr, F> writer::Stats<W> for FailOnSkipped<Wr, F>
where
    Wr: writer::Stats<W>,
    Self: Writer<W>,
{
    fn passed_steps(&self) -> usize {
        self.writer.passed_steps()
    }

    fn skipped_steps(&self) -> usize {
        self.writer.skipped_steps()
    }

    fn failed_steps(&self) -> usize {
        self.writer.failed_steps()
    }

    fn parsing_errors(&self) -> usize {
        self.writer.parsing_errors()
    }

    fn execution_has_failed(&self) -> bool {
        self.writer.execution_has_failed()
    }
}

#[warn(clippy::missing_trait_methods)]
impl<Wr: writer::Normalized, F> writer::Normalized for FailOnSkipped<Wr, F> {}

impl<Writer> From<Writer> for FailOnSkipped<Writer> {
    fn from(writer: Writer) -> Self {
        Self {
            writer,
            should_fail: |_, _, _| true,
        }
    }
}

impl<Writer> FailOnSkipped<Writer> {
    /// Wraps the given [`Writer`] in a new [`FailOnSkipped`] one.
    #[must_use]
    pub fn new(writer: Writer) -> Self {
        Self::from(writer)
    }

    /// Wraps the given [`Writer`] in a new [`FailOnSkipped`] one with the given
    /// `predicate` indicating when a [`Skipped`] [`Step`] is considered
    /// [`Failed`].
    ///
    /// [`Failed`]: event::Step::Failed
    /// [`Skipped`]: event::Step::Skipped
    /// [`Step`]: gherkin::Step
    #[must_use]
    pub const fn with<P>(
        writer: Writer,
        predicate: P,
    ) -> FailOnSkipped<Writer, P>
    where
        P: Fn(
            &gherkin::Feature,
            Option<&gherkin::Rule>,
            &gherkin::Scenario,
        ) -> bool,
    {
        FailOnSkipped { writer, should_fail: predicate }
    }

    /// Returns the original [`Writer`], wrapped by this [`FailOnSkipped`] one.
    #[must_use]
    pub fn inner_writer(&self) -> &Writer {
        &self.writer
    }
}
