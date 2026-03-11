//! Writer wrapper for CLI compatibility.

use derive_more::with_trait::{Deref, DerefMut};

use crate::{World, Writer, event, parser, writer};

/// Wrapper for a [`Writer`] that adds compatibility CLI arguments that are ignored.
#[derive(Clone, Debug, Deref, DerefMut)]
pub struct Compat<Inner>(pub Inner);

/// CLI options for [`Compat`].
#[derive(Clone, Debug, Default, clap::Args)]
pub struct Cli<Inner: clap::Args> {
    /// Inner CLI options.
    #[command(flatten)]
    pub inner: Inner,

    /// Ignored compatibility CLI option.
    #[arg(long, value_name = "FORMAT", hide = true)]
    pub format: Option<String>,

    /// Ignored compatibility CLI option.
    #[arg(long, hide = true)]
    pub show_output: bool,

    /// Ignored compatibility CLI option.
    #[arg(short = 'Z', hide = true)]
    pub z: Option<String>,
}

impl<W, Inner> Writer<W> for Compat<Inner>
where
    W: World,
    Inner: Writer<W>,
{
    type Cli = Cli<Inner::Cli>;

    async fn handle_event(
        &mut self,
        event: parser::Result<event::Event<event::Namako<W>>>,
        cli: &Self::Cli,
    ) {
        self.0.handle_event(event, &cli.inner).await;
    }
}

impl<W, Inner> writer::Stats<W> for Compat<Inner>
where
    W: World,
    Inner: writer::Stats<W>,
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

impl<Inner: writer::Normalized> writer::Normalized for Compat<Inner> {}

impl<Inner: writer::NonTransforming> writer::NonTransforming for Compat<Inner> {}

impl<W, Val, Inner> writer::Arbitrary<W, Val> for Compat<Inner>
where
    W: World,
    Inner: writer::Arbitrary<W, Val>,
{
    async fn write(&mut self, val: Val) {
        self.0.write(val).await;
    }
}
