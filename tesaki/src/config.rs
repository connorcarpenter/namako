//! Configuration discovery for Tesaki
//!
//! Tesaki supports automatic configuration discovery via `.tesaki/config.toml` files.
//! Starting from the current working directory, Tesaki walks up parent directories
//! until it finds a `.tesaki/config.toml` file.
//!
//! # Schema
//!
//! ```toml
//! # Required
//! specs_dir = "test/specs"
//! adapter_cmd = "cargo run --manifest-path test/npa/Cargo.toml --"
//!
//! # Optional
//! runner = "mock"           # mock, cmd, or claude
//! runner_cmd = "..."        # only used when runner = "cmd"
//! max_retries = 2
//! max_cert_updates = 3
//! max_runtime_seconds = 600
//! max_files_changed = 10
//! ```
//!
//! # Path Resolution
//!
//! Relative paths in `specs_dir` are resolved relative to the directory containing
//! `.tesaki/` (the config root), not relative to the current working directory.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Configuration file name within the .tesaki directory
const CONFIG_FILENAME: &str = "config.toml";
/// Directory name to search for
const CONFIG_DIR: &str = ".tesaki";

/// Raw configuration as read from config.toml
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    /// Path to the specs directory (relative to config root or absolute)
    pub specs_dir: String,

    /// Adapter command (e.g., "cargo run --manifest-path test/npa/Cargo.toml --")
    pub adapter_cmd: String,

    /// Runner backend to use (mock, cmd, claude)
    #[serde(default)]
    pub runner: Option<String>,

    /// Command template for cmd runner (use {mission_dir} placeholder)
    #[serde(default)]
    pub runner_cmd: Option<String>,

    /// Maximum retry attempts on runner failure
    #[serde(default)]
    pub max_retries: Option<u32>,

    /// Maximum number of autonomous update-cert operations per session
    #[serde(default)]
    pub max_cert_updates: Option<u32>,

    /// Maximum runtime in seconds per mission
    #[serde(default)]
    pub max_runtime_seconds: Option<u64>,

    /// Maximum files the runner may change per mission
    #[serde(default)]
    pub max_files_changed: Option<usize>,

    /// Optional surface definitions (glob patterns)
    #[serde(default)]
    pub surfaces: Option<SurfacesConfig>,
}

/// Surface patterns override
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SurfaceConfig {
    pub patterns: Vec<String>,
}

/// Surface definitions section
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SurfacesConfig {
    #[serde(default)]
    pub spec: Option<SurfaceConfig>,
    #[serde(default)]
    pub tests: Option<SurfaceConfig>,
    #[serde(default)]
    pub sut: Option<SurfaceConfig>,
}

/// Resolved configuration with all paths made absolute
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedConfig {
    /// The directory containing .tesaki/ (the config root)
    pub config_root: PathBuf,

    /// Full path to the config.toml file
    pub config_path: PathBuf,

    /// Resolved specs directory (absolute path)
    pub specs_dir: PathBuf,

    /// Adapter command (with paths resolved)
    pub adapter_cmd: String,

    /// Runner backend
    pub runner: Option<String>,

    /// Runner command template
    pub runner_cmd: Option<String>,

    /// Maximum retry attempts
    pub max_retries: Option<u32>,

    /// Maximum update-cert operations
    pub max_cert_updates: Option<u32>,

    /// Maximum runtime in seconds
    pub max_runtime_seconds: Option<u64>,

    /// Maximum files changed
    pub max_files_changed: Option<usize>,

    /// Surface definitions overrides
    pub surfaces: Option<SurfacesConfig>,
}

impl ResolvedConfig {
    /// Get specs_dir as a PathBuf reference
    pub fn specs_dir(&self) -> &PathBuf {
        &self.specs_dir
    }

    /// Get the adapter command
    pub fn adapter_cmd(&self) -> &str {
        &self.adapter_cmd
    }
}

/// Result of config discovery
#[derive(Debug)]
pub enum ConfigDiscoveryResult {
    /// Found a config file
    Found(ResolvedConfig),
    /// No config file found
    NotFound {
        /// Directories that were searched
        searched_dirs: Vec<PathBuf>,
    },
}

/// Discover and load configuration starting from the given directory.
///
/// Walks up parent directories looking for `.tesaki/config.toml`.
/// First match wins.
pub fn discover_config(start_dir: &Path) -> Result<ConfigDiscoveryResult> {
    let mut searched_dirs = Vec::new();
    let mut current = start_dir.to_path_buf();

    // Canonicalize the start directory if possible
    if let Ok(canonical) = fs::canonicalize(&current) {
        current = canonical;
    }

    loop {
        searched_dirs.push(current.clone());
        let config_dir = current.join(CONFIG_DIR);
        let config_path = config_dir.join(CONFIG_FILENAME);

        if config_path.is_file() {
            let config = load_config(&config_path)?;
            let resolved = resolve_config(config, &current, &config_path)?;
            return Ok(ConfigDiscoveryResult::Found(resolved));
        }

        // Move to parent directory
        match current.parent() {
            Some(parent) if parent != current => {
                current = parent.to_path_buf();
            }
            _ => break,
        }
    }

    Ok(ConfigDiscoveryResult::NotFound { searched_dirs })
}

/// Load configuration from a specific file path
pub fn load_config(path: &Path) -> Result<Config> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;

    toml::from_str(&content)
        .with_context(|| format!("Failed to parse config file: {}", path.display()))
}

/// Resolve a config with paths made absolute
fn resolve_config(config: Config, config_root: &Path, config_path: &Path) -> Result<ResolvedConfig> {
    // Resolve specs_dir relative to config_root
    let specs_dir_raw = PathBuf::from(&config.specs_dir);
    let specs_dir = if specs_dir_raw.is_absolute() {
        specs_dir_raw
    } else {
        config_root.join(&specs_dir_raw)
    };

    // Canonicalize specs_dir if it exists, otherwise keep as absolute
    let specs_dir = fs::canonicalize(&specs_dir).unwrap_or(specs_dir);

    // Resolve any relative paths in adapter_cmd
    // Handle --manifest-path arguments specially
    let adapter_cmd = resolve_adapter_cmd(&config.adapter_cmd, config_root)?;

    Ok(ResolvedConfig {
        config_root: config_root.to_path_buf(),
        config_path: config_path.to_path_buf(),
        specs_dir,
        adapter_cmd,
        runner: config.runner,
        runner_cmd: config.runner_cmd,
        max_retries: config.max_retries,
        max_cert_updates: config.max_cert_updates,
        max_runtime_seconds: config.max_runtime_seconds,
        max_files_changed: config.max_files_changed,
        surfaces: config.surfaces,
    })
}

/// Resolve relative paths in the adapter command (e.g., --manifest-path)
fn resolve_adapter_cmd(adapter: &str, config_root: &Path) -> Result<String> {
    let parts: Vec<&str> = adapter.split_whitespace().collect();
    let mut result = Vec::new();
    let mut i = 0;

    while i < parts.len() {
        if parts[i] == "--manifest-path" && i + 1 < parts.len() {
            result.push(parts[i].to_string());
            i += 1;
            let path = PathBuf::from(parts[i]);
            let abs_path = if path.is_absolute() {
                path
            } else {
                config_root.join(&path)
            };
            // Try to canonicalize, or use the absolute path if it doesn't exist yet
            let final_path = fs::canonicalize(&abs_path).unwrap_or(abs_path);
            result.push(final_path.display().to_string());
        } else if parts[i].starts_with("--manifest-path=") {
            let path_str = parts[i].strip_prefix("--manifest-path=").unwrap_or("");
            let path = PathBuf::from(path_str);
            let abs_path = if path.is_absolute() {
                path
            } else {
                config_root.join(&path)
            };
            let final_path = fs::canonicalize(&abs_path).unwrap_or(abs_path);
            result.push(format!("--manifest-path={}", final_path.display()));
        } else {
            result.push(parts[i].to_string());
        }
        i += 1;
    }

    Ok(result.join(" "))
}

/// Generate a minimal example configuration string
pub fn example_config() -> &'static str {
    r#"# Tesaki configuration
# Place this file at .tesaki/config.toml in your repository root

# Required: Path to specs directory (relative to this config's directory)
specs_dir = "test/specs"

# Required: Adapter command to run the test adapter
adapter_cmd = "cargo run --manifest-path test/npa/Cargo.toml --"

# Optional: Runner backend (mock, cmd, or claude)
runner = "mock"

# Optional: Command for cmd runner (use {mission_dir} placeholder)
# runner_cmd = "my-agent --mission {mission_dir}"

# Optional: Budget limits
max_retries = 2
max_cert_updates = 3
max_runtime_seconds = 600
max_files_changed = 10

# Optional: Surface overrides
[surfaces.spec]
patterns = ["test/specs/**/*.feature"]

[surfaces.tests]
patterns = ["test/tests/**", "test/harness/**"]

[surfaces.sut]
patterns = ["src/**", "client/**", "server/**"]
"#
}

/// Print configuration discovery error with guidance
pub fn print_config_error() {
    eprintln!("Error: No configuration found.");
    eprintln!();
    eprintln!("Either provide -s and -a flags, or create .tesaki/config.toml");
    eprintln!();
    eprintln!("Minimal example config:");
    eprintln!("------------------------");
    eprintln!("{}", example_config());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_discovers_config_in_cwd() {
        let temp = TempDir::new().unwrap();
        let tesaki_dir = temp.path().join(".tesaki");
        fs::create_dir_all(&tesaki_dir).unwrap();

        let config_content = r#"
specs_dir = "test/specs"
adapter_cmd = "cargo run -p npa --"
"#;
        fs::write(tesaki_dir.join("config.toml"), config_content).unwrap();

        let result = discover_config(temp.path()).unwrap();
        match result {
            ConfigDiscoveryResult::Found(config) => {
                assert_eq!(config.specs_dir, temp.path().join("test/specs"));
                assert!(config.adapter_cmd.contains("cargo run -p npa"));
            }
            ConfigDiscoveryResult::NotFound { .. } => panic!("Expected to find config"),
        }
    }

    #[test]
    fn test_discovers_config_in_parent_dir() {
        let temp = TempDir::new().unwrap();
        let tesaki_dir = temp.path().join(".tesaki");
        fs::create_dir_all(&tesaki_dir).unwrap();

        let config_content = r#"
specs_dir = "test/specs"
adapter_cmd = "cargo run -p npa --"
"#;
        fs::write(tesaki_dir.join("config.toml"), config_content).unwrap();

        // Create a subdirectory and search from there
        let subdir = temp.path().join("some/nested/dir");
        fs::create_dir_all(&subdir).unwrap();

        let result = discover_config(&subdir).unwrap();
        match result {
            ConfigDiscoveryResult::Found(config) => {
                assert_eq!(config.config_root, temp.path());
                assert_eq!(config.specs_dir, temp.path().join("test/specs"));
            }
            ConfigDiscoveryResult::NotFound { .. } => panic!("Expected to find config"),
        }
    }

    #[test]
    fn test_relative_specs_dir_resolved_correctly() {
        let temp = TempDir::new().unwrap();
        let tesaki_dir = temp.path().join(".tesaki");
        fs::create_dir_all(&tesaki_dir).unwrap();

        let config_content = r#"
specs_dir = "../other/specs"
adapter_cmd = "test"
"#;
        fs::write(tesaki_dir.join("config.toml"), config_content).unwrap();

        let result = discover_config(temp.path()).unwrap();
        match result {
            ConfigDiscoveryResult::Found(config) => {
                // Should resolve relative to config root, not CWD
                // The path won't be canonicalized since it doesn't exist
                assert!(config.specs_dir.to_string_lossy().contains("other"));
            }
            ConfigDiscoveryResult::NotFound { .. } => panic!("Expected to find config"),
        }
    }

    #[test]
    fn test_missing_config_yields_deterministic_error() {
        let temp = TempDir::new().unwrap();
        // Don't create any config file

        let result = discover_config(temp.path()).unwrap();
        match result {
            ConfigDiscoveryResult::NotFound { searched_dirs } => {
                // Should have searched at least the temp directory
                assert!(!searched_dirs.is_empty());
            }
            ConfigDiscoveryResult::Found(_) => panic!("Should not find config"),
        }
    }

    #[test]
    fn test_optional_fields_default_to_none() {
        let temp = TempDir::new().unwrap();
        let tesaki_dir = temp.path().join(".tesaki");
        fs::create_dir_all(&tesaki_dir).unwrap();

        // Minimal config with only required fields
        let config_content = r#"
specs_dir = "specs"
adapter_cmd = "test"
"#;
        fs::write(tesaki_dir.join("config.toml"), config_content).unwrap();

        let result = discover_config(temp.path()).unwrap();
        match result {
            ConfigDiscoveryResult::Found(config) => {
                assert!(config.runner.is_none());
                assert!(config.runner_cmd.is_none());
                assert!(config.max_retries.is_none());
                assert!(config.max_cert_updates.is_none());
                assert!(config.max_runtime_seconds.is_none());
                assert!(config.max_files_changed.is_none());
                assert!(config.surfaces.is_none());
            }
            ConfigDiscoveryResult::NotFound { .. } => panic!("Expected to find config"),
        }
    }

    #[test]
    fn test_all_optional_fields_parse() {
        let temp = TempDir::new().unwrap();
        let tesaki_dir = temp.path().join(".tesaki");
        fs::create_dir_all(&tesaki_dir).unwrap();

        let config_content = r#"
specs_dir = "specs"
adapter_cmd = "test"
runner = "claude"
runner_cmd = "my-cmd {mission_dir}"
max_retries = 5
max_cert_updates = 10
max_runtime_seconds = 1200
max_files_changed = 20

[surfaces.spec]
patterns = ["specs/**/*.feature"]

[surfaces.tests]
patterns = ["tests/**"]

[surfaces.sut]
patterns = ["src/**"]
"#;
        fs::write(tesaki_dir.join("config.toml"), config_content).unwrap();

        let result = discover_config(temp.path()).unwrap();
        match result {
            ConfigDiscoveryResult::Found(config) => {
                assert_eq!(config.runner, Some("claude".to_string()));
                assert_eq!(config.runner_cmd, Some("my-cmd {mission_dir}".to_string()));
                assert_eq!(config.max_retries, Some(5));
                assert_eq!(config.max_cert_updates, Some(10));
                assert_eq!(config.max_runtime_seconds, Some(1200));
                assert_eq!(config.max_files_changed, Some(20));
                assert!(config.surfaces.is_some());
            }
            ConfigDiscoveryResult::NotFound { .. } => panic!("Expected to find config"),
        }
    }

    #[test]
    fn test_surface_overrides_parse() {
        let temp = TempDir::new().unwrap();
        let tesaki_dir = temp.path().join(".tesaki");
        fs::create_dir_all(&tesaki_dir).unwrap();

        let config_content = r#"
specs_dir = "specs"
adapter_cmd = "test"

[surfaces.spec]
patterns = ["specs/**/*.feature"]

[surfaces.tests]
patterns = ["tests/**"]
"#;
        fs::write(tesaki_dir.join("config.toml"), config_content).unwrap();

        let result = discover_config(temp.path()).unwrap();
        match result {
            ConfigDiscoveryResult::Found(config) => {
                let surfaces = config.surfaces.unwrap();
                assert_eq!(surfaces.spec.unwrap().patterns, vec!["specs/**/*.feature".to_string()]);
                assert_eq!(surfaces.tests.unwrap().patterns, vec!["tests/**".to_string()]);
            }
            ConfigDiscoveryResult::NotFound { .. } => panic!("Expected to find config"),
        }
    }
}
