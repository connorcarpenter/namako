//! Mission Bundle module for v1.7 Runner Integration.
//!
//! This module implements the filesystem contract between Tesaki and the runner,
//! per GOLD_PLAN.md §10.7.5.
//!
//! # Important: Runner Scope
//!
//! The runner operates on the **specs repository only**. It does NOT edit the
//! Namako/Tesaki toolchain. The mission bundle lives in `.tesaki/` within the
//! specs repository.
//!
//! # Mission Bundle Structure
//!
//! ```text
//! .tesaki/missions/<mission_id>/
//! ├── NEXT_TASK.md          # Single prompt payload for the runner
//! ├── POLICY.md             # Rules: no commits, no orchestration, scope limits
//! ├── EXPECTED.md           # Explicit postconditions Tesaki will check
//! ├── INPUTS/               # Namako packet snapshots relevant to mission
//! │   ├── status.json       # namako status --json output
//! │   ├── review.json       # namako review output
//! │   ├── explain.json      # namako explain output (if relevant)
//! │   ├── gate.json         # namako gate --json output (pre-mission state)
//! │   └── workspace.json    # Workspace configuration
//! └── OUTPUT/               # Runner writes here; Tesaki writes gate results
//!     ├── attempt_report.md # Runner's self-reported attempt summary
//!     ├── transcript.txt    # Optional: runner session transcript
//!     └── gate_result.json  # Tesaki writes post-run gate output
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Mission ID format: `NNN-<task_slug>-<short_hash>`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionId(String);

impl MissionId {
    /// Generate a new mission ID.
    ///
    /// Format: `NNN-<task_slug>-<short_hash>` where:
    /// - NNN = next integer by scanning `.tesaki/missions` and `.tesaki/failed`
    /// - task_slug = sanitized version of task name (max 20 chars)
    /// - short_hash = first 8 chars of blake3 hash of (task identity + packet hashes)
    pub fn generate(
        tesaki_root: &Path,
        task_name: &str,
        identity_data: &str,
    ) -> Result<Self> {
        let next_num = Self::find_next_number(tesaki_root)?;
        let slug = Self::slugify(task_name, 20);
        let short_hash = Self::compute_short_hash(identity_data);

        Ok(MissionId(format!("{:03}-{}-{}", next_num, slug, short_hash)))
    }

    /// Get the mission ID string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Find the next available mission number by scanning both missions and failed directories.
    fn find_next_number(tesaki_root: &Path) -> Result<u32> {
        let missions_dir = tesaki_root.join("missions");
        let failed_dir = tesaki_root.join("failed");

        let mut max_num: u32 = 0;

        for dir in [&missions_dir, &failed_dir] {
            if dir.exists() {
                for entry in fs::read_dir(dir).with_context(|| format!("Failed to read {}", dir.display()))? {
                    let entry = entry?;
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();

                    // Parse the leading NNN from directory name
                    if let Some(num_str) = name_str.split('-').next() {
                        if let Ok(num) = num_str.parse::<u32>() {
                            max_num = max_num.max(num);
                        }
                    }
                }
            }
        }

        Ok(max_num + 1)
    }

    /// Convert a task name to a URL-safe slug.
    fn slugify(name: &str, max_len: usize) -> String {
        let slug: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();

        // Remove consecutive dashes and trim
        let mut result = String::new();
        let mut last_was_dash = true; // Start true to trim leading dashes
        for c in slug.chars().take(max_len) {
            if c == '-' {
                if !last_was_dash {
                    result.push(c);
                    last_was_dash = true;
                }
            } else {
                result.push(c);
                last_was_dash = false;
            }
        }

        // Trim trailing dashes
        result.trim_end_matches('-').to_string()
    }

    /// Compute a short hash (first 8 chars of blake3) for uniqueness.
    fn compute_short_hash(data: &str) -> String {
        let hash = blake3::hash(data.as_bytes());
        hash.to_hex()[..8].to_string()
    }
}

impl std::fmt::Display for MissionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Mission budget configuration per GOLD_PLAN.md §10.7.6.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionBudgets {
    pub max_files_changed: u32,
    pub max_scenarios_promoted: u32,
    pub max_runtime_seconds: u32,
    pub max_retries: u32,
    pub max_cert_updates: u32,
}

impl Default for MissionBudgets {
    fn default() -> Self {
        Self {
            max_files_changed: 10,
            max_scenarios_promoted: 3,
            max_runtime_seconds: 600,
            max_retries: 2,
            max_cert_updates: 3,
        }
    }
}

/// Input files for a mission bundle.
#[derive(Debug, Clone)]
pub struct MissionInputs {
    pub status_json: String,
    pub review_json: String,
    pub gate_json: String,
    pub explain_json: Option<String>,
    pub workspace_json: String,
}

/// Task information for generating NEXT_TASK.md and EXPECTED.md.
#[derive(Debug, Clone)]
pub struct MissionTask {
    /// The selected task name/identifier.
    pub name: String,
    /// Feature file path.
    pub feature_path: String,
    /// Rule name.
    pub rule_name: String,
    /// Description of what needs to be done.
    pub description: String,
    /// Missing step bindings to implement.
    pub missing_bindings: Vec<String>,
    /// Expected postconditions (machine-checkable).
    pub expected_postconditions: Vec<String>,
}

/// Mission Bundle representing the filesystem contract.
#[derive(Debug)]
pub struct MissionBundle {
    pub id: MissionId,
    pub path: PathBuf,
    pub budgets: MissionBudgets,
}

impl MissionBundle {
    /// Create a new mission bundle with all required files.
    ///
    /// This creates the directory structure and writes:
    /// - NEXT_TASK.md
    /// - POLICY.md
    /// - EXPECTED.md
    /// - INPUTS/ directory with all input files
    /// - OUTPUT/ directory (empty, for runner to write to)
    pub fn create(
        tesaki_root: &Path,
        task: &MissionTask,
        inputs: &MissionInputs,
        budgets: MissionBudgets,
        mode: &str,
    ) -> Result<Self> {
        // Generate identity data for hash
        let identity_data = format!(
            "{}:{}:{}:{}",
            task.name,
            inputs.status_json,
            inputs.review_json,
            inputs.gate_json
        );

        let id = MissionId::generate(tesaki_root, &task.name, &identity_data)?;
        let mission_dir = tesaki_root.join("missions").join(id.as_str());

        // Create directory structure
        fs::create_dir_all(&mission_dir)
            .with_context(|| format!("Failed to create mission dir: {}", mission_dir.display()))?;

        let inputs_dir = mission_dir.join("INPUTS");
        fs::create_dir_all(&inputs_dir)?;

        let output_dir = mission_dir.join("OUTPUT");
        fs::create_dir_all(&output_dir)?;

        // Write NEXT_TASK.md
        let next_task_content = Self::generate_next_task_content(task, &id, &budgets);
        fs::write(mission_dir.join("NEXT_TASK.md"), &next_task_content)?;

        // Write POLICY.md
        let policy_content = Self::generate_policy_content(&budgets, mode);
        fs::write(mission_dir.join("POLICY.md"), &policy_content)?;

        // Write EXPECTED.md
        let expected_content = Self::generate_expected_content(task);
        fs::write(mission_dir.join("EXPECTED.md"), &expected_content)?;

        // Write INPUTS
        fs::write(inputs_dir.join("status.json"), &inputs.status_json)?;
        fs::write(inputs_dir.join("review.json"), &inputs.review_json)?;
        fs::write(inputs_dir.join("gate.json"), &inputs.gate_json)?;
        fs::write(inputs_dir.join("workspace.json"), &inputs.workspace_json)?;

        if let Some(explain) = &inputs.explain_json {
            fs::write(inputs_dir.join("explain.json"), explain)?;
        }

        Ok(MissionBundle {
            id,
            path: mission_dir,
            budgets,
        })
    }

    /// Preserve a failed mission by moving it to .tesaki/failed/<mission_id>/.
    pub fn preserve_failed(self) -> Result<PathBuf> {
        let tesaki_root = self.path.parent()
            .and_then(|p| p.parent())
            .context("Invalid mission path structure")?;

        let failed_dir = tesaki_root.join("failed");
        fs::create_dir_all(&failed_dir)?;

        let failed_path = failed_dir.join(self.id.as_str());

        // Move the directory
        fs::rename(&self.path, &failed_path)
            .with_context(|| format!(
                "Failed to move mission from {} to {}",
                self.path.display(),
                failed_path.display()
            ))?;

        Ok(failed_path)
    }

    /// Write gate result JSON to OUTPUT/gate_result.json.
    pub fn write_gate_result(&self, gate_json: &str) -> Result<()> {
        let output_path = self.path.join("OUTPUT").join("gate_result.json");
        fs::write(&output_path, gate_json)
            .with_context(|| format!("Failed to write gate_result.json: {}", output_path.display()))
    }

    /// Check if the runner wrote an attempt report.
    pub fn has_attempt_report(&self) -> bool {
        self.path.join("OUTPUT").join("attempt_report.md").exists()
    }

    /// Generate NEXT_TASK.md content.
    fn generate_next_task_content(task: &MissionTask, id: &MissionId, budgets: &MissionBudgets) -> String {
        let mut content = String::new();

        content.push_str(&format!("# Mission {}\n\n", id));
        content.push_str(&format!("## Task: {}\n\n", task.name));
        content.push_str(&format!("**Feature:** `{}`\n", task.feature_path));
        content.push_str(&format!("**Rule:** {}\n\n", task.rule_name));
        content.push_str("---\n\n");

        content.push_str("## Description\n\n");
        content.push_str(&task.description);
        content.push_str("\n\n");

        if !task.missing_bindings.is_empty() {
            content.push_str("## Missing Bindings to Implement\n\n");
            for binding in &task.missing_bindings {
                content.push_str(&format!("- `{}`\n", binding));
            }
            content.push_str("\n");
        }

        content.push_str("## Budgets\n\n");
        content.push_str("| Limit | Value |\n");
        content.push_str("|-------|-------|\n");
        content.push_str(&format!("| Max files changed | {} |\n", budgets.max_files_changed));
        content.push_str(&format!("| Max scenarios promoted | {} |\n", budgets.max_scenarios_promoted));
        content.push_str(&format!("| Max runtime (seconds) | {} |\n", budgets.max_runtime_seconds));
        content.push_str("\n");

        content.push_str("## Instructions\n\n");
        content.push_str("1. Read POLICY.md for constraints\n");
        content.push_str("2. Read EXPECTED.md for success criteria\n");
        content.push_str("3. Review INPUTS/ for current state\n");
        content.push_str("4. Implement the required changes\n");
        content.push_str("5. Write a summary to OUTPUT/attempt_report.md\n\n");

        content.push_str("---\n\n");
        content.push_str("*Generated by Tesaki v1.7*\n");

        content
    }

    /// Generate POLICY.md content.
    fn generate_policy_content(budgets: &MissionBudgets, mode: &str) -> String {
        let mut content = String::new();

        content.push_str("# Mission Policy\n\n");
        content.push_str("**READ THIS BEFORE MAKING ANY CHANGES.**\n\n");
        content.push_str("---\n\n");

        content.push_str("## Non-Negotiable Rules\n\n");
        content.push_str("1. **NO COMMITS** — Do not run `git commit`. Tesaki handles all commits.\n");
        content.push_str("2. **NO ORCHESTRATION** — Do not call `tesaki run` or `tesaki next`. You are the runner, not the orchestrator.\n");
        content.push_str("3. **NO GIT OPERATIONS** — Do not run `git push`, `git reset`, or other destructive git commands.\n");
        content.push_str("4. **WRITE ATTEMPT REPORT** — Write a summary of your work to `OUTPUT/attempt_report.md`.\n\n");

        content.push_str("## Allowed Edit Surfaces\n\n");
        content.push_str("**IMPORTANT: You operate on the specs repository ONLY.**\n");
        content.push_str("**You must NEVER edit files in the Namako/Tesaki toolchain.**\n\n");

        if mode == "BOOTSTRAP" {
            content.push_str("**MODE: BOOTSTRAP** — You may only edit:\n\n");
            content.push_str("- `test/**` (test harness, specs, bindings)\n\n");
            content.push_str("**FORBIDDEN in BOOTSTRAP mode:**\n");
            content.push_str("- Core project code outside `test/**`\n");
            content.push_str("- Any file outside this repository\n\n");
        } else {
            content.push_str("**MODE: CONSUMPTION** — You may edit:\n\n");
            content.push_str("- `test/**` (test harness, specs, bindings)\n");
            content.push_str("- Core project code as needed to satisfy specs\n\n");
            content.push_str("**FORBIDDEN (always):**\n");
            content.push_str("- Any file outside this repository (especially Namako/Tesaki)\n\n");
        }

        content.push_str("## Budget Limits\n\n");
        content.push_str("| Limit | Value | What Happens If Exceeded |\n");
        content.push_str("|-------|-------|-------------------------|\n");
        content.push_str(&format!(
            "| Max files changed | {} | Mission fails with BUDGET stop condition |\n",
            budgets.max_files_changed
        ));
        content.push_str(&format!(
            "| Max scenarios promoted | {} | Mission fails with BUDGET stop condition |\n",
            budgets.max_scenarios_promoted
        ));
        content.push_str(&format!(
            "| Max runtime (seconds) | {} | Runner process killed, BUDGET stop condition |\n",
            budgets.max_runtime_seconds
        ));
        content.push_str(&format!(
            "| Max retries | {} | Mission fails after this many attempts |\n",
            budgets.max_retries
        ));
        content.push_str("\n");

        content.push_str("## Validation\n\n");
        content.push_str("After you exit, Tesaki will:\n\n");
        content.push_str("1. Run `namako gate --json` to validate your changes\n");
        content.push_str("2. Check that budgets were not exceeded\n");
        content.push_str("3. Record results to `OUTPUT/gate_result.json`\n\n");
        content.push_str("Your `attempt_report.md` is informational only — success is determined by the gate result.\n\n");

        content.push_str("---\n\n");
        content.push_str("*Generated by Tesaki v1.7*\n");

        content
    }

    /// Generate EXPECTED.md content.
    fn generate_expected_content(task: &MissionTask) -> String {
        let mut content = String::new();

        content.push_str("# Expected Postconditions\n\n");
        content.push_str("After runner exit, Tesaki will verify these conditions.\n\n");
        content.push_str("---\n\n");

        content.push_str("## Primary Success Criterion\n\n");
        content.push_str("`namako gate --json` must pass (all phases: lint, run, verify).\n\n");
        content.push_str("If verify fails only due to baseline mismatch and governance allows,\n");
        content.push_str("Tesaki may run `namako update-cert` and re-verify.\n\n");

        if !task.expected_postconditions.is_empty() {
            content.push_str("## Mission-Specific Postconditions\n\n");
            for (i, condition) in task.expected_postconditions.iter().enumerate() {
                content.push_str(&format!("{}. {}\n", i + 1, condition));
            }
            content.push_str("\n");
        }

        content.push_str("## Failure Handling\n\n");
        content.push_str("If the gate fails, the mission will be preserved at `.tesaki/failed/<mission_id>/`\n");
        content.push_str("for inspection. Tesaki will emit a structured stop reason.\n\n");

        content.push_str("---\n\n");
        content.push_str("*Generated by Tesaki v1.7*\n");

        content
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_mission_id_slugify() {
        assert_eq!(MissionId::slugify("Hello World", 20), "hello-world");
        assert_eq!(MissionId::slugify("test_scenario_1", 20), "test-scenario-1");
        assert_eq!(MissionId::slugify("  spaces  ", 20), "spaces");
        assert_eq!(MissionId::slugify("a-b--c---d", 20), "a-b-c-d");
        assert_eq!(MissionId::slugify("VeryLongScenarioNameThatExceedsLimit", 20), "verylongscenarioname");
    }

    #[test]
    fn test_mission_id_short_hash() {
        let hash1 = MissionId::compute_short_hash("test data 1");
        let hash2 = MissionId::compute_short_hash("test data 2");
        let hash3 = MissionId::compute_short_hash("test data 1");

        assert_eq!(hash1.len(), 8);
        assert_ne!(hash1, hash2);
        assert_eq!(hash1, hash3); // Deterministic
    }

    #[test]
    fn test_mission_id_generation() {
        let temp_dir = TempDir::new().unwrap();
        let tesaki_root = temp_dir.path().join(".tesaki");
        fs::create_dir_all(&tesaki_root).unwrap();

        let id1 = MissionId::generate(&tesaki_root, "First Task", "identity1").unwrap();
        assert!(id1.as_str().starts_with("001-"));

        // Create a mission directory to simulate existing missions
        fs::create_dir_all(tesaki_root.join("missions").join("005-old-task-12345678")).unwrap();

        let id2 = MissionId::generate(&tesaki_root, "Second Task", "identity2").unwrap();
        assert!(id2.as_str().starts_with("006-")); // Should be 006 since 005 exists
    }

    #[test]
    fn test_mission_bundle_create() {
        let temp_dir = TempDir::new().unwrap();
        let tesaki_root = temp_dir.path().join(".tesaki");
        fs::create_dir_all(&tesaki_root).unwrap();

        let task = MissionTask {
            name: "Test Task".to_string(),
            feature_path: "features/test.feature".to_string(),
            rule_name: "Rule(01)".to_string(),
            description: "Implement the test scenario.".to_string(),
            missing_bindings: vec!["Given a test".to_string()],
            expected_postconditions: vec!["Gate passes".to_string()],
        };

        let inputs = MissionInputs {
            status_json: r#"{"status": "ok"}"#.to_string(),
            review_json: r#"{"review": "ok"}"#.to_string(),
            gate_json: r#"{"lint": {"status": "pass"}}"#.to_string(),
            explain_json: None,
            workspace_json: r#"{"repos": []}"#.to_string(),
        };

        let bundle = MissionBundle::create(
            &tesaki_root,
            &task,
            &inputs,
            MissionBudgets::default(),
            "BOOTSTRAP",
        ).unwrap();

        // Verify directory structure
        assert!(bundle.path.exists());
        assert!(bundle.path.join("NEXT_TASK.md").exists());
        assert!(bundle.path.join("POLICY.md").exists());
        assert!(bundle.path.join("EXPECTED.md").exists());
        assert!(bundle.path.join("INPUTS").exists());
        assert!(bundle.path.join("INPUTS/status.json").exists());
        assert!(bundle.path.join("INPUTS/review.json").exists());
        assert!(bundle.path.join("INPUTS/gate.json").exists());
        assert!(bundle.path.join("INPUTS/workspace.json").exists());
        assert!(bundle.path.join("OUTPUT").exists());

        // Verify content includes key elements
        let policy = fs::read_to_string(bundle.path.join("POLICY.md")).unwrap();
        assert!(policy.contains("NO COMMITS"));
        assert!(policy.contains("BOOTSTRAP"));
        assert!(policy.contains("specs repository ONLY"));
        assert!(policy.contains("NEVER edit files in the Namako"));
    }

    #[test]
    fn test_mission_bundle_preserve_failed() {
        let temp_dir = TempDir::new().unwrap();
        let tesaki_root = temp_dir.path().join(".tesaki");
        fs::create_dir_all(&tesaki_root).unwrap();

        let task = MissionTask {
            name: "Failing Task".to_string(),
            feature_path: "features/test.feature".to_string(),
            rule_name: "Rule(01)".to_string(),
            description: "This will fail.".to_string(),
            missing_bindings: vec![],
            expected_postconditions: vec![],
        };

        let inputs = MissionInputs {
            status_json: "{}".to_string(),
            review_json: "{}".to_string(),
            gate_json: "{}".to_string(),
            explain_json: None,
            workspace_json: "{}".to_string(),
        };

        let bundle = MissionBundle::create(
            &tesaki_root,
            &task,
            &inputs,
            MissionBudgets::default(),
            "BOOTSTRAP",
        ).unwrap();

        let original_path = bundle.path.clone();
        let mission_id = bundle.id.as_str().to_string();

        // Preserve as failed
        let failed_path = bundle.preserve_failed().unwrap();

        // Original should not exist
        assert!(!original_path.exists());

        // Failed path should exist
        assert!(failed_path.exists());
        assert!(failed_path.ends_with(&mission_id));

        // Content should be preserved
        assert!(failed_path.join("NEXT_TASK.md").exists());
        assert!(failed_path.join("POLICY.md").exists());
    }

    #[test]
    fn test_mission_bundle_write_gate_result() {
        let temp_dir = TempDir::new().unwrap();
        let tesaki_root = temp_dir.path().join(".tesaki");
        fs::create_dir_all(&tesaki_root).unwrap();

        let task = MissionTask {
            name: "Gate Test".to_string(),
            feature_path: "features/test.feature".to_string(),
            rule_name: "Rule(01)".to_string(),
            description: "Test gate result writing.".to_string(),
            missing_bindings: vec![],
            expected_postconditions: vec![],
        };

        let inputs = MissionInputs {
            status_json: "{}".to_string(),
            review_json: "{}".to_string(),
            gate_json: "{}".to_string(),
            explain_json: None,
            workspace_json: "{}".to_string(),
        };

        let bundle = MissionBundle::create(
            &tesaki_root,
            &task,
            &inputs,
            MissionBudgets::default(),
            "BOOTSTRAP",
        ).unwrap();

        let gate_result = r#"{"lint": {"status": "pass"}, "run": {"status": "pass"}, "verify": {"status": "pass"}}"#;
        bundle.write_gate_result(gate_result).unwrap();

        let result_path = bundle.path.join("OUTPUT/gate_result.json");
        assert!(result_path.exists());
        let content = fs::read_to_string(result_path).unwrap();
        assert_eq!(content, gate_result);
    }
}
