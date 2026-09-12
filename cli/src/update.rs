//! `namako update` command implementation.
//!
//! Brings the installed binary up to the remote mainline, bosun-style: the
//! binary carries the commit it was built from (stamped by `cli/build.rs`),
//! compares it against `git ls-remote origin HEAD`, and reinstalls with
//! `cargo install --locked --force --path` when behind. Explicit command
//! only — no other subcommand ever checks freshness or rebuilds.
//!
//! Deliberately read-only toward the source checkout: if the local tree does
//! not contain the remote commit, the command says so and stops instead of
//! mutating the checkout. `ls-remote` (not `fetch`) answers "what is latest"
//! without touching local refs.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use clap::Args;

/// The commit `HEAD` pointed at when this binary was compiled, or `unknown`.
const BUILT_FROM: &str = env!("NAMAKO_BUILT_FROM");

/// Absolute path of the source tree this binary was compiled from.
const SOURCE_DIR: &str = env!("NAMAKO_SOURCE_DIR");

/// The crate directory `cargo install --path` must point at.
const INSTALL_DIR: &str = env!("NAMAKO_INSTALL_DIR");

/// Arguments for the update command.
#[derive(Args, Debug)]
pub struct UpdateArgs {}

/// How the installed binary relates to the remote mainline.
#[derive(Debug, PartialEq, Eq)]
enum Freshness {
    /// Built from the remote HEAD (or ahead of it with local commits).
    Current,
    /// The remote has a commit this binary was not built from.
    Stale { remote_main: String },
    /// Nothing trustworthy to compare: unstamped build, missing source
    /// tree, or the remote is unreachable. Never guess from this state.
    Unknown(String),
}

/// Run the update command.
pub fn run(_args: UpdateArgs) -> Result<()> {
    match check() {
        Freshness::Current => {
            println!("namako is current (built from {}).", short(BUILT_FROM));
            Ok(())
        }
        Freshness::Unknown(reason) => {
            bail!("cannot determine freshness: {reason}; refusing to reinstall")
        }
        Freshness::Stale { remote_main } => {
            let source_dir = Path::new(SOURCE_DIR);
            if !rebuild_would_help(source_dir, &remote_main) {
                bail!(
                    "namako is built from {}, but origin HEAD is at {}, and the checkout at {} \
                     does not contain it. Update that checkout (e.g. git pull) and run \
                     `namako update` again; rebuilding now would stamp the same commit.",
                    short(BUILT_FROM),
                    short(&remote_main),
                    source_dir.display()
                );
            }
            let exe = std::env::current_exe().context("cannot locate running binary")?;
            let Some(root) = install_root(&exe, source_dir) else {
                bail!(
                    "running from {}: refusing to reinstall a binary that is not in an installed <root>/bin layout",
                    exe.display()
                );
            };
            println!(
                "namako is built from {}, but origin HEAD is at {}. Reinstalling.",
                short(BUILT_FROM),
                short(&remote_main)
            );
            let status = reinstall(&root).context("could not run cargo install")?;
            if status.success() {
                println!(
                    "Reinstalled from {}. New binary takes effect on next invocation.",
                    short(&remote_main)
                );
                Ok(())
            } else {
                bail!(
                    "reinstall failed with {status}. Run `cargo install --locked --force --path {INSTALL_DIR} --root {}` to see why.",
                    root.display()
                );
            }
        }
    }
}

/// Compare the stamped commit against the remote HEAD via `ls-remote`.
/// Read-only: answers "what is latest" without touching local refs.
fn check() -> Freshness {
    inspect(
        Path::new(SOURCE_DIR),
        BUILT_FROM,
        &remote_head(Path::new(SOURCE_DIR)),
    )
}

fn inspect(source_dir: &Path, built_from: &str, remote: &Result<String>) -> Freshness {
    if built_from == "unknown" || !source_dir.join(".git").exists() {
        return Freshness::Unknown("unstamped build or missing source tree".into());
    }
    let remote_main = match remote {
        Ok(sha) => sha.clone(),
        Err(e) => return Freshness::Unknown(format!("origin unreachable: {e:#}")),
    };
    if remote_main == built_from {
        return Freshness::Current;
    }
    if rev_parse(source_dir, &format!("{built_from}^{{commit}}")).is_none() {
        return Freshness::Stale { remote_main };
    }
    if is_ancestor(source_dir, &remote_main, built_from) {
        Freshness::Current
    } else {
        Freshness::Stale { remote_main }
    }
}

/// Latest commit on the remote's default branch, without fetching.
fn remote_head(source_dir: &Path) -> Result<String> {
    let out = Command::new("git")
        .args(["ls-remote", "origin", "HEAD"])
        .current_dir(source_dir)
        .output()
        .context("failed to run git ls-remote")?;
    if !out.status.success() {
        bail!("git ls-remote exited with {}", out.status);
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    stdout
        .split_whitespace()
        .next()
        .map(str::to_string)
        .context("empty ls-remote output")
}

/// Whether rebuilding could actually clear the staleness: the local checkout
/// must already contain the remote commit, or the rebuild restamps the same
/// commit and the next run finds it stale again.
fn rebuild_would_help(source_dir: &Path, remote_main: &str) -> bool {
    rev_parse(source_dir, "HEAD").is_some_and(|head| is_ancestor(source_dir, remote_main, &head))
}

/// Rebuild and replace the installed binary. The build cache goes to a temp
/// dir, never into the source checkout: reinstalling must not write into the
/// tree it was built from.
fn reinstall(root: &Path) -> std::io::Result<std::process::ExitStatus> {
    let target_dir = std::env::temp_dir().join("namako-update-build");
    Command::new("cargo")
        .stdin(Stdio::null())
        .env("CARGO_TARGET_DIR", target_dir)
        .args(["install", "--locked", "--force", "--path"])
        .arg(INSTALL_DIR)
        .arg("--root")
        .arg(root)
        .status()
}

/// Where `cargo install --root` should point, derived from the running binary.
/// `~/.local/bin/namako` gives `~/.local`. Returns `None` for a binary running
/// out of the source tree's own `target/` directory (cargo run / tests).
fn install_root(current_exe: &Path, source_dir: &Path) -> Option<PathBuf> {
    if current_exe.starts_with(source_dir.join("target")) {
        return None;
    }
    let bin_dir = current_exe.parent()?;
    if bin_dir.file_name()? != "bin" {
        return None;
    }
    Some(bin_dir.parent()?.to_path_buf())
}

fn rev_parse(source_dir: &Path, reference: &str) -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", reference])
        .current_dir(source_dir)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn is_ancestor(source_dir: &Path, ancestor: &str, descendant: &str) -> bool {
    Command::new("git")
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .current_dir(source_dir)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn short(commit: &str) -> &str {
    commit.get(..7).unwrap_or(commit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .expect("git must be available for update tests");
        assert!(status.success(), "git {args:?} failed");
    }

    fn repo_with_commits(n: u32) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        git(dir.path(), &["init", "-q"]);
        git(dir.path(), &["config", "user.email", "t@t"]);
        git(dir.path(), &["config", "user.name", "t"]);
        for i in 0..n {
            std::fs::write(dir.path().join("f"), format!("{i}")).unwrap();
            git(dir.path(), &["add", "."]);
            git(dir.path(), &["commit", "-qm", &format!("c{i}")]);
        }
        dir
    }

    fn head(dir: &Path) -> String {
        rev_parse(dir, "HEAD").unwrap()
    }

    #[test]
    fn same_commit_is_current() {
        let repo = repo_with_commits(2);
        let sha = head(repo.path());
        assert_eq!(
            inspect(repo.path(), &sha, &Ok(sha.clone())),
            Freshness::Current
        );
    }

    #[test]
    fn local_commit_ahead_of_remote_is_current() {
        let repo = repo_with_commits(2);
        let old = rev_parse(repo.path(), "HEAD~1").unwrap();
        let new = head(repo.path());
        // Binary built from newest, remote still at older: ahead counts.
        assert_eq!(inspect(repo.path(), &new, &Ok(old)), Freshness::Current);
    }

    #[test]
    fn remote_ahead_is_stale() {
        let repo = repo_with_commits(2);
        let old = rev_parse(repo.path(), "HEAD~1").unwrap();
        let new = head(repo.path());
        assert_eq!(
            inspect(repo.path(), &old, &Ok(new.clone())),
            Freshness::Stale { remote_main: new }
        );
    }

    #[test]
    fn unstamped_build_is_unknown() {
        let repo = repo_with_commits(1);
        let sha = head(repo.path());
        assert!(matches!(
            inspect(repo.path(), "unknown", &Ok(sha)),
            Freshness::Unknown(_)
        ));
    }

    #[test]
    fn dir_without_git_is_unknown() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(matches!(
            inspect(dir.path(), "abc123", &Ok("def456".into())),
            Freshness::Unknown(_)
        ));
    }

    #[test]
    fn built_commit_gone_locally_is_stale() {
        let repo = repo_with_commits(1);
        // A stamp that resolves nowhere locally with a differing remote.
        assert_eq!(
            inspect(
                repo.path(),
                "0000000000000000000000000000000000000000",
                &Ok(head(repo.path()))
            ),
            Freshness::Stale {
                remote_main: head(repo.path())
            }
        );
    }

    #[test]
    fn unreachable_remote_is_unknown() {
        let repo = repo_with_commits(1);
        let sha = head(repo.path());
        assert!(matches!(
            inspect(repo.path(), &sha, &Err(anyhow::anyhow!("offline"))),
            Freshness::Unknown(_)
        ));
    }

    #[test]
    fn target_dir_binary_has_no_install_root() {
        let source = Path::new("/src/namako");
        let exe = Path::new("/src/namako/target/debug/namako");
        assert_eq!(install_root(exe, source), None);
    }

    #[test]
    fn installed_binary_derives_root() {
        let source = Path::new("/src/namako");
        let exe = Path::new("/home/u/.local/bin/namako");
        assert_eq!(
            install_root(exe, source),
            Some(PathBuf::from("/home/u/.local"))
        );
    }
}
