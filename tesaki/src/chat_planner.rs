//! Plan-only chat planner implementations and data structures.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub use servling::{Servling, LLMRequest};

/// A single allowlisted command request from the chat planner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowedCommand {
    /// Must be "namako". Enforced by Tesaki allowlist.
    pub tool: String,
    /// Args only (no shell). Enforced by Tesaki allowlist.
    pub args: Vec<String>,
    /// Optional explanation for UX.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Optional mission proposal emitted by the chat planner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionProposal {
    pub mission_type: String,
    pub stage: String,
    pub target: String,
    pub surfaces: SurfacePolicy,
    pub objective: String,
    pub validation: Vec<String>,
}

/// Surface policy shape used in chat plans.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfacePolicy {
    pub spec: SurfaceLock,
    pub tests: SurfaceLock,
    pub sut: SurfaceLock,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SurfaceLock {
    Locked,
    Unlocked,
}

/// The result for one REPL turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatPlan {
    pub say: String,
    #[serde(default)]
    pub run: Vec<AllowedCommand>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mission_proposal: Option<MissionProposal>,
    pub done: bool,
}

/// Input to the chat planner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatTurnInput {
    pub user_message: String,
    pub session_state_json: serde_json::Value,
    pub recent_command_results: Vec<CommandResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planner_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    pub tool: String,
    pub args: Vec<String>,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Plan-only chat interface.
pub trait ChatPlanner: Send + Sync {
    fn plan_turn(&self, input: &ChatTurnInput) -> Result<ChatPlan>;
    fn name(&self) -> &'static str;
}

/// Blanket implementation: Every Servling is a ChatPlanner.
impl<T: Servling + ?Sized> ChatPlanner for T {
    fn name(&self) -> &'static str {
        self.name()
    }

    fn plan_turn(&self, input: &ChatTurnInput) -> Result<ChatPlan> {
        let prompt = format_planner_prompt(input);
        let request = LLMRequest {
            prompt,
            model: None,
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            max_runtime_seconds: 60,
            stream_output: false,
            input_file: None,
        };

        let resp = self.execute(&request)?;
        let json_text = strip_markdown_code_fences(&resp.text);
        
        let plan: ChatPlan = serde_json::from_str(&json_text)
            .with_context(|| format!("LLM returned invalid JSON for plan: {}", resp.text))?;
        Ok(plan)
    }
}

/// Mock chat planner for tests and offline usage.
pub struct MockChatPlanner {
    response: ChatPlan,
}

impl MockChatPlanner {
    pub fn new(response: ChatPlan) -> Self {
        Self { response }
    }
}

impl ChatPlanner for MockChatPlanner {
    fn plan_turn(&self, _input: &ChatTurnInput) -> Result<ChatPlan> {
        Ok(self.response.clone())
    }

    fn name(&self) -> &'static str {
        "mock"
    }
}

/// Format the ChatTurnInput as a compact prompt for LLM planners.
pub fn format_planner_prompt(input: &ChatTurnInput) -> String {
    let system_prompt = input.system_prompt.as_deref().unwrap_or(DEFAULT_SYSTEM_PROMPT);
    
    let mut prompt = String::new();
    prompt.push_str(system_prompt);
    prompt.push_str("\n\n");
    
    prompt.push_str("## Repository State\n");
    if let Some(summary) = input.session_state_json.get("last_repo_state_summary").and_then(|v| v.as_str()) {
        prompt.push_str(summary);
        prompt.push('\n');
    }
    if let Some(intent) = input.session_state_json.get("intent") {
        if let Some(stage) = intent.get("stage").and_then(|v| v.as_str()) {
            prompt.push_str(&format!("Stage: {}\n", stage));
        }
    }
    prompt.push('\n');
    
    if let Some(hint) = &input.planner_hint {
        prompt.push_str("Note: ");
        prompt.push_str(hint);
        prompt.push_str("\n\n");
    }
    
    prompt.push_str("## User\n");
    prompt.push_str(&input.user_message);
    prompt.push_str("\n\n");
    
    prompt.push_str("Respond with JSON only.\n");
    prompt
}

const DEFAULT_SYSTEM_PROMPT: &str = r#"You are Tesaki, a spec-driven development assistant.

Given the repository state below, help the developer by:
1. Answering questions about the current state
2. Proposing a mission when they want to make progress

Respond with JSON only:
```json
{"say": "Brief response", "mission_proposal": null, "done": true}
```

For mission_proposal (when the user wants to make progress):
```json
{
  "mission_type": "CreateMissingBindings",
  "stage": "Implement Tests & Bindings", 
  "target": "02_transport.feature",
  "surfaces": {"spec": "LOCKED", "tests": "UNLOCKED", "sut": "LOCKED"},
  "objective": "Add step bindings for missing steps",
  "validation": ["namako lint passes"]
}
```

Mission types:
- CreateMissingBindings: Add step bindings for unbound steps
- ImplementBehaviorForScenario: Implement SUT code to pass a failing scenario
- FixRegressionFromGateFailure: Fix a newly failing test
- NormalizeIdentityTags: Add/fix @Feature/@Rule/@Scenario tags

Be concise. Focus on actionable next steps.
"#;

/// Strip markdown code fences from LLM output.
pub fn strip_markdown_code_fences(text: &str) -> String {
    let trimmed = text.trim();
    let start_patterns = ["```json", "```JSON", "```"];
    
    for pattern in &start_patterns {
        if let Some(start_pos) = trimmed.find(pattern) {
            let after_pattern = &trimmed[start_pos + pattern.len()..];
            if let Some(end_pos) = after_pattern.rfind("```") {
                return after_pattern[..end_pos].trim().to_string();
            }
            return after_pattern.trim().to_string();
        }
    }
    trimmed.to_string()
}

/// Factory to build a ChatPlanner from agent candidates.
pub fn build_planner(candidates: Vec<servling::AgentCandidate>) -> anyhow::Result<Box<dyn ChatPlanner>> {
    let agent = servling::build_coding_agent(candidates)?;
    struct PlannerWrap(Box<dyn Servling>);
    impl ChatPlanner for PlannerWrap {
        fn plan_turn(&self, input: &ChatTurnInput) -> anyhow::Result<ChatPlan> {
            self.0.plan_turn(input)
        }
        fn name(&self) -> &'static str {
            Servling::name(&*self.0)
        }
    }
    Ok(Box::new(PlannerWrap(agent)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::MockAgent;

    #[test]
    fn test_blanket_planner_impl() {
        let agent = MockAgent::success();
        let input = ChatTurnInput {
            user_message: "hi".to_string(),
            session_state_json: serde_json::json!({}),
            recent_command_results: vec![],
            planner_hint: None,
            system_prompt: None,
        };
        
        let plan = ChatPlanner::plan_turn(&agent, &input).unwrap();
        assert_eq!(plan.say, "Mock success");
    }

    #[test]
    fn test_chat_plan_json_round_trip() {
        let plan = ChatPlan {
            say: "ok".to_string(),
            run: vec![AllowedCommand {
                tool: "namako".to_string(),
                args: vec!["status".to_string(), "--json".to_string()],
                reason: None,
            }],
            mission_proposal: Some(MissionProposal {
                mission_type: "CreateMissingBindings".to_string(),
                stage: "Implement Tests & Bindings".to_string(),
                target: "@Scenario(03)".to_string(),
                surfaces: SurfacePolicy {
                    spec: SurfaceLock::Locked,
                    tests: SurfaceLock::Unlocked,
                    sut: SurfaceLock::Locked,
                },
                objective: "Create bindings".to_string(),
                validation: vec!["namako gate --json passes".to_string()],
            }),
            done: true,
        };
        let json = serde_json::to_string(&plan).unwrap();
        let parsed: ChatPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.say, "ok");
    }
}
