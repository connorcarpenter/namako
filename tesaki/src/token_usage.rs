//! Token usage parsing and aggregation for Tesaki.
//!
//! This module handles parsing token usage information from runner stderr (particularly
//! the Copilot CLI format) and provides types for tracking and displaying usage stats.
//!
//! # Expected Format (Copilot CLI)
//! ```text
//! Total usage est: 3 Premium requests API time spent: 2m 19.91s Total session time: 2m 35.177s
//! Total code changes: +10 -9 Breakdown by AI model:
//!  claude-opus-4.5   1.0m in, 6.0k out, 935.5k cached (Est. 3 Premium requests)
//! ```

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;

// ============================================================================
// Compiled Regex Patterns (thread-safe, compile-once)
// ============================================================================

static RE_PREMIUM_REQUESTS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Total usage est:\s*(\d+)\s*Premium").unwrap()
});

static RE_API_TIME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"API time spent:\s*((?:\d+m\s*)?\d+\.?\d*s?)").unwrap()
});

static RE_SESSION_TIME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Total session time:\s*((?:\d+m\s*)?\d+\.?\d*s?)").unwrap()
});

static RE_CODE_CHANGES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Total code changes:\s*\+(\d+)\s*-(\d+)").unwrap()
});

static RE_MODEL_LINE: LazyLock<Regex> = LazyLock::new(|| {
    // Match: " claude-opus-4.5   1.0m in, 6.0k out, 935.5k cached (Est. 3 Premium requests)"
    Regex::new(
        r"(?i)(claude-[a-z0-9.-]+|gpt-[a-z0-9.-]+|gemini-[a-z0-9.-]+)\s+([\d.]+[kmb]?)\s*in,\s*([\d.]+[kmb]?)\s*out(?:,\s*([\d.]+[kmb]?)\s*cached)?(?:\s*\(Est\.\s*(\d+)\s*Premium)?"
    ).unwrap()
});

// ============================================================================
// TokenUsage: Per-mission token usage data
// ============================================================================

/// Token usage information parsed from runner stderr.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TokenUsage {
    /// Model used (e.g., "claude-opus-4.5", "claude-sonnet-4")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    
    /// Tokens input (parsed from "1.0m in" → 1_000_000)
    pub tokens_in: u64,
    
    /// Tokens output (parsed from "6.0k out" → 6_000)
    pub tokens_out: u64,
    
    /// Tokens cached (parsed from "935.5k cached" → 935_500)
    pub tokens_cached: u64,
    
    /// Estimated premium requests
    pub premium_requests: u32,
    
    /// API time spent in seconds
    pub api_time_seconds: f64,
    
    /// Total session time in seconds
    pub session_time_seconds: f64,
    
    /// Code changes: lines added
    pub lines_added: u32,
    
    /// Code changes: lines removed
    pub lines_removed: u32,
}

impl TokenUsage {
    /// Check if any meaningful data was parsed.
    pub fn has_data(&self) -> bool {
        self.tokens_in > 0 || self.tokens_out > 0 || self.premium_requests > 0
    }

    /// Format for single-line console display after a mission.
    pub fn to_display_line(&self) -> String {
        if !self.has_data() {
            return String::new();
        }

        let model = self.model.as_deref().unwrap_or("unknown");
        let model_short = extract_model_tier(model);

        format!(
            "📊 Tokens: {} in, {} out ({} cached) | Model: {} | Premium: {}",
            format_tokens(self.tokens_in),
            format_tokens(self.tokens_out),
            format_tokens(self.tokens_cached),
            model_short,
            self.premium_requests
        )
    }

    /// Parse token usage from runner stderr.
    pub fn parse(stderr: &str) -> Self {
        let mut usage = TokenUsage::default();

        // Parse "Total usage est: N Premium requests"
        if let Some(caps) = RE_PREMIUM_REQUESTS.captures(stderr) {
            if let Some(m) = caps.get(1) {
                usage.premium_requests = m.as_str().parse().unwrap_or(0);
            }
        }

        // Parse "API time spent: Xm Y.YYs"
        if let Some(caps) = RE_API_TIME.captures(stderr) {
            if let Some(m) = caps.get(1) {
                usage.api_time_seconds = parse_time_str(m.as_str());
            }
        }

        // Parse "Total session time: Xm Y.YYs"
        if let Some(caps) = RE_SESSION_TIME.captures(stderr) {
            if let Some(m) = caps.get(1) {
                usage.session_time_seconds = parse_time_str(m.as_str());
            }
        }

        // Parse "Total code changes: +N -M"
        if let Some(caps) = RE_CODE_CHANGES.captures(stderr) {
            if let Some(m) = caps.get(1) {
                usage.lines_added = m.as_str().parse().unwrap_or(0);
            }
            if let Some(m) = caps.get(2) {
                usage.lines_removed = m.as_str().parse().unwrap_or(0);
            }
        }

        // Parse model line with token counts
        if let Some(caps) = RE_MODEL_LINE.captures(stderr) {
            if let Some(m) = caps.get(1) {
                usage.model = Some(m.as_str().to_string());
            }
            if let Some(m) = caps.get(2) {
                usage.tokens_in = parse_token_count(m.as_str());
            }
            if let Some(m) = caps.get(3) {
                usage.tokens_out = parse_token_count(m.as_str());
            }
            if let Some(m) = caps.get(4) {
                usage.tokens_cached = parse_token_count(m.as_str());
            }
            // Premium requests from model line (may override earlier value)
            if let Some(m) = caps.get(5) {
                usage.premium_requests = m.as_str().parse().unwrap_or(usage.premium_requests);
            }
        }

        usage
    }
}

// ============================================================================
// MissionTokenStats: Stats for a single mission (for session aggregation)
// ============================================================================

/// Token usage statistics for a single mission (used for session aggregation).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MissionTokenStats {
    /// Mission type name
    pub mission_type: String,
    /// Tokens input
    pub tokens_in: u64,
    /// Tokens output
    pub tokens_out: u64,
    /// Tokens cached
    pub tokens_cached: u64,
    /// Premium requests
    pub premium_requests: u32,
    /// Model used
    pub model: Option<String>,
    /// Elapsed wall-clock time in seconds
    pub elapsed_seconds: f64,
}

impl MissionTokenStats {
    /// Create from TokenUsage and additional context.
    #[allow(dead_code)]
    pub fn from_usage(usage: &TokenUsage, mission_type: &str, elapsed_seconds: f64) -> Self {
        Self {
            mission_type: mission_type.to_string(),
            tokens_in: usage.tokens_in,
            tokens_out: usage.tokens_out,
            tokens_cached: usage.tokens_cached,
            premium_requests: usage.premium_requests,
            model: usage.model.clone(),
            elapsed_seconds,
        }
    }
}

// ============================================================================
// MissionTypeStats: Aggregated stats per mission type
// ============================================================================

/// Aggregate stats for a mission type within a session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MissionTypeStats {
    pub count: u32,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub premium_requests: u32,
}

// ============================================================================
// SessionTokenStats: Aggregate stats for entire session
// ============================================================================

/// Aggregate token usage for an entire session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionTokenStats {
    /// Total missions completed successfully
    pub missions_completed: u32,
    /// Total missions that failed
    pub missions_failed: u32,
    /// Total tokens in
    pub total_tokens_in: u64,
    /// Total tokens out
    pub total_tokens_out: u64,
    /// Total tokens cached
    pub total_tokens_cached: u64,
    /// Total premium requests
    pub total_premium_requests: u32,
    /// Total runner elapsed time in seconds
    pub total_elapsed_seconds: f64,
    /// Stats per mission type
    #[serde(default)]
    pub by_mission_type: HashMap<String, MissionTypeStats>,
}

impl SessionTokenStats {
    /// Record stats from a completed mission.
    pub fn record_mission(&mut self, stats: &MissionTokenStats, success: bool) {
        if success {
            self.missions_completed += 1;
        } else {
            self.missions_failed += 1;
        }
        self.total_tokens_in += stats.tokens_in;
        self.total_tokens_out += stats.tokens_out;
        self.total_tokens_cached += stats.tokens_cached;
        self.total_premium_requests += stats.premium_requests;
        self.total_elapsed_seconds += stats.elapsed_seconds;

        let entry = self.by_mission_type
            .entry(stats.mission_type.clone())
            .or_default();
        entry.count += 1;
        entry.tokens_in += stats.tokens_in;
        entry.tokens_out += stats.tokens_out;
        entry.premium_requests += stats.premium_requests;
    }

    /// Check if any missions were recorded.
    #[allow(dead_code)]
    pub fn has_data(&self) -> bool {
        self.missions_completed > 0 || self.missions_failed > 0
    }

    /// Format a session summary for console display.
    pub fn format_summary(&self, initial_issues: usize, final_issues: usize, duration_secs: f64) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        
        writeln!(out, "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━").unwrap();
        writeln!(out, "SESSION SUMMARY").unwrap();
        writeln!(out, "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━").unwrap();
        writeln!(out, "Missions:     {} completed, {} failed", self.missions_completed, self.missions_failed).unwrap();
        writeln!(out, "Duration:     {}", format_duration(duration_secs)).unwrap();
        
        let issues_delta = final_issues as i32 - initial_issues as i32;
        let issues_str = if issues_delta < 0 {
            format!("{} → {} ({:+})", initial_issues, final_issues, issues_delta)
        } else if issues_delta > 0 {
            format!("{} → {} (+{})", initial_issues, final_issues, issues_delta)
        } else {
            format!("{} → {} (no change)", initial_issues, final_issues)
        };
        writeln!(out, "Issues:       {}", issues_str).unwrap();
        writeln!(out).unwrap();

        if !self.by_mission_type.is_empty() {
            writeln!(out, "Token Usage by Mission Type:").unwrap();
            let mut types: Vec<_> = self.by_mission_type.iter().collect();
            types.sort_by_key(|(_, s)| std::cmp::Reverse(s.tokens_in));
            for (mission_type, stats) in types {
                writeln!(out, "  {} ({:>2}):  {:>6} in, {:>5} out | Premium: {}",
                    truncate_and_pad(mission_type, 28),
                    stats.count,
                    format_tokens(stats.tokens_in),
                    format_tokens(stats.tokens_out),
                    stats.premium_requests
                ).unwrap();
            }
            writeln!(out).unwrap();
        }

        writeln!(out, "Total: {} tokens in, {} tokens out",
            format_tokens(self.total_tokens_in),
            format_tokens(self.total_tokens_out)
        ).unwrap();
        writeln!(out, "Premium requests: {}", self.total_premium_requests).unwrap();

        if self.missions_completed > 0 && initial_issues > final_issues {
            let issues_resolved = initial_issues - final_issues;
            let premium_per_issue = self.total_premium_requests as f64 / issues_resolved as f64;
            writeln!(out, "Avg premium requests per issue resolved: {:.2}", premium_per_issue).unwrap();
        }

        out
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse a token count string like "1.0m", "6.0k", "935.5k", "1000"
fn parse_token_count(s: &str) -> u64 {
    let s = s.trim().to_lowercase();
    let (num_str, multiplier) = if s.ends_with('m') {
        (&s[..s.len()-1], 1_000_000.0)
    } else if s.ends_with('k') {
        (&s[..s.len()-1], 1_000.0)
    } else if s.ends_with('b') {
        (&s[..s.len()-1], 1_000_000_000.0)
    } else {
        (s.as_str(), 1.0)
    };

    num_str.parse::<f64>().map(|n| (n * multiplier) as u64).unwrap_or(0)
}

/// Parse a time string like "2m 19.91s" or "19.91s" or "2m" into seconds
fn parse_time_str(s: &str) -> f64 {
    let s = s.trim().to_lowercase();
    let mut total_seconds = 0.0;

    // Extract minutes if present
    if let Some(m_pos) = s.find('m') {
        if let Ok(mins) = s[..m_pos].trim().parse::<f64>() {
            total_seconds += mins * 60.0;
        }
    }

    // Extract seconds if present
    if let Some(s_pos) = s.find('s') {
        let before_s = &s[..s_pos];
        // Find where seconds start (after 'm ' or from beginning)
        let sec_start = before_s.rfind('m').map(|i| i + 1).unwrap_or(0);
        if let Ok(secs) = before_s[sec_start..].trim().parse::<f64>() {
            total_seconds += secs;
        }
    } else if !s.contains('m') {
        // Just a number without unit - treat as seconds
        if let Ok(secs) = s.parse::<f64>() {
            total_seconds = secs;
        }
    }

    total_seconds
}

/// Format token count for display (e.g., 1_000_000 → "1.0M")
pub fn format_tokens(count: u64) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}k", count as f64 / 1_000.0)
    } else {
        format!("{}", count)
    }
}

/// Format duration in seconds to human-readable string
fn format_duration(secs: f64) -> String {
    let mins = (secs / 60.0) as u32;
    let secs_remainder = secs % 60.0;
    if mins > 0 {
        format!("{}m {:.0}s", mins, secs_remainder)
    } else {
        format!("{:.1}s", secs)
    }
}

/// Truncate string to max length and pad to that length
fn truncate_and_pad(name: &str, max_len: usize) -> String {
    if name.len() <= max_len {
        format!("{:width$}", name, width = max_len)
    } else {
        format!("{}…", &name[..max_len - 1])
    }
}

/// Extract model tier from full model name (e.g., "claude-opus-4.5" → "opus")
fn extract_model_tier(model: &str) -> &str {
    let lower = model.to_lowercase();
    if lower.contains("opus") {
        "opus"
    } else if lower.contains("sonnet") {
        "sonnet"
    } else if lower.contains("haiku") {
        "haiku"
    } else if lower.contains("gpt-4") || lower.contains("gpt4") {
        "gpt-4"
    } else if lower.contains("gpt-3") || lower.contains("gpt3") {
        "gpt-3.5"
    } else {
        model.split('-').last().unwrap_or(model)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_token_count() {
        assert_eq!(parse_token_count("1.0m"), 1_000_000);
        assert_eq!(parse_token_count("6.0k"), 6_000);
        assert_eq!(parse_token_count("935.5k"), 935_500);
        assert_eq!(parse_token_count("1000"), 1000);
        assert_eq!(parse_token_count("2.5M"), 2_500_000);
        assert_eq!(parse_token_count("0"), 0);
        assert_eq!(parse_token_count(""), 0);
    }

    #[test]
    fn test_parse_time_str() {
        assert!((parse_time_str("2m 19.91s") - 139.91).abs() < 0.01);
        assert!((parse_time_str("19.91s") - 19.91).abs() < 0.01);
        assert!((parse_time_str("2m") - 120.0).abs() < 0.01);
        assert!((parse_time_str("2m 35.177s") - 155.177).abs() < 0.01);
        assert!((parse_time_str("0s") - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_copilot_format_full() {
        let stderr = r#"
Total usage est: 3 Premium requests API time spent: 2m 19.91s Total session time: 2m 35.177s
Total code changes: +10 -9 Breakdown by AI model:
 claude-opus-4.5   1.0m in, 6.0k out, 935.5k cached (Est. 3 Premium requests)
"#;
        let usage = TokenUsage::parse(stderr);
        assert_eq!(usage.model, Some("claude-opus-4.5".to_string()));
        assert_eq!(usage.tokens_in, 1_000_000);
        assert_eq!(usage.tokens_out, 6_000);
        assert_eq!(usage.tokens_cached, 935_500);
        assert_eq!(usage.premium_requests, 3);
        assert!((usage.api_time_seconds - 139.91).abs() < 0.1);
        assert!((usage.session_time_seconds - 155.177).abs() < 0.1);
        assert_eq!(usage.lines_added, 10);
        assert_eq!(usage.lines_removed, 9);
        assert!(usage.has_data());
    }

    #[test]
    fn test_parse_model_line_only() {
        let stderr = " claude-sonnet-4   2.5m in, 15.2k out, 100k cached (Est. 5 Premium requests)";
        let usage = TokenUsage::parse(stderr);
        assert_eq!(usage.model, Some("claude-sonnet-4".to_string()));
        assert_eq!(usage.tokens_in, 2_500_000);
        assert_eq!(usage.tokens_out, 15_200);
        assert_eq!(usage.tokens_cached, 100_000);
        assert_eq!(usage.premium_requests, 5);
    }

    #[test]
    fn test_parse_model_line_no_cached() {
        let stderr = " gpt-4-turbo   500k in, 10k out";
        let usage = TokenUsage::parse(stderr);
        assert_eq!(usage.model, Some("gpt-4-turbo".to_string()));
        assert_eq!(usage.tokens_in, 500_000);
        assert_eq!(usage.tokens_out, 10_000);
        assert_eq!(usage.tokens_cached, 0);
    }

    #[test]
    fn test_parse_empty() {
        let usage = TokenUsage::parse("");
        assert!(!usage.has_data());
        assert_eq!(usage.tokens_in, 0);
        assert_eq!(usage.tokens_out, 0);
        assert_eq!(usage.model, None);
    }

    #[test]
    fn test_parse_unrelated_content() {
        let stderr = "Building project...\nCompiling foo.rs\nDone.";
        let usage = TokenUsage::parse(stderr);
        assert!(!usage.has_data());
    }

    #[test]
    fn test_display_line() {
        let usage = TokenUsage {
            model: Some("claude-opus-4.5".to_string()),
            tokens_in: 1_000_000,
            tokens_out: 6_000,
            tokens_cached: 935_500,
            premium_requests: 3,
            api_time_seconds: 139.91,
            session_time_seconds: 155.177,
            lines_added: 10,
            lines_removed: 9,
        };
        let display = usage.to_display_line();
        assert!(display.contains("1.0M in"));
        assert!(display.contains("6.0k out"));
        assert!(display.contains("935.5k cached"));
        assert!(display.contains("opus"));
        assert!(display.contains("Premium: 3"));
    }

    #[test]
    fn test_display_line_empty() {
        let usage = TokenUsage::default();
        assert_eq!(usage.to_display_line(), "");
    }

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

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(500), "500");
        assert_eq!(format_tokens(1_000), "1.0k");
        assert_eq!(format_tokens(1_500), "1.5k");
        assert_eq!(format_tokens(1_000_000), "1.0M");
        assert_eq!(format_tokens(2_500_000), "2.5M");
    }

    #[test]
    fn test_extract_model_tier() {
        assert_eq!(extract_model_tier("claude-opus-4.5"), "opus");
        assert_eq!(extract_model_tier("claude-sonnet-4"), "sonnet");
        assert_eq!(extract_model_tier("claude-haiku-3"), "haiku");
        assert_eq!(extract_model_tier("gpt-4-turbo"), "gpt-4");
        assert_eq!(extract_model_tier("unknown-model"), "model");
    }
}
