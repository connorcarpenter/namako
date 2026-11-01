//! Stamp the source commit into `namako_cli` so `namako update` can tell whether
//! the installed binary matches the remote mainline. A tree without usable git
//! metadata stamps `unknown`, and the update command refuses to guess from it.

use std::process::Command;

fn git(dir: &std::path::Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = std::path::Path::new(&manifest_dir)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap();
    let head = git(&workspace_root, &["rev-parse", "HEAD"]).unwrap_or("unknown".into());
    // Rerun whenever HEAD moves. Cargo's default (rerun when a file of this
    // package changes) misses commits that touch only other workspace
    // members, which left the stamp on an older commit. Watch HEAD, the
    // branch ref it names, and packed-refs; `--git-path` resolves each for
    // worktrees too. A path that does not exist yet makes Cargo rerun every
    // build, which keeps the stamp correct.
    let mut watched = vec!["HEAD".to_string(), "packed-refs".to_string()];
    if let Some(branch) = git(&workspace_root, &["symbolic-ref", "-q", "HEAD"]) {
        watched.push(branch);
    }
    for name in &watched {
        match git(
            &workspace_root,
            &["rev-parse", "--path-format=absolute", "--git-path", name],
        ) {
            Some(path) => println!("cargo:rerun-if-changed={path}"),
            None => println!("cargo:rerun-if-changed=.git-unresolved-{name}"),
        }
    }
    println!("cargo:rustc-env=NAMAKO_BUILT_FROM={head}");
    println!(
        "cargo:rustc-env=NAMAKO_SOURCE_DIR={}",
        workspace_root.display()
    );
    println!("cargo:rustc-env=NAMAKO_INSTALL_DIR={manifest_dir}");
}
