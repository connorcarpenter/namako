//! Mission Bundle module for v1.8 Runner Integration.
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
//! ├── MISSION.md            # Single prompt payload for the runner
//! ├── POLICY.md             # Rules: no commits, no orchestration, scope limits
//! ├── INPUTS/               # Namako packet snapshots relevant to mission
//! │   ├── status.json       # namako status --json output
//! │   ├── review.json       # namako review output
//! │   ├── explain.json      # namako explain output (if relevant)
//! │   ├── gate.json         # namako gate --json output (pre-mission state)
//! │   ├── repo_state.json   # computed RepoState
//! │   └── mission_config.json # mission config + surface policy
//! │   └── workspace.json    # Workspace configuration
//! ├── POST_GATE.json        # Tesaki writes post-run gate output
//! └── RUNNER_OUTPUT/        # Runner writes here
//!     ├── attempt_report.md # Runner's self-reported attempt summary
//!     ├── transcript.txt    # Optional: runner session transcript
//!     └── stop_reason.json  # Runner stop reason (if available)
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::mission_type::{MissionBrief, MissionType};
use crate::prompts::{
    render_mission_md, render_policy_md, BudgetsContext, MissionContext, PolicyContext,
    SurfaceDefinitionsContext, SurfacePolicyContext, TESAKI_VERSION, PreviousFailureContext,
};
use crate::stage::Stage;
use crate::surface_policy::{SurfaceDefinition, SurfacePolicy};

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
    pub repo_state_json: String,
}

/// Mission config written to INPUTS/mission_config.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionConfig {
    pub mission_type: String,
    pub stage: String,
    pub surface_policy: SurfacePolicy,
    pub surface_definitions: SurfaceDefinitions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfaceDefinitions {
    pub spec: SurfaceDefinition,
    pub tests_bindings: SurfaceDefinition,
    pub sut: SurfaceDefinition,
}

/// Mission Bundle representing the filesystem contract.
#[derive(Debug)]
pub struct MissionBundle {
    pub id: MissionId,
    pub path: PathBuf,
    #[allow(dead_code)]
    pub budgets: MissionBudgets,
}

impl MissionBundle {
    /// Create a new mission bundle with all required files.
    ///
    /// This creates the directory structure and writes:
    /// - MISSION.md
    /// - POLICY.md
    /// - INPUTS/ directory with all input files
    /// - RUNNER_OUTPUT/ directory (empty, for runner to write to)
    pub fn create(
        tesaki_root: &Path,
        mission_type: &MissionType,
        brief: &MissionBrief,
        stage: &Stage,
        surface_policy: &SurfacePolicy,
        surface_definitions: &SurfaceDefinitions,
        inputs: &MissionInputs,
        budgets: MissionBudgets,
        previous_failure: Option<PreviousFailureContext>,
    ) -> Result<Self> {
        // Generate identity data for hash
        let identity_data = format!(
            "{}:{}:{}:{}",
            mission_type.name(),
            inputs.status_json,
            inputs.review_json,
            inputs.gate_json
        );

        let id = MissionId::generate(tesaki_root, mission_type.name(), &identity_data)?;
        let mission_dir = tesaki_root.join("missions").join(id.as_str());

        // Create directory structure
        fs::create_dir_all(&mission_dir)
            .with_context(|| format!("Failed to create mission dir: {}", mission_dir.display()))?;

        let inputs_dir = mission_dir.join("INPUTS");
        fs::create_dir_all(&inputs_dir)?;

        let runner_output_dir = mission_dir.join("RUNNER_OUTPUT");
        fs::create_dir_all(&runner_output_dir)?;

        // Write MISSION.md
        let mission_content = Self::generate_mission_content(
            brief,
            mission_type,
            stage,
            surface_policy,
            surface_definitions,
            &id,
            &budgets,
            previous_failure,
        );
        fs::write(mission_dir.join("MISSION.md"), &mission_content)?;

        // Write POLICY.md
        let policy_content = Self::generate_policy_content(&budgets, surface_policy, surface_definitions);
        fs::write(mission_dir.join("POLICY.md"), &policy_content)?;

        // Write INPUTS
        fs::write(inputs_dir.join("status.json"), &inputs.status_json)?;
        fs::write(inputs_dir.join("review.json"), &inputs.review_json)?;
        fs::write(inputs_dir.join("gate.json"), &inputs.gate_json)?;
        fs::write(inputs_dir.join("workspace.json"), &inputs.workspace_json)?;
        fs::write(inputs_dir.join("repo_state.json"), &inputs.repo_state_json)?;

        let mission_config = MissionConfig {
            mission_type: mission_type.name().to_string(),
            stage: stage.name().to_string(),
            surface_policy: surface_policy.clone(),
            surface_definitions: surface_definitions.clone(),
        };
        let mission_config_json = serde_json::to_string_pretty(&mission_config)
            .context("Failed to serialize mission_config.json")?;
        fs::write(inputs_dir.join("mission_config.json"), mission_config_json)?;

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

    /// Write gate result JSON to POST_GATE.json.
    pub fn write_gate_result(&self, gate_json: &str) -> Result<()> {
        let output_path = self.path.join("POST_GATE.json");
        fs::write(&output_path, gate_json)
            .with_context(|| format!("Failed to write POST_GATE.json: {}", output_path.display()))
    }

    /// Check if the runner wrote an attempt report.
    #[allow(dead_code)]
    pub fn has_attempt_report(&self) -> bool {
        self.path.join("RUNNER_OUTPUT").join("attempt_report.md").exists()
    }

    /// Generate MISSION.md content using templates.
    fn generate_mission_content(
        brief: &MissionBrief,
        mission_type: &MissionType,
        stage: &Stage,
        surface_policy: &SurfacePolicy,
        surface_definitions: &SurfaceDefinitions,
        id: &MissionId,
        budgets: &MissionBudgets,
        previous_failure: Option<PreviousFailureContext>,
    ) -> String {
        let ctx = MissionContext {
            mission_id: id.as_str().to_string(),
            mission_type: mission_type.name().to_string(),
            stage: stage.name().to_string(),
            target: mission_type.target_label(),
            objective: brief.objective.clone(),
            context: brief.context.clone(),
            validation_criteria: brief.validation_criteria.clone(),
            surface_policy: SurfacePolicyContext::from(surface_policy),
            surface_definitions: SurfaceDefinitionsContext::from(surface_definitions),
            budgets: BudgetsContext::from(budgets),
            version: TESAKI_VERSION.to_string(),
            previous_failure,
            previous_lessons: None, // Will be populated by run_run
        };

        render_mission_md(&ctx).unwrap_or_else(|e| {
            // Fallback to simple content if template fails
            log::error!("Failed to render MISSION.md template: {}", e);
            format!(
                "# Mission {}\n\n**Type:** {}\n**Stage:** {}\n\n## Objective\n\n{}\n",
                id, mission_type.name(), stage.name(), brief.objective
            )
        })
    }

    /// Generate POLICY.md content using templates.
    fn generate_policy_content(
        budgets: &MissionBudgets,
        surface_policy: &SurfacePolicy,
        surface_definitions: &SurfaceDefinitions,
    ) -> String {
        let ctx = PolicyContext {
            surface_policy: SurfacePolicyContext::from(surface_policy),
            surface_definitions: SurfaceDefinitionsContext::from(surface_definitions),
            budgets: BudgetsContext::from(budgets),
            version: TESAKI_VERSION.to_string(),
        };

        render_policy_md(&ctx).unwrap_or_else(|e| {
            // Fallback to simple content if template fails
            log::error!("Failed to render POLICY.md template: {}", e);
            "# Mission Policy\n\n**READ THIS BEFORE MAKING ANY CHANGES.**\n\n1. NO COMMITS\n2. NO ORCHESTRATION\n".to_string()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo_state::RepoState;
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

        let inputs = MissionInputs {
            status_json: r#"{"status": "ok"}"#.to_string(),
            review_json: r#"{"review": "ok"}"#.to_string(),
            gate_json: r#"{"lint": {"status": "pass"}}"#.to_string(),
            explain_json: None,
            workspace_json: r#"{"repos": []}"#.to_string(),
            repo_state_json: r#"{"lint_status":"pass"}"#.to_string(),
        };

        let mission_type = MissionType::CreateMissingBindings {
            scenario_key: "feature:Rule(01):Scenario(01)".to_string(),
            missing_steps: vec!["Given a test".to_string()],
        };
        let brief = mission_type.generate_brief(&RepoState::default());
        let surface_policy = SurfacePolicy::for_implement_tests();
        let surface_definitions = SurfaceDefinitions {
            spec: SurfaceDefinition::spec(),
            tests_bindings: SurfaceDefinition::tests_bindings(),
            sut: SurfaceDefinition::sut(),
        };

        let bundle = MissionBundle::create(
            &tesaki_root,
            &mission_type,
            &brief,
            &Stage::ImplementTests,
            &surface_policy,
            &surface_definitions,
            &inputs,
            MissionBudgets::default(),
            None,
        ).unwrap();

        // Verify directory structure
        assert!(bundle.path.exists());
        assert!(bundle.path.join("MISSION.md").exists());
        assert!(bundle.path.join("POLICY.md").exists());
        assert!(bundle.path.join("INPUTS").exists());
        assert!(bundle.path.join("INPUTS/status.json").exists());
        assert!(bundle.path.join("INPUTS/review.json").exists());
        assert!(bundle.path.join("INPUTS/gate.json").exists());
        assert!(bundle.path.join("INPUTS/workspace.json").exists());
        assert!(bundle.path.join("INPUTS/repo_state.json").exists());
        assert!(bundle.path.join("INPUTS/mission_config.json").exists());
        assert!(bundle.path.join("RUNNER_OUTPUT").exists());

        // Verify content includes key elements
        let policy = fs::read_to_string(bundle.path.join("POLICY.md")).unwrap();
        assert!(policy.contains("NO COMMITS"));
        assert!(policy.contains("specs repository ONLY"));
        assert!(policy.contains("NEVER edit files in the Namako"));
    }

    #[test]
    fn test_mission_bundle_preserve_failed() {
        let temp_dir = TempDir::new().unwrap();
        let tesaki_root = temp_dir.path().join(".tesaki");
        fs::create_dir_all(&tesaki_root).unwrap();

        let inputs = MissionInputs {
            status_json: "{}".to_string(),
            review_json: "{}".to_string(),
            gate_json: "{}".to_string(),
            explain_json: None,
            workspace_json: "{}".to_string(),
            repo_state_json: "{}".to_string(),
        };

        let mission_type = MissionType::SummarizeAndClose;
        let brief = mission_type.generate_brief(&RepoState::default());
        let surface_policy = SurfacePolicy::for_finalize();
        let surface_definitions = SurfaceDefinitions {
            spec: SurfaceDefinition::spec(),
            tests_bindings: SurfaceDefinition::tests_bindings(),
            sut: SurfaceDefinition::sut(),
        };

        let bundle = MissionBundle::create(
            &tesaki_root,
            &mission_type,
            &brief,
            &Stage::Finalize,
            &surface_policy,
            &surface_definitions,
            &inputs,
            MissionBudgets::default(),
            None,
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
        assert!(failed_path.join("MISSION.md").exists());
        assert!(failed_path.join("POLICY.md").exists());
    }

    #[test]
    fn test_mission_bundle_write_gate_result() {
        let temp_dir = TempDir::new().unwrap();
        let tesaki_root = temp_dir.path().join(".tesaki");
        fs::create_dir_all(&tesaki_root).unwrap();

        let inputs = MissionInputs {
            status_json: "{}".to_string(),
            review_json: "{}".to_string(),
            gate_json: "{}".to_string(),
            explain_json: None,
            workspace_json: "{}".to_string(),
            repo_state_json: "{}".to_string(),
        };

        let mission_type = MissionType::SummarizeAndClose;
        let brief = mission_type.generate_brief(&RepoState::default());
        let surface_policy = SurfacePolicy::for_finalize();
        let surface_definitions = SurfaceDefinitions {
            spec: SurfaceDefinition::spec(),
            tests_bindings: SurfaceDefinition::tests_bindings(),
            sut: SurfaceDefinition::sut(),
        };

        let bundle = MissionBundle::create(
            &tesaki_root,
            &mission_type,
            &brief,
            &Stage::Finalize,
            &surface_policy,
            &surface_definitions,
            &inputs,
            MissionBudgets::default(),
            None,
        ).unwrap();

        let gate_result = r#"{"lint": {"status": "pass"}, "run": {"status": "pass"}, "verify": {"status": "pass"}}"#;
        bundle.write_gate_result(gate_result).unwrap();

        let result_path = bundle.path.join("POST_GATE.json");
        assert!(result_path.exists());
        let content = fs::read_to_string(result_path).unwrap();
        assert_eq!(content, gate_result);
    }
}
