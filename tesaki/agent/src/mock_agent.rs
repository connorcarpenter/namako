//! Mock agent for testing.

use anyhow::Result;
use crate::llm_backend::{LLMBackend, LLMRequest, LLMResponse};
use crate::runner::OutcomeClassification;

pub struct MockAgent {
    pub response_text: String,
}

impl MockAgent {
    pub fn success() -> Self {
        Self {
            response_text: r#"{"say": "Mock success", "done": true}"#.to_string(),
        }
    }
}

impl LLMBackend for MockAgent {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn execute(&self, _request: &LLMRequest) -> Result<LLMResponse> {
        Ok(LLMResponse {
            text: self.response_text.clone(),
            exit_code: Some(0),
            classification: OutcomeClassification::Ok,
            stdout_path: None,
            stderr_path: None,
            token_usage: None,
            elapsed_seconds: 0.1,
        })
    }
}
