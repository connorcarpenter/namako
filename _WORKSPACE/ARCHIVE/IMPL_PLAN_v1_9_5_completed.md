# IMPL_PLAN.md — Tesaki Gap Closure

**Audience:** Sonnet coding agent executing this plan from zero context
**Goal:** Close the remaining implementation gaps from the previous plan
**Scope:** Tesaki toolchain only (`namako/tesaki/`)
**Date:** 2026-02-03

---

## Background Context

Tesaki is an AI-friendly autonomous task orchestrator for spec-driven development. It operates in a loop:

```
select mission → execute (via runner agent) → validate (via namako gate) → repeat
```

### Key Concepts

- **Mission Types:** Enumerated task types (e.g., `CreateMissingBindings`, `AddOrClarifyScenario`, `DraftSpecScenarios`)
- **Surface Policy:** Each mission declares which code surfaces (spec, tests, SUT) are locked/unlocked for editing
- **RepoState:** Computed from Namako packet JSON files; contains issue lists (`spec_issues`, `binding_issues`, `sut_issues`, `structure_issues`)
- **Mission Selector:** Deterministically picks the next mission based on RepoState evidence

### Crate Structure

Tesaki is a **binary crate** with modules declared in `main.rs`:

```rust
// main.rs (lines 19-50)
mod binding_extractor;
mod config;
mod mission_selector;
mod mission_type;
mod spec_quality;
mod workspace;
// ... etc
```

The `lib.rs` re-exports modules for testing, but runtime uses `main.rs` declarations.

---

## Gaps to Close

| Gap | Phase | Priority |
|-----|-------|----------|
| Surface lock enforcement is dead code | 1 | HIGH |
| Draft/Promote mission types never selected | 2 | HIGH |
| `tesaki diagnose` command missing | 3 | MEDIUM |
| `quality_gates_enabled` config flag missing | 4 | LOW |
| Evidence details not shown in "Why this mission" | 5 | LOW |

---

## Phase 1 — Wire Surface Lock Enforcement

### Problem

`check_surface_violations()` exists in `base_runner.rs:547` but is never called. The function validates that changed files respect surface lock policies (e.g., if spec is LOCKED, no `.feature` files should be modified).

### Files to Modify

- `tesaki/src/main.rs` — Add violation check after runner completes, before gate

### Implementation Steps

#### Step 1.1: Locate the runner completion point in `run_run()`

In `main.rs`, the `run_run()` function starts at line 1142. After the runner executes but before validation, we need to check for surface violations.

Search for the pattern where `workspace.compute_changes()` is called — this is right after the runner finishes. The check should happen there.

#### Step 1.2: Add the surface violation check

After the runner completes and changes are computed, add:

```rust
// Check surface policy violations
let violations = crate::base_runner::check_surface_violations(
    &changes.changed_files,
    &surface_defs.spec.patterns,
    &surface_defs.tests_bindings.patterns,
    &surface_defs.sut.patterns,
    surface_policy.spec == crate::surface_policy::SurfaceLock::Unlocked,
    surface_policy.tests_bindings == crate::surface_policy::SurfaceLock::Unlocked,
    surface_policy.sut == crate::surface_policy::SurfaceLock::Unlocked,
);

if !violations.is_empty() {
    eprintln!("  ⚠️  Surface policy violations detected:");
    for v in &violations {
        eprintln!("      - {}", v);
    }
    // Roll back changes
    let _ = Command::new("git")
        .args(["checkout", "--", "."])
        .current_dir(&spec_root)
        .output();

    let details = format!("Surface policy violation: {}", violations.join(", "));
    let failed_path = mission.preserve_failed()?;
    let result = RunResult::error(StopReason::PolicyViolation, details.clone())
        .with_mission_path(failed_path.display().to_string())
        .with_missions(attempts_made);
    emit_run_result(&result, &spec_root)?;
    log_session_end(logger, StopReason::PolicyViolation, Some(details));
    return Ok(());
}
```

#### Step 1.3: Find the exact insertion point

The check must occur at **line 1741** in `main.rs`, after:
```rust
let changes = workspace.compute_changes()?;
if changes.total_files_changed == 0 {
    // ... no-changes handling ...
    return Ok(());
}
// INSERT SURFACE VIOLATION CHECK HERE (line 1741)
```

And before:
```rust
// Step 6: Validate via namako gate --json
eprintln!("[6/6] Validating (namako gate --json)...");
```

The surface definitions and policy are available in scope as `surface_defs` and `surface_policy`.

#### Step 1.4: Ensure imports are present

At the top of the runner execution block, verify these are imported:
- `crate::base_runner::check_surface_violations`
- `crate::surface_policy::SurfaceLock`

#### Step 1.5: Add a test

Add a test in `tesaki/src/main.rs` tests section that verifies surface violations trigger rollback:

```rust
#[test]
fn test_surface_violation_triggers_rollback() {
    use crate::base_runner::check_surface_violations;

    let changed = vec!["features/test.feature".to_string()];
    let spec_patterns = vec!["features/**/*.feature".to_string()];
    let tests_patterns = vec!["tests/**/*.rs".to_string()];
    let sut_patterns = vec!["src/**/*.rs".to_string()];

    // Spec is LOCKED, but a feature file changed
    let violations = check_surface_violations(
        &changed,
        &spec_patterns,
        &tests_patterns,
        &sut_patterns,
        false, // spec locked
        true,  // tests unlocked
        true,  // sut unlocked
    );

    assert!(!violations.is_empty());
    assert!(violations[0].contains("spec surface LOCKED"));
}
```

### Acceptance Criteria

- [ ] `check_surface_violations` is called after runner completes
- [ ] Violations trigger `git checkout -- .` rollback
- [ ] Mission marked as `PolicyViolation` stop reason
- [ ] Test passes

---

## Phase 2 — Wire Draft/Promote Mission Types into Selector

### Problem

`DraftSpecScenarios` and `PromoteScenariosToExecutable` exist in `mission_type.rs:72-81` but the selector in `mission_selector.rs` never returns them. The selector still only returns `AddOrClarifyScenario` for spec coverage issues.

### Design

Per the original plan:
- **DraftSpecScenarios:** Selected when a rule has 0 scenarios AND tests are locked (can't create bindings)
- **PromoteScenariosToExecutable:** Selected when deferred scenarios exist AND tests are unlocked (can create bindings)

### Files to Modify

- `tesaki/src/mission_selector.rs`
- `tesaki/src/repo_state.rs` (if needed for deferred scenario helpers)

### Implementation Steps

#### Step 2.1: Add helper to detect deferred scenarios

First, check `packet_parser.rs` for the `DeferredScenarioItem` struct (around line 191):
```rust
pub struct DeferredScenarioItem {
    pub scenario_key: String,
    pub scenario_name: String,
    pub feature_path: String,
    pub rule_name: String,  // NOTE: This is String, not Option<String>
    pub blocker: BlockerType,
}
```

In `repo_state.rs`, add methods to `RepoState`. The `RepoState` has a field `review: Option<ReviewPacket>`:

```rust
impl RepoState {
    /// Returns deferred scenario names that could be promoted for a given feature/rule.
    pub fn deferred_scenarios_for_rule(&self, feature_path: &str, rule_name: &str) -> Vec<String> {
        self.review
            .as_ref()
            .map(|r| {
                r.deferred_items
                    .iter()
                    .filter(|d| d.feature_path == feature_path && d.rule_name == rule_name)
                    .map(|d| d.scenario_name.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Returns true if there are any deferred scenarios in the repo.
    pub fn has_deferred_scenarios(&self) -> bool {
        self.review
            .as_ref()
            .map(|r| !r.deferred_items.is_empty())
            .unwrap_or(false)
    }
}
```

#### Step 2.2: Update `select_mission_type()` in `mission_selector.rs`

The current logic at lines 47-52 selects `AddOrClarifyScenario` for spec issues. Modify it to consider Draft/Promote:

```rust
// Replace the existing AddOrClarifyScenario selection block with:
if let Some(issue) = select_spec_issue_for_add_scenario(state) {
    // Check if this is a zero-coverage rule
    let rule_has_zero_scenarios = issue.rule_name.as_ref()
        .map(|r| state.scenario_count_for_rule(&issue.feature_path, r) == Some(0))
        .unwrap_or(false);

    // Check for deferred scenarios that could be promoted
    let deferred = issue.rule_name.as_ref()
        .map(|r| state.deferred_scenarios_for_rule(&issue.feature_path, r))
        .unwrap_or_default();

    if !deferred.is_empty() {
        // Has deferred scenarios — pick PromoteScenariosToExecutable
        return Some(MissionType::PromoteScenariosToExecutable {
            feature_path: issue.feature_path.clone(),
            scenario_name: deferred[0].clone(),
            rule_name: issue.rule_name.clone().unwrap_or_default(),
        });
    } else if rule_has_zero_scenarios {
        // Zero coverage, no deferred — pick DraftSpecScenarios
        return Some(MissionType::DraftSpecScenarios {
            feature_path: issue.feature_path.clone(),
            rule_name: issue.rule_name.clone(),
        });
    } else {
        // Partial coverage — use AddOrClarifyScenario
        return Some(MissionType::AddOrClarifyScenario {
            feature_path: issue.feature_path.clone(),
            rule_name: issue.rule_name.clone(),
        });
    }
}
```

#### Step 2.3: Update `select_alternative_for_stage()` similarly

The function at lines 75-138 also needs the same logic for the Spec category branch (lines 109-128).

#### Step 2.4: Add imports

At the top of `mission_selector.rs`, ensure `MissionType` import includes the new variants:
```rust
use crate::mission_type::MissionType;
```

(This should already work since `MissionType` is the enum itself.)

#### Step 2.5: Add tests

```rust
#[test]
fn selects_draft_spec_for_zero_coverage_rule() {
    let state = RepoState {
        spec_issues: vec![SpecIssue {
            kind: SpecIssueKind::MissingCoverage,
            feature_path: "features/a.feature".to_string(),
            description: "Rule has 0 scenarios".to_string(),
            rule_name: Some("Empty Rule".to_string()),
        }],
        scenarios_per_rule: {
            let mut map = std::collections::HashMap::new();
            map.insert("features/a.feature::Empty Rule".to_string(), 0);
            map
        },
        ..Default::default()
    };

    let mission = select_mission_type(&state).unwrap();
    assert!(matches!(mission, MissionType::DraftSpecScenarios { .. }));
}

#[test]
fn selects_promote_when_deferred_exists() {
    use crate::packet_parser::{ReviewPacket, DeferredScenarioItem};

    let state = RepoState {
        spec_issues: vec![SpecIssue {
            kind: SpecIssueKind::MissingCoverage,
            feature_path: "features/a.feature".to_string(),
            description: "Rule has 0 executable scenarios".to_string(),
            rule_name: Some("Deferred Rule".to_string()),
        }],
        review: Some(ReviewPacket {
            deferred_items: vec![DeferredScenarioItem {
                feature_path: "features/a.feature".to_string(),
                rule_name: Some("Deferred Rule".to_string()),
                scenario_name: "Deferred scenario".to_string(),
                // ... other fields with defaults
            }],
            ..Default::default()
        }),
        ..Default::default()
    };

    let mission = select_mission_type(&state).unwrap();
    assert!(matches!(mission, MissionType::PromoteScenariosToExecutable { .. }));
}
```

### Acceptance Criteria

- [ ] `DraftSpecScenarios` is returned when a rule has 0 scenarios and no deferred items
- [ ] `PromoteScenariosToExecutable` is returned when deferred scenarios exist
- [ ] `AddOrClarifyScenario` is still returned for partial coverage without deferred
- [ ] All new tests pass

---

## Phase 3 — Add `tesaki diagnose` Command

### Problem

The plan called for a `tesaki diagnose <mission_id>` command that shows why a mission was selected, what evidence failed, and how to fix it. This command does not exist.

### Design

```
$ tesaki diagnose M-abc123

Mission: M-abc123
Type: CreateMissingBindings
Target: @Scenario(03) in features/entity.feature

Selection Reason:
  - binding_issues[0]: MissingBinding for step "Given an entity exists"
  - scenario_key: "features/entity.feature::@Scenario(03)"

Gate Result: FAIL (lint)
  - Missing step binding: "Given an entity exists"
  - Missing step binding: "When the entity is updated"

Suggested Fix:
  - Add step bindings in test harness for the missing steps
  - Or mark scenario @Deferred if bindings cannot be added yet
```

### Files to Modify

- `tesaki/src/main.rs` — Add CLI subcommand and handler

### Implementation Steps

#### Step 3.1: Add Clap subcommand

The current CLI at line 87-91 uses a simple struct. Convert to subcommands:

```rust
#[derive(Parser)]
#[command(name = "tesaki", about = "AI-friendly task orchestrator for Namako")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Run autonomous loop for N iterations (or until done/stalled)
    #[arg(long, short = 'l')]
    r#loop: Option<u32>,
}

#[derive(Subcommand)]
enum Commands {
    /// Diagnose a mission by ID
    Diagnose {
        /// Mission ID (e.g., M-abc123)
        mission_id: String,
    },
    /// Run a single mission (default behavior)
    Run,
}
```

#### Step 3.2: Implement `diagnose` handler

```rust
fn cmd_diagnose(mission_id: &str, spec_root: &Path) -> Result<()> {
    use crate::mission::MissionBundle;

    // Find mission directory
    let missions_dir = spec_root.join(".tesaki").join("missions");
    let mission_dir = find_mission_by_id(&missions_dir, mission_id)?;

    // Load mission bundle
    let bundle = MissionBundle::load(&mission_dir)?;

    // Print mission info
    println!("Mission: {}", bundle.id);
    println!("Type: {}", bundle.mission_type.name());
    if let Some(target) = bundle.mission_type.target_label() {
        println!("Target: {}", target);
    }
    println!();

    // Print selection reason from INPUTS.json
    let inputs_path = mission_dir.join("INPUTS.json");
    if inputs_path.exists() {
        let inputs: serde_json::Value = serde_json::from_str(&fs::read_to_string(&inputs_path)?)?;
        println!("Selection Reason:");
        if let Some(evidence) = inputs.get("selection_evidence") {
            println!("  {}", serde_json::to_string_pretty(evidence)?);
        }
    }
    println!();

    // Print gate result from GATE_RESULT.json
    let gate_path = mission_dir.join("GATE_RESULT.json");
    if gate_path.exists() {
        let gate: serde_json::Value = serde_json::from_str(&fs::read_to_string(&gate_path)?)?;
        let outcome = gate.get("outcome").and_then(|v| v.as_str()).unwrap_or("unknown");
        println!("Gate Result: {}", outcome);
        if let Some(errors) = gate.get("errors").and_then(|v| v.as_array()) {
            for err in errors.iter().take(5) {
                println!("  - {}", err.as_str().unwrap_or(""));
            }
        }
    }
    println!();

    // Print suggested fix based on mission type
    println!("Suggested Fix:");
    match &bundle.mission_type {
        MissionType::CreateMissingBindings { .. } => {
            println!("  - Add step bindings in test harness for the missing steps");
            println!("  - Or mark scenario @Deferred if bindings cannot be added yet");
        }
        MissionType::AddOrClarifyScenario { .. } => {
            println!("  - Add a scenario that covers the rule's core guarantee");
            println!("  - Use domain-specific language from the rule header");
        }
        MissionType::FixRegressionFromGateFailure { .. } => {
            println!("  - Fix the failing assertion in the SUT code");
            println!("  - Or update the scenario if the expected behavior changed");
        }
        _ => {
            println!("  - Review the mission brief for specific guidance");
        }
    }

    Ok(())
}

fn find_mission_by_id(missions_dir: &Path, mission_id: &str) -> Result<PathBuf> {
    // Mission directories are named like "M-{timestamp}-{hash}"
    // The mission_id could be the full name or just the hash suffix
    for entry in fs::read_dir(missions_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name == mission_id || name.ends_with(mission_id) || name.contains(mission_id) {
            return Ok(entry.path());
        }
    }
    bail!("Mission not found: {}", mission_id)
}
```

#### Step 3.3: Wire into main

In `main()`, dispatch based on command:

```rust
fn main() -> Result<()> {
    let cli = Cli::parse();

    // ... config discovery ...

    match cli.command {
        Some(Commands::Diagnose { mission_id }) => {
            cmd_diagnose(&mission_id, &spec_root)
        }
        Some(Commands::Run) | None => {
            // Existing run logic
            if let Some(loop_count) = cli.r#loop {
                // ... loop logic
            } else {
                run_run(/* ... */)
            }
        }
    }
}
```

#### Step 3.4: Add imports

```rust
use clap::Subcommand;
use anyhow::bail;
```

### Acceptance Criteria

- [ ] `tesaki diagnose M-xxx` prints mission details
- [ ] Shows selection reason, gate result, and suggested fix
- [ ] Handles missing mission gracefully with error message
- [ ] Existing `tesaki` and `tesaki --loop N` behavior unchanged

---

## Phase 4 — Add `quality_gates_enabled` Config Flag

### Problem

The plan specified a config flag `quality_gates_enabled = true` to toggle spec quality checks. This flag does not exist; the checks are always on.

### Files to Modify

- `tesaki/src/config.rs` — Add field
- `tesaki/src/main.rs` or `tesaki/src/repl.rs` — Check flag before running quality gate

### Implementation Steps

#### Step 4.1: Add config field

In `config.rs`, add to the `Config` struct:

```rust
/// Enable spec quality gates (placeholder step detection, domain noun check, etc.)
/// Default: true
#[serde(default = "default_quality_gates_enabled")]
pub quality_gates_enabled: bool,

// Add helper function
fn default_quality_gates_enabled() -> bool {
    true
}
```

#### Step 4.2: Pass through to quality check callsite

In `repl.rs` around line 984 where `spec_quality::check_feature_quality` is called, wrap with config check:

```rust
if config.quality_gates_enabled {
    let quality = crate::spec_quality::check_feature_quality(feature_path, &content);
    if !quality.is_ok() {
        // ... existing violation handling
    }
}
```

The config needs to be passed to the REPL or the check site. Trace how `config` flows and add the flag.

#### Step 4.3: Add test

```rust
#[test]
fn test_quality_gates_disabled_skips_check() {
    // Parse config with quality_gates_enabled = false
    let toml = r#"
        specs_dir = "test/specs"
        adapter_cmd = "cargo run --"
        quality_gates_enabled = false
    "#;
    let config: Config = toml::from_str(toml).unwrap();
    assert!(!config.quality_gates_enabled);
}
```

### Acceptance Criteria

- [ ] Config parses `quality_gates_enabled` field
- [ ] Defaults to `true` when not specified
- [ ] When `false`, spec quality checks are skipped
- [ ] Test passes

---

## Phase 5 — Add Evidence Details to "Why this mission"

### Problem

The mission brief template has a "Why this mission" section that renders `{{ context }}`, but this doesn't include the specific packet evidence that triggered selection (e.g., which exact issue, file, line).

### Files to Modify

- `tesaki/src/prompts.rs` — Enhance `BriefContext` to include evidence
- `tesaki/src/mission_type.rs` — Add `selection_evidence()` method

### Implementation Steps

#### Step 5.1: Add evidence to BriefContext

In `prompts.rs`, add to `BriefContext`:

```rust
pub struct BriefContext {
    // ... existing fields ...

    /// Structured evidence that triggered mission selection
    pub selection_evidence: Option<SelectionEvidence>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelectionEvidence {
    /// The issue type that triggered selection
    pub issue_type: String,
    /// File path if applicable
    pub file_path: Option<String>,
    /// Line number if applicable
    pub line: Option<u32>,
    /// Description of the specific issue
    pub description: String,
    /// Example from the issue list (first entry)
    pub example: Option<String>,
}
```

#### Step 5.2: Populate evidence in mission generation

When `BriefContext::from_mission_type()` is called, include the evidence:

```rust
impl BriefContext {
    pub fn from_mission_type(
        mission_type: &MissionType,
        repo_state: &RepoState,
        // ... other params
    ) -> Self {
        let selection_evidence = match mission_type {
            MissionType::CreateMissingBindings { scenario_key, missing_steps } => {
                Some(SelectionEvidence {
                    issue_type: "MissingBinding".to_string(),
                    file_path: Some(scenario_key.split("::").next().unwrap_or("").to_string()),
                    line: None,
                    description: format!("{} missing step(s)", missing_steps.len()),
                    example: missing_steps.first().cloned(),
                })
            }
            MissionType::AddOrClarifyScenario { feature_path, rule_name } => {
                let issue = repo_state.spec_issues.iter()
                    .find(|i| &i.feature_path == feature_path);
                Some(SelectionEvidence {
                    issue_type: "MissingCoverage".to_string(),
                    file_path: Some(feature_path.clone()),
                    line: None,
                    description: issue.map(|i| i.description.clone()).unwrap_or_default(),
                    example: rule_name.clone(),
                })
            }
            // ... handle other mission types
            _ => None,
        };

        Self {
            // ... existing fields
            selection_evidence,
        }
    }
}
```

#### Step 5.3: Update template

In `MISSION.md.j2`, enhance the "Why this mission" section:

```jinja2
## Why this mission

{{ context }}

{% if selection_evidence %}
**Selection Evidence:**
- Type: {{ selection_evidence.issue_type }}
{% if selection_evidence.file_path %}- File: `{{ selection_evidence.file_path }}`{% endif %}
{% if selection_evidence.line %}- Line: {{ selection_evidence.line }}{% endif %}
- {{ selection_evidence.description }}
{% if selection_evidence.example %}- Example: {{ selection_evidence.example }}{% endif %}
{% endif %}
```

### Acceptance Criteria

- [ ] `SelectionEvidence` struct is populated for relevant mission types
- [ ] Template renders evidence details
- [ ] Evidence includes file path and example issue when available
- [ ] Test verifies evidence is included in rendered output

---

## Testing Plan

After all phases:

```bash
cd /home/ccarpenter/Personal/specops/namako
cargo test -p tesaki
```

All tests must pass. Warnings for unused code are acceptable only if the code is infrastructure for future use.

---

## Done Definition

- [ ] Surface lock violations trigger rollback and `PolicyViolation` stop
- [ ] `DraftSpecScenarios` is selected for zero-coverage rules
- [ ] `PromoteScenariosToExecutable` is selected when deferred scenarios exist
- [ ] `tesaki diagnose <id>` command works
- [ ] `quality_gates_enabled` config flag exists and works
- [ ] Mission briefs include concrete selection evidence
- [ ] All 421+ tests pass

---

## File Reference

| File | Purpose |
|------|---------|
| `tesaki/src/main.rs` | CLI entry, `run_run()` loop, diagnose command |
| `tesaki/src/mission_selector.rs` | Mission type selection logic |
| `tesaki/src/mission_type.rs` | Mission type enum and helpers |
| `tesaki/src/base_runner.rs` | `check_surface_violations()` function |
| `tesaki/src/config.rs` | Config struct with `quality_gates_enabled` |
| `tesaki/src/prompts.rs` | `BriefContext` and template rendering |
| `tesaki/src/repo_state.rs` | `RepoState` with issue lists and helpers |
| `tesaki/src/spec_quality.rs` | Spec quality check rules |
| `tesaki/src/workspace.rs` | `get_changed_files()` for violation check |
| `tesaki/prompts/mission/MISSION.md.j2` | Mission brief template |
