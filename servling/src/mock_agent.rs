//! Mock agent for testing.

use anyhow::Result;
use crate::backend::{Servling, LLMRequest, LLMResponse};
use crate::outcome::OutcomeClassification;

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

impl Servling for MockAgent {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn execute(&self, _request: &LLMRequest) -> Result<LLMResponse> {
        Ok(LLMResponse {
            text: self.response_text.clone(),
            classification: OutcomeClassification::Ok,
            exit_code: Some(0),
            token_usage: None,
            elapsed_seconds: 0.1,
            stdout_path: None,
            stderr_path: None,
        })
    }
}
