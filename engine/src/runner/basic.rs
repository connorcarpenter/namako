//! Default [`Runner`] implementation.

use std::{
    any::Any,
    collections::HashMap,
    mem,
    ops::ControlFlow,
    panic::{self, AssertUnwindSafe},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::Duration,
};

#[cfg(feature = "tracing")]
use crossbeam_utils::atomic::AtomicCell;
use derive_more::with_trait::{Debug, Display, FromStr};
use futures::{
    FutureExt as _, Stream, StreamExt as _, TryFutureExt as _, TryStreamExt as _,
    channel::{mpsc, oneshot},
    future::{self, Either},
    lock::Mutex,
    pin_mut,
    stream::{self, LocalBoxStream},
};
use itertools::Itertools as _;
use regex::{CaptureLocations, Regex};

#[cfg(feature = "tracing")]
use crate::tracing::{Collector as TracingCollector, SpanCloseWaiter};
use crate::{
    Event, Runner, Step, World,
    event::{self, Info, Source},
    feature::Ext as _,
    future::{FutureExt as _, select_with_biased_first},
    parser, step,
};

/// CLI options of a [`Basic`] [`Runner`].
#[derive(Clone, Copy, Debug, Default, clap::Args)]
#[group(skip)]
pub struct Cli {
    /// Number of scenarios to run concurrently. If not specified, uses the
    /// value configured in tests runner, or 64 by default.
    #[arg(long, short, value_name = "int", global = true)]
    pub concurrency: Option<usize>,

    /// Run tests until the first failure.
    #[arg(long, global = true, visible_alias = "ff")]
    pub fail_fast: bool,
}

/// Type determining whether [`Scenario`]s should run concurrently or
/// sequentially.
///
/// [`Scenario`]: gherkin::Scenario
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ScenarioType {
    /// Run [`Scenario`]s sequentially (one-by-one).
    ///
    /// [`Scenario`]: gherkin::Scenario
    Serial,

    /// Run [`Scenario`]s concurrently.
    ///
    /// [`Scenario`]: gherkin::Scenario
    Concurrent,
}

/// Alias for [`fn`] used to determine whether a [`Scenario`] is [`Concurrent`]
/// or a [`Serial`] one.
///
/// [`Concurrent`]: ScenarioType::Concurrent
/// [`Serial`]: ScenarioType::Serial
/// [`Scenario`]: gherkin::Scenario
pub type WhichScenarioFn =
    fn(&gherkin::Feature, Option<&gherkin::Rule>, &gherkin::Scenario) -> ScenarioType;

/// Alias for a failed [`Scenario`].
///
/// [`Scenario`]: gherkin::Scenario
type IsFailed = bool;

/// Default [`Runner`] implementation which follows [_order guarantees_][1] from
/// the [`Runner`] trait docs.
///
/// Executes [`Scenario`]s concurrently based on the custom function, which
/// returns [`ScenarioType`]. Also, can limit maximum number of concurrent
/// [`Scenario`]s.
///
/// [1]: Runner#order-guarantees
/// [`Scenario`]: gherkin::Scenario
#[derive(Debug)]
pub struct Basic<World, F = WhichScenarioFn> {
    /// Optional number of concurrently executed [`Scenario`]s.
    ///
    /// [`Scenario`]: gherkin::Scenario
    max_concurrent_scenarios: Option<usize>,

    /// [`Collection`] of functions to match [`Step`]s.
    ///
    /// [`Collection`]: step::Collection
    steps: step::Collection<World>,

    /// Function determining whether a [`Scenario`] is [`Concurrent`] or
    /// a [`Serial`] one.
    ///
    /// [`Concurrent`]: ScenarioType::Concurrent
    /// [`Serial`]: ScenarioType::Serial
    /// [`Scenario`]: gherkin::Scenario
    #[debug(ignore)]
    which_scenario: F,

    /// Indicates whether execution should be stopped after the first failure.
    fail_fast: bool,

    #[cfg(feature = "tracing")]
    /// [`TracingCollector`] for [`event::Scenario::Log`]s forwarding.
    #[debug(ignore)]
    pub(crate) logs_collector: Arc<AtomicCell<Box<Option<TracingCollector>>>>,
}

#[cfg(feature = "tracing")]
/// Assertion that [`Basic::logs_collector`] [`AtomicCell::is_lock_free`].
const _: () = {
    assert!(
        AtomicCell::<Box<Option<TracingCollector>>>::is_lock_free(),
        "`AtomicCell::<Box<Option<TracingCollector>>>` is not lock-free",
    );
};

// Implemented manually to omit redundant `World: Clone` trait bound, imposed by
// `#[derive(Clone)]`.
impl<World, F: Clone> Clone for Basic<World, F> {
    fn clone(&self) -> Self {
        Self {
            max_concurrent_scenarios: self.max_concurrent_scenarios,
            steps: self.steps.clone(),
            which_scenario: self.which_scenario.clone(),
            fail_fast: self.fail_fast,
            #[cfg(feature = "tracing")]
            logs_collector: Arc::clone(&self.logs_collector),
        }
    }
}

impl<World> Default for Basic<World> {
    fn default() -> Self {
        let which_scenario: WhichScenarioFn = |_, _, _| ScenarioType::Concurrent;

        Self {
            max_concurrent_scenarios: Some(64),
            steps: step::Collection::new(),
            which_scenario,
            fail_fast: false,
            #[cfg(feature = "tracing")]
            logs_collector: Arc::new(AtomicCell::new(Box::new(None))),
        }
    }
}

impl<World, Which> Basic<World, Which> {
    /// If `max` is [`Some`], then number of concurrently executed [`Scenario`]s
    /// will be limited.
    ///
    /// [`Scenario`]: gherkin::Scenario
    #[must_use]
    pub fn max_concurrent_scenarios(mut self, max: impl Into<Option<usize>>) -> Self {
        self.max_concurrent_scenarios = max.into();
        self
    }

    /// Makes stop running tests on the first failure.
    ///
    /// __NOTE__: All the already started [`Scenario`]s at the moment of failure
    ///           will be finished.
    ///
    ///
    /// [`Scenario`]: gherkin::Scenario
    #[must_use]
    pub const fn fail_fast(mut self) -> Self {
        self.fail_fast = true;
        self
    }

    /// Function determining whether a [`Scenario`] is [`Concurrent`] or
    /// a [`Serial`] one.
    ///
    /// [`Concurrent`]: ScenarioType::Concurrent
    /// [`Serial`]: ScenarioType::Serial
    /// [`Scenario`]: gherkin::Scenario
    #[must_use]
    pub fn which_scenario<F>(self, func: F) -> Basic<World, F>
    where
        F: Fn(&gherkin::Feature, Option<&gherkin::Rule>, &gherkin::Scenario) -> ScenarioType
            + 'static,
    {
        let Self {
            max_concurrent_scenarios,
            steps,
            fail_fast,
            #[cfg(feature = "tracing")]
            logs_collector,
            ..
        } = self;
        Basic {
            max_concurrent_scenarios,
            steps,
            which_scenario: func,
            fail_fast,
            #[cfg(feature = "tracing")]
            logs_collector,
        }
    }

    /// Sets the given [`Collection`] of [`Step`]s to this [`Runner`].
    ///
    /// [`Collection`]: step::Collection
    #[must_use]
    pub fn steps(mut self, steps: step::Collection<World>) -> Self {
        self.steps = steps;
        self
    }

    /// Adds a [Given] [`Step`] matching the given `regex`.
    ///
    /// [Given]: https://cucumber.io/docs/gherkin/reference#given
    #[must_use]
    pub fn given(mut self, regex: Regex, step: Step<World>) -> Self {
        self.steps = mem::take(&mut self.steps).given(None, regex, step);
        self
    }

    /// Adds a [When] [`Step`] matching the given `regex`.
    ///
    /// [When]: https://cucumber.io/docs/gherkin/reference#given
    #[must_use]
    pub fn when(mut self, regex: Regex, step: Step<World>) -> Self {
        self.steps = mem::take(&mut self.steps).when(None, regex, step);
        self
    }

    /// Adds a [Then] [`Step`] matching the given `regex`.
    ///
    /// [Then]: https://cucumber.io/docs/gherkin/reference#then
    #[must_use]
    pub fn then(mut self, regex: Regex, step: Step<World>) -> Self {
        self.steps = mem::take(&mut self.steps).then(None, regex, step);
        self
    }
}

impl<W, Which> Runner<W> for Basic<W, Which>
where
    W: World,
    Which:
        Fn(&gherkin::Feature, Option<&gherkin::Rule>, &gherkin::Scenario) -> ScenarioType + 'static,
{
    type Cli = Cli;

    type EventStream = LocalBoxStream<'static, parser::Result<Event<event::Namako<W>>>>;

    fn run<S>(self, features: S, cli: Cli) -> Self::EventStream
    where
        S: Stream<Item = parser::Result<gherkin::Feature>> + 'static,
    {
        #[cfg(feature = "tracing")]
        let logs_collector = *self.logs_collector.swap(Box::new(None));
        let Self {
            max_concurrent_scenarios,
            steps,
            which_scenario,
            fail_fast,
            ..
        } = self;

        let fail_fast = cli.fail_fast || fail_fast;
        let concurrency = cli.concurrency.or(max_concurrent_scenarios);

        let buffer = Features::default();
        let (sender, receiver) = mpsc::unbounded();

        let insert = insert_features(
            buffer.clone(),
            features,
            which_scenario,
            sender.clone(),
            cli,
            fail_fast,
        );
        let execute = execute(
            buffer,
            concurrency,
            steps,
            sender,
            fail_fast,
            #[cfg(feature = "tracing")]
            logs_collector,
        );

        stream::select(
            receiver.map(Either::Left),
            future::join(insert, execute)
                .into_stream()
                .map(Either::Right),
        )
        .filter_map(async |r| match r {
            Either::Left(ev) => Some(ev),
            Either::Right(_) => None,
        })
        .boxed_local()
    }
}

/// Stores [`Feature`]s for later use by [`execute()`].
///
/// [`Feature`]: gherkin::Feature
async fn insert_features<W, S, F>(
    into: Features,
    features_stream: S,
    which_scenario: F,
    sender: mpsc::UnboundedSender<parser::Result<Event<event::Namako<W>>>>,
    cli: Cli,
    fail_fast: bool,
) where
    S: Stream<Item = parser::Result<gherkin::Feature>> + 'static,
    F: Fn(&gherkin::Feature, Option<&gherkin::Rule>, &gherkin::Scenario) -> ScenarioType + 'static,
{
    let mut features = 0;
    let mut rules = 0;
    let mut scenarios = 0;
    let mut steps = 0;
    let mut parser_errors = 0;

    pin_mut!(features_stream);
    while let Some(feat) = features_stream.next().await {
        match feat {
            Ok(f) => {
                features += 1;
                rules += f.rules.len();
                scenarios += f.count_scenarios();
                steps += f.count_steps();

                into.insert(f, &which_scenario, &cli).await;
            }
            Err(e) => {
                parser_errors += 1;

                // If the receiver end is dropped, then no one listens for the
                // events, so we can just stop from here.
                if sender.unbounded_send(Err(e)).is_err() || fail_fast {
                    break;
                }
            }
        }
    }

    drop(
        sender.unbounded_send(Ok(Event::new(event::Namako::ParsingFinished {
            features,
            rules,
            scenarios,
            steps,
            parser_errors,
        }))),
    );

    into.finish();
}

/// Retrieves [`Feature`]s and executes them.
///
/// # Events
///
/// - [`Scenario`] events are emitted by [`Executor`].
/// - If [`Scenario`] was first or last for particular [`Rule`] or [`Feature`],
///   emits starting or finishing events for them.
///
/// [`Feature`]: gherkin::Feature
/// [`Rule`]: gherkin::Rule
/// [`Scenario`]: gherkin::Scenario
// TODO: Needs refactoring.
#[expect(clippy::too_many_lines, reason = "needs refactoring")]
#[cfg_attr(
    feature = "tracing",
    expect(clippy::too_many_arguments, reason = "needs refactoring")
)]
async fn execute<W>(
    features: Features,
    max_concurrent_scenarios: Option<usize>,
    collection: step::Collection<W>,
    event_sender: mpsc::UnboundedSender<parser::Result<Event<event::Namako<W>>>>,
    fail_fast: bool,
    #[cfg(feature = "tracing")] mut logs_collector: Option<TracingCollector>,
) where
    W: World,
{
    // Those panic hook shenanigans are done to avoid console messages like
    // "thread 'main' panicked at ..."
    //
    // 1. We obtain the current panic hook and replace it with an empty one.
    // 2. We run tests, which can panic. In that case we pass all panic info
    //    down the line to the Writer, which will print it at a right time.
    // 3. We restore original panic hook, because suppressing all panics doesn't
    //    sound like a very good idea.
    let hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));

    let (finished_sender, finished_receiver) = mpsc::unbounded();
    let mut storage = FinishedRulesAndFeatures::new(finished_receiver);
    let executor = Executor::new(collection, event_sender, finished_sender);

    executor.send_event(event::Namako::Started);

    #[cfg(feature = "tracing")]
    let waiter = logs_collector
        .as_ref()
        .map(TracingCollector::scenario_span_event_waiter);

    let mut started_scenarios = ControlFlow::Continue(max_concurrent_scenarios);
    let mut run_scenarios = stream::FuturesUnordered::new();
    loop {
        let (runnable, sleep) = features
            .get(started_scenarios.continue_value().unwrap_or(Some(0)))
            .await;
        if run_scenarios.is_empty() && runnable.is_empty() {
            if features.is_finished(started_scenarios.is_break()).await {
                break;
            }

            // To avoid busy-polling of `Features::get()`, in case there are no
            // scenarios that are running or scheduled for execution, we spawn a
            // thread, that sleeps for minimal deadline of all retried
            // scenarios.
            // TODO: Replace `thread::spawn` with async runtime agnostic sleep,
            //       once it's available.
            if let Some(dur) = sleep {
                let (sender, receiver) = oneshot::channel();
                drop(thread::spawn(move || {
                    thread::sleep(dur);
                    sender.send(())
                }));
                _ = receiver.await.ok();
            }

            continue;
        }

        let started = storage.start_scenarios(&runnable);
        executor.send_all_events(started);

        {
            #[cfg(feature = "tracing")]
            let forward_logs = {
                if let Some(coll) = logs_collector.as_mut() {
                    coll.start_scenarios(&runnable);
                }
                async {
                    loop {
                        while let Some(logs) = logs_collector
                            .as_mut()
                            .and_then(TracingCollector::emitted_logs)
                        {
                            executor.send_all_events(logs);
                        }
                        future::ready(()).then_yield().await;
                    }
                }
            };
            #[cfg(feature = "tracing")]
            pin_mut!(forward_logs);
            #[cfg(not(feature = "tracing"))]
            let forward_logs = future::pending();

            if let ControlFlow::Continue(Some(sc)) = &mut started_scenarios {
                *sc -= runnable.len();
            }

            for (id, f, r, s, ty) in runnable {
                run_scenarios.push(
                    executor
                        .run_scenario(
                            id,
                            f,
                            r,
                            s,
                            ty,
                            #[cfg(feature = "tracing")]
                            waiter.as_ref(),
                        )
                        .then_yield(),
                );
            }

            let (finished_scenario, _) =
                select_with_biased_first(forward_logs, run_scenarios.next())
                    .await
                    .factor_first();
            if finished_scenario.is_some()
                && let ControlFlow::Continue(Some(sc)) = &mut started_scenarios
            {
                *sc += 1;
            }
        }

        while let Ok((id, feat, rule, scenario_failed)) = storage.finished_receiver.try_recv() {
            if let Some(rule) = rule
                && let Some(f) = storage.rule_scenario_finished(feat.clone(), rule)
            {
                executor.send_event(f);
            }
            if let Some(f) = storage.feature_scenario_finished(feat) {
                executor.send_event(f);
            }
            #[cfg(feature = "tracing")]
            {
                if let Some(coll) = logs_collector.as_mut() {
                    coll.finish_scenario(id);
                }
            }
            #[cfg(not(feature = "tracing"))]
            let _: ScenarioId = id;

            if fail_fast && scenario_failed {
                started_scenarios = ControlFlow::Break(());
            }
        }
    }

    // This is done in case of `fail_fast: true`, when not all `Scenario`s might
    // be executed.
    executor.send_all_events(storage.finish_all_rules_and_features());

    executor.send_event(event::Namako::Finished);

    panic::set_hook(hook);
}

/// Runs [`Scenario`]s and notifies about their state of completion.
///
/// [`Scenario`]: gherkin::Scenario
struct Executor<W> {
    /// [`Step`]s [`Collection`].
    ///
    /// [`Collection`]: step::Collection
    collection: step::Collection<W>,

    /// Sender for [`Scenario`] [events][1].
    ///
    /// [`Scenario`]: gherkin::Scenario
    /// [1]: event::Scenario
    event_sender: mpsc::UnboundedSender<parser::Result<Event<event::Namako<W>>>>,

    /// Sender for notifying of [`Scenario`]s completion.
    ///
    /// [`Scenario`]: gherkin::Scenario
    finished_sender: FinishedFeaturesSender,
}

impl<W: World> Executor<W> {
    /// Creates a new [`Executor`].
    const fn new(
        collection: step::Collection<W>,
        event_sender: mpsc::UnboundedSender<parser::Result<Event<event::Namako<W>>>>,
        finished_sender: FinishedFeaturesSender,
    ) -> Self {
        Self {
            collection,
            event_sender,
            finished_sender,
        }
    }

    /// Runs a [`Scenario`].
    ///
    /// # Events
    ///
    /// - Emits all [`Scenario`] events.
    ///
    /// [`Feature`]: gherkin::Feature
    /// [`Rule`]: gherkin::Rule
    /// [`Scenario`]: gherkin::Scenario
    // TODO: Needs refactoring.
    #[expect(clippy::too_many_lines, reason = "needs refactoring")]
    #[cfg_attr(
        feature = "tracing",
        expect(clippy::too_many_arguments, reason = "needs refactoring")
    )]
    async fn run_scenario(
        &self,
        id: ScenarioId,
        feature: Source<gherkin::Feature>,
        rule: Option<Source<gherkin::Rule>>,
        scenario: Source<gherkin::Scenario>,
        _scenario_ty: ScenarioType,
        #[cfg(feature = "tracing")] waiter: Option<&SpanCloseWaiter>,
    ) {
        let ok = |e: fn(_) -> event::Scenario<W>| {
            let (f, r, s) = (&feature, &rule, &scenario);
            move |step| {
                let (f, r, s) = (f.clone(), r.clone(), s.clone());
                let event = e(step);
                event::Namako::scenario(f, r, s, event)
            }
        };
        let ok_capt = |e: fn(_, _, _) -> event::Scenario<W>| {
            let (f, r, s) = (&feature, &rule, &scenario);
            move |step, cap, loc| {
                let (f, r, s) = (f.clone(), r.clone(), s.clone());
                let event = e(step, cap, loc);
                event::Namako::scenario(f, r, s, event)
            }
        };

        let compose = |started, passed, skipped| (ok(started), ok_capt(passed), ok(skipped));
        let into_bg_step_ev = compose(
            event::Scenario::background_step_started,
            event::Scenario::background_step_passed,
            event::Scenario::background_step_skipped,
        );
        let into_step_ev = compose(
            event::Scenario::step_started,
            event::Scenario::step_passed,
            event::Scenario::step_skipped,
        );

        self.send_event(event::Namako::scenario(
            feature.clone(),
            rule.clone(),
            scenario.clone(),
            event::Scenario::Started,
        ));

        let is_failed = async {
            let mut result = async {
                let world_res = AssertUnwindSafe(async { W::new().await })
                    .catch_unwind()
                    .await;

                let world = match world_res {
                    Ok(Ok(w)) => Some(w),
                    Ok(Err(e)) => {
                        let step = gherkin::Step {
                            keyword: "World".into(),
                            value: "Initialization".into(),
                            docstring: None,
                            table: None,
                            span: gherkin::Span { start: 0, end: 0 },
                            position: gherkin::LineCol { line: 0, col: 0 },
                            ty: gherkin::StepType::Given,
                        };
                        return Err(ExecutionFailure::StepPanicked {
                            world: None,
                            step: Source::new(step),
                            captures: None,
                            loc: None,
                            err: event::StepError::Panic(Arc::new(format!(
                                "World Init Error: {e}"
                            ))),
                            meta: event::Metadata::new(()),
                            is_background: false,
                        });
                    }
                    Err(p) => {
                        let step = gherkin::Step {
                            keyword: "World".into(),
                            value: "Initialization".into(),
                            docstring: None,
                            table: None,
                            span: gherkin::Span { start: 0, end: 0 },
                            position: gherkin::LineCol { line: 0, col: 0 },
                            ty: gherkin::StepType::Given,
                        };
                        return Err(ExecutionFailure::StepPanicked {
                            world: None,
                            step: Source::new(step),
                            captures: None,
                            loc: None,
                            err: event::StepError::Panic(p.into()),
                            meta: event::Metadata::new(()),
                            is_background: false,
                        });
                    }
                };

                let before_hook = world;

                let feature_background = feature
                    .background
                    .as_ref()
                    .map(|b| b.steps.iter().map(|s| Source::new(s.clone())))
                    .into_iter()
                    .flatten();

                let feature_background = stream::iter(feature_background)
                    .map(Ok)
                    .try_fold(before_hook, |world, bg_step| {
                        self.run_step(
                            world,
                            bg_step,
                            true,
                            into_bg_step_ev,
                            id,
                            #[cfg(feature = "tracing")]
                            waiter,
                        )
                        .map_ok(Some)
                    })
                    .await?;

                let rule_background = rule
                    .as_ref()
                    .map(|r| {
                        r.background
                            .as_ref()
                            .map(|b| b.steps.iter().map(|s| Source::new(s.clone())))
                            .into_iter()
                            .flatten()
                    })
                    .into_iter()
                    .flatten();

                let rule_background = stream::iter(rule_background)
                    .map(Ok)
                    .try_fold(feature_background, |world, bg_step| {
                        self.run_step(
                            world,
                            bg_step,
                            true,
                            into_bg_step_ev,
                            id,
                            #[cfg(feature = "tracing")]
                            waiter,
                        )
                        .map_ok(Some)
                    })
                    .await?;

                stream::iter(scenario.steps.iter().map(|s| Source::new(s.clone())))
                    .map(Ok)
                    .try_fold(rule_background, |world, step| {
                        self.run_step(
                            world,
                            step,
                            false,
                            into_step_ev,
                            id,
                            #[cfg(feature = "tracing")]
                            waiter,
                        )
                        .map_ok(Some)
                    })
                    .await
            }
            .await;

            let world = match &mut result {
                Ok(world) => world.take(),
                Err(exec_err) => exec_err.take_world(),
            };

            let world = world.map(Arc::new);

            let scenario_failed = match &result {
                Ok(_) | Err(ExecutionFailure::StepSkipped(_)) => false,
                Err(ExecutionFailure::StepPanicked { .. }) => true,
            };
            let is_failed = scenario_failed;

            if let Some(exec_error) = result.err() {
                self.emit_failed_events(
                    feature.clone(),
                    rule.clone(),
                    scenario.clone(),
                    world.clone(),
                    exec_error,
                );
            }

            is_failed
        };
        #[cfg(feature = "tracing")]
        let (is_failed, span_id) = {
            let span = id.scenario_span();
            let span_id = span.id();
            let is_failed = tracing::Instrument::instrument(is_failed, span);
            (is_failed, span_id)
        };
        let is_failed = is_failed.then_yield().await;

        #[cfg(feature = "tracing")]
        if let Some((waiter, span_id)) = waiter.zip(span_id) {
            waiter.wait_for_span_close(span_id).then_yield().await;
        }

        self.send_event(event::Namako::scenario(
            feature.clone(),
            rule.clone(),
            scenario.clone(),
            event::Scenario::Finished,
        ));

        self.scenario_finished(id, feature, rule, is_failed);
    }

    /// Runs a [`Step`].
    ///
    /// # Events
    ///
    /// - Emits all the [`Step`] events, except [`Step::Failed`]. See
    ///   [`Self::emit_failed_events()`] for more details.
    ///
    /// [`Step`]: gherkin::Step
    /// [`Step::Failed`]: event::Step::Failed
    async fn run_step<St, Ps, Sk>(
        &self,
        world_opt: Option<W>,
        step: Source<gherkin::Step>,
        is_background: bool,
        (started, passed, skipped): (St, Ps, Sk),
        scenario_id: ScenarioId,
        #[cfg(feature = "tracing")] waiter: Option<&SpanCloseWaiter>,
    ) -> Result<W, ExecutionFailure<W>>
    where
        St: FnOnce(Source<gherkin::Step>) -> event::Namako<W>,
        Ps: FnOnce(
            Source<gherkin::Step>,
            CaptureLocations,
            Option<step::Location>,
        ) -> event::Namako<W>,
        Sk: FnOnce(Source<gherkin::Step>) -> event::Namako<W>,
    {
        self.send_event(started(step.clone()));

        let run = async {
            let (step_fn, captures, loc, ctx) = match self.collection.find(&step) {
                Ok(Some(f)) => f,
                Ok(None) => return Ok((None, None, world_opt)),
                Err(e) => {
                    let e = event::StepError::AmbiguousMatch(e);
                    return Err((e, None, None, world_opt));
                }
            };

            let mut world = if let Some(w) = world_opt {
                w
            } else {
                match AssertUnwindSafe(async { W::new().await })
                    .catch_unwind()
                    .then_yield()
                    .await
                {
                    Ok(Ok(w)) => w,
                    Ok(Err(e)) => {
                        let e = event::StepError::Panic(coerce_into_info(format!(
                            "failed to initialize `World`: {e}"
                        )));
                        return Err((e, None, loc, None));
                    }
                    Err(e) => {
                        let e = event::StepError::Panic(e.into());
                        return Err((e, None, loc, None));
                    }
                }
            };

            match AssertUnwindSafe(async { step_fn(&mut world, ctx).await })
                .catch_unwind()
                .await
            {
                Ok(()) => Ok((Some(captures), loc, Some(world))),
                Err(e) => {
                    let e = event::StepError::Panic(e.into());
                    Err((e, Some(captures), loc, Some(world)))
                }
            }
        };

        #[cfg(feature = "tracing")]
        let (run, span_id) = {
            let span = scenario_id.step_span(is_background);
            let span_id = span.id();
            let run = tracing::Instrument::instrument(run, span);
            (run, span_id)
        };
        let result = run.then_yield().await;

        #[cfg(feature = "tracing")]
        if let Some((waiter, id)) = waiter.zip(span_id) {
            waiter.wait_for_span_close(id).then_yield().await;
        }
        #[cfg(not(feature = "tracing"))]
        let _: ScenarioId = scenario_id;

        match result {
            Ok((Some(captures), loc, Some(world))) => {
                self.send_event(passed(step, captures, loc));
                Ok(world)
            }
            Ok((_, _, world)) => {
                self.send_event(skipped(step));
                Err(ExecutionFailure::StepSkipped(world))
            }
            Err((err, captures, loc, world)) => Err(ExecutionFailure::StepPanicked {
                world,
                step,
                captures,
                loc,
                err,
                meta: event::Metadata::new(()),
                is_background,
            }),
        }
    }

    /// Emits all the failure events of [`Step`].
    ///
    /// [`Step`]: gherkin::Step
    /// [1]: crate::Runner#order-guarantees
    fn emit_failed_events(
        &self,
        feature: Source<gherkin::Feature>,
        rule: Option<Source<gherkin::Rule>>,
        scenario: Source<gherkin::Scenario>,
        world: Option<Arc<W>>,
        err: ExecutionFailure<W>,
    ) {
        match err {
            ExecutionFailure::StepSkipped(_) => {}
            ExecutionFailure::StepPanicked {
                step,
                captures,
                loc,
                err: error,
                meta,
                is_background: true,
                ..
            } => self.send_event_with_meta(
                event::Namako::scenario(
                    feature,
                    rule,
                    scenario,
                    event::Scenario::background_step_failed(step, captures, loc, world, error),
                ),
                meta,
            ),
            ExecutionFailure::StepPanicked {
                step,
                captures,
                loc,
                err: error,
                meta,
                is_background: false,
                ..
            } => self.send_event_with_meta(
                event::Namako::scenario(
                    feature,
                    rule,
                    scenario,
                    event::Scenario::step_failed(step, captures, loc, world, error),
                ),
                meta,
            ),
        }
    }

    /// Executes the [`HookType::After`], if present.
    ///
    /// Notifies [`FinishedRulesAndFeatures`] about [`Scenario`] being finished.
    ///
    /// [`Scenario`]: gherkin::Scenario
    fn scenario_finished(
        &self,
        id: ScenarioId,
        feature: Source<gherkin::Feature>,
        rule: Option<Source<gherkin::Rule>>,
        is_failed: IsFailed,
    ) {
        // If the receiver end is dropped, then no one listens for events
        // so we can just ignore it.
        drop(
            self.finished_sender
                .unbounded_send((id, feature, rule, is_failed)),
        );
    }

    /// Notifies with the given [`Namako`] event.
    ///
    /// [`Namako`]: event::Namako
    fn send_event(&self, event: event::Namako<W>) {
        // If the receiver end is dropped, then no one listens for events,
        // so we can just ignore it.
        drop(self.event_sender.unbounded_send(Ok(Event::new(event))));
    }

    /// Notifies with the given [`Namako`] event along with its [`Metadata`].
    ///
    /// [`Namako`]: event::Namako
    /// [`Metadata`]: event::Metadata
    fn send_event_with_meta(&self, event: event::Namako<W>, meta: event::Metadata) {
        // If the receiver end is dropped, then no one listens for events,
        // so we can just ignore it.
        drop(self.event_sender.unbounded_send(Ok(meta.wrap(event))));
    }

    /// Notifies with the given [`Namako`] events.
    ///
    /// [`Namako`]: event::Namako
    fn send_all_events(&self, events: impl IntoIterator<Item = event::Namako<W>>) {
        for v in events {
            // If the receiver end is dropped, then no one listens for events,
            // so we can just stop from here.
            if self.event_sender.unbounded_send(Ok(Event::new(v))).is_err() {
                break;
            }
        }
    }
}

/// ID of a [`Scenario`], uniquely identifying it.
///
/// [`Scenario`]: gherkin::Scenario
#[derive(Clone, Copy, Debug, Display, Eq, FromStr, Hash, PartialEq)]
pub struct ScenarioId(pub(crate) u64);

impl ScenarioId {
    /// Creates a new unique [`ScenarioId`].
    pub fn new() -> Self {
        /// [`AtomicU64`] ID.
        static ID: AtomicU64 = AtomicU64::new(0);

        Self(ID.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for ScenarioId {
    fn default() -> Self {
        Self::new()
    }
}

/// Stores currently running [`Rule`]s and [`Feature`]s and notifies about their
/// state of completion.
///
/// [`Feature`]: gherkin::Feature
/// [`Rule`]: gherkin::Rule
struct FinishedRulesAndFeatures {
    /// Number of finished [`Scenario`]s of [`Feature`].
    ///
    /// [`Feature`]: gherkin::Feature
    /// [`Scenario`]: gherkin::Scenario
    features_scenarios_count: HashMap<Source<gherkin::Feature>, usize>,

    /// Number of finished [`Scenario`]s of [`Rule`].
    ///
    /// We also store path to a [`Feature`], so [`Rule`]s with same names and
    /// spans in different `.feature` files will have different hashes.
    ///
    /// [`Feature`]: gherkin::Feature
    /// [`Rule`]: gherkin::Rule
    /// [`Scenario`]: gherkin::Scenario
    rule_scenarios_count: HashMap<(Source<gherkin::Feature>, Source<gherkin::Rule>), usize>,

    /// Receiver for notifying state of [`Scenario`]s completion.
    ///
    /// [`Scenario`]: gherkin::Scenario
    finished_receiver: FinishedFeaturesReceiver,
}

/// Alias of a [`mpsc::UnboundedSender`] that notifies about finished
/// [`Feature`]s.
///
/// [`Feature`]: gherkin::Feature
type FinishedFeaturesSender = mpsc::UnboundedSender<(
    ScenarioId,
    Source<gherkin::Feature>,
    Option<Source<gherkin::Rule>>,
    IsFailed,
)>;

/// Alias of a [`mpsc::UnboundedReceiver`] that receives events about finished
/// [`Feature`]s.
///
/// [`Feature`]: gherkin::Feature
type FinishedFeaturesReceiver = mpsc::UnboundedReceiver<(
    ScenarioId,
    Source<gherkin::Feature>,
    Option<Source<gherkin::Rule>>,
    IsFailed,
)>;

impl FinishedRulesAndFeatures {
    /// Creates a new [`FinishedRulesAndFeatures`] store.
    fn new(finished_receiver: FinishedFeaturesReceiver) -> Self {
        Self {
            features_scenarios_count: HashMap::new(),
            rule_scenarios_count: HashMap::new(),
            finished_receiver,
        }
    }

    /// Marks [`Rule`]'s [`Scenario`] as finished and returns [`Rule::Finished`]
    /// event if no [`Scenario`]s left.
    ///
    /// [`Rule`]: gherkin::Rule
    /// [`Rule::Finished`]: event::Rule::Finished
    /// [`Scenario`]: gherkin::Scenario
    fn rule_scenario_finished<W>(
        &mut self,
        feature: Source<gherkin::Feature>,
        rule: Source<gherkin::Rule>,
    ) -> Option<event::Namako<W>> {
        let finished_scenarios = self
            .rule_scenarios_count
            .get_mut(&(feature.clone(), rule.clone()))
            .unwrap_or_else(|| panic!("no `Rule: {}`", rule.name));
        *finished_scenarios += 1;
        (rule.scenarios.len() == *finished_scenarios).then(|| {
            _ = self
                .rule_scenarios_count
                .remove(&(feature.clone(), rule.clone()));
            event::Namako::rule_finished(feature, rule)
        })
    }

    /// Marks [`Feature`]'s [`Scenario`] as finished and returns
    /// [`Feature::Finished`] event if no [`Scenario`]s left.
    ///
    /// [`Feature`]: gherkin::Feature
    /// [`Feature::Finished`]: event::Feature::Finished
    /// [`Scenario`]: gherkin::Scenario
    fn feature_scenario_finished<W>(
        &mut self,
        feature: Source<gherkin::Feature>,
    ) -> Option<event::Namako<W>> {
        let finished_scenarios = self
            .features_scenarios_count
            .get_mut(&feature)
            .unwrap_or_else(|| panic!("no `Feature: {}`", feature.name));
        *finished_scenarios += 1;
        let scenarios = feature.count_scenarios();
        (scenarios == *finished_scenarios).then(|| {
            _ = self.features_scenarios_count.remove(&feature);
            event::Namako::feature_finished(feature)
        })
    }

    /// Marks all the unfinished [`Rule`]s and [`Feature`]s as finished, and
    /// returns all the appropriate finished events.
    ///
    /// [`Feature`]: gherkin::Feature
    /// [`Rule`]: gherkin::Rule
    fn finish_all_rules_and_features<W>(&mut self) -> impl Iterator<Item = event::Namako<W>> {
        self.rule_scenarios_count
            .drain()
            .map(|((feat, rule), _)| event::Namako::rule_finished(feat, rule))
            .chain(
                self.features_scenarios_count
                    .drain()
                    .map(|(feat, _)| event::Namako::feature_finished(feat)),
            )
    }

    /// Marks [`Scenario`]s as started and returns [`Rule::Started`] and
    /// [`Feature::Started`] if given [`Scenario`] was first for particular
    /// [`Rule`] or [`Feature`].
    ///
    /// [`Feature`]: gherkin::Feature
    /// [`Feature::Started`]: event::Feature::Started
    /// [`Rule`]: gherkin::Rule
    /// [`Rule::Started`]: event::Rule::Started
    /// [`Scenario`]: gherkin::Scenario
    fn start_scenarios<W, R>(
        &mut self,
        runnable: R,
    ) -> impl Iterator<Item = event::Namako<W>> + use<W, R>
    where
        R: AsRef<
            [(
                ScenarioId,
                Source<gherkin::Feature>,
                Option<Source<gherkin::Rule>>,
                Source<gherkin::Scenario>,
                ScenarioType,
            )],
        >,
    {
        let runnable = runnable.as_ref();

        let mut started_features = Vec::new();
        for feature in runnable.iter().map(|(_, f, ..)| f.clone()).dedup() {
            _ = self
                .features_scenarios_count
                .entry(feature.clone())
                .or_insert_with(|| {
                    started_features.push(feature);
                    0
                });
        }

        let mut started_rules = Vec::new();
        for (feat, rule) in runnable
            .iter()
            .filter_map(|(_, feat, rule, _, _)| rule.clone().map(|r| (feat.clone(), r)))
            .dedup()
        {
            _ = self
                .rule_scenarios_count
                .entry((feat.clone(), rule.clone()))
                .or_insert_with(|| {
                    started_rules.push((feat, rule));
                    0
                });
        }

        started_features
            .into_iter()
            .map(event::Namako::feature_started)
            .chain(
                started_rules
                    .into_iter()
                    .map(|(f, r)| event::Namako::rule_started(f, r)),
            )
    }
}

/// [`Scenario`]s storage.
///
/// [`Scenario`]: gherkin::Scenario
type Scenarios = HashMap<
    ScenarioType,
    Vec<(
        ScenarioId,
        Source<gherkin::Feature>,
        Option<Source<gherkin::Rule>>,
        Source<gherkin::Scenario>,
    )>,
>;

/// Alias of a [`Features::insert_scenarios()`] argument.
type InsertedScenarios = HashMap<
    ScenarioType,
    Vec<(
        ScenarioId,
        Source<gherkin::Feature>,
        Option<Source<gherkin::Rule>>,
        Source<gherkin::Scenario>,
    )>,
>;

/// Storage sorted by [`ScenarioType`] [`Feature`]'s [`Scenario`]s.
///
/// [`Feature`]: gherkin::Feature
/// [`Scenario`]: gherkin::Scenario
#[derive(Clone, Default)]
struct Features {
    /// Storage itself.
    scenarios: Arc<Mutex<Scenarios>>,

    /// Indicates whether all parsed [`Feature`]s are sorted and stored.
    ///
    /// [`Feature`]: gherkin::Feature
    finished: Arc<AtomicBool>,
}

impl Features {
    /// Splits [`Feature`] into [`Scenario`]s, sorts by [`ScenarioType`] and
    /// stores them.
    ///
    /// [`Feature`]: gherkin::Feature
    /// [`Scenario`]: gherkin::Scenario
    async fn insert<Which>(&self, feature: gherkin::Feature, which_scenario: &Which, _cli: &Cli)
    where
        Which: Fn(&gherkin::Feature, Option<&gherkin::Rule>, &gherkin::Scenario) -> ScenarioType
            + 'static,
    {
        let feature = Source::new(feature);

        let local = feature
            .scenarios
            .iter()
            .map(|s| (None, s))
            .chain(feature.rules.iter().flat_map(|r| {
                let rule = Some(Source::new(r.clone()));
                r.scenarios
                    .iter()
                    .map(|s| (rule.clone(), s))
                    .collect::<Vec<_>>()
            }))
            .map(|(rule, scenario)| {
                (
                    ScenarioId::new(),
                    feature.clone(),
                    rule,
                    Source::new(scenario.clone()),
                )
            })
            .into_group_map_by(|(_, f, r, s)| which_scenario(f, r.as_ref().map(AsRef::as_ref), s));

        self.insert_scenarios(local).await;
    }

    /// Inserts the provided [`Scenario`]s into this [`Features`] storage.
    ///
    /// [`Scenario`]: gherkin::Scenario
    async fn insert_scenarios(&self, scenarios: InsertedScenarios) {
        let mut without_retries: Scenarios = HashMap::new();
        #[expect(clippy::iter_over_hash_type, reason = "order doesn't matter")]
        for (which, values) in scenarios {
            for (id, f, r, s) in values {
                without_retries
                    .entry(which)
                    .or_default()
                    .push((id, f, r, s));
            }
        }

        let mut storage = self.scenarios.lock().await;

        if without_retries.contains_key(&ScenarioType::Serial) {
            // If there are Serial Scenarios we insert all Serial and Concurrent
            // Scenarios in front.
            // This is done to execute them closely to one another, so the
            // output wouldn't hang on executing other Concurrent Scenarios.
            #[expect(clippy::iter_over_hash_type, reason = "order doesn't matter")]
            for (which, mut values) in without_retries {
                let old = mem::take(storage.entry(which).or_default());
                values.extend(old);
                storage.entry(which).or_default().extend(values);
            }
        } else {
            // If there are no Serial Scenarios, we just extend already existing
            // Concurrent Scenarios.
            #[expect(clippy::iter_over_hash_type, reason = "order doesn't matter")]
            for (which, values) in without_retries {
                storage.entry(which).or_default().extend(values);
            }
        }
    }

    /// Returns [`Scenario`]s which are ready to run and the minimal deadline of
    /// all retried [`Scenario`]s.
    ///
    /// [`Scenario`]: gherkin::Scenario
    async fn get(
        &self,
        max_concurrent_scenarios: Option<usize>,
    ) -> (
        Vec<(
            ScenarioId,
            Source<gherkin::Feature>,
            Option<Source<gherkin::Rule>>,
            Source<gherkin::Scenario>,
            ScenarioType,
        )>,
        Option<Duration>,
    ) {
        use ScenarioType::{Concurrent, Serial};

        if max_concurrent_scenarios == Some(0) {
            return (Vec::new(), None);
        }

        let min_dur = None;
        let drain = |storage: &mut Vec<(_, _, _, _)>, ty, count: Option<usize>| {
            let mut i = 0;
            let drained = storage
                .extract_if(.., |(_, _, _, _)| {
                    // Because of retries involved, we cannot just specify
                    // `..count` range to `.extract_if()`.
                    if count.filter(|c| i >= *c).is_some() {
                        return false;
                    }

                    i += 1;
                    true
                })
                .map(|(id, f, r, s)| (id, f, r, s, ty))
                .collect::<Vec<_>>();
            (!drained.is_empty()).then_some(drained)
        };

        let mut guard = self.scenarios.lock().await;
        let scenarios = guard
            .get_mut(&Serial)
            .and_then(|storage| drain(storage, Serial, Some(1)))
            .or_else(|| {
                guard
                    .get_mut(&Concurrent)
                    .and_then(|storage| drain(storage, Concurrent, max_concurrent_scenarios))
            })
            .unwrap_or_default();

        (scenarios, min_dur)
    }

    /// Marks that there will be no more [`Feature`]s to execute.
    ///
    /// [`Feature`]: gherkin::Feature
    fn finish(&self) {
        self.finished.store(true, Ordering::SeqCst);
    }

    /// Indicates whether there are more [`Feature`]s to execute.
    ///
    /// `fail_fast` argument indicates whether not yet executed scenarios should
    /// be omitted.
    ///
    /// [`Feature`]: gherkin::Feature
    async fn is_finished(&self, fail_fast: bool) -> bool {
        self.finished.load(Ordering::SeqCst)
            && (fail_fast || self.scenarios.lock().await.values().all(Vec::is_empty))
    }
}

/// Coerces the given `value` into a type-erased [`Info`].
fn coerce_into_info<T: Any + Send + 'static>(val: T) -> Info {
    Arc::new(val)
}

/// Failure encountered during execution of [`HookType::Before`] or [`Step`].
/// See [`Executor::emit_failed_events()`] for more info.
///
/// [`Step`]: gherkin::Step
enum ExecutionFailure<World> {
    /// [`Step`] was skipped.
    ///
    /// [`Step`]: gherkin::Step.
    StepSkipped(Option<World>),

    /// [`Step`] failed.
    ///
    /// [`Step`]: gherkin::Step.
    StepPanicked {
        /// [`World`] at the time when [`Step`] has failed.
        ///
        /// [`Step`]: gherkin::Step
        world: Option<World>,

        /// [`Step`] itself.
        ///
        /// [`Step`]: gherkin::Step
        step: Source<gherkin::Step>,

        /// [`Step`]s [`regex`] [`CaptureLocations`].
        ///
        /// [`Step`]: gherkin::Step
        captures: Option<CaptureLocations>,

        /// [`Location`] of the [`fn`] that matched this [`Step`].
        ///
        /// [`Location`]: step::Location
        /// [`Step`]: gherkin::Step
        loc: Option<step::Location>,

        /// [`StepError`] of the [`Step`].
        ///
        /// [`Step`]: gherkin::Step
        /// [`StepError`]: event::StepError
        err: event::StepError,

        /// [`Metadata`] at the time when [`Step`] failed.
        ///
        /// [`Metadata`]: event::Metadata
        /// [`Step`]: gherkin::Step.
        meta: event::Metadata,

        /// Indicator whether the [`Step`] was background or not.
        ///
        /// [`Step`]: gherkin::Step
        is_background: bool,
    },
}

impl<W> ExecutionFailure<W> {
    /// Takes the [`World`] leaving a [`None`] in its place.
    const fn take_world(&mut self) -> Option<W> {
        match self {
            Self::StepSkipped(world) | Self::StepPanicked { world, .. } => world.take(),
        }
    }
}
