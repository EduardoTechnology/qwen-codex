use std::process::Stdio;
use std::time::Duration;
use std::time::Instant;

use tokio::process::Command;

use crate::redaction::redact_text;
use crate::yolo::agent::tail;
use crate::yolo::types::ExternalVerificationResult;

const OUTPUT_LIMIT: usize = 500;

pub(crate) async fn run_external_verification(
    commands: &[String],
    timeout_secs: u64,
) -> Vec<ExternalVerificationResult> {
    let mut results = Vec::with_capacity(commands.len());
    for command in commands {
        results.push(run_one(command, timeout_secs).await);
    }
    results
}

async fn run_one(command: &str, timeout_secs: u64) -> ExternalVerificationResult {
    let started_at = Instant::now();
    let output = match shell_command(command).spawn() {
        Ok(child) => {
            tokio::time::timeout(Duration::from_secs(timeout_secs), child.wait_with_output()).await
        }
        Err(error) => {
            let duration_ms = started_at.elapsed().as_millis();
            return failed_result(
                command,
                format!("failed to run external verification command: {error}"),
                duration_ms,
            );
        }
    };
    let duration_ms = started_at.elapsed().as_millis();
    match output {
        Ok(Ok(output)) => ExternalVerificationResult {
            command: command.to_string(),
            exit_code: output.status.code(),
            output: combined_output(&output.stdout, &output.stderr),
            duration_ms,
        },
        Ok(Err(error)) => failed_result(
            command,
            format!("failed to run external verification command: {error}"),
            duration_ms,
        ),
        Err(_) => failed_result(
            command,
            format!("external verification command timed out after {timeout_secs} second(s)"),
            duration_ms,
        ),
    }
}

fn shell_command(command: &str) -> Command {
    let mut shell = if cfg!(windows) {
        let mut command_process = Command::new("cmd");
        command_process.arg("/C").arg(command);
        command_process
    } else {
        let mut command_process = Command::new("sh");
        command_process.arg("-c").arg(command);
        command_process
    };
    shell
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    shell
}

fn failed_result(command: &str, output: String, duration_ms: u128) -> ExternalVerificationResult {
    ExternalVerificationResult {
        command: command.to_string(),
        exit_code: None,
        output: redact_text(&tail(&output, OUTPUT_LIMIT)),
        duration_ms,
    }
}

fn combined_output(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    let combined = match (stdout.trim().is_empty(), stderr.trim().is_empty()) {
        (true, true) => String::new(),
        (false, true) => stdout.to_string(),
        (true, false) => stderr.to_string(),
        (false, false) => format!("stdout:\n{stdout}\nstderr:\n{stderr}"),
    };
    redact_text(&tail(&combined, OUTPUT_LIMIT))
}

pub(crate) fn verification_summary(results: &[ExternalVerificationResult]) -> String {
    if results.is_empty() {
        return "- none".to_string();
    }
    results
        .iter()
        .map(|result| {
            format!(
                "- `{}` -> exitCode={:?}, durationMs={}, output={}",
                result.command,
                result.exit_code,
                result.duration_ms,
                tail(&result.output, OUTPUT_LIMIT)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn failed_verification_summary(results: &[ExternalVerificationResult]) -> String {
    let failures = results
        .iter()
        .filter(|result| result.exit_code != Some(0))
        .collect::<Vec<_>>();
    if failures.is_empty() {
        return "- none".to_string();
    }
    failures
        .iter()
        .map(|result| {
            format!(
                "{} -> exitCode={:?}\n{}",
                result.command,
                result.exit_code,
                tail(&result.output, OUTPUT_LIMIT)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn yolo_external_verification_records_success_and_failure() {
        let results =
            run_external_verification(&["echo ok".to_string(), "sh -c 'exit 7'".to_string()], 5)
                .await;

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].command, "echo ok");
        assert_eq!(results[0].exit_code, Some(0));
        assert_eq!(results[0].output.trim(), "ok");
        assert_eq!(results[1].command, "sh -c 'exit 7'");
        assert_eq!(results[1].exit_code, Some(7));
    }

    #[tokio::test]
    async fn yolo_external_verification_times_out() {
        let results = run_external_verification(&["sleep 2".to_string()], 1).await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].command, "sleep 2");
        assert_eq!(results[0].exit_code, None);
        assert!(results[0].output.contains("timed out after 1 second"));
    }

    #[test]
    fn yolo_external_verification_summary_highlights_failures() {
        let results = vec![
            ExternalVerificationResult {
                command: "echo ok".to_string(),
                exit_code: Some(0),
                output: "ok".to_string(),
                duration_ms: 1,
            },
            ExternalVerificationResult {
                command: "curl -sf http://localhost:2225".to_string(),
                exit_code: Some(7),
                output: "connection failed".to_string(),
                duration_ms: 2,
            },
        ];

        assert_eq!(
            failed_verification_summary(&results),
            "curl -sf http://localhost:2225 -> exitCode=Some(7)\nconnection failed"
        );
    }
}
