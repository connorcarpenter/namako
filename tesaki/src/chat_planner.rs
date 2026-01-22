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
        let json = serde_json::to_string_pretty(input)?;
        file.write_all(json.as_bytes())?;
        let path = file.path().to_path_buf();
        Ok((file, path))
    }
}

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
                let json = serde_json::to_string_pretty(input)?;
                stdin.write_all(json.as_bytes())?;
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

        let value: Value = serde_json::from_str(&output_text)
            .with_context(|| format!("Planner output is not valid JSON: {}", output_text))?;
        let plan: ChatPlan = serde_json::from_value(value)
            .with_context(|| "Planner output JSON does not match ChatPlan schema")?;
        Ok(plan)
    }

    fn name(&self) -> &'static str {
        "cli"
    }
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
