//! [`Writer`]-wrapper for outputting events in a normalized readable order.

use std::{hash::Hash, mem};

use derive_more::with_trait::Deref;
use either::Either;
use linked_hash_map::LinkedHashMap;

use crate::{
    Event, World, Writer,
    event::{self, Metadata, Source},
    parser, writer,
};

/// Wrapper for a [`Writer`] implementation for outputting events corresponding
/// to _order guarantees_ from the [`Runner`] in a [`Normalized`] readable
/// order.
///
/// Doesn't output anything by itself, but rather is used as a combinator for
/// rearranging events and feeding them to the underlying [`Writer`].
///
/// If some [`Feature`]([`Rule`]/[`Scenario`]/[`Step`]) has started to be
/// written into an output, then it will be written uninterruptedly until its
/// end, even if some other [`Feature`]s have finished their execution. It makes
/// much easier to understand what is really happening in the running
/// [`Feature`] while don't impose any restrictions on the running order.
///
/// [`Feature`]: gherkin::Feature
/// [`Rule`]: gherkin::Rule
/// [`Runner`]: crate::Runner
/// [`Scenario`]: gherkin::Scenario
/// [`Step`]: gherkin::Step
#[derive(Debug, Deref)]
pub struct Normalize<World, Writer> {
    /// Original [`Writer`] to normalize output of.
    #[deref]
    writer: Writer,

    /// Normalization queue of happened events.
    queue: NamakoQueue<World>,
}

// Implemented manually to omit redundant `World: Clone` trait bound, imposed by
// `#[derive(Clone)]`.
impl<World, Writer: Clone> Clone for Normalize<World, Writer> {
    fn clone(&self) -> Self {
        Self {
            writer: self.writer.clone(),
            queue: self.queue.clone(),
        }
    }
}

impl<W, Writer> Normalize<W, Writer> {
    /// Creates a new [`Normalized`] wrapper, which will rearrange [`event`]s
    /// and feed them to the given [`Writer`].
    #[must_use]
    pub fn new(writer: Writer) -> Self {
        Self {
            writer,
            queue: NamakoQueue::new(Metadata::new(())),
        }
    }

    /// Returns the original [`Writer`], wrapped by this [`Normalized`] one.
    #[must_use]
    pub const fn inner_writer(&self) -> &Writer {
        &self.writer
    }
}

impl<World, Wr: Writer<World>> Writer<World> for Normalize<World, Wr> {
    type Cli = Wr::Cli;

    async fn handle_event(
        &mut self,
        event: parser::Result<Event<event::Namako<World>>>,
        cli: &Self::Cli,
    ) {
        use event::{Feature, Namako, Rule};

        // Once `Namako::Finished` is emitted, we just pass events through,
        // without any normalization.
        // This is done to avoid panic if this `Writer` happens to be wrapped
        // inside `writer::Repeat` or similar.
        if self.queue.is_finished_and_emitted() {
            self.writer.handle_event(event, cli).await;
            return;
        }

        match event.map(Event::split) {
            res @ (Err(_) | Ok((Namako::Started | Namako::ParsingFinished { .. }, _))) => {
                self.writer
                    .handle_event(res.map(|(ev, meta)| meta.insert(ev)), cli)
                    .await;
            }
            Ok((Namako::Finished, meta)) => self.queue.finished(meta),
            Ok((Namako::Feature(f, ev), meta)) => match ev {
                Feature::Started => self.queue.new_feature(meta.wrap(f)),
                Feature::Scenario(s, ev) => {
                    self.queue.insert_scenario_event(&f, None, s, meta.wrap(ev));
                }
                Feature::Finished => self.queue.feature_finished(meta.wrap(&f)),
                Feature::Rule(r, ev) => match ev {
                    Rule::Started => self.queue.new_rule(&f, meta.wrap(r)),
                    Rule::Scenario(s, ev) => {
                        self.queue
                            .insert_scenario_event(&f, Some(r), s, meta.wrap(ev));
                    }
                    Rule::Finished => {
                        self.queue.rule_finished(&f, meta.wrap(r));
                    }
                },
            },
        }

        while let Some(feature_to_remove) = self.queue.emit((), &mut self.writer, cli).await {
            self.queue.remove(&feature_to_remove);
        }

        if let Some(meta) = self.queue.state.take_to_emit() {
            self.writer
                .handle_event(Ok(meta.wrap(Namako::Finished)), cli)
                .await;
        }
    }
}

#[warn(clippy::missing_trait_methods)]
impl<W, Wr, Val> writer::Arbitrary<W, Val> for Normalize<W, Wr>
where
    Wr: writer::Arbitrary<W, Val>,
{
    async fn write(&mut self, val: Val) {
        self.writer.write(val).await;
    }
}

#[warn(clippy::missing_trait_methods)]
impl<W, Wr> writer::Stats<W> for Normalize<W, Wr>
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
impl<W, Wr: writer::NonTransforming> writer::NonTransforming for Normalize<W, Wr> {}

/// Marker indicating that a [`Writer`] can accept events in a [happened-before]
/// order.
///
/// This means one of two things:
///
/// 1. Either [`Writer`] doesn't depend on events ordering.
///    For example, [`Writer`] which prints only [`Failed`] [`Step`]s.
///
/// 2. Or [`Writer`] does depend on events ordering, but implements some logic
///    to rearrange them.
///    For example, a [`Normalize`] wrapper will rearrange events and pass them
///    to the underlying [`Writer`], like a [`Runner`] wasn't concurrent at all.
///
/// [`Step`]: gherkin::Step
/// [`Failed`]: event::Step::Failed
/// [`Runner`]: crate::Runner
/// [happened-before]: https://en.wikipedia.org/wiki/Happened-before
pub trait Normalized {}

impl<World, Writer> Normalized for Normalize<World, Writer> {}

/// Wrapper for a [`Writer`] asserting it being [`Normalized`].
///
/// Technically is no-op, only forcing the [`Writer`] to become [`Normalized`]
/// despite it actually doesn't represent the one.
///
/// > ⚠️ __WARNING__: Should be used only in case you are absolutely sure, that
/// >                 incoming events will be emitted in a [`Normalized`] order.
/// >                 For example, in case [`max_concurrent_scenarios()`][1] is
/// >                 set to `1`.
///
/// [1]: crate::runner::Basic::max_concurrent_scenarios
#[derive(Clone, Copy, Debug, Deref)]
pub struct AssertNormalized<W: ?Sized>(W);

impl<Writer> AssertNormalized<Writer> {
    /// Creates a new no-op [`AssertNormalized`] wrapper forcing [`Normalized`]
    /// implementation.
    ///
    /// > ⚠️ __WARNING__: Should be used only in case you are absolutely sure,
    /// >                 that incoming events will be emitted in a
    /// >                 [`Normalized`] order.
    /// >                 For example, in case [`max_concurrent_scenarios()`][1]
    /// >                 is set to `1`.
    ///
    /// [1]: crate::runner::Basic::max_concurrent_scenarios
    #[must_use]
    pub const fn new(writer: Writer) -> Self {
        Self(writer)
    }
}

#[warn(clippy::missing_trait_methods)]
impl<W: World, Wr: Writer<W> + ?Sized> Writer<W> for AssertNormalized<Wr> {
    type Cli = Wr::Cli;

    async fn handle_event(
        &mut self,
        event: parser::Result<Event<event::Namako<W>>>,
        cli: &Self::Cli,
    ) {
        self.0.handle_event(event, cli).await;
    }
}

#[warn(clippy::missing_trait_methods)]
impl<W, Wr, Val> writer::Arbitrary<W, Val> for AssertNormalized<Wr>
where
    W: World,
    Wr: writer::Arbitrary<W, Val> + ?Sized,
{
    async fn write(&mut self, val: Val) {
        self.0.write(val).await;
    }
}

#[warn(clippy::missing_trait_methods)]
impl<W, Wr> writer::Stats<W> for AssertNormalized<Wr>
where
    Wr: writer::Stats<W>,
    Self: Writer<W>,
{
    fn passed_steps(&self) -> usize {
        self.0.passed_steps()
    }

    fn skipped_steps(&self) -> usize {
        self.0.skipped_steps()
    }

    fn failed_steps(&self) -> usize {
        self.0.failed_steps()
    }

    fn parsing_errors(&self) -> usize {
        self.0.parsing_errors()
    }

    fn execution_has_failed(&self) -> bool {
        self.0.execution_has_failed()
    }
}

#[warn(clippy::missing_trait_methods)]
impl<Wr: writer::NonTransforming> writer::NonTransforming for AssertNormalized<Wr> {}

#[warn(clippy::missing_trait_methods)]
impl<Writer> Normalized for AssertNormalized<Writer> {}

/// Normalization queue for incoming events.
///
/// We use [`LinkedHashMap`] everywhere throughout this module to ensure FIFO
/// queue for our events. This means by calling [`next()`] we reliably get the
/// currently outputting item's events. We're doing that until it yields an
/// event that corresponds to the item being finished, after which we remove the
/// current item, as all its events have been printed out, and we should do it
/// all over again with a [`next()`] item.
///
/// [`next()`]: Iterator::next()
#[derive(Clone, Debug)]
struct Queue<K: Eq + Hash, V> {
    /// Underlying FIFO queue of values.
    fifo: LinkedHashMap<K, V>,

    /// Initial [`Metadata`] of this [`Queue`] creation.
    ///
    /// If this value is [`Some`], then `Started` [`Event`] hasn't been passed
    /// on to the inner [`Writer`] yet.
    initial: Option<Metadata>,

    /// [`FinishedState`] of this [`Queue`].
    state: FinishedState,
}

impl<K: Eq + Hash, V> Queue<K, V> {
    /// Creates a new normalization [`Queue`] with an initial metadata.
    fn new(initial: Metadata) -> Self {
        Self {
            fifo: LinkedHashMap::new(),
            initial: Some(initial),
            state: FinishedState::NotFinished,
        }
    }

    /// Marks this [`Queue`] as [`FinishedButNotEmitted`].
    ///
    /// [`FinishedButNotEmitted`]: FinishedState::FinishedButNotEmitted
    const fn finished(&mut self, meta: Metadata) {
        self.state = FinishedState::FinishedButNotEmitted(meta);
    }

    /// Checks whether this [`Queue`] transited to [`FinishedAndEmitted`] state.
    ///
    /// [`FinishedAndEmitted`]: FinishedState::FinishedAndEmitted
    const fn is_finished_and_emitted(&self) -> bool {
        matches!(self.state, FinishedState::FinishedAndEmitted)
    }

    /// Removes the given `key` from this [`Queue`].
    fn remove(&mut self, key: &K) {
        drop(self.fifo.remove(key));
    }
}

/// Finishing state of a [`Queue`].
#[derive(Clone, Copy, Debug)]
enum FinishedState {
    /// `Finished` event hasn't been encountered yet.
    NotFinished,

    /// `Finished` event has been encountered, but not passed to the inner
    /// [`Writer`] yet.
    ///
    /// This happens when output is busy due to outputting some other item.
    FinishedButNotEmitted(Metadata),

    /// `Finished` event has been encountered and passed to the inner
    /// [`Writer`].
    FinishedAndEmitted,
}

impl FinishedState {
    /// Returns [`Metadata`] of this [`FinishedState::FinishedButNotEmitted`],
    /// and makes it [`FinishedAndEmitted`].
    ///
    /// [`FinishedAndEmitted`]: FinishedState::FinishedAndEmitted
    const fn take_to_emit(&mut self) -> Option<Metadata> {
        let current = mem::replace(self, Self::FinishedAndEmitted);
        if let Self::FinishedButNotEmitted(meta) = current {
            Some(meta)
        } else {
            *self = current;
            None
        }
    }
}

/// [`Queue`] which can remember its current item ([`Feature`], [`Rule`],
/// [`Scenario`] or [`Step`]) and pass events connected to it to the provided
/// [`Writer`].
///
/// [`Feature`]: gherkin::Feature
/// [`Rule`]: gherkin::Rule
/// [`Scenario`]: gherkin::Scenario
/// [`Step`]: gherkin::Step
trait Emitter<World> {
    /// Currently outputted key and value from this [`Queue`].
    type Current;

    /// Currently outputted item ([`Feature`], [`Rule`], [`Scenario`] or
    /// [`Step`]). If returned from [`Self::emit()`], means that all events
    /// associated with that item were passed to the underlying [`Writer`], so
    /// should be removed from the [`Queue`].
    ///
    /// [`Feature`]: gherkin::Feature
    /// [`Rule`]: gherkin::Rule
    /// [`Scenario`]: gherkin::Scenario
    /// [`Step`]: gherkin::Step
    type Emitted;

    /// Path to the [`Self::Emitted`] item. For [`Feature`] its `()`, as it's
    /// top-level item. For [`Scenario`] it's
    /// `(`[`Feature`]`, `[`Option`]`<`[`Rule`]`>)`, because [`Scenario`]
    /// definitely has parent [`Feature`] and optionally can have parent
    /// [`Rule`].
    ///
    /// [`Feature`]: gherkin::Feature
    /// [`Rule`]: gherkin::Rule
    /// [`Scenario`]: gherkin::Scenario
    type EmittedPath;

    /// Currently outputted key and value from this [`Queue`].
    fn current_item(self) -> Option<Self::Current>;

    /// Passes events of the current item ([`Feature`], [`Rule`], [`Scenario`]
    /// or [`Step`]) to the provided [`Writer`].
    ///
    /// If this method returns [`Some`], then all events of the current item
    /// were passed to the provided [`Writer`] and that means it should be
    /// [`remove`]d.
    ///
    /// [`remove`]: Queue::remove()
    /// [`Feature`]: gherkin::Feature
    /// [`Rule`]: gherkin::Rule
    /// [`Scenario`]: gherkin::Scenario
    /// [`Step`]: gherkin::Step
    fn emit<W: Writer<World>>(
        self,
        path: Self::EmittedPath,
        writer: &mut W,
        cli: &W::Cli,
    ) -> impl Future<Output = Option<Self::Emitted>>;
}

/// [`Queue`] of all incoming events.
type NamakoQueue<World> = Queue<Source<gherkin::Feature>, FeatureQueue<World>>;

impl<World> NamakoQueue<World> {
    /// Inserts a new [`Feature`] on [`event::Feature::Started`].
    ///
    /// [`Feature`]: gherkin::Feature
    fn new_feature(&mut self, feat: Event<Source<gherkin::Feature>>) {
        let (feat, meta) = feat.split();
        drop(self.fifo.insert(feat, FeatureQueue::new(meta)));
    }

    /// Marks a [`Feature`] as finished on [`event::Feature::Finished`].
    ///
    /// We don't emit it by the way, as there may be other in-progress
    /// [`Feature`]s holding the output.
    ///
    /// [`Feature`]: gherkin::Feature
    fn feature_finished(&mut self, feat: Event<&Source<gherkin::Feature>>) {
        let (feat, meta) = feat.split();
        self.fifo
            .get_mut(feat)
            .unwrap_or_else(|| panic!("no `Feature: {}`", feat.name))
            .finished(meta);
    }

    /// Inserts a new [`Rule`] on [`event::Rule::Started`].
    ///
    /// [`Rule`]: gherkin::Feature
    fn new_rule(&mut self, feat: &Source<gherkin::Feature>, rule: Event<Source<gherkin::Rule>>) {
        self.fifo
            .get_mut(feat)
            .unwrap_or_else(|| panic!("no `Feature: {}`", feat.name))
            .new_rule(rule);
    }

    /// Marks a [`Rule`] as finished on [`event::Rule::Finished`].
    ///
    /// We don't emit it by the way, as there may be other in-progress [`Rule`]s
    /// holding the output.
    ///
    /// [`Rule`]: gherkin::Feature
    fn rule_finished(
        &mut self,
        feat: &Source<gherkin::Feature>,
        rule: Event<Source<gherkin::Rule>>,
    ) {
        self.fifo
            .get_mut(feat)
            .unwrap_or_else(|| panic!("no `Feature: {}`", feat.name))
            .rule_finished(rule);
    }

    /// Inserts a new [`event::Scenario::Started`].
    fn insert_scenario_event(
        &mut self,
        feat: &Source<gherkin::Feature>,
        rule: Option<Source<gherkin::Rule>>,
        scenario: Source<gherkin::Scenario>,
        event: Event<event::Scenario<World>>,
    ) {
        self.fifo
            .get_mut(feat)
            .unwrap_or_else(|| panic!("no `Feature: {}`", feat.name))
            .insert_scenario_event(rule, scenario, event);
    }
}

impl<'me, World> Emitter<World> for &'me mut NamakoQueue<World> {
    type Current = (Source<gherkin::Feature>, &'me mut FeatureQueue<World>);
    type Emitted = Source<gherkin::Feature>;
    type EmittedPath = ();

    fn current_item(self) -> Option<Self::Current> {
        self.fifo.iter_mut().next().map(|(f, ev)| (f.clone(), ev))
    }

    async fn emit<W: Writer<World>>(
        self,
        (): (),
        writer: &mut W,
        cli: &W::Cli,
    ) -> Option<Self::Emitted> {
        if let Some((f, events)) = self.current_item() {
            if let Some(meta) = events.initial.take() {
                writer
                    .handle_event(
                        Ok(meta.wrap(event::Namako::feature_started(f.clone()))),
                        cli,
                    )
                    .await;
            }

            while let Some(scenario_or_rule_to_remove) = events.emit(f.clone(), writer, cli).await {
                events.remove(&scenario_or_rule_to_remove);
            }

            if let Some(meta) = events.state.take_to_emit() {
                writer
                    .handle_event(
                        Ok(meta.wrap(event::Namako::feature_finished(f.clone()))),
                        cli,
                    )
                    .await;
                return Some(f.clone());
            }
        }
        None
    }
}

/// Either a [`Rule`] or a [`Scenario`].
///
/// [`Rule`]: gherkin::Rule
/// [`Scenario`]: gherkin::Scenario
type RuleOrScenario = Either<Source<gherkin::Rule>, Source<gherkin::Scenario>>;

/// Either a [`Rule`]'s or a [`Scenario`]'s [`Queue`].
///
/// [`Rule`]: gherkin::Rule
/// [`Scenario`]: gherkin::Scenario
type RuleOrScenarioQueue<World> = Either<RulesQueue<World>, ScenariosQueue<World>>;

/// Either a [`Rule`]'s or a [`Scenario`]'s [`Queue`] with the corresponding
/// [`Rule`] or [`Scenario`] which is currently being outputted.
///
/// [`Rule`]: gherkin::Rule
/// [`Scenario`]: gherkin::Scenario
type NextRuleOrScenario<'events, World> = Either<
    (Source<gherkin::Rule>, &'events mut RulesQueue<World>),
    (
        Source<gherkin::Scenario>,
        &'events mut ScenariosQueue<World>,
    ),
>;

/// [`Queue`] of all events of a single [`Feature`].
///
/// [`Feature`]: gherkin::Feature
type FeatureQueue<World> = Queue<RuleOrScenario, RuleOrScenarioQueue<World>>;

impl<World> FeatureQueue<World> {
    /// Inserts a new [`Rule`].
    ///
    /// [`Rule`]: gherkin::Rule
    fn new_rule(&mut self, rule: Event<Source<gherkin::Rule>>) {
        let (rule, meta) = rule.split();
        drop(
            self.fifo
                .insert(Either::Left(rule), Either::Left(RulesQueue::new(meta))),
        );
    }

    /// Marks a [`Rule`] as finished on [`event::Rule::Finished`].
    ///
    /// [`Rule`]: gherkin::Rule
    fn rule_finished(&mut self, rule: Event<Source<gherkin::Rule>>) {
        let (rule, meta) = rule.split();
        match self.fifo.get_mut(&Either::Left(rule)) {
            Some(Either::Left(ev)) => {
                ev.finished(meta);
            }
            Some(Either::Right(_)) | None => unreachable!(),
        }
    }

    /// Inserts a new [`Scenario`] event.
    ///
    /// [`Scenario`]: gherkin::Scenario
    fn insert_scenario_event(
        &mut self,
        rule: Option<Source<gherkin::Rule>>,
        scenario: Source<gherkin::Scenario>,
        ev: Event<event::Scenario<World>>,
    ) {
        if let Some(r) = rule {
            match self
                .fifo
                .get_mut(&Either::Left(r.clone()))
                .unwrap_or_else(|| panic!("no `Rule: {}`", r.name))
            {
                Either::Left(rules) => rules
                    .fifo
                    .entry(scenario)
                    .or_insert_with(ScenariosQueue::new)
                    .0
                    .push(ev),
                Either::Right(_) => unreachable!(),
            }
        } else {
            match self
                .fifo
                .entry(Either::Right(scenario))
                .or_insert_with(|| Either::Right(ScenariosQueue::new()))
            {
                Either::Right(events) => events.0.push(ev),
                Either::Left(_) => unreachable!(),
            }
        }
    }
}

impl<'me, World> Emitter<World> for &'me mut FeatureQueue<World> {
    type Current = NextRuleOrScenario<'me, World>;
    type Emitted = RuleOrScenario;
    type EmittedPath = Source<gherkin::Feature>;

    fn current_item(self) -> Option<Self::Current> {
        Some(match self.fifo.iter_mut().next()? {
            (Either::Left(rule), Either::Left(events)) => Either::Left((rule.clone(), events)),
            (Either::Right(scenario), Either::Right(events)) => {
                Either::Right((scenario.clone(), events))
            }
            _ => unreachable!(),
        })
    }

    async fn emit<W: Writer<World>>(
        self,
        path: Self::EmittedPath,
        writer: &mut W,
        cli: &W::Cli,
    ) -> Option<Self::Emitted> {
        match self.current_item()? {
            Either::Left((rule, events)) => events
                .emit((path, rule), writer, cli)
                .await
                .map(Either::Left),
            Either::Right((scenario, events)) => events
                .emit((path, None, scenario), writer, cli)
                .await
                .map(Either::Right),
        }
    }
}

/// [`Queue`] of all events of a single [`Rule`].
///
/// [`Rule`]: gherkin::Rule
type RulesQueue<World> = Queue<Source<gherkin::Scenario>, ScenariosQueue<World>>;

impl<'me, World> Emitter<World> for &'me mut RulesQueue<World> {
    type Current = (Source<gherkin::Scenario>, &'me mut ScenariosQueue<World>);
    type Emitted = Source<gherkin::Rule>;
    type EmittedPath = (Source<gherkin::Feature>, Source<gherkin::Rule>);

    fn current_item(self) -> Option<Self::Current> {
        self.fifo.iter_mut().next().map(|(sc, ev)| (sc.clone(), ev))
    }

    async fn emit<W: Writer<World>>(
        self,
        (feature, rule): Self::EmittedPath,
        writer: &mut W,
        cli: &W::Cli,
    ) -> Option<Self::Emitted> {
        if let Some(meta) = self.initial.take() {
            writer
                .handle_event(
                    Ok(meta.wrap(event::Namako::rule_started(feature.clone(), rule.clone()))),
                    cli,
                )
                .await;
        }

        while let Some((scenario, events)) = self.current_item() {
            if let Some(should_be_removed) = events
                .emit((feature.clone(), Some(rule.clone()), scenario), writer, cli)
                .await
            {
                self.remove(&should_be_removed);
            } else {
                break;
            }
        }

        if let Some(meta) = self.state.take_to_emit() {
            writer
                .handle_event(
                    Ok(meta.wrap(event::Namako::rule_finished(feature, rule.clone()))),
                    cli,
                )
                .await;
            return Some(rule);
        }

        None
    }
}

/// [`Queue`] of all events of a single [`Scenario`].
///
/// [`Scenario`]: gherkin::Scenario
#[derive(Debug)]
struct ScenariosQueue<World>(Vec<Event<event::Scenario<World>>>);

// Implemented manually to omit redundant `World: Clone` trait bound, imposed by
// `#[derive(Clone)]`.
impl<World> Clone for ScenariosQueue<World> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<World> ScenariosQueue<World> {
    /// Creates a new [`ScenariosQueue`].
    const fn new() -> Self {
        Self(Vec::new())
    }
}

impl<World> Emitter<World> for &mut ScenariosQueue<World> {
    type Current = Event<event::Scenario<World>>;
    type Emitted = Source<gherkin::Scenario>;
    type EmittedPath = (
        Source<gherkin::Feature>,
        Option<Source<gherkin::Rule>>,
        Source<gherkin::Scenario>,
    );

    fn current_item(self) -> Option<Self::Current> {
        (!self.0.is_empty()).then(|| self.0.remove(0))
    }

    async fn emit<W: Writer<World>>(
        self,
        (feature, rule, scenario): Self::EmittedPath,
        writer: &mut W,
        cli: &W::Cli,
    ) -> Option<Self::Emitted> {
        while let Some((ev, meta)) = self.current_item().map(Event::split) {
            let should_be_removed = matches!(ev, event::Scenario::Finished);

            let ev = meta.wrap(event::Namako::scenario(
                feature.clone(),
                rule.clone(),
                scenario.clone(),
                ev,
            ));
            writer.handle_event(Ok(ev), cli).await;

            if should_be_removed {
                return Some(scenario.clone());
            }
        }
        None
    }
}
