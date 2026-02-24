//! Core Servling trait and shared data structures.

use anyhow::Result;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

use crate::outcome::OutcomeClassification;
use crate::token_usage::TokenUsage;

/// The core trait for any AI agent provider.
pub trait Servling: Send + Sync {
    /// Execute a raw prompt against the LLM and return a standardized response.
    fn execute(&self, request: &LLMRequest) -> Result<LLMResponse>;
    
    /// The display name of this agent.
    fn name(&self) -> &'static str;
    
    /// Optional: Describe how to invoke this as a CLI command.
    fn planned_invocation(&self, _request: &LLMRequest) -> Option<RunnerInvocation> {
        None
    }
}

/// Implement Servling for Boxed trait objects to allow delegation.
impl Servling for Box<dyn Servling> {
    fn execute(&self, request: &LLMRequest) -> Result<LLMResponse> {
        (**self).execute(request)
    }

    fn name(&self) -> &'static str {
        (**self).name()
    }

    fn planned_invocation(&self, request: &LLMRequest) -> Option<RunnerInvocation> {
        (**self).planned_invocation(request)
    }
}

/// A standardized request to a Servling.
#[derive(Debug, Clone)]
pub struct LLMRequest {
    pub prompt: String,
    pub working_dir: PathBuf,
    pub model: Option<String>,
    pub max_runtime_seconds: u32,
    pub stream_output: bool,
    /// Optional: If the prompt is already stored in a file.
    pub input_file: Option<PathBuf>,
}

/// A standardized response from a Servling.
#[derive(Debug, Clone)]
pub struct LLMResponse {
    pub text: String,
    pub classification: OutcomeClassification,
    pub exit_code: Option<i32>,
    pub token_usage: Option<TokenUsage>,
    pub elapsed_seconds: f64,
    pub stdout_path: Option<String>,
    pub stderr_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerInvocation {
    pub program: String,
    pub args: Vec<String>,
    pub working_dir: String,
    pub env: Vec<(String, String)>,
}
