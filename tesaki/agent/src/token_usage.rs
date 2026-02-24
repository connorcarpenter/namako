//! Token usage parsing and aggregation for Tesaki.

pub use servling::{TokenUsage, MissionTokenStats, SessionTokenStats, MissionTypeStats, EfficiencyRating};
pub use servling::token_usage::format_tokens;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_stats_aggregation() {
        let mut stats = SessionTokenStats::default();
        
        let mission1 = MissionTokenStats {
            mission_type: "CreateMissingBindings".to_string(),
            tokens_in: 1_000_000,
            tokens_out: 5_000,
            tokens_cached: 0,
            premium_requests: 1,
            model: Some("claude-sonnet-4".to_string()),
            elapsed_seconds: 60.0,
        };
        
        let mission2 = MissionTokenStats {
            mission_type: "CreateMissingBindings".to_string(),
            tokens_in: 1_500_000,
            tokens_out: 8_000,
            tokens_cached: 500_000,
            premium_requests: 2,
            model: Some("claude-sonnet-4".to_string()),
            elapsed_seconds: 90.0,
        };
        
        let mission3 = MissionTokenStats {
            mission_type: "FixRegressionFromGateFailure".to_string(),
            tokens_in: 3_000_000,
            tokens_out: 20_000,
            tokens_cached: 1_000_000,
            premium_requests: 3,
            model: Some("claude-opus-4.5".to_string()),
            elapsed_seconds: 120.0,
        };
        
        stats.record_mission(&mission1, true);
        stats.record_mission(&mission2, true);
        stats.record_mission(&mission3, false);
        
        assert_eq!(stats.missions_completed, 2);
        assert_eq!(stats.missions_failed, 1);
        assert_eq!(stats.total_tokens_in, 5_500_000);
        assert_eq!(stats.total_tokens_out, 33_000);
        assert_eq!(stats.total_premium_requests, 6);
        assert_eq!(stats.by_mission_type.len(), 2);
        assert_eq!(stats.by_mission_type["CreateMissingBindings"].count, 2);
        assert_eq!(stats.by_mission_type["FixRegressionFromGateFailure"].count, 1);
    }
}
