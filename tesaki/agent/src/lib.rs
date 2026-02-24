//! Agent abstraction traits and implementations for Namako Tesaki orchestrator.

pub mod chat_planner;
pub mod runner;

// Re-export core roles
pub use chat_planner::{
    AllowedCommand, ChatPlan, ChatTurnInput, CommandResult, MissionProposal, ChatPlanner,
    SurfaceLock, SurfacePolicy, MockChatPlanner, build_planner,
};
pub use runner::{
    OutcomeClassification, Runner, RunnerConfig, RunnerInvocation, RunnerOutcome, MockRunner,
    MockAgent, check_surface_violations, matches_any_pattern, outcome_from_error, build_runner,
};

// Re-export the entire agent engine and factory from servling
pub use servling::{
    agent_candidates, build_coding_agent, build_servling, describe_candidates, normalize_model,
    AgentCandidate, ClaudeAgent, CodexAgent, CodingAgent, CodingAgentBuilder,
    CopilotAgent, EfficiencyRating, LLMRequest, LLMResponse, MissionTokenStats, MissionTypeStats,
    Servling, SessionTokenStats, TokenUsage,
};
