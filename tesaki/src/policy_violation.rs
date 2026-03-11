//! Policy violation detection for runner output.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const MAX_EXAMPLES_PER_COMMAND: usize = 3;
const MAX_EXAMPLE_LEN: usize = 160;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyViolationEntry {
    pub command: String,
    pub count: usize,
    pub examples: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyViolationsReport {
    pub total: usize,
    pub by_command: Vec<PolicyViolationEntry>,
}

impl PolicyViolationsReport {
    pub fn is_empty(&self) -> bool {
        self.total == 0
    }

    pub fn summary_line(&self) -> String {
        if self.is_empty() {
            return "Policy violations: none".to_string();
        }
        let per_cmd = self
            .by_command
            .iter()
            .map(|entry| format!("{}: {}", entry.command, entry.count))
            .collect::<Vec<_>>()
            .join(", ");
        format!("Policy violations: {} ({})", self.total, per_cmd)
    }

    pub fn to_markdown(&self) -> String {
        if self.is_empty() {
            return "## Policy Violations\n\nNone detected.\n".to_string();
        }

        let mut out = String::from("## Policy Violations\n\n");
        out.push_str(&format!("Total: {}\n\n", self.total));
        for entry in &self.by_command {
            out.push_str(&format!("- {}: {}\n", entry.command, entry.count));
            if !entry.examples.is_empty() {
                out.push_str("  Examples:\n");
                for example in &entry.examples {
                    out.push_str(&format!("  - {}\n", example));
                }
            }
        }
        out
    }
}

pub fn scan_policy_violations(output: &str) -> PolicyViolationsReport {
    let re = Regex::new(r"(?i)(^|[\s>$])(?P<cmd>cargo|git|make|npm|pnpm|yarn|bun)(\s|$)")
        .expect("invalid policy violation regex");

    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut examples: HashMap<String, Vec<String>> = HashMap::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut seen_in_line: HashSet<String> = HashSet::new();
        for cap in re.captures_iter(trimmed) {
            let cmd = cap
                .name("cmd")
                .map(|m| m.as_str().to_ascii_lowercase())
                .unwrap_or_default();
            if cmd.is_empty() {
                continue;
            }

            *counts.entry(cmd.clone()).or_insert(0) += 1;

            if !seen_in_line.contains(&cmd) {
                let entry = examples.entry(cmd.clone()).or_default();
                if entry.len() < MAX_EXAMPLES_PER_COMMAND {
                    let example = truncate_line(trimmed, MAX_EXAMPLE_LEN);
                    if !entry.contains(&example) {
                        entry.push(example);
                    }
                }
                seen_in_line.insert(cmd);
            }
        }
    }

    let mut entries = counts
        .into_iter()
        .map(|(command, count)| PolicyViolationEntry {
            command: command.clone(),
            count,
            examples: examples.remove(&command).unwrap_or_default(),
        })
        .collect::<Vec<_>>();

    entries.sort_by(|a, b| a.command.cmp(&b.command));

    let total = entries.iter().map(|entry| entry.count).sum();

    PolicyViolationsReport {
        total,
        by_command: entries,
    }
}

fn truncate_line(line: &str, max_len: usize) -> String {
    if line.len() <= max_len {
        return line.to_string();
    }

    let mut out = String::new();
    for (i, ch) in line.chars().enumerate() {
        if i >= max_len.saturating_sub(3) {
            break;
        }
        out.push(ch);
    }
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_policy_violations() {
        let output = r#"
> cargo build
Running: git status
make test
npm run lint
"#;
        let report = scan_policy_violations(output);
        assert_eq!(report.total, 4);
        assert!(report.by_command.iter().any(|e| e.command == "cargo"));
        assert!(report.by_command.iter().any(|e| e.command == "git"));
        assert!(report.by_command.iter().any(|e| e.command == "make"));
        assert!(report.by_command.iter().any(|e| e.command == "npm"));
    }

    #[test]
    fn ignores_cargo_toml_mentions() {
        let output = "Edited Cargo.toml and src/lib.rs";
        let report = scan_policy_violations(output);
        assert_eq!(report.total, 0);
    }
}
