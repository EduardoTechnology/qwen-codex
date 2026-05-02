use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use tokio::fs;

use crate::redaction::redact_text;
use crate::yolo::analysis::analysis_markdown;
use crate::yolo::analysis::build_run_analysis;
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
        write_json_redacted(&self.run_dir.join("run.json"), run).await?;
        write_text_redacted(&self.run_dir.join("run.md"), &run_markdown(run)).await
    }

    pub(crate) async fn write_analysis(&self, run: &YoloRunLog) -> anyhow::Result<()> {
        let analysis = build_run_analysis(run);
        write_json_redacted(&self.run_dir.join("analysis.json"), &analysis).await?;
        write_text_redacted(
            &self.run_dir.join("analysis.md"),
            &analysis_markdown(&analysis),
        )
        .await
    }

    pub(crate) async fn write_iteration(&self, iteration: &YoloIterationLog) -> anyhow::Result<()> {
        let stem = format!("iteration-{:03}", iteration.iteration);
        write_json_redacted(&self.run_dir.join(format!("{stem}.json")), iteration).await?;
        write_text_redacted(
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

async fn write_json_redacted<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    let mut value = serde_json::to_value(value).context("failed to serialize YOLO JSON value")?;
    redact_json_strings(&mut value);
    let contents =
        serde_json::to_string_pretty(&value).context("failed to serialize redacted YOLO JSON")?;
    fs::write(path, contents)
        .await
        .with_context(|| format!("failed to write {}", path.display()))
}

fn redact_json_strings(value: &mut Value) {
    match value {
        Value::String(text) => *text = redact_text(text),
        Value::Array(values) => {
            for value in values {
                redact_json_strings(value);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                redact_json_strings(value);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

async fn write_text_redacted(path: &Path, contents: &str) -> anyhow::Result<()> {
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
    out.push_str(&format!(
        "- Round budget: `{} files / {} actions / {} tests / {} chars / {}`\n",
        run.round_budget.max_files,
        run.round_budget.max_actions,
        run.round_budget.max_tests,
        run.round_budget.max_next_prompt_chars,
        run.round_budget.style
    ));
    out.push_str(&format!(
        "- Continue after timeout: `{}`\n",
        run.continue_after_timeout
    ));
    out.push_str(&format!(
        "- Allow refiner stop: `{}`\n",
        run.allow_refiner_stop
    ));
    out.push_str(&format!(
        "- External verification timeout: `{}s`\n",
        run.verify_timeout_secs
    ));
    out.push_str(&format!("- Stop reason: `{:?}`\n\n", run.stop_reason));
    list_section(
        &mut out,
        "External Verification Commands",
        &run.verify_commands,
    );
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
    out.push_str(&format!(
        "- Round budget: `{} files / {} actions / {} tests / {} chars / {}`\n",
        iteration.round_budget.max_files,
        iteration.round_budget.max_actions,
        iteration.round_budget.max_tests,
        iteration.round_budget.max_next_prompt_chars,
        iteration.round_budget.style
    ));
    out.push_str(&format!(
        "- Next prompt validation: `{:?}` repaired=`{}` issues=`{}`\n\n",
        iteration.next_prompt_validation_passed,
        iteration.next_prompt_repaired,
        iteration.next_prompt_validation_issues.len()
    ));
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
    external_verification_section(&mut out, &iteration.external_verification);
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
    list_section(
        &mut out,
        "Next Prompt Validation Issues",
        &iteration.next_prompt_validation_issues,
    );
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

fn external_verification_section(
    out: &mut String,
    values: &[crate::yolo::types::ExternalVerificationResult],
) {
    out.push_str("## External Verification\n\n");
    if values.is_empty() {
        out.push_str("- None\n\n");
        return;
    }
    for value in values {
        out.push_str(&format!(
            "- `{}` exitCode=`{:?}` durationMs=`{}` output=`{}`\n",
            value.command.replace('`', "'"),
            value.exit_code,
            value.duration_ms,
            value.output.replace('`', "'")
        ));
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::yolo::types::RoundBudget;
    use crate::yolo::types::YoloStopReason;

    #[tokio::test]
    async fn logger_redacts_secrets() {
        let temp = TempDir::new().unwrap();
        let logger = YoloLogger::new(temp.path(), "run").await.unwrap();
        let iteration = YoloIterationLog {
            run_id: "run".to_string(),
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
            external_verification: Vec::new(),
            current_git_status: String::new(),
            interrupt_received: false,
            timeout_occurred: false,
            agent_process_exit_code: Some(0),
            agent_process_signal: None,
            agent_round_duration_seconds: 1,
            agent_finished_normally: true,
            refiner_skipped_reason: None,
            refiner_input_summary: None,
            refiner_raw_response: None,
            refiner_error: None,
            next_prompt_injected_into_agent: None,
            next_prompt_validation_passed: None,
            next_prompt_validation_issues: Vec::new(),
            next_prompt_repaired: false,
            round_budget: RoundBudget::default(),
            stop_reason: Some(YoloStopReason::RefinerStopSignal),
        };

        logger.write_iteration(&iteration).await.unwrap();

        let json = fs::read_to_string(logger.run_dir().join("iteration-001.json"))
            .await
            .unwrap();
        serde_json::from_str::<Value>(&json).unwrap();
        assert!(json.contains("[REDACTED]"));
        assert!(!json.contains("sk_test_123456789abcdef"));
    }

    #[tokio::test]
    async fn logger_redaction_preserves_valid_json_for_interrupted_output() {
        let temp = TempDir::new().unwrap();
        let logger = YoloLogger::new(temp.path(), "run").await.unwrap();
        let iteration = YoloIterationLog {
            run_id: "run".to_string(),
            session_id: Some("session".to_string()),
            iteration: 1,
            timestamp: now_timestamp(),
            original_user_prompt: "prompt".to_string(),
            current_agent_input_prompt: "prompt".to_string(),
            agent_output_summary:
                "command: cat << 'EOF'\nMONGO_INITDB_ROOT_PASSWORD=secret123\n\u{0003}\u{001b}[31minterrupted\u{001b}[0m\nEOF"
                    .to_string(),
            actions_taken: vec![
                "command: cat << 'EOF'\nMONGO_INITDB_ROOT_PASSWORD=secret123\n\u{0003}\u{001b}[31minterrupted\u{001b}[0m\nEOF"
                    .to_string(),
            ],
            tool_calls_summary: Vec::new(),
            changed_files: Vec::new(),
            git_diff_summary: String::new(),
            commands_tests_run: Vec::new(),
            errors: Vec::new(),
            external_verification: Vec::new(),
            current_git_status: String::new(),
            interrupt_received: true,
            timeout_occurred: false,
            agent_process_exit_code: None,
            agent_process_signal: Some("SIGINT".to_string()),
            agent_round_duration_seconds: 1,
            agent_finished_normally: false,
            refiner_skipped_reason: None,
            refiner_input_summary: None,
            refiner_raw_response: None,
            refiner_error: None,
            next_prompt_injected_into_agent: None,
            next_prompt_validation_passed: None,
            next_prompt_validation_issues: Vec::new(),
            next_prompt_repaired: false,
            round_budget: RoundBudget::default(),
            stop_reason: Some(YoloStopReason::Interrupted),
        };

        logger.write_iteration(&iteration).await.unwrap();

        let json = fs::read_to_string(logger.run_dir().join("iteration-001.json"))
            .await
            .unwrap();
        serde_json::from_str::<Value>(&json).unwrap();
        assert!(json.contains("[REDACTED]"));
        assert!(!json.contains("secret123"));
    }
}
