//! High-level agent orchestration with fallback logic.

use anyhow::{bail, Result};
use std::sync::Mutex;

use crate::core::{OutcomeClassification, Servling, LLMRequest, LLMResponse, RunnerInvocation};

/// A high-level agent that orchestrates one or more backends with fallback logic.
pub struct CodingAgent {
    backends: Vec<Box<dyn Servling>>,
    current_index: Mutex<usize>,
}

impl CodingAgent {
    /// Start building a new CodingAgent.
    pub fn builder() -> CodingAgentBuilder {
        CodingAgentBuilder::default()
    }

    fn should_fallback(classification: OutcomeClassification) -> bool {
        matches!(classification, OutcomeClassification::RateLimited)
    }
}

/// Builder for Configuring a CodingAgent.
#[derive(Default)]
pub struct CodingAgentBuilder {
    backends: Vec<Box<dyn Servling>>,
}

impl CodingAgentBuilder {
    /// Register a backend. Order of registration defines priority.
    pub fn register(mut self, backend: Box<dyn Servling>) -> Self {
        self.backends.push(backend);
        self
    }

    /// Convenience for registering multiple backends.
    pub fn with_backends(mut self, backends: Vec<Box<dyn Servling>>) -> Self {
        self.backends.extend(backends);
        self
    }

    pub fn build(self) -> Result<CodingAgent> {
        if self.backends.is_empty() {
            bail!("CodingAgent must have at least one backend");
        }
        Ok(CodingAgent {
            backends: self.backends,
            current_index: Mutex::new(0),
        })
    }
}

impl Servling for CodingAgent {
    fn name(&self) -> &'static str {
        let idx = *self.current_index.lock().unwrap();
        self.backends[idx].name()
    }

    fn execute(&self, request: &LLMRequest) -> Result<LLMResponse> {
        loop {
            let (idx, backend) = {
                let current = *self.current_index.lock().unwrap();
                if current >= self.backends.len() {
                    bail!("No backends available in CodingAgent");
                }
                (current, &self.backends[current])
            };

            match backend.execute(request) {
                Ok(resp) if CodingAgent::should_fallback(resp.classification) => {
                    let mut current = self.current_index.lock().unwrap();
                    if *current == idx {
                        *current += 1;
                        if *current >= self.backends.len() {
                            return Ok(resp);
                        }
                        log::warn!("Backend {} rate limited. Falling back to next.", backend.name());
                    }
                    continue;
                }
                Ok(resp) => return Ok(resp),
                Err(err) => {
                    let mut current = self.current_index.lock().unwrap();
                    if *current == idx {
                        *current += 1;
                        if *current >= self.backends.len() {
                            return Err(err);
                        }
                        log::warn!("Backend {} failed: {}. Falling back.", backend.name(), err);
                    }
                    continue;
                }
            }
        }
    }

    fn planned_invocation(&self, request: &LLMRequest) -> Option<RunnerInvocation> {
        let idx = *self.current_index.lock().unwrap();
        self.backends.get(idx).and_then(|b| b.planned_invocation(request))
    }
}
