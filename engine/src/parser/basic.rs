

//! Default [`Parser`] implementation.

use std::{
    path::{Path, PathBuf},
    str::FromStr,
    vec,
};

use futures::stream;
use gherkin::GherkinEnv;
use globwalk::{GlobWalker, GlobWalkerBuilder};
use itertools::Itertools as _;

use super::{Error as ParseError, Parser};
use crate::feature::Ext as _;

/// CLI options of a [`Basic`] [`Parser`].
#[derive(Clone, Debug, Default, clap::Args)]
#[group(skip)]
pub struct Cli {
    /// Glob pattern to look for feature files with. If not specified, looks for
    /// `*.feature` files in the path configured in the test runner.
    #[arg(
        id = "input",
        long = "input",
        short = 'i',
        value_name = "glob",
        global = true
    )]
    pub features: Option<Walker>,
}

/// Default [`Parser`].
///
/// As there is no async runtime-agnostic way to interact with IO, this
/// [`Parser`] is blocking.
#[derive(Copy, Clone, Debug, Default)]
pub struct Basic {}

impl<I: AsRef<Path>> Parser<I> for Basic {
    type Cli = Cli;

    type Output =
        stream::Iter<vec::IntoIter<Result<gherkin::Feature, ParseError>>>;

    fn parse(self, input: I, cli: Self::Cli) -> Self::Output {
        let walk = |walker: GlobWalker| {
            walker
                .filter_map(Result::ok)
                .sorted_by(|l, r| Ord::cmp(l.path(), r.path()))
                .map(|file| {
                    gherkin::Feature::parse_path(file.path(), GherkinEnv::default())
                })
                .collect::<Vec<_>>()
        };

        let get_features_path = || {
            let path = input.as_ref();
            path.canonicalize()
                .or_else(|_| {
                    let buf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
                        path.strip_prefix("/")
                            .or_else(|_| path.strip_prefix("./"))
                            .unwrap_or(path),
                    );
                    buf.as_path().canonicalize()
                })
                .map_err(|e| gherkin::ParseFileError::Reading {
                    path: path.to_path_buf(),
                    source: e,
                })
        };

        let features = || {
            let features = if let Some(walker) = cli.features {
                walk(globwalk::glob(walker.0).unwrap_or_else(|e| {
                    unreachable!("invalid glob pattern: {e}")
                }))
            } else {
                let feats_path = match get_features_path() {
                    Ok(p) => p,
                    Err(e) => return vec![Err(e.into())],
                };

                if feats_path.is_file() {
                    vec![gherkin::Feature::parse_path(feats_path, GherkinEnv::default())]
                } else {
                    let w = GlobWalkerBuilder::new(feats_path, "*.feature")
                        .case_insensitive(true)
                        .build()
                        .unwrap_or_else(|e| {
                            unreachable!("`GlobWalkerBuilder` panicked: {e}")
                        });
                    walk(w)
                }
            };


            features
                .into_iter()
                .map(|f| match f {
                    Ok(f) => f.expand_examples().map_err(ParseError::from),
                    Err(e) => Err(e.into()),
                })
                .collect()
        };

        stream::iter(features())
    }
}

impl Basic {
    /// Creates a new [`Basic`] [`Parser`].
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }
}

/// Wrapper over [`GlobWalker`] implementing a [`FromStr`].
#[derive(Clone, Debug)]
pub struct Walker(String);

impl FromStr for Walker {
    type Err = globwalk::GlobError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        globwalk::glob(s).map(|_| Self(s.to_owned()))
    }
}
