//! Plan-only chat planner implementations.

use anyhow::{Context, Result};
use serde_json::Value;
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use crate::chat_plan::{ChatPlan, ChatTurnInput};

/// Plan-only chat interface. Implement this for chat planner backends.
pub trait ChatPlanner: Send + Sync {
    fn plan_turn(&self, input: &ChatTurnInput) -> Result<ChatPlan>;
    fn name(&self) -> &'static str;
}

/// Mock chat planner for tests and offline usage.
pub struct MockChatPlanner {
    response: ChatPlan,
}

impl MockChatPlanner {
    pub fn new(response: ChatPlan) -> Self {
        Self { response }
    }
}

impl ChatPlanner for MockChatPlanner {
    fn plan_turn(&self, _input: &ChatTurnInput) -> Result<ChatPlan> {
        Ok(self.response.clone())
    }

    fn name(&self) -> &'static str {
        "mock"
    }
}

/// Command-driven planner. The command must emit a JSON ChatPlan to stdout.
pub(crate) struct BaseChatPlanner {
    command_template: String,
    working_dir: PathBuf,
    timeout: Option<Duration>,
    stream_output: bool,
}

impl BaseChatPlanner {
    pub fn new_with_timeout_and_stream(
        command_template: String,
        working_dir: PathBuf,
        timeout: Option<Duration>,
        stream_output: bool,
    ) -> Self {
        Self {
            command_template,
            working_dir,
            timeout,
            stream_output,
        }
    }

    fn expand_command(
        &self,
        input_path: Option<&PathBuf>,
        output_path: Option<&PathBuf>,
    ) -> String {
        let mut command = self.command_template.clone();
        if let Some(path) = input_path {
            command = command.replace("{input_file}", &path.display().to_string());
        }
        if let Some(path) = output_path {
            command = command.replace("{output_file}", &path.display().to_string());
        }
        command
    }

    fn write_input_temp(&self, input: &ChatTurnInput) -> Result<(tempfile::NamedTempFile, PathBuf)> {
        let mut file = tempfile::NamedTempFile::new()
            .context("Failed to create temp file for chat planner")?;
        // Format as a natural language prompt, not raw JSON
        let prompt = format_planner_prompt(input);
        file.write_all(prompt.as_bytes())?;
        let path = file.path().to_path_buf();
        Ok((file, path))
    }
}

/// Format the ChatTurnInput as a compact prompt for LLM planners.
/// Only sends essential state, not full JSON dumps.
fn format_planner_prompt(input: &ChatTurnInput) -> String {
    let system_prompt = input.system_prompt.as_deref().unwrap_or(DEFAULT_SYSTEM_PROMPT);
    
    let mut prompt = String::new();
    prompt.push_str(system_prompt);
    prompt.push_str("\n\n");
    
    // Extract compact state from session_state_json
    prompt.push_str("## Repository State\n");
    if let Some(summary) = input.session_state_json.get("last_repo_state_summary").and_then(|v| v.as_str()) {
        prompt.push_str(summary);
        prompt.push('\n');
    }
    if let Some(intent) = input.session_state_json.get("intent") {
        if let Some(stage) = intent.get("stage").and_then(|v| v.as_str()) {
            prompt.push_str(&format!("Stage: {}\n", stage));
        }
    }
    prompt.push('\n');
    
    // Add planner hint if present (important context)
    if let Some(hint) = &input.planner_hint {
        prompt.push_str("Note: ");
        prompt.push_str(hint);
        prompt.push_str("\n\n");
    }
    
    // User message
    prompt.push_str("## User\n");
    prompt.push_str(&input.user_message);
    prompt.push_str("\n\n");
    
    // Response instruction
    prompt.push_str("Respond with JSON only.\n");
    
    prompt
}

const DEFAULT_SYSTEM_PROMPT: &str = r#"You are Tesaki, a spec-driven development assistant.

Given the repository state below, help the developer by:
1. Answering questions about the current state
2. Proposing a mission when they want to make progress

Respond with JSON only:
```json
{"say": "Brief response", "mission_proposal": null, "done": true}
```

For mission_proposal (when the user wants to make progress):
```json
{
  "mission_type": "CreateMissingBindings",
  "stage": "Implement Tests & Bindings", 
  "target": "02_transport.feature",
  "surfaces": {"spec": "LOCKED", "tests": "UNLOCKED", "sut": "LOCKED"},
  "objective": "Add step bindings for missing steps",
  "validation": ["namako lint passes"]
}
```

Mission types:
- CreateMissingBindings: Add step bindings for unbound steps
- ImplementBehaviorForScenario: Implement SUT code to pass a failing scenario
- FixRegressionFromGateFailure: Fix a newly failing test
- NormalizeIdentityTags: Add/fix @Feature/@Rule/@Scenario tags

Be concise. Focus on actionable next steps.
"#;

fn wait_with_output_timeout(mut child: Child, timeout: Duration) -> Result<Output> {
    let start = Instant::now();
    let poll_interval = Duration::from_millis(100);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = child
                    .stdout
                    .take()
                    .map(|mut s| -> Result<Vec<u8>> {
                        let mut buf = Vec::new();
                        s.read_to_end(&mut buf)
                            .context("Failed to read planner stdout")?;
                        Ok(buf)
                    })
                    .transpose()?
                    .unwrap_or_default();

                let stderr = child
                    .stderr
                    .take()
                    .map(|mut s| -> Result<Vec<u8>> {
                        let mut buf = Vec::new();
                        s.read_to_end(&mut buf)
                            .context("Failed to read planner stderr")?;
                        Ok(buf)
                    })
                    .transpose()?
                    .unwrap_or_default();

                return Ok(Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    anyhow::bail!(
                        "Planner command timed out after {} seconds",
                        timeout.as_secs()
                    );
                }
                std::thread::sleep(poll_interval);
            }
            Err(e) => return Err(e).context("Planner command failed"),
        }
    }
}

fn wait_with_streaming_output(
    mut child: Child,
    timeout: Option<Duration>,
    stream_output: bool,
) -> Result<Output> {
    let stdout_handle = child.stdout.take();
    let stderr_handle = child.stderr.take();

    let stdout_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let stderr_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

    let stdout_buf_clone = std::sync::Arc::clone(&stdout_buf);
    let stderr_buf_clone = std::sync::Arc::clone(&stderr_buf);

    let stdout_thread = stdout_handle.map(|stdout| {
        std::thread::spawn(move || {
            let reader = std::io::BufReader::new(stdout);
            for line_result in reader.lines() {
                if let Ok(line) = line_result {
                    if stream_output {
                        let _ = writeln!(std::io::stdout(), "{}", line);
                    }
                    let mut buf = stdout_buf_clone.lock().unwrap();
                    buf.extend_from_slice(line.as_bytes());
                    buf.push(b'\n');
                }
            }
        })
    });

    let stderr_thread = stderr_handle.map(|stderr| {
        std::thread::spawn(move || {
            let reader = std::io::BufReader::new(stderr);
            for line_result in reader.lines() {
                if let Ok(line) = line_result {
                    if stream_output {
                        let _ = writeln!(std::io::stderr(), "{}", line);
                    }
                    let mut buf = stderr_buf_clone.lock().unwrap();
                    buf.extend_from_slice(line.as_bytes());
                    buf.push(b'\n');
                }
            }
        })
    });

    let status = if let Some(timeout) = timeout {
        let start = Instant::now();
        let poll_interval = Duration::from_millis(100);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    if start.elapsed() > timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        anyhow::bail!(
                            "Planner command timed out after {} seconds",
                            timeout.as_secs()
                        );
                    }
                    std::thread::sleep(poll_interval);
                }
                Err(e) => return Err(e).context("Planner command failed"),
            }
        }
    } else {
        child.wait().context("Planner command failed")?
    };

    if let Some(t) = stdout_thread {
        let _ = t.join();
    }
    if let Some(t) = stderr_thread {
        let _ = t.join();
    }

    let stdout = match std::sync::Arc::try_unwrap(stdout_buf) {
        Ok(mutex) => mutex.into_inner().unwrap_or_default(),
        Err(arc) => arc.lock().unwrap().clone(),
    };
    let stderr = match std::sync::Arc::try_unwrap(stderr_buf) {
        Ok(mutex) => mutex.into_inner().unwrap_or_default(),
        Err(arc) => arc.lock().unwrap().clone(),
    };

    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

impl ChatPlanner for BaseChatPlanner {
    fn plan_turn(&self, input: &ChatTurnInput) -> Result<ChatPlan> {
        let wants_input_file = self.command_template.contains("{input_file}");
        let wants_output_file = self.command_template.contains("{output_file}");
        let (temp_input, input_path) = if wants_input_file {
            let (file, path) = self.write_input_temp(input)?;
            (Some(file), Some(path))
        } else {
            (None, None)
        };
        let (temp_output, output_path) = if wants_output_file {
            let file = tempfile::NamedTempFile::new()
                .context("Failed to create temp file for planner output")?;
            let path = file.path().to_path_buf();
            (Some(file), Some(path))
        } else {
            (None, None)
        };

        let expanded = self.expand_command(input_path.as_ref(), output_path.as_ref());
        let parts: Vec<&str> = expanded.split_whitespace().collect();
        if parts.is_empty() {
            anyhow::bail!("Empty planner command");
        }

        let program = parts[0];
        let args: Vec<&str> = parts[1..].to_vec();

        let mut cmd = Command::new(program);
        cmd.args(&args)
            .current_dir(&self.working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if !wants_input_file {
            cmd.stdin(Stdio::piped());
        } else {
            cmd.stdin(Stdio::null());
        }

        let mut child = cmd.spawn().context("Failed to spawn planner command")?;

        if !wants_input_file {
            if let Some(mut stdin) = child.stdin.take() {
                // Format as natural language prompt, not raw JSON
                let prompt = format_planner_prompt(input);
                stdin.write_all(prompt.as_bytes())?;
            }
        }

        let output = if self.stream_output {
            wait_with_streaming_output(child, self.timeout, self.stream_output)?
        } else if let Some(timeout) = self.timeout {
            wait_with_output_timeout(child, timeout)?
        } else {
            child.wait_with_output().context("Planner command failed")?
        };
        drop(temp_input);
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Planner command failed: {}", stderr.trim());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let file_text = output_path
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_default();
        let output_text = if file_text.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            file_text
        };
        drop(temp_output);

        if output_text.trim().is_empty() {
            anyhow::bail!("Planner returned empty output");
        }

        // Strip markdown code fences if present (LLMs often wrap JSON in ```json...```)
        let json_text = strip_markdown_code_fences(&output_text);

        let value: Value = serde_json::from_str(&json_text)
            .with_context(|| format!("Planner output is not valid JSON: {}", output_text))?;
        let plan: ChatPlan = serde_json::from_value(value)
            .with_context(|| "Planner output JSON does not match ChatPlan schema")?;
        Ok(plan)
    }

    fn name(&self) -> &'static str {
        "cli"
    }
}

/// Strip markdown code fences from LLM output.
/// Handles both ```json ... ``` and ``` ... ``` formats.
/// Also handles text before the code fence (LLMs sometimes add explanatory text).
fn strip_markdown_code_fences(text: &str) -> String {
    let trimmed = text.trim();
    
    // Find the start of a code fence (may be preceded by other text)
    let start_patterns = ["```json", "```JSON", "```"];
    
    for pattern in &start_patterns {
        if let Some(start_pos) = trimmed.find(pattern) {
            let after_pattern = &trimmed[start_pos + pattern.len()..];
            // Find the closing fence
            if let Some(end_pos) = after_pattern.rfind("```") {
                return after_pattern[..end_pos].trim().to_string();
            }
            // No closing fence, return everything after the opening
            return after_pattern.trim().to_string();
        }
    }
    
    // No code fence found, return as-is
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_plan::{AllowedCommand, MissionProposal, SurfacePolicy, SurfaceLock};

    #[test]
    fn mock_planner_returns_plan() {
        let plan = ChatPlan {
            say: "hello".to_string(),
            run: vec![],
            mission_proposal: None,
            done: true,
        };
        let planner = MockChatPlanner::new(plan.clone());
        let input = ChatTurnInput {
            user_message: "hi".to_string(),
            session_state_json: serde_json::json!({}),
            recent_command_results: vec![],
            planner_hint: None,
        };
        let result = planner.plan_turn(&input).unwrap();
        assert_eq!(result.say, plan.say);
    }

    #[test]
    fn chat_plan_json_round_trip() {
        let plan = ChatPlan {
            say: "ok".to_string(),
            run: vec![AllowedCommand {
                tool: "namako".to_string(),
                args: vec!["status".to_string(), "--json".to_string()],
                reason: None,
            }],
            mission_proposal: Some(MissionProposal {
                mission_type: "CreateMissingBindings".to_string(),
                stage: "Implement Tests & Bindings".to_string(),
                target: "@Scenario(03)".to_string(),
                surfaces: SurfacePolicy {
                    spec: SurfaceLock::Locked,
                    tests: SurfaceLock::Unlocked,
                    sut: SurfaceLock::Locked,
                },
                objective: "Create bindings".to_string(),
                validation: vec!["namako gate --json passes".to_string()],
            }),
            done: true,
        };
        let json = serde_json::to_string(&plan).unwrap();
        let parsed: ChatPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.say, "ok");
    }
}
