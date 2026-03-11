//! Top-level [Namako] executor.
//!
//! [Namako]: https://cucumber.io

use std::{marker::PhantomData, mem, path::Path};

use derive_more::with_trait::Debug;
use futures::StreamExt as _;
use regex::Regex;

use crate::{
    Event, Parser, Runner, ScenarioType, Step, World, Writer, WriterExt as _, cli, event, parser,
    runner, step, tag::Ext as _, writer,
};

/// Top-level [Namako] executor.
///
/// Most of the time you don't need to work with it directly, just use
/// [`World::run()`] or [`World::namako()`] on your [`World`] deriver to get
/// [Namako] up and running.
///
/// Otherwise, use [`Namako::new()`] to get the default [Namako] executor,
/// provide [`Step`]s with [`World::collection()`] or by hand with
/// [`Namako::given()`], [`Namako::when()`] and [`Namako::then()`].
///
/// In case you want a custom [`Parser`], [`Runner`] or [`Writer`], or some
/// other finer control, use [`Namako::custom()`] or
/// [`Namako::with_parser()`], [`Namako::with_runner()`] and
/// [`Namako::with_writer()`] to construct your dream [Namako] executor!
///
/// [Namako]: https://cucumber.io
#[derive(Debug)]
pub struct Namako<W, P, I, R, Wr, Cli = cli::Empty>
where
    W: World,
    P: Parser<I>,
    R: Runner<W>,
    Wr: Writer<W>,
    Cli: clap::Args,
{
    /// [`Parser`] sourcing [`Feature`]s for execution.
    ///
    /// [`Feature`]: gherkin::Feature
    parser: P,

    /// [`Runner`] executing [`Scenario`]s and producing [`event`]s.
    ///
    /// [`Scenario`]: gherkin::Scenario
    pub(crate) runner: R,

    /// [`Writer`] outputting [`event`]s to some output.
    writer: Wr,

    /// CLI options this [`Namako`] has been run with.
    ///
    /// If empty, then will be parsed from a command line.
    cli: Option<cli::Opts<P::Cli, R::Cli, Wr::Cli, Cli>>,

    /// Type of the [`World`] this [`Namako`] run on.
    #[debug(ignore)]
    _world: PhantomData<W>,

    /// Type of the input consumed by [`Namako::parser`].
    #[debug(ignore)]
    _parser_input: PhantomData<I>,
}

impl<W, P, I, R, Wr, Cli> Namako<W, P, I, R, Wr, Cli>
where
    W: World,
    P: Parser<I>,
    R: Runner<W>,
    Wr: Writer<W>,
    Cli: clap::Args,
{
    /// Creates a custom [`Namako`] executor with the provided [`Parser`],
    /// [`Runner`] and [`Writer`].
    #[must_use]
    pub const fn custom(parser: P, runner: R, writer: Wr) -> Self {
        Self {
            parser,
            runner,
            writer,
            cli: None,
            _world: PhantomData,
            _parser_input: PhantomData,
        }
    }

    /// Replaces [`Parser`].
    #[must_use]
    pub fn with_parser<NewP, NewI>(self, parser: NewP) -> Namako<W, NewP, NewI, R, Wr, Cli>
    where
        NewP: Parser<NewI>,
    {
        let Self { runner, writer, .. } = self;
        Namako {
            parser,
            runner,
            writer,
            cli: None,
            _world: PhantomData,
            _parser_input: PhantomData,
        }
    }

    /// Replaces [`Runner`].
    #[must_use]
    pub fn with_runner<NewR>(self, runner: NewR) -> Namako<W, P, I, NewR, Wr, Cli>
    where
        NewR: Runner<W>,
    {
        let Self { parser, writer, .. } = self;
        Namako {
            parser,
            runner,
            writer,
            cli: None,
            _world: PhantomData,
            _parser_input: PhantomData,
        }
    }

    /// Replaces [`Writer`].
    #[must_use]
    pub fn with_writer<NewWr>(self, writer: NewWr) -> Namako<W, P, I, R, NewWr, Cli>
    where
        NewWr: Writer<W>,
    {
        let Self { parser, runner, .. } = self;
        Namako {
            parser,
            runner,
            writer,
            cli: None,
            _world: PhantomData,
            _parser_input: PhantomData,
        }
    }

    /// Re-outputs [`Skipped`] steps for easier navigation.
    ///
    /// # Example
    ///
    /// Output with a regular [`Namako::run()`]:
    /// <script
    ///     id="asciicast-0d92qlT8Mbc4WXyvRbHJmjsqN"
    ///     src="https://asciinema.org/a/0d92qlT8Mbc4WXyvRbHJmjsqN.js"
    ///     async data-autoplay="true" data-rows="17">
    /// </script>
    ///
    /// Adjust [`Namako`] to re-output all the [`Skipped`] steps at the end:
    /// ```rust
    /// # use namako_engine::World;
    /// #
    /// # #[derive(Debug, Default, World)]
    /// # struct MyWorld;
    /// #
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// MyWorld::namako()
    ///     .repeat_skipped()
    ///     .run_and_exit("tests/features/readme")
    ///     .await;
    /// # }
    /// ```
    /// <script
    ///     id="asciicast-ox14HynkBIw8atpfhyfvKrsO3"
    ///     src="https://asciinema.org/a/ox14HynkBIw8atpfhyfvKrsO3.js"
    ///     async data-autoplay="true" data-rows="19">
    /// </script>
    ///
    /// [`Scenario`]: gherkin::Scenario
    /// [`Skipped`]: event::Step::Skipped
    #[must_use]
    pub fn repeat_skipped(self) -> Namako<W, P, I, R, writer::Repeat<W, Wr>, Cli>
    where
        Wr: writer::NonTransforming,
    {
        Namako {
            parser: self.parser,
            runner: self.runner,
            writer: self.writer.repeat_skipped(),
            cli: self.cli,
            _world: PhantomData,
            _parser_input: PhantomData,
        }
    }

    /// Re-outputs [`Failed`] steps for easier navigation.
    ///
    /// # Example
    ///
    /// Output with a regular [`Namako::fail_on_skipped()`]:
    /// ```rust,should_panic
    /// # use namako_engine::World;
    /// #
    /// # #[derive(Debug, Default, World)]
    /// # struct MyWorld;
    /// #
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// MyWorld::namako()
    ///     .fail_on_skipped()
    ///     .run_and_exit("tests/features/readme")
    ///     .await;
    /// # }
    /// ```
    /// <script
    ///     id="asciicast-UcipuopO6IFEsIDty6vaJlCH9"
    ///     src="https://asciinema.org/a/UcipuopO6IFEsIDty6vaJlCH9.js"
    ///     async data-autoplay="true" data-rows="21">
    /// </script>
    ///
    /// Adjust [`Namako`] to re-output all the [`Failed`] steps at the end:
    /// ```rust,should_panic
    /// # use namako_engine::World;
    /// #
    /// # #[derive(Debug, Default, World)]
    /// # struct MyWorld;
    /// #
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// MyWorld::namako()
    ///     .repeat_failed()
    ///     .fail_on_skipped()
    ///     .run_and_exit("tests/features/readme")
    ///     .await;
    /// # }
    /// ```
    /// <script
    ///     id="asciicast-ofOljvyEMb41OTLhE081QKv68"
    ///     src="https://asciinema.org/a/ofOljvyEMb41OTLhE081QKv68.js"
    ///     async data-autoplay="true" data-rows="24">
    /// </script>
    ///
    /// [`Failed`]: event::Step::Failed
    #[must_use]
    pub fn repeat_failed(self) -> Namako<W, P, I, R, writer::Repeat<W, Wr>, Cli>
    where
        Wr: writer::NonTransforming,
    {
        Namako {
            parser: self.parser,
            runner: self.runner,
            writer: self.writer.repeat_failed(),
            cli: self.cli,
            _world: PhantomData,
            _parser_input: PhantomData,
        }
    }

    /// Re-outputs steps by the given `filter` predicate.
    ///
    /// # Example
    ///
    /// Output with a regular [`Namako::fail_on_skipped()`]:
    /// ```rust,should_panic
    /// # use namako_engine::World;
    /// # use futures::FutureExt as _;
    /// #
    /// # #[derive(Debug, Default, World)]
    /// # struct MyWorld;
    /// #
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// MyWorld::namako()
    ///     .fail_on_skipped()
    ///     .run_and_exit("tests/features/readme")
    ///     .await;
    /// # }
    /// ```
    /// <script
    ///     id="asciicast-UcipuopO6IFEsIDty6vaJlCH9"
    ///     src="https://asciinema.org/a/UcipuopO6IFEsIDty6vaJlCH9.js"
    ///     async data-autoplay="true" data-rows="21">
    /// </script>
    ///
    /// Adjust [`Namako`] to re-output all the [`Failed`] steps ta the end by
    /// providing a custom `filter` predicate:
    /// ```rust,should_panic
    /// # use namako_engine::World;
    /// #
    /// # #[derive(Debug, Default, World)]
    /// # struct MyWorld;
    /// #
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// MyWorld::namako()
    ///     .repeat_if(|ev| {
    ///         use namako_engine::event::{
    ///             Namako, Feature, Rule, Scenario, Step,
    ///         };
    ///
    ///         matches!(
    ///             ev.as_deref(),
    ///             Ok(Namako::Feature(
    ///                 _,
    ///                 Feature::Rule(
    ///                     _,
    ///                     Rule::Scenario(
    ///                         _,
    ///                         Scenario::Step(_, Step::Failed(..))
    ///                             | Scenario::Background(
    ///                                 _,
    ///                                 Step::Failed(_, _, _, _),
    ///                             ),
    ///                     )
    ///                 ) | Feature::Scenario(
    ///                     _,
    ///                     Scenario::Step(_, Step::Failed(..))
    ///                         | Scenario::Background(_, Step::Failed(..)),
    ///                 )
    ///             )) | Err(_)
    ///         )
    ///     })
    ///     .fail_on_skipped()
    ///     .run_and_exit("tests/features/readme")
    ///     .await;
    /// # }
    /// ```
    /// <script
    ///     id="asciicast-ofOljvyEMb41OTLhE081QKv68"
    ///     src="https://asciinema.org/a/ofOljvyEMb41OTLhE081QKv68.js"
    ///     async data-autoplay="true" data-rows="24">
    /// </script>
    ///
    /// [`Failed`]: event::Step::Failed
    #[must_use]
    pub fn repeat_if<F>(self, filter: F) -> Namako<W, P, I, R, writer::Repeat<W, Wr, F>, Cli>
    where
        F: Fn(&parser::Result<Event<event::Namako<W>>>) -> bool,
        Wr: writer::NonTransforming,
    {
        Namako {
            parser: self.parser,
            runner: self.runner,
            writer: self.writer.repeat_if(filter),
            cli: self.cli,
            _world: PhantomData,
            _parser_input: PhantomData,
        }
    }

    /// Consider [`Skipped`] [`Background`] or regular [`Step`]s as [`Failed`]
    /// if their [`Scenario`] isn't marked with `@allow.skipped` tag.
    ///
    /// It's useful option for ensuring that all the steps were covered.
    ///
    /// # Example
    ///
    /// Output with a regular [`Namako::run()`]:
    /// <script
    ///     id="asciicast-0d92qlT8Mbc4WXyvRbHJmjsqN"
    ///     src="https://asciinema.org/a/0d92qlT8Mbc4WXyvRbHJmjsqN.js"
    ///     async data-autoplay="true" data-rows="17">
    /// </script>
    ///
    /// To fail all the [`Skipped`] steps setup [`Namako`] like this:
    /// ```rust,should_panic
    /// # use namako_engine::World;
    /// #
    /// # #[derive(Debug, Default, World)]
    /// # struct MyWorld;
    /// #
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// MyWorld::namako()
    ///     .fail_on_skipped()
    ///     .run_and_exit("tests/features/readme")
    ///     .await;
    /// # }
    /// ```
    /// <script
    ///     id="asciicast-IHLxMEgku9BtBVkR4k2DtOjMd"
    ///     src="https://asciinema.org/a/IHLxMEgku9BtBVkR4k2DtOjMd.js"
    ///     async data-autoplay="true" data-rows="21">
    /// </script>
    ///
    /// To intentionally suppress some [`Skipped`] steps failing, use the
    /// `@allow.skipped` tag:
    /// ```gherkin
    /// Feature: Animal feature
    ///
    ///   Scenario: If we feed a hungry cat it will no longer be hungry
    ///     Given a hungry cat
    ///     When I feed the cat
    ///     Then the cat is not hungry
    ///
    ///   @allow.skipped
    ///   Scenario: If we feed a satiated dog it will not become hungry
    ///     Given a satiated dog
    ///     When I feed the dog
    ///     Then the dog is not hungry
    /// ```
    ///
    /// [`Background`]: gherkin::Background
    /// [`Failed`]: event::Step::Failed
    /// [`Scenario`]: gherkin::Scenario
    /// [`Skipped`]: event::Step::Skipped
    /// [`Step`]: gherkin::Step
    #[must_use]
    pub fn fail_on_skipped(self) -> Namako<W, P, I, R, writer::FailOnSkipped<Wr>, Cli> {
        Namako {
            parser: self.parser,
            runner: self.runner,
            writer: self.writer.fail_on_skipped(),
            cli: self.cli,
            _world: PhantomData,
            _parser_input: PhantomData,
        }
    }

    /// Consider [`Skipped`] [`Background`] or regular [`Step`]s as [`Failed`]
    /// if the given `filter` predicate returns `true`.
    ///
    /// # Example
    ///
    /// Output with a regular [`Namako::run()`]:
    /// <script
    ///     id="asciicast-0d92qlT8Mbc4WXyvRbHJmjsqN"
    ///     src="https://asciinema.org/a/0d92qlT8Mbc4WXyvRbHJmjsqN.js"
    ///     async data-autoplay="true" data-rows="17">
    /// </script>
    ///
    /// Adjust [`Namako`] to fail on all [`Skipped`] steps, but the ones
    /// marked with a `@dog` tag:
    /// ```rust,should_panic
    /// # use namako_engine::World;
    /// #
    /// # #[derive(Debug, Default, World)]
    /// # struct MyWorld;
    /// #
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// MyWorld::namako()
    ///     .fail_on_skipped_with(|_, _, s| !s.tags.iter().any(|t| t == "dog"))
    ///     .run_and_exit("tests/features/readme")
    ///     .await;
    /// # }
    /// ```
    /// ```gherkin
    /// Feature: Animal feature
    ///
    ///   Scenario: If we feed a hungry cat it will no longer be hungry
    ///     Given a hungry cat
    ///     When I feed the cat
    ///     Then the cat is not hungry
    ///
    ///   Scenario: If we feed a satiated dog it will not become hungry
    ///     Given a satiated dog
    ///     When I feed the dog
    ///     Then the dog is not hungry
    /// ```
    /// <script
    ///     id="asciicast-IHLxMEgku9BtBVkR4k2DtOjMd"
    ///     src="https://asciinema.org/a/IHLxMEgku9BtBVkR4k2DtOjMd.js"
    ///     async data-autoplay="true" data-rows="21">
    /// </script>
    ///
    /// And to avoid failing, use the `@dog` tag:
    /// ```gherkin
    /// Feature: Animal feature
    ///
    ///   Scenario: If we feed a hungry cat it will no longer be hungry
    ///     Given a hungry cat
    ///     When I feed the cat
    ///     Then the cat is not hungry
    ///
    ///   @dog
    ///   Scenario: If we feed a satiated dog it will not become hungry
    ///     Given a satiated dog
    ///     When I feed the dog
    ///     Then the dog is not hungry
    /// ```
    ///
    /// [`Background`]: gherkin::Background
    /// [`Failed`]: event::Step::Failed
    /// [`Scenario`]: gherkin::Scenario
    /// [`Skipped`]: event::Step::Skipped
    /// [`Step`]: gherkin::Step
    #[must_use]
    pub fn fail_on_skipped_with<Filter>(
        self,
        filter: Filter,
    ) -> Namako<W, P, I, R, writer::FailOnSkipped<Wr, Filter>, Cli>
    where
        Filter: Fn(&gherkin::Feature, Option<&gherkin::Rule>, &gherkin::Scenario) -> bool,
    {
        Namako {
            parser: self.parser,
            runner: self.runner,
            writer: self.writer.fail_on_skipped_with(filter),
            cli: self.cli,
            _world: PhantomData,
            _parser_input: PhantomData,
        }
    }
}

impl<W, P, I, R, Wr, Cli> Namako<W, P, I, R, Wr, Cli>
where
    W: World,
    P: Parser<I>,
    R: Runner<W>,
    Wr: Writer<W> + writer::Normalized,
    Cli: clap::Args,
{
    /// Runs [`Namako`].
    ///
    /// [`Feature`]s sourced from a [`Parser`] are fed to a [`Runner`], which
    /// produces events handled by a [`Writer`].
    ///
    /// [`Feature`]: gherkin::Feature
    pub async fn run(self, input: I) -> Wr {
        self.filter_run(input, |_, _, _| true).await
    }

    /// Consumes already parsed [`cli::Opts`].
    ///
    /// This method allows to pre-parse [`cli::Opts`] for custom needs before
    /// using them inside [`Namako`].
    ///
    /// Also, any additional custom CLI options may be specified as a
    /// [`clap::Args`] deriving type, used as the last type parameter of
    /// [`cli::Opts`].
    ///
    /// > ⚠️ __WARNING__: Any CLI options of [`Parser`], [`Runner`], [`Writer`]
    /// >                 or custom ones should not overlap, otherwise
    /// >                 [`cli::Opts`] will fail to parse on startup.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use std::time::Duration;
    /// #
    /// # use namako_engine::{cli, World};
    /// # use futures::FutureExt as _;
    /// # use tokio::time;
    /// #
    /// # #[derive(Debug, Default, World)]
    /// # struct MyWorld;
    /// #
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// #[derive(clap::Args)]
    /// struct CustomCli {
    ///     /// Some custom option.
    ///     #[arg(long)]
    ///     custom_option: bool,
    /// }
    ///
    /// let cli = cli::Opts::<_, _, _, CustomCli>::parsed();
    ///
    /// MyWorld::namako()
    ///     .with_cli(cli)
    ///     .run_and_exit("tests/features/readme")
    ///     .await;
    /// # }
    /// ```
    /// ```gherkin
    /// Feature: Animal feature
    ///
    ///   Scenario: If we feed a hungry cat it will no longer be hungry
    ///     Given a hungry cat
    ///     When I feed the cat
    ///     Then the cat is not hungry
    /// ```
    /// <script
    ///     id="asciicast-0KvTxnfaMRjsvsIKsalS611Ta"
    ///     src="https://asciinema.org/a/0KvTxnfaMRjsvsIKsalS611Ta.js"
    ///     async data-autoplay="true" data-rows="14">
    /// </script>
    ///
    /// Also, specifying `--help` flag will describe `--before-time` now.
    ///
    /// [`Feature`]: gherkin::Feature
    #[must_use]
    pub fn with_cli<CustomCli>(
        self,
        cli: cli::Opts<P::Cli, R::Cli, Wr::Cli, CustomCli>,
    ) -> Namako<W, P, I, R, Wr, CustomCli>
    where
        CustomCli: clap::Args,
    {
        let Self {
            parser,
            runner,
            writer,
            ..
        } = self;
        Namako {
            parser,
            runner,
            writer,
            cli: Some(cli),
            _world: PhantomData,
            _parser_input: PhantomData,
        }
    }

    /// Initializes [`Default`] [`cli::Opts`].
    ///
    /// This method allows to omit parsing real [`cli::Opts`], as eagerly
    /// initializes [`Default`] ones instead.
    #[must_use]
    pub fn with_default_cli(mut self) -> Self
    where
        cli::Opts<P::Cli, R::Cli, Wr::Cli, Cli>: Default,
    {
        self.cli = Some(cli::Opts::default());
        self
    }

    /// Runs [`Namako`] with [`Scenario`]s filter.
    ///
    /// [`Feature`]s sourced from a [`Parser`] are fed to a [`Runner`], which
    /// produces events handled by a [`Writer`].
    ///
    /// # Example
    ///
    /// Adjust [`Namako`] to run only [`Scenario`]s marked with `@cat` tag:
    /// ```rust
    /// # use namako_engine::World;
    /// #
    /// # #[derive(Debug, Default, World)]
    /// # struct MyWorld;
    /// #
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// MyWorld::namako()
    ///     .filter_run("tests/features/readme", |_, _, sc| {
    ///         sc.tags.iter().any(|t| t == "cat")
    ///     })
    ///     .await;
    /// # }
    /// ```
    /// ```gherkin
    /// Feature: Animal feature
    ///
    ///   @cat
    ///   Scenario: If we feed a hungry cat it will no longer be hungry
    ///     Given a hungry cat
    ///     When I feed the cat
    ///     Then the cat is not hungry
    ///
    ///   @dog
    ///   Scenario: If we feed a satiated dog it will not become hungry
    ///     Given a satiated dog
    ///     When I feed the dog
    ///     Then the dog is not hungry
    /// ```
    /// <script
    ///     id="asciicast-0KvTxnfaMRjsvsIKsalS611Ta"
    ///     src="https://asciinema.org/a/0KvTxnfaMRjsvsIKsalS611Ta.js"
    ///     async data-autoplay="true" data-rows="14">
    /// </script>
    ///
    /// [`Feature`]: gherkin::Feature
    /// [`Scenario`]: gherkin::Scenario
    pub async fn filter_run<F>(self, input: I, filter: F) -> Wr
    where
        F: Fn(&gherkin::Feature, Option<&gherkin::Rule>, &gherkin::Scenario) -> bool + 'static,
    {
        let cli::Opts {
            re_filter,
            tags_filter,
            parser: parser_cli,
            runner: runner_cli,
            writer: writer_cli,
            ..
        } = self.cli.unwrap_or_else(cli::Opts::<_, _, _, _>::parsed);

        let filter = move |feat: &gherkin::Feature,
                           rule: Option<&gherkin::Rule>,
                           scenario: &gherkin::Scenario| {
            re_filter.as_ref().map_or_else(
                || {
                    tags_filter.as_ref().map_or_else(
                        || filter(feat, rule, scenario),
                        |tags| {
                            // The order `Feature` -> `Rule` -> `Scenario`
                            // matters here.
                            tags.eval(
                                feat.tags
                                    .iter()
                                    .chain(rule.iter().flat_map(|r| &r.tags))
                                    .chain(scenario.tags.iter()),
                            )
                        },
                    )
                },
                |re| re.is_match(&scenario.name),
            )
        };

        let Self {
            parser,
            runner,
            mut writer,
            ..
        } = self;

        let features = parser.parse(input, parser_cli);

        let filtered = features.map(move |feature| {
            let mut feature = feature?;
            let feat_scenarios = mem::take(&mut feature.scenarios);
            feature.scenarios = feat_scenarios
                .into_iter()
                .filter(|s| filter(&feature, None, s))
                .collect();

            let mut rules = mem::take(&mut feature.rules);
            for r in &mut rules {
                let rule_scenarios = mem::take(&mut r.scenarios);
                r.scenarios = rule_scenarios
                    .into_iter()
                    .filter(|s| filter(&feature, Some(r), s))
                    .collect();
            }
            feature.rules = rules;

            Ok(feature)
        });

        let events_stream = runner.run(filtered, runner_cli);
        futures::pin_mut!(events_stream);
        while let Some(ev) = events_stream.next().await {
            writer.handle_event(ev, &writer_cli).await;
        }
        writer
    }
}

// Implemented manually to omit redundant `W: Clone` and `I: Clone` trait
// bounds, imposed by `#[derive(Clone)]`.
impl<W, P, I, R, Wr, Cli> Clone for Namako<W, P, I, R, Wr, Cli>
where
    W: World,
    P: Clone + Parser<I>,
    R: Clone + Runner<W>,
    Wr: Clone + Writer<W>,
    Cli: Clone + clap::Args,
    P::Cli: Clone,
    R::Cli: Clone,
    Wr::Cli: Clone,
{
    fn clone(&self) -> Self {
        Self {
            parser: self.parser.clone(),
            runner: self.runner.clone(),
            writer: self.writer.clone(),
            cli: self.cli.clone(),
            _world: PhantomData,
            _parser_input: PhantomData,
        }
    }
}

/// Shortcut for the [`Namako`] type returned by its [`Default`] impl.
pub(crate) type DefaultNamako<W, I> = Namako<
    W,
    parser::Basic,
    I,
    runner::Basic<W>,
    writer::Compat<writer::Summarize<writer::Normalize<W, writer::Basic>>>,
>;

impl<W, I> Default for DefaultNamako<W, I>
where
    W: World + Debug,
    I: AsRef<Path>,
{
    fn default() -> Self {
        Self::custom(
            parser::Basic::new(),
            runner::Basic::default(),
            writer::Compat(writer::Basic::stdout().summarized()),
        )
    }
}

impl<W, I> DefaultNamako<W, I>
where
    W: World + Debug,
    I: AsRef<Path>,
{
    /// Creates a default [`Namako`] executor.
    ///
    /// * [`Parser`] — [`parser::Basic`]
    ///
    /// * [`Runner`] — [`runner::Basic`]
    ///   * [`ScenarioType`] — [`Concurrent`] by default, [`Serial`] if
    ///     `@serial` [tag] is present on a [`Scenario`];
    ///   * Allowed to run up to 64 [`Concurrent`] [`Scenario`]s.
    ///
    /// * [`Writer`] — [`Normalize`] and [`Summarize`] [`writer::Basic`].
    ///
    /// [`Concurrent`]: ScenarioType::Concurrent
    /// [`Normalize`]: writer::Normalize
    /// [`Scenario`]: gherkin::Scenario
    /// [`Serial`]: ScenarioType::Serial
    /// [`Summarize`]: writer::Summarize
    ///
    /// [tag]: https://cucumber.io/docs/namako/api#tags
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<W, I, R, Wr, Cli> Namako<W, parser::Basic, I, R, Wr, Cli>
where
    W: World,
    R: Runner<W>,
    Wr: Writer<W>,
    Cli: clap::Args,
    I: AsRef<Path>,
{
}

impl<W, I, P, Wr, F, Cli> Namako<W, P, I, runner::Basic<W, F>, Wr, Cli>
where
    W: World,
    P: Parser<I>,
    Wr: Writer<W>,
    Cli: clap::Args,
    F: Fn(&gherkin::Feature, Option<&gherkin::Rule>, &gherkin::Scenario) -> ScenarioType + 'static,
{
    /// If `max` is [`Some`] number of concurrently executed [`Scenario`]s will
    /// be limited.
    ///
    /// [`Scenario`]: gherkin::Scenario
    #[must_use]
    pub fn max_concurrent_scenarios(mut self, max: impl Into<Option<usize>>) -> Self {
        self.runner = self.runner.max_concurrent_scenarios(max);
        self
    }

    /// Makes failed [`Scenario`]s being retried the specified number of times.
    ///
    /// [`Scenario`]: gherkin::Scenario
    #[must_use]
    pub fn fail_fast(mut self) -> Self {
        self.runner = self.runner.fail_fast();
        self
    }

    /// Function determining whether a [`Scenario`] is [`Concurrent`] or
    /// a [`Serial`] one.
    ///
    /// [`Concurrent`]: ScenarioType::Concurrent
    /// [`Serial`]: ScenarioType::Serial
    /// [`Scenario`]: gherkin::Scenario
    #[must_use]
    pub fn which_scenario<Which>(
        self,
        func: Which,
    ) -> Namako<W, P, I, runner::Basic<W, Which>, Wr, Cli>
    where
        Which: Fn(&gherkin::Feature, Option<&gherkin::Rule>, &gherkin::Scenario) -> ScenarioType
            + 'static,
    {
        let Self {
            parser,
            runner,
            writer,
            cli,
            ..
        } = self;
        Namako {
            parser,
            runner: runner.which_scenario(func),
            writer,
            cli,
            _world: PhantomData,
            _parser_input: PhantomData,
        }
    }

    /// Replaces [`Collection`] of [`Step`]s.
    ///
    /// [`Collection`]: step::Collection
    /// [`Step`]: step::Step
    #[must_use]
    pub fn steps(mut self, steps: step::Collection<W>) -> Self {
        self.runner = self.runner.steps(steps);
        self
    }

    /// Inserts [Given] [`Step`].
    ///
    /// [Given]: https://cucumber.io/docs/gherkin/reference#given
    #[must_use]
    pub fn given(mut self, regex: Regex, step: Step<W>) -> Self {
        self.runner = self.runner.given(regex, step);
        self
    }

    /// Inserts [When] [`Step`].
    ///
    /// [When]: https://cucumber.io/docs/gherkin/reference#when
    #[must_use]
    pub fn when(mut self, regex: Regex, step: Step<W>) -> Self {
        self.runner = self.runner.when(regex, step);
        self
    }

    /// Inserts [Then] [`Step`].
    ///
    /// [Then]: https://cucumber.io/docs/gherkin/reference#then
    #[must_use]
    pub fn then(mut self, regex: Regex, step: Step<W>) -> Self {
        self.runner = self.runner.then(regex, step);
        self
    }
}

impl<W, I, P, R, Wr, Cli> Namako<W, P, I, R, Wr, Cli>
where
    W: World,
    P: Parser<I>,
    R: Runner<W>,
    Wr: writer::Stats<W> + writer::Normalized,
    Cli: clap::Args,
{
    /// Runs [`Namako`].
    ///
    /// [`Feature`]s sourced from a [`Parser`] are fed to a [`Runner`], which
    /// produces events handled by a [`Writer`].
    ///
    /// # Panics
    ///
    /// If encountered errors while parsing [`Feature`]s or at least one
    /// [`Step`] [`Failed`].
    ///
    /// [`Failed`]: event::Step::Failed
    /// [`Feature`]: gherkin::Feature
    /// [`Step`]: gherkin::Step
    pub async fn run_and_exit(self, input: I) {
        self.filter_run_and_exit(input, |_, _, _| true).await;
    }

    /// Runs [`Namako`] with [`Scenario`]s filter.
    ///
    /// [`Feature`]s sourced from a [`Parser`] are fed to a [`Runner`], which
    /// produces events handled by a [`Writer`].
    ///
    /// # Panics
    ///
    /// If encountered errors while parsing [`Feature`]s or at least one
    /// [`Step`] [`Failed`].
    ///
    /// # Example
    ///
    /// Adjust [`Namako`] to run only [`Scenario`]s marked with `@cat` tag:
    /// ```rust
    /// # use namako_engine::World;
    /// #
    /// # #[derive(Debug, Default, World)]
    /// # struct MyWorld;
    /// #
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// MyWorld::namako()
    ///     .filter_run_and_exit("tests/features/readme", |_, _, sc| {
    ///         sc.tags.iter().any(|t| t == "cat")
    ///     })
    ///     .await;
    /// # }
    /// ```
    /// ```gherkin
    /// Feature: Animal feature
    ///
    ///   @cat
    ///   Scenario: If we feed a hungry cat it will no longer be hungry
    ///     Given a hungry cat
    ///     When I feed the cat
    ///     Then the cat is not hungry
    ///
    ///   @dog
    ///   Scenario: If we feed a satiated dog it will not become hungry
    ///     Given a satiated dog
    ///     When I feed the dog
    ///     Then the dog is not hungry
    /// ```
    /// <script
    ///     id="asciicast-0KvTxnfaMRjsvsIKsalS611Ta"
    ///     src="https://asciinema.org/a/0KvTxnfaMRjsvsIKsalS611Ta.js"
    ///     async data-autoplay="true" data-rows="14">
    /// </script>
    ///
    /// [`Failed`]: event::Step::Failed
    /// [`Feature`]: gherkin::Feature
    /// [`Scenario`]: gherkin::Scenario
    pub async fn filter_run_and_exit<Filter>(self, input: I, filter: Filter)
    where
        Filter: Fn(&gherkin::Feature, Option<&gherkin::Rule>, &gherkin::Scenario) -> bool + 'static,
    {
        let writer = self.filter_run(input, filter).await;
        if writer.execution_has_failed() {
            let mut msg = Vec::with_capacity(3);

            let failed_steps = writer.failed_steps();
            if failed_steps > 0 {
                msg.push(format!(
                    "{failed_steps} step{} failed",
                    if failed_steps > 1 { "s" } else { "" },
                ));
            }

            let parsing_errors = writer.parsing_errors();
            if parsing_errors > 0 {
                msg.push(format!(
                    "{parsing_errors} parsing error{}",
                    if parsing_errors > 1 { "s" } else { "" },
                ));
            }

            panic!("{}", msg.join(", "));
        }
    }
}
