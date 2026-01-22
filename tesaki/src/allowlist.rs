//! Allowlist enforcement for plan-only command execution.

use anyhow::{bail, Result};

use crate::chat_plan::AllowedCommand;

const NAMAKO_SUBCOMMANDS: &[&str] = &[
    "status",
    "review",
    "explain",
    "gate",
    "lint",
    "verify",
    "manifest",
    "run",
];

pub fn validate_command(command: &AllowedCommand) -> Result<()> {
    if command.tool.as_str() != "namako" {
        bail!("Command tool '{}' is not allowlisted", command.tool);
    }

    if let Some(first) = command.args.first() {
        let allowed = NAMAKO_SUBCOMMANDS.contains(&first.as_str());
        if !allowed {
            bail!("Command '{}' is not allowlisted for namako", first);
        }
    }

    if command.args.iter().any(|arg| contains_shell_meta(arg)) {
        bail!("Command args contain forbidden shell metacharacters");
    }

    Ok(())
}

fn contains_shell_meta(value: &str) -> bool {
    value.contains(';')
        || value.contains("&&")
        || value.contains("||")
        || value.contains('|')
        || value.contains('`')
        || value.contains('$')
        || value.contains(">")
        || value.contains("<")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_namako_gate() {
        let cmd = AllowedCommand {
            tool: "namako".to_string(),
            args: vec!["gate".to_string(), "--json".to_string()],
            reason: None,
        };
        assert!(validate_command(&cmd).is_ok());
    }

    #[test]
    fn rejects_unknown_tool() {
        let cmd = AllowedCommand {
            tool: "bash".to_string(),
            args: vec!["-c".to_string(), "echo nope".to_string()],
            reason: None,
        };
        assert!(validate_command(&cmd).is_err());
    }

    #[test]
    fn rejects_disallowed_subcommand() {
        let cmd = AllowedCommand {
            tool: "namako".to_string(),
            args: vec!["update-cert".to_string()],
            reason: None,
        };
        assert!(validate_command(&cmd).is_err());
    }

    #[test]
    fn rejects_shell_metacharacters() {
        let cmd = AllowedCommand {
            tool: "namako".to_string(),
            args: vec!["gate".to_string(), "--json; rm -rf /".to_string()],
            reason: None,
        };
        assert!(validate_command(&cmd).is_err());
    }
}
