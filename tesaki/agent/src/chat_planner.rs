//! Plan-only chat planner implementations.

use anyhow::{Context, Result};
use std::path::PathBuf;

pub use servling::{Servling, LLMRequest, LLMResponse};
use crate::chat_plan::{ChatPlan, ChatTurnInput};

/// Plan-only chat interface.
pub trait ChatPlanner: Send + Sync {
    fn plan_turn(&self, input: &ChatTurnInput) -> Result<ChatPlan>;
    fn name(&self) -> &'static str;
}

/// Blanket implementation: Every Servling is a ChatPlanner.
impl<T: Servling> ChatPlanner for T {
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

#[cfg(test)]
mod tests {
    use super::*;
    use servling::MockAgent;

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
}
