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
    // No `rerun-if-changed`: watching `.git/HEAD` alone misses same-branch
    // commits (the SHA lives in refs/heads/*), packed-refs, and worktree
    // layouts. The default — rerun when any package file changes — costs one
    // `git rev-parse` per real rebuild and keeps the stamp fresh everywhere.
    println!("cargo:rustc-env=NAMAKO_BUILT_FROM={head}");
    println!(
        "cargo:rustc-env=NAMAKO_SOURCE_DIR={}",
        workspace_root.display()
    );
    println!("cargo:rustc-env=NAMAKO_INSTALL_DIR={manifest_dir}");
}
