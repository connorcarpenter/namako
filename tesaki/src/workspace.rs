//! Workspace module for v1.7 Runner Integration.
//!
//! Handles change tracking for runner missions.
//!
//! # Important: Single-Repo Model
//!
//! The runner operates on the **specs repository only** (the repository containing
//! the specs directory). It does NOT edit the Namako/Tesaki toolchain code.
//!
//! The "workspace" for a mission is simply the git repository where specs live.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Workspace configuration for a mission.
///
/// This captures the specs repository where the runner will operate.
/// The runner NEVER edits the Namako/Tesaki toolchain repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    /// Root of the specs repository.
    /// This is where the runner operates.
    pub repo_root: PathBuf,

    /// Path to the specs directory within the repo.
    pub specs_dir: PathBuf,

    /// Adapter invocation string.
    pub adapter_cmd: String,

    /// Operating mode (BOOTSTRAP or CONSUMPTION).
    pub mode: String,

    /// Budget configuration.
    pub budgets: crate::mission::MissionBudgets,
}

/// Represents the workspace state (clean/dirty).
#[derive(Debug, Clone)]
pub struct WorkspaceState {
    /// Whether the repo is clean (no uncommitted changes).
    pub is_clean: bool,

    /// List of dirty files (if not clean).
    pub dirty_files: Vec<String>,
}

/// Change summary after runner execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangesSummary {
    /// Total number of files changed.
    pub total_files_changed: usize,

    /// List of changed files.
    pub changed_files: Vec<String>,

    /// Whether the file change budget was exceeded.
    pub budget_exceeded: bool,
}

/// Workspace helper for managing repo state during missions.
///
/// # Runner Scope
///
/// The runner operates ONLY on the specs repository. Edit surfaces are
/// controlled by the operating mode (BOOTSTRAP vs CONSUMPTION) and
/// configured in the mission's POLICY.md.
///
/// The runner NEVER edits the Namako/Tesaki toolchain repository.
#[derive(Debug)]
pub struct Workspace {
    config: WorkspaceConfig,
}

impl Workspace {
    /// Create a new Workspace from the given configuration.
    pub fn new(config: WorkspaceConfig) -> Self {
        Self { config }
    }

    /// Derive workspace configuration from specs directory.
    ///
    /// This finds the git repo root from the specs directory and builds
    /// the workspace config. The specs directory must be inside a git repo.
    pub fn from_specs_dir(
        specs_dir: &Path,
        adapter_cmd: &str,
        mode: &str,
        budgets: crate::mission::MissionBudgets,
    ) -> Result<Self> {
        let specs_dir = std::fs::canonicalize(specs_dir)
            .with_context(|| format!("Failed to canonicalize specs dir: {}", specs_dir.display()))?;

        // Find the repo root (specs repository)
        let repo_root = find_git_root(&specs_dir)?;

        let config = WorkspaceConfig {
            repo_root,
            specs_dir,
            adapter_cmd: adapter_cmd.to_string(),
            mode: mode.to_string(),
            budgets,
        };

        Ok(Self { config })
    }

    /// Get the workspace configuration.
    pub fn config(&self) -> &WorkspaceConfig {
        &self.config
    }

    /// Check if the repo is clean (no uncommitted changes).
    ///
    /// Missions should start with a clean repo to ensure we can accurately
    /// track what the runner changed.
    pub fn check_clean(&self) -> Result<WorkspaceState> {
        let (is_clean, dirty_files) = check_repo_clean(&self.config.repo_root)?;

        Ok(WorkspaceState {
            is_clean,
            dirty_files,
        })
    }

    /// Compute changed files after runner execution.
    ///
    /// This counts all modified, staged, and untracked files in the repo.
    pub fn compute_changes(&self) -> Result<ChangesSummary> {
        let changed_files = get_changed_files(&self.config.repo_root)?;
        let total = changed_files.len();
        let budget_exceeded = total > self.config.budgets.max_files_changed as usize;

        Ok(ChangesSummary {
            total_files_changed: total,
            changed_files,
            budget_exceeded,
        })
    }

    /// Export workspace config as JSON for mission INPUTS.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(&self.config)
            .context("Failed to serialize workspace config")
    }

    /// Get the working directory for the runner.
    ///
    /// This is the repo root where the runner should execute.
    pub fn working_dir(&self) -> &Path {
        &self.config.repo_root
    }
}

/// Find the git repository root from a given path.
fn find_git_root(from: &Path) -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(from)
        .output()
        .with_context(|| format!("Failed to run git in {}", from.display()))?;

    if !output.status.success() {
        bail!(
            "Not a git repository: {}. stderr: {}",
            from.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let root = String::from_utf8(output.stdout)
        .context("Git output is not valid UTF-8")?
        .trim()
        .to_string();

    Ok(PathBuf::from(root))
}

/// Check if a git repository is clean.
///
/// Returns (is_clean, list of dirty files).
fn check_repo_clean(repo_root: &Path) -> Result<(bool, Vec<String>)> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_root)
        .output()
        .with_context(|| format!("Failed to run git status in {}", repo_root.display()))?;

    if !output.status.success() {
        bail!(
            "git status failed in {}: {}",
            repo_root.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8(output.stdout)
        .context("Git output is not valid UTF-8")?;

    if stdout.trim().is_empty() {
        Ok((true, vec![]))
    } else {
        let dirty_files: Vec<String> = stdout
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| {
                // Format is: XY filename or XY -> filename (for renames)
                line.get(3..).unwrap_or(line).to_string()
            })
            .collect();

        Ok((false, dirty_files))
    }
}

/// Get list of changed files (staged + unstaged + untracked) in a repo.
fn get_changed_files(repo_root: &Path) -> Result<Vec<String>> {
    let mut files = HashSet::new();

    // Get unstaged changes
    let output = Command::new("git")
        .args(["diff", "--name-only"])
        .current_dir(repo_root)
        .output()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if !line.is_empty() {
                files.insert(line.to_string());
            }
        }
    }

    // Get staged changes
    let output = Command::new("git")
        .args(["diff", "--name-only", "--staged"])
        .current_dir(repo_root)
        .output()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if !line.is_empty() {
                files.insert(line.to_string());
            }
        }
    }

    // Get untracked files
    let output = Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .current_dir(repo_root)
        .output()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if !line.is_empty() {
                files.insert(line.to_string());
            }
        }
    }

    let mut sorted: Vec<String> = files.into_iter().collect();
    sorted.sort();
    Ok(sorted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn init_git_repo(dir: &Path) -> Result<()> {
        Command::new("git")
            .args(["init"])
            .current_dir(dir)
            .output()?;
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir)
            .output()?;
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .output()?;
        // Create initial commit so we have a valid repo
        std::fs::write(dir.join("README.md"), "# Test\n")?;
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir)
            .output()?;
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(dir)
            .output()?;
        Ok(())
    }

    #[test]
    fn test_find_git_root() {
        let temp_dir = TempDir::new().unwrap();
        init_git_repo(temp_dir.path()).unwrap();

        let subdir = temp_dir.path().join("subdir");
        std::fs::create_dir_all(&subdir).unwrap();

        let root = find_git_root(&subdir).unwrap();
        assert_eq!(root, std::fs::canonicalize(temp_dir.path()).unwrap());
    }

    #[test]
    fn test_check_repo_clean() {
        let temp_dir = TempDir::new().unwrap();
        init_git_repo(temp_dir.path()).unwrap();

        // Should be clean after initial commit
        let (is_clean, dirty_files) = check_repo_clean(temp_dir.path()).unwrap();
        assert!(is_clean);
        assert!(dirty_files.is_empty());

        // Add a new file (untracked)
        std::fs::write(temp_dir.path().join("new_file.txt"), "content").unwrap();

        let (is_clean, dirty_files) = check_repo_clean(temp_dir.path()).unwrap();
        assert!(!is_clean);
        assert!(dirty_files.iter().any(|f| f.contains("new_file.txt")));
    }

    #[test]
    fn test_get_changed_files() {
        let temp_dir = TempDir::new().unwrap();
        init_git_repo(temp_dir.path()).unwrap();

        // Add a new file
        std::fs::write(temp_dir.path().join("changed.txt"), "content").unwrap();

        let changes = get_changed_files(temp_dir.path()).unwrap();
        assert!(changes.contains(&"changed.txt".to_string()));
    }

    #[test]
    fn test_workspace_config_serialization() {
        let config = WorkspaceConfig {
            repo_root: PathBuf::from("/test/myproject"),
            specs_dir: PathBuf::from("/test/myproject/test/specs"),
            adapter_cmd: "cargo run -p npa --".to_string(),
            mode: "CONSUMPTION".to_string(),
            budgets: crate::mission::MissionBudgets::default(),
        };

        let json = serde_json::to_string_pretty(&config).unwrap();
        assert!(json.contains("repo_root"));
        assert!(json.contains("CONSUMPTION"));

        // Can deserialize back
        let parsed: WorkspaceConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.mode, "CONSUMPTION");
    }
}
