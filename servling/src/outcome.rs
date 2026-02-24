use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OutcomeClassification {
    Ok,
    Failed,
    Timeout,
    EnvironmentError,
    /// Rate limited by the AI provider (Claude, Codex, etc.)
    RateLimited,
}
