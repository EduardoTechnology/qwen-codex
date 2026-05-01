use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;

use anyhow::Context;
use serde_json::Value;
use tokio::process::Command;

use crate::QWEN_ENTRYPOINT_ENV;
use crate::config::ResolvedQwenConfig;
use crate::yolo::types::AgentRoundRequest;
use crate::yolo::types::AgentRoundResult;

const OUTPUT_TAIL_LIMIT: usize = 8_000;
const DANGEROUS_BYPASS_FLAG: &str = "--dangerously-bypass-approvals-and-sandbox";

pub(crate) struct CodexAgentRunner {
    codex_exe: PathBuf,
    config: ResolvedQwenConfig,
    dangerously_bypass_approvals_and_sandbox: bool,
}

impl CodexAgentRunner {
    pub(crate) fn new(
        codex_exe: PathBuf,
        config: ResolvedQwenConfig,
        dangerously_bypass_approvals_and_sandbox: bool,
    ) -> Self {
        Self {
            codex_exe,
            config,
            dangerously_bypass_approvals_and_sandbox,
        }
    }

    pub(crate) async fn run_round(
        &self,
        request: AgentRoundRequest,
    ) -> anyhow::Result<AgentRoundResult> {
        let args = build_codex_exec_args(
            self.config.codex_config_args(),
            request,
            self.dangerously_bypass_approvals_and_sandbox,
        );

        let mut command = Command::new(&self.codex_exe);
        command
            .kill_on_drop(true)
            .args(args)
            .env_remove(QWEN_ENTRYPOINT_ENV)
            .envs(self.config.child_env())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = command.output().await.with_context(|| {
            format!(
                "failed to launch upstream Codex binary at {}",
                self.codex_exe.display()
            )
        })?;

        Ok(parse_agent_output(
            output.status.code(),
            &String::from_utf8_lossy(&output.stdout),
            &String::from_utf8_lossy(&output.stderr),
        ))
    }
}

pub(crate) fn build_codex_exec_args(
    mut args: Vec<String>,
    request: AgentRoundRequest,
    dangerously_bypass_approvals_and_sandbox: bool,
) -> Vec<String> {
    args.extend([
        "exec".to_string(),
        "--json".to_string(),
        "--skip-git-repo-check".to_string(),
    ]);
    if dangerously_bypass_approvals_and_sandbox {
        args.push(DANGEROUS_BYPASS_FLAG.to_string());
    } else {
        args.extend(["--sandbox".to_string(), "workspace-write".to_string()]);
    }
    match request.thread_id {
        Some(thread_id) => {
            args.extend(["resume".to_string(), thread_id, request.prompt]);
        }
        None => {
            args.push(request.prompt);
        }
    }
    args
}

pub(crate) fn parse_agent_output(
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> AgentRoundResult {
    let mut result = AgentRoundResult {
        exit_code,
        stdout_tail: tail(stdout, OUTPUT_TAIL_LIMIT),
        stderr_tail: tail(stderr, OUTPUT_TAIL_LIMIT),
        ..AgentRoundResult::default()
    };
    let mut changed_files = BTreeSet::new();

    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        match value.get("type").and_then(Value::as_str) {
            Some("thread.started") => {
                result.session_id = value
                    .get("thread_id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
            }
            Some("turn.failed") => {
                if let Some(message) = value
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str)
                {
                    result.errors.push(message.to_string());
                }
            }
            Some("error") => {
                if let Some(message) = value.get("message").and_then(Value::as_str) {
                    result.errors.push(message.to_string());
                }
            }
            Some("item.completed") | Some("item.started") | Some("item.updated") => {
                collect_item(&value, &mut result, &mut changed_files);
            }
            Some("turn.started") | Some("turn.completed") | None => {}
            Some(other) => result.actions_taken.push(format!("event: {other}")),
        }
    }

    let failed_exit = exit_code.is_some_and(|code| code != 0);
    if let Some(code) = exit_code
        && failed_exit
    {
        result
            .errors
            .push(format!("agent process exited with code {code}"));
    }
    if failed_exit && !stderr.trim().is_empty() {
        result.errors.push(tail(stderr, OUTPUT_TAIL_LIMIT));
    }

    result.changed_files.extend(changed_files);
    result.actions_taken.sort();
    result.actions_taken.dedup();
    result.tool_calls.sort();
    result.tool_calls.dedup();
    result.commands_tests_run.sort();
    result.commands_tests_run.dedup();
    result.changed_files.sort();
    result.changed_files.dedup();
    result.errors.sort();
    result.errors.dedup();

    result
}

fn collect_item(
    value: &Value,
    result: &mut AgentRoundResult,
    changed_files: &mut BTreeSet<String>,
) {
    let Some(item) = value.get("item") else {
        return;
    };
    match item.get("type").and_then(Value::as_str) {
        Some("agent_message") => {
            if let Some(text) = item.get("text").and_then(Value::as_str) {
                result.final_response = Some(text.to_string());
            }
        }
        Some("command_execution") => {
            if let Some(command) = item.get("command").and_then(Value::as_str) {
                result.commands_tests_run.push(command.to_string());
                result.actions_taken.push(format!("command: {command}"));
            }
        }
        Some("file_change") => {
            for change in item
                .get("changes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(path) = change.get("path").and_then(Value::as_str) {
                    changed_files.insert(path.to_string());
                    result.actions_taken.push(format!("file change: {path}"));
                }
            }
        }
        Some("mcp_tool_call") => collect_tool_call("mcp", item, result),
        Some("collab_tool_call") => collect_tool_call("collab", item, result),
        Some("web_search") => {
            if let Some(query) = item.get("query").and_then(Value::as_str) {
                result.tool_calls.push(format!("web_search: {query}"));
                result.actions_taken.push(format!("web_search: {query}"));
            }
        }
        Some("error") => {
            if let Some(message) = item.get("message").and_then(Value::as_str) {
                result.errors.push(message.to_string());
            }
        }
        Some("reasoning") | Some("todo_list") | None => {}
        Some(other) => result.actions_taken.push(format!("item: {other}")),
    }
}

fn collect_tool_call(prefix: &str, item: &Value, result: &mut AgentRoundResult) {
    let tool = item
        .get("tool")
        .and_then(Value::as_str)
        .or_else(|| item.get("server").and_then(Value::as_str))
        .unwrap_or("unknown");
    let summary = format!("{prefix}: {tool}");
    result.tool_calls.push(summary.clone());
    result.actions_taken.push(summary);
    if let Some(error) = item
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
    {
        result.errors.push(error.to_string());
    }
}

pub(crate) async fn git_status(cwd: &Path) -> String {
    run_git(cwd, ["status", "--short"]).await
}

pub(crate) async fn git_diff_summary(cwd: &Path) -> String {
    let stat = run_git(cwd, ["diff", "--stat"]).await;
    let names = run_git(cwd, ["diff", "--name-status"]).await;
    match (stat.trim().is_empty(), names.trim().is_empty()) {
        (true, true) => String::new(),
        (false, true) => stat,
        (true, false) => names,
        (false, false) => format!("{stat}\n{names}"),
    }
}

pub(crate) fn changed_files_from_git_status(status: &str) -> Vec<String> {
    let mut files = status
        .lines()
        .filter_map(|line| line.split_whitespace().last())
        .filter(|path| !path.starts_with(".qwen-codex/"))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    files.sort();
    files.dedup();
    files
}

async fn run_git<const N: usize>(cwd: &Path, args: [&str; N]) -> String {
    match Command::new("git")
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
    {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        Ok(output) => String::from_utf8_lossy(&output.stderr).trim().to_string(),
        Err(err) => format!("git unavailable: {err}"),
    }
}

pub(crate) fn summarize_agent_result(result: &AgentRoundResult) -> String {
    let mut lines = Vec::new();
    if let Some(response) = &result.final_response {
        lines.push(format!("Final response: {}", tail(response, 2_000)));
    }
    if !result.actions_taken.is_empty() {
        lines.push(format!("Actions: {}", result.actions_taken.join("; ")));
    }
    if !result.errors.is_empty() {
        lines.push(format!("Errors: {}", result.errors.join("; ")));
    }
    if lines.is_empty() {
        lines.push("No final assistant message or structured action was captured.".to_string());
    }
    lines.join("\n")
}

pub(crate) fn tail(value: &str, max_chars: usize) -> String {
    let total = value.chars().count();
    if total <= max_chars {
        return value.to_string();
    }
    let tail = value
        .chars()
        .skip(total.saturating_sub(max_chars))
        .collect::<String>();
    format!("[truncated]\n{tail}")
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn parses_agent_json_events() {
        let stdout = [
            r#"{"type":"thread.started","thread_id":"thread-1"}"#,
            r#"{"type":"item.completed","item":{"id":"item_1","type":"command_execution","command":"cargo test","aggregated_output":"","exit_code":0,"status":"completed"}}"#,
            r#"{"type":"item.completed","item":{"id":"item_2","type":"file_change","changes":[{"path":"src/lib.rs","kind":"update"}],"status":"completed"}}"#,
            r#"{"type":"item.completed","item":{"id":"item_3","type":"agent_message","text":"done"}}"#,
        ]
        .join("\n");

        let parsed = parse_agent_output(Some(0), &stdout, "");

        assert_eq!(
            parsed,
            AgentRoundResult {
                session_id: Some("thread-1".to_string()),
                exit_code: Some(0),
                final_response: Some("done".to_string()),
                actions_taken: vec![
                    "command: cargo test".to_string(),
                    "file change: src/lib.rs".to_string(),
                ],
                tool_calls: Vec::new(),
                changed_files: vec!["src/lib.rs".to_string()],
                commands_tests_run: vec!["cargo test".to_string()],
                errors: Vec::new(),
                stdout_tail: stdout,
                stderr_tail: String::new(),
                timed_out: false,
            }
        );
    }

    #[test]
    fn codex_exec_args_default_to_workspace_write_sandbox() {
        let args = build_codex_exec_args(
            vec!["-c".to_string(), "model=\"qwen35-local\"".to_string()],
            AgentRoundRequest {
                iteration: 1,
                prompt: "Create files".to_string(),
                thread_id: None,
            },
            false,
        );

        assert_eq!(
            args,
            vec![
                "-c",
                "model=\"qwen35-local\"",
                "exec",
                "--json",
                "--skip-git-repo-check",
                "--sandbox",
                "workspace-write",
                "Create files",
            ]
        );
    }

    #[test]
    fn codex_exec_args_forward_explicit_dangerous_bypass() {
        let args = build_codex_exec_args(
            Vec::new(),
            AgentRoundRequest {
                iteration: 2,
                prompt: "Continue".to_string(),
                thread_id: Some("thread-1".to_string()),
            },
            true,
        );

        assert_eq!(
            args,
            vec![
                "exec",
                "--json",
                "--skip-git-repo-check",
                DANGEROUS_BYPASS_FLAG,
                "resume",
                "thread-1",
                "Continue",
            ]
        );
    }
}
