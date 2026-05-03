use std::collections::BTreeSet;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;
use tokio::fs;
use tokio::process::Command;

use crate::yolo::agent::tail;
use crate::yolo::types::ExternalVerificationResult;
use crate::yolo::types::YoloIterationLog;

const CHECK_OUTPUT_LIMIT: usize = 500;
const CHECK_TIMEOUT_SECS: u64 = 15;

pub(crate) async fn collect_file_validation_summary(
    cwd: &Path,
    iteration: &YoloIterationLog,
) -> Vec<String> {
    let mut summary = validation_command_feedback(iteration);
    if iteration.changed_files.is_empty() && iteration.git_diff_summary.trim().is_empty() {
        return summary;
    }

    let package_json_paths = package_json_candidates(cwd, &iteration.changed_files);
    for path in &package_json_paths {
        summary.push(validate_package_json(cwd, path).await);
    }

    let check_compose = should_check_compose(cwd, &iteration.changed_files);
    if check_compose {
        summary.push(validate_docker_compose(cwd).await);
    }

    let check_backend_js = should_check_backend_server(cwd, &iteration.changed_files);
    if check_backend_js {
        summary.push(validate_backend_server(cwd).await);
    }

    if should_check_frontend_urls(cwd, &iteration.changed_files) {
        summary.push(frontend_backend_url_feedback(cwd));
    }

    summary.extend(missing_agent_validation_feedback(
        iteration,
        &package_json_paths,
        check_compose,
        check_backend_js,
    ));
    summary
}

pub(crate) fn acceptance_feedback(iteration: &YoloIterationLog) -> String {
    if !iteration.acceptance_gate_enabled {
        return "- acceptance gate disabled".to_string();
    }
    if iteration.acceptance_commands.is_empty() {
        return "- acceptance gate enabled but no commands configured".to_string();
    }
    if iteration.acceptance_results.is_empty() {
        return "- acceptance commands not run yet".to_string();
    }
    if iteration.acceptance_results.len() != iteration.acceptance_commands.len() {
        return format!(
            "- acceptance incomplete: {}/{} command(s) returned results",
            iteration.acceptance_results.len(),
            iteration.acceptance_commands.len()
        );
    }
    let failures = iteration
        .acceptance_results
        .iter()
        .filter(|result| result.exit_code != Some(0))
        .collect::<Vec<_>>();
    if failures.is_empty() {
        return format!(
            "- acceptance passed: {} command(s)",
            iteration.acceptance_results.len()
        );
    }
    let failed = failures
        .iter()
        .map(|result| format!("`{}` exitCode={:?}", result.command, result.exit_code))
        .collect::<Vec<_>>()
        .join("; ");
    format!("- acceptance failed: {failed}")
}

pub(crate) fn is_blocking_feedback(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("invalid")
        || lower.contains("syntax check failed")
        || lower.contains("module mode check failed")
        || lower.contains("docker compose config failed")
        || lower.contains("validation command failed")
        || lower.contains("found http://backend:")
}

fn package_json_candidates(cwd: &Path, changed_files: &[String]) -> Vec<String> {
    let mut paths = BTreeSet::new();
    for changed_file in changed_files {
        let changed = changed_file.trim_end_matches('/');
        if changed == "package.json" || changed.ends_with("/package.json") {
            paths.insert(changed.to_string());
        }
        if changed == "backend" || changed.starts_with("backend/") {
            paths.insert("backend/package.json".to_string());
        }
        if changed == "frontend" || changed.starts_with("frontend/") {
            paths.insert("frontend/package.json".to_string());
        }
    }

    paths
        .into_iter()
        .filter(|path| {
            workspace_path(cwd, path)
                .map(|path| path.exists())
                .unwrap_or(false)
                || changed_files
                    .iter()
                    .any(|changed| changed.trim_end_matches('/') == path)
        })
        .collect()
}

async fn validate_package_json(cwd: &Path, relative_path: &str) -> String {
    let Some(path) = workspace_path(cwd, relative_path) else {
        return format!("package.json {relative_path}: skipped unsafe path");
    };
    match fs::read_to_string(&path).await {
        Ok(contents) => match serde_json::from_str::<Value>(&contents) {
            Ok(_) => format!("package.json {relative_path}: valid"),
            Err(err) => format!("package.json {relative_path}: invalid JSON: {err}"),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            format!("package.json {relative_path}: missing after related file change")
        }
        Err(err) => format!("package.json {relative_path}: unreadable: {err}"),
    }
}

fn should_check_compose(cwd: &Path, changed_files: &[String]) -> bool {
    workspace_path(cwd, "docker-compose.yml").is_some_and(|path| path.is_file())
        && changed_files.iter().any(|path| {
            let path = path.trim_end_matches('/');
            path == "docker-compose.yml"
                || path == "backend"
                || path == "frontend"
                || path.starts_with("backend/")
                || path.starts_with("frontend/")
        })
}

async fn validate_docker_compose(cwd: &Path) -> String {
    match run_command(cwd, "docker", &["compose", "config"]).await {
        CommandCheck::Passed { output } => {
            let warning = if output.trim().is_empty() {
                String::new()
            } else {
                format!(": {}", tail(output.trim(), CHECK_OUTPUT_LIMIT))
            };
            format!("docker-compose.yml: valid (docker compose config passed){warning}")
        }
        CommandCheck::Failed { exit_code, output } => format!(
            "docker-compose.yml: docker compose config failed exitCode={exit_code:?}: {}",
            tail(output.trim(), CHECK_OUTPUT_LIMIT)
        ),
        CommandCheck::Unavailable { message } => {
            format!("docker-compose.yml: not checked: {message}")
        }
    }
}

fn should_check_backend_server(cwd: &Path, changed_files: &[String]) -> bool {
    workspace_path(cwd, "backend/server.js").is_some_and(|path| path.is_file())
        && changed_files.iter().any(|path| {
            let path = path.trim_end_matches('/');
            path == "backend" || path == "backend/server.js" || path.starts_with("backend/")
        })
}

async fn validate_backend_server(cwd: &Path) -> String {
    let syntax = match run_command(cwd, "node", &["--check", "backend/server.js"]).await {
        CommandCheck::Passed { .. } => "backend/server.js: JS syntax check passed".to_string(),
        CommandCheck::Failed { exit_code, output } => format!(
            "backend/server.js: JS syntax check failed exitCode={exit_code:?}: {}",
            tail(output.trim(), CHECK_OUTPUT_LIMIT)
        ),
        CommandCheck::Unavailable { message } => {
            return format!("backend/server.js: not checked: {message}");
        }
    };

    if let Some(module_error) = backend_module_mode_error(cwd).await {
        format!("{syntax}\n{module_error}")
    } else {
        syntax
    }
}

async fn backend_module_mode_error(cwd: &Path) -> Option<String> {
    let server = fs::read_to_string(workspace_path(cwd, "backend/server.js")?)
        .await
        .ok()?;
    if !server
        .lines()
        .any(|line| line.trim_start().starts_with("import "))
    {
        return None;
    }
    let package = fs::read_to_string(workspace_path(cwd, "backend/package.json")?)
        .await
        .ok()?;
    let package_json = serde_json::from_str::<Value>(&package).ok()?;
    let is_module = package_json
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|value| value == "module");
    (!is_module).then(|| {
        "backend/server.js: JS module mode check failed: uses ESM import while backend/package.json lacks \"type\": \"module\""
            .to_string()
    })
}

fn should_check_frontend_urls(cwd: &Path, changed_files: &[String]) -> bool {
    workspace_path(cwd, "frontend").is_some_and(|path| path.is_dir())
        && changed_files.iter().any(|path| {
            let path = path.trim_end_matches('/');
            path == "frontend" || path.starts_with("frontend/")
        })
}

fn frontend_backend_url_feedback(cwd: &Path) -> String {
    let Some(frontend_dir) = workspace_path(cwd, "frontend") else {
        return "frontend URL check: skipped unsafe frontend path".to_string();
    };
    let matches = find_text_matches(&frontend_dir, "http://backend:");
    if matches.is_empty() {
        "frontend URL check: no http://backend: references found".to_string()
    } else {
        format!(
            "frontend URL check: found http://backend: in {}",
            matches.join(", ")
        )
    }
}

fn find_text_matches(root: &Path, needle: &str) -> Vec<String> {
    let mut matches = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        if metadata.is_dir() {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if matches!(name, "node_modules" | ".git" | ".qwen-codex") {
                continue;
            }
            if let Ok(entries) = std::fs::read_dir(&path) {
                for entry in entries.flatten() {
                    stack.push(entry.path());
                }
            }
        } else if metadata.is_file()
            && is_frontend_text_file(&path)
            && std::fs::read_to_string(&path).is_ok_and(|contents| contents.contains(needle))
        {
            matches.push(path.display().to_string());
        }
    }
    matches.sort();
    matches
}

fn is_frontend_text_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension,
                "css" | "html" | "js" | "jsx" | "mjs" | "ts" | "tsx"
            )
        })
}

fn missing_agent_validation_feedback(
    iteration: &YoloIterationLog,
    package_json_paths: &[String],
    check_compose: bool,
    check_backend_js: bool,
) -> Vec<String> {
    let mut missing = Vec::new();
    let commands = iteration.commands_tests_run.join("\n").to_ascii_lowercase();
    if !package_json_paths.is_empty() && !commands.contains("json.tool") {
        missing.push(format!(
            "validation missing: run `python3 -m json.tool {}` after changing JSON",
            package_json_paths.join("` and `python3 -m json.tool ")
        ));
    }
    if check_backend_js && !commands.contains("node --check backend/server.js") {
        missing.push(
            "validation missing: run `node --check backend/server.js` after changing backend JS"
                .to_string(),
        );
    }
    if check_compose && !commands.contains("docker compose config") {
        missing.push(
            "validation missing: run `docker compose config` after changing Docker Compose/YAML"
                .to_string(),
        );
    }
    missing
}

fn validation_command_feedback(iteration: &YoloIterationLog) -> Vec<String> {
    iteration
        .external_verification
        .iter()
        .chain(iteration.acceptance_results.iter())
        .filter(|result| is_validation_command(&result.command))
        .map(validation_command_line)
        .collect()
}

fn validation_command_line(result: &ExternalVerificationResult) -> String {
    if result.exit_code == Some(0) {
        return format!("validation command passed: `{}`", result.command);
    }
    format!(
        "validation command failed: `{}` exitCode={:?}: {}",
        result.command,
        result.exit_code,
        tail(result.output.trim(), CHECK_OUTPUT_LIMIT)
    )
}

fn is_validation_command(command: &str) -> bool {
    let command = command.to_ascii_lowercase();
    command.contains("python3 -m json.tool")
        || command.contains("node --check")
        || command.contains("docker compose config")
}

enum CommandCheck {
    Passed {
        output: String,
    },
    Failed {
        exit_code: Option<i32>,
        output: String,
    },
    Unavailable {
        message: String,
    },
}

async fn run_command(cwd: &Path, program: &str, args: &[&str]) -> CommandCheck {
    let child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn();
    let Ok(child) = child else {
        return CommandCheck::Unavailable {
            message: format!("{program} unavailable"),
        };
    };
    let output = match tokio::time::timeout(
        Duration::from_secs(CHECK_TIMEOUT_SECS),
        child.wait_with_output(),
    )
    .await
    {
        Ok(Ok(output)) => output,
        Ok(Err(err)) => {
            return CommandCheck::Unavailable {
                message: format!("{program} failed to run: {err}"),
            };
        }
        Err(_) => {
            return CommandCheck::Failed {
                exit_code: None,
                output: format!("timed out after {CHECK_TIMEOUT_SECS} second(s)"),
            };
        }
    };
    let output_text = combined_output(&output.stdout, &output.stderr);
    if output.status.success() {
        CommandCheck::Passed {
            output: output_text,
        }
    } else {
        CommandCheck::Failed {
            exit_code: output.status.code(),
            output: output_text,
        }
    }
}

fn combined_output(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    match (stdout.trim().is_empty(), stderr.trim().is_empty()) {
        (true, true) => String::new(),
        (false, true) => tail(&stdout, CHECK_OUTPUT_LIMIT),
        (true, false) => tail(&stderr, CHECK_OUTPUT_LIMIT),
        (false, false) => tail(
            &format!("stdout:\n{stdout}\nstderr:\n{stderr}"),
            CHECK_OUTPUT_LIMIT,
        ),
    }
}

fn workspace_path(cwd: &Path, relative_path: &str) -> Option<PathBuf> {
    let path = Path::new(relative_path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return None;
    }
    Some(cwd.join(path))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::*;
    use crate::yolo::types::RoundBudget;

    fn iteration(changed_files: Vec<String>) -> YoloIterationLog {
        YoloIterationLog {
            run_id: "run".to_string(),
            session_id: Some("session".to_string()),
            iteration: 1,
            timestamp: "2026-05-02T00:00:00Z".to_string(),
            original_user_prompt: "original".to_string(),
            current_agent_input_prompt: "prompt".to_string(),
            agent_output_summary: "summary".to_string(),
            actions_taken: Vec::new(),
            tool_calls_summary: Vec::new(),
            changed_files,
            git_diff_summary: String::new(),
            file_validation_summary: Vec::new(),
            commands_tests_run: Vec::new(),
            errors: Vec::new(),
            external_verification: Vec::new(),
            acceptance_gate_enabled: false,
            acceptance_commands: Vec::new(),
            acceptance_results: Vec::new(),
            stop_signal_received: false,
            stop_signal_accepted: false,
            stop_signal_rejected: false,
            stop_signal_rejection_reason: None,
            repair_prompt_after_failed_acceptance: None,
            actions_captured_count: 0,
            tool_calls_captured_count: 0,
            commands_captured_count: 0,
            files_changed_count: 0,
            no_action_round: false,
            no_action_round_reason: None,
            unproductive_round: false,
            unproductive_round_reason: None,
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
            stop_reason: None,
        }
    }

    #[tokio::test]
    async fn file_validation_detects_corrupted_package_json() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir(temp.path().join("backend")).unwrap();
        std::fs::write(
            temp.path().join("backend/package.json"),
            r#"{"dependencies":{"express":""'^4.18.2"}}"#,
        )
        .unwrap();
        let mut iteration = iteration(vec!["backend/package.json".to_string()]);
        iteration.git_diff_summary = "M backend/package.json".to_string();

        let summary = collect_file_validation_summary(temp.path(), &iteration).await;

        assert!(
            summary
                .iter()
                .any(|line| line.contains("backend/package.json: invalid JSON"))
        );
    }

    #[tokio::test]
    async fn file_validation_reports_failed_validation_commands() {
        let temp = TempDir::new().unwrap();
        let mut iteration = iteration(Vec::new());
        iteration.external_verification = vec![
            ExternalVerificationResult {
                command: "python3 -m json.tool backend/package.json".to_string(),
                exit_code: Some(1),
                output: "Expecting ',' delimiter".to_string(),
                duration_ms: 1,
            },
            ExternalVerificationResult {
                command: "node --check backend/server.js".to_string(),
                exit_code: Some(1),
                output: "SyntaxError: Invalid or unexpected token".to_string(),
                duration_ms: 1,
            },
        ];

        let summary = collect_file_validation_summary(temp.path(), &iteration).await;

        assert_eq!(
            summary,
            vec![
                "validation command failed: `python3 -m json.tool backend/package.json` exitCode=Some(1): Expecting ',' delimiter".to_string(),
                "validation command failed: `node --check backend/server.js` exitCode=Some(1): SyntaxError: Invalid or unexpected token".to_string(),
            ]
        );
    }
}
