//! Allowlist enforcement for plan-only command execution.

use anyhow::{bail, Result};

use crate::runner::AllowedCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllowedTool {
    Namako,
    Tesaki,
}

impl AllowedTool {
    fn as_str(&self) -> &'static str {
        match self {
            AllowedTool::Namako => "namako",
            AllowedTool::Tesaki => "tesaki",
        }
    }
}

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

const TESAKI_SUBCOMMANDS: &[&str] = &["config", "next", "run", "status", "explain"];

pub fn validate_command(command: &AllowedCommand) -> Result<AllowedTool> {
    let tool = match command.tool.as_str() {
        "namako" => AllowedTool::Namako,
        "tesaki" => AllowedTool::Tesaki,
        other => bail!("Command tool '{}' is not allowlisted", other),
    };

    if let Some(first) = command.args.first() {
        let allowed = match tool {
            AllowedTool::Namako => NAMAKO_SUBCOMMANDS.contains(&first.as_str()),
            AllowedTool::Tesaki => TESAKI_SUBCOMMANDS.contains(&first.as_str()),
        };
        if !allowed {
            bail!(
                "Command '{}' is not allowlisted for {}",
                first,
                tool.as_str()
            );
        }
    }

    if command.args.iter().any(|arg| contains_shell_meta(arg)) {
        bail!("Command args contain forbidden shell metacharacters");
    }

    Ok(tool)
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
        let tool = validate_command(&cmd).unwrap();
        assert_eq!(tool, AllowedTool::Namako);
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
