use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use chrono::Utc;
use tokio::fs;

use crate::redaction::redact_text;
use crate::yolo::types::YoloIterationLog;
use crate::yolo::types::YoloRunLog;

pub(crate) struct YoloLogger {
    run_dir: PathBuf,
}

impl YoloLogger {
    pub(crate) async fn new(log_root: &Path, run_id: &str) -> anyhow::Result<Self> {
        let run_dir = log_root.join(run_id);
        fs::create_dir_all(&run_dir).await.with_context(|| {
            format!("failed to create YOLO log directory {}", run_dir.display())
        })?;
        Ok(Self { run_dir })
    }

    pub(crate) fn run_dir(&self) -> &Path {
        &self.run_dir
    }

    pub(crate) async fn write_run(&self, run: &YoloRunLog) -> anyhow::Result<()> {
        write_redacted(
            &self.run_dir.join("run.json"),
            &serde_json::to_string_pretty(run).context("failed to serialize YOLO run JSON")?,
        )
        .await?;
        write_redacted(&self.run_dir.join("run.md"), &run_markdown(run)).await
    }

    pub(crate) async fn write_iteration(&self, iteration: &YoloIterationLog) -> anyhow::Result<()> {
        let stem = format!("iteration-{:03}", iteration.iteration);
        write_redacted(
            &self.run_dir.join(format!("{stem}.json")),
            &serde_json::to_string_pretty(iteration)
                .context("failed to serialize YOLO iteration JSON")?,
        )
        .await?;
        write_redacted(
            &self.run_dir.join(format!("{stem}.md")),
            &iteration_markdown(iteration),
        )
        .await
    }
}

pub(crate) fn new_run_id() -> String {
    format!(
        "{}-{}",
        Utc::now().format("%Y%m%dT%H%M%SZ"),
        std::process::id()
    )
}

pub(crate) fn now_timestamp() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

async fn write_redacted(path: &Path, contents: &str) -> anyhow::Result<()> {
    fs::write(path, redact_text(contents))
        .await
        .with_context(|| format!("failed to write {}", path.display()))
}

fn run_markdown(run: &YoloRunLog) -> String {
    let mut out = String::new();
    out.push_str(&format!("# YOLO Run {}\n\n", run.run_id));
    out.push_str(&format!("- Started: `{}`\n", run.started_at));
    if let Some(completed_at) = &run.completed_at {
        out.push_str(&format!("- Completed: `{completed_at}`\n"));
    }
    out.push_str(&format!("- Model: `{}`\n", run.model));
    out.push_str(&format!("- Base URL: `{}`\n", run.base_url));
    out.push_str(&format!("- Iteration limit: `{:?}`\n", run.iteration_limit));
    out.push_str(&format!("- Stop reason: `{:?}`\n\n", run.stop_reason));
    out.push_str("## Original Prompt\n\n");
    out.push_str(&run.original_prompt);
    out.push_str("\n\n## Iterations\n\n");
    for iteration in &run.iterations {
        out.push_str(&format!(
            "- Iteration {}: `{}`\n",
            iteration.iteration,
            iteration
                .stop_reason
                .as_ref()
                .map(|reason| format!("{reason:?}"))
                .unwrap_or_else(|| "continued".to_string())
        ));
    }
    out
}

fn iteration_markdown(iteration: &YoloIterationLog) -> String {
    let mut out = String::new();
    out.push_str(&format!("# YOLO Iteration {}\n\n", iteration.iteration));
    out.push_str(&format!("- Timestamp: `{}`\n", iteration.timestamp));
    out.push_str(&format!("- Session ID: `{:?}`\n", iteration.session_id));
    out.push_str(&format!("- Stop reason: `{:?}`\n\n", iteration.stop_reason));
    section(
        &mut out,
        "Current Agent Input",
        &iteration.current_agent_input_prompt,
    );
    section(
        &mut out,
        "Agent Output Summary",
        &iteration.agent_output_summary,
    );
    list_section(&mut out, "Actions Taken", &iteration.actions_taken);
    list_section(&mut out, "Tool Calls", &iteration.tool_calls_summary);
    list_section(&mut out, "Changed Files", &iteration.changed_files);
    list_section(
        &mut out,
        "Commands And Tests",
        &iteration.commands_tests_run,
    );
    list_section(&mut out, "Errors", &iteration.errors);
    section(&mut out, "Git Status", &iteration.current_git_status);
    section(&mut out, "Git Diff Summary", &iteration.git_diff_summary);
    if let Some(summary) = &iteration.refiner_input_summary {
        section(&mut out, "Refiner Input Summary", summary);
    }
    if let Some(raw) = &iteration.refiner_raw_response {
        section(&mut out, "Refiner Raw Response", raw);
    }
    if let Some(prompt) = &iteration.next_prompt_injected_into_agent {
        section(&mut out, "Next Prompt Injected", prompt);
    }
    out
}

fn section(out: &mut String, title: &str, body: &str) {
    out.push_str(&format!("## {title}\n\n"));
    out.push_str("```text\n");
    out.push_str(body);
    out.push_str("\n```\n\n");
}

fn list_section(out: &mut String, title: &str, values: &[String]) {
    out.push_str(&format!("## {title}\n\n"));
    if values.is_empty() {
        out.push_str("- None\n\n");
        return;
    }
    for value in values {
        out.push_str(&format!("- `{}`\n", value.replace('`', "'")));
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::yolo::types::YoloStopReason;

    #[tokio::test]
    async fn logger_redacts_secrets() {
        let temp = TempDir::new().unwrap();
        let logger = YoloLogger::new(temp.path(), "run").await.unwrap();
        let iteration = YoloIterationLog {
            session_id: Some("session".to_string()),
            iteration: 1,
            timestamp: now_timestamp(),
            original_user_prompt: "prompt".to_string(),
            current_agent_input_prompt: "prompt".to_string(),
            agent_output_summary: "Authorization: Bearer sk_test_123456789abcdef".to_string(),
            actions_taken: Vec::new(),
            tool_calls_summary: Vec::new(),
            changed_files: Vec::new(),
            git_diff_summary: String::new(),
            commands_tests_run: Vec::new(),
            errors: Vec::new(),
            current_git_status: String::new(),
            refiner_input_summary: None,
            refiner_raw_response: None,
            next_prompt_injected_into_agent: None,
            stop_reason: Some(YoloStopReason::RefinerStopSignal),
        };

        logger.write_iteration(&iteration).await.unwrap();

        let json = fs::read_to_string(logger.run_dir().join("iteration-001.json"))
            .await
            .unwrap();
        assert!(json.contains("[REDACTED]"));
        assert!(!json.contains("sk_test_123456789abcdef"));
    }
}
