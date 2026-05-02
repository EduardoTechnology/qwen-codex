use std::collections::BTreeSet;

use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;

use crate::yolo::agent::tail;
use crate::yolo::types::ExternalVerificationResult;
use crate::yolo::types::RefinerSkippedReason;
use crate::yolo::types::RoundBudget;
use crate::yolo::types::YoloRunLog;
use crate::yolo::types::YoloStopReason;

const PREVIEW_CHARS: usize = 600;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct YoloRunAnalysis {
    pub run_id: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub duration_seconds: u64,
    pub model: String,
    pub base_url: String,
    pub iteration_limit: Option<u32>,
    pub round_budget: RoundBudget,
    pub continue_after_timeout: bool,
    pub allow_refiner_stop: bool,
    pub verify_commands: Vec<String>,
    pub verify_timeout_secs: u64,
    pub acceptance_gate_enabled: bool,
    pub acceptance_commands: Vec<String>,
    pub acceptance_max_seconds: u64,
    pub reject_stop_on_failed_acceptance: bool,
    pub completed_iterations: usize,
    pub stop_reason: Option<YoloStopReason>,
    pub final_status: String,
    pub original_prompt_preview: String,
    pub rounds: Vec<YoloRoundAnalysis>,
    pub round_chaining: RoundChainingAnalysis,
    pub secret_redaction: SecretRedactionAnalysis,
    pub project_artifacts: ProjectArtifactsAnalysis,
    pub diagnostics: DiagnosticsAnalysis,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct YoloRoundAnalysis {
    pub iteration: u32,
    pub duration_seconds: u64,
    pub stop_reason: Option<YoloStopReason>,
    pub agent_input_preview: String,
    pub agent_output_summary_preview: String,
    pub files_changed: Vec<String>,
    pub commands_tests_run: Vec<String>,
    pub errors: Vec<String>,
    pub external_verification: Vec<ExternalVerificationResult>,
    pub acceptance_gate_enabled: bool,
    pub acceptance_results: Vec<ExternalVerificationResult>,
    pub stop_signal_received: bool,
    pub stop_signal_accepted: bool,
    pub stop_signal_rejected: bool,
    pub stop_signal_rejection_reason: Option<String>,
    pub repair_prompt_after_failed_acceptance: Option<String>,
    pub actions_captured_count: usize,
    pub tool_calls_captured_count: usize,
    pub commands_captured_count: usize,
    pub files_changed_count: usize,
    pub no_action_round: bool,
    pub no_action_round_reason: Option<String>,
    pub refiner_was_called: bool,
    pub refiner_response_preview: Option<String>,
    pub next_prompt_preview: Option<String>,
    pub next_prompt_injected_into_following_round: Option<bool>,
    pub agent_process_exit_code: Option<i32>,
    pub agent_finished_normally: bool,
    pub refiner_skipped_reason: Option<String>,
    pub next_prompt_validation_passed: Option<bool>,
    pub next_prompt_validation_issues: Vec<String>,
    pub next_prompt_repaired: bool,
    pub round_budget: RoundBudget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoundChainingAnalysis {
    pub checked: bool,
    pub all_round_inputs_match_previous_next_prompt: bool,
    pub mismatches: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SecretRedactionAnalysis {
    pub checked: bool,
    pub leaks_detected: bool,
    pub leak_patterns_checked: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectArtifactsAnalysis {
    pub files_created_count: usize,
    pub important_files_detected: Vec<String>,
    pub docker_compose_detected: bool,
    pub readme_detected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiagnosticsAnalysis {
    pub timeouts: Vec<String>,
    pub interruptions: Vec<String>,
    pub agent_errors: Vec<String>,
    pub provider_errors: Vec<String>,
    pub refiner_errors: Vec<String>,
    pub json_log_valid: bool,
}

pub(crate) fn build_run_analysis(run: &YoloRunLog) -> YoloRunAnalysis {
    let rounds = run
        .iterations
        .iter()
        .enumerate()
        .map(|(index, iteration)| {
            let next_prompt_injected_into_following_round =
                run.iterations.get(index + 1).and_then(|next| {
                    iteration
                        .next_prompt_injected_into_agent
                        .as_ref()
                        .map(|prompt| next.current_agent_input_prompt == *prompt)
                });
            YoloRoundAnalysis {
                iteration: iteration.iteration,
                duration_seconds: iteration.agent_round_duration_seconds,
                stop_reason: iteration.stop_reason.clone(),
                agent_input_preview: preview(&iteration.current_agent_input_prompt),
                agent_output_summary_preview: preview(&iteration.agent_output_summary),
                files_changed: iteration.changed_files.clone(),
                commands_tests_run: iteration.commands_tests_run.clone(),
                errors: iteration.errors.clone(),
                external_verification: iteration.external_verification.clone(),
                acceptance_gate_enabled: iteration.acceptance_gate_enabled,
                acceptance_results: iteration.acceptance_results.clone(),
                stop_signal_received: iteration.stop_signal_received,
                stop_signal_accepted: iteration.stop_signal_accepted,
                stop_signal_rejected: iteration.stop_signal_rejected,
                stop_signal_rejection_reason: iteration.stop_signal_rejection_reason.clone(),
                repair_prompt_after_failed_acceptance: iteration
                    .repair_prompt_after_failed_acceptance
                    .as_deref()
                    .map(preview),
                actions_captured_count: iteration.actions_captured_count,
                tool_calls_captured_count: iteration.tool_calls_captured_count,
                commands_captured_count: iteration.commands_captured_count,
                files_changed_count: iteration.files_changed_count,
                no_action_round: iteration.no_action_round,
                no_action_round_reason: iteration.no_action_round_reason.clone(),
                refiner_was_called: iteration.refiner_raw_response.is_some(),
                refiner_response_preview: iteration.refiner_raw_response.as_deref().map(preview),
                next_prompt_preview: iteration
                    .next_prompt_injected_into_agent
                    .as_deref()
                    .map(preview),
                next_prompt_injected_into_following_round,
                agent_process_exit_code: iteration.agent_process_exit_code,
                agent_finished_normally: iteration.agent_finished_normally,
                refiner_skipped_reason: iteration
                    .refiner_skipped_reason
                    .as_ref()
                    .map(refiner_skipped_reason_name),
                next_prompt_validation_passed: iteration.next_prompt_validation_passed,
                next_prompt_validation_issues: iteration.next_prompt_validation_issues.clone(),
                next_prompt_repaired: iteration.next_prompt_repaired,
                round_budget: iteration.round_budget.clone(),
            }
        })
        .collect::<Vec<_>>();

    let round_chaining = analyze_round_chaining(run);
    let project_artifacts = analyze_project_artifacts(run);
    let diagnostics = analyze_diagnostics(run);
    let secret_redaction = analyze_secret_redaction(run);

    YoloRunAnalysis {
        run_id: run.run_id.clone(),
        started_at: run.started_at.clone(),
        completed_at: run.completed_at.clone(),
        duration_seconds: duration_seconds(&run.started_at, run.completed_at.as_deref()),
        model: run.model.clone(),
        base_url: run.base_url.clone(),
        iteration_limit: run.iteration_limit,
        round_budget: run.round_budget.clone(),
        continue_after_timeout: run.continue_after_timeout,
        allow_refiner_stop: run.allow_refiner_stop,
        verify_commands: run.verify_commands.clone(),
        verify_timeout_secs: run.verify_timeout_secs,
        acceptance_gate_enabled: run.acceptance_gate_enabled,
        acceptance_commands: run.acceptance_commands.clone(),
        acceptance_max_seconds: run.acceptance_max_seconds,
        reject_stop_on_failed_acceptance: run.reject_stop_on_failed_acceptance,
        completed_iterations: run.iterations.len(),
        stop_reason: run.stop_reason.clone(),
        final_status: final_status(run).to_string(),
        original_prompt_preview: preview(&run.original_prompt),
        rounds,
        round_chaining,
        secret_redaction,
        project_artifacts,
        diagnostics,
    }
}

pub(crate) fn analysis_markdown(analysis: &YoloRunAnalysis) -> String {
    let mut out = String::new();
    out.push_str(&format!("# YOLO Analysis {}\n\n", analysis.run_id));
    out.push_str(&format!("- Started: `{}`\n", analysis.started_at));
    if let Some(completed_at) = &analysis.completed_at {
        out.push_str(&format!("- Completed: `{completed_at}`\n"));
    }
    out.push_str(&format!(
        "- Duration: `{}s`\n- Model: `{}`\n- Stop reason: `{:?}`\n- Final status: `{}`\n- Completed iterations: `{}`\n\n",
        analysis.duration_seconds,
        analysis.model,
        analysis.stop_reason,
        analysis.final_status,
        analysis.completed_iterations
    ));
    out.push_str("## Rounds\n\n");
    out.push_str(
        "| Round | Status | Refiner | Prompt validation | External checks | Files | Errors |\n",
    );
    out.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");
    for round in &analysis.rounds {
        out.push_str(&format!(
            "| {} | {:?} | {} | {} | {} | {} | {} |\n",
            round.iteration,
            round.stop_reason,
            if round.refiner_was_called {
                "yes"
            } else {
                "no"
            },
            validation_status(round),
            verification_status(round),
            round.files_changed.len(),
            round.errors.len()
        ));
    }
    out.push_str("\n## Round Budget\n\n");
    out.push_str(&format!(
        "- Style: `{}`\n- Max files per round: `{}`\n- Max actions per round: `{}`\n- Max verification commands per round: `{}`\n- Max next prompt chars: `{}`\n- Continue after timeout: `{}`\n- Allow refiner stop: `{}`\n",
        analysis.round_budget.style,
        analysis.round_budget.max_files,
        analysis.round_budget.max_actions,
        analysis.round_budget.max_tests,
        analysis.round_budget.max_next_prompt_chars,
        analysis.continue_after_timeout,
        analysis.allow_refiner_stop
    ));
    out.push_str(&format!(
        "- External verification timeout: `{}s`\n",
        analysis.verify_timeout_secs
    ));
    list(
        &mut out,
        "External verification commands",
        &analysis.verify_commands,
    );
    out.push_str("\n## Acceptance Gate\n\n");
    out.push_str(&format!(
        "- Enabled: `{}`\n- Max seconds per command: `{}s`\n- Reject stop on failed acceptance: `{}`\n",
        analysis.acceptance_gate_enabled,
        analysis.acceptance_max_seconds,
        analysis.reject_stop_on_failed_acceptance
    ));
    list(
        &mut out,
        "Acceptance commands",
        &analysis.acceptance_commands,
    );
    out.push_str("\n## Round Chaining\n\n");
    out.push_str(&format!(
        "- Checked: `{}`\n- All inputs matched previous nextPrompt: `{}`\n",
        analysis.round_chaining.checked,
        analysis
            .round_chaining
            .all_round_inputs_match_previous_next_prompt
    ));
    if !analysis.round_chaining.mismatches.is_empty() {
        out.push_str("- Mismatches:\n");
        for mismatch in &analysis.round_chaining.mismatches {
            out.push_str(&format!("  - {mismatch}\n"));
        }
    }
    out.push_str("\n## Diagnostics\n\n");
    list(&mut out, "Timeouts", &analysis.diagnostics.timeouts);
    list(
        &mut out,
        "Interruptions",
        &analysis.diagnostics.interruptions,
    );
    list(&mut out, "Agent errors", &analysis.diagnostics.agent_errors);
    list(
        &mut out,
        "Provider errors",
        &analysis.diagnostics.provider_errors,
    );
    list(
        &mut out,
        "Refiner errors",
        &analysis.diagnostics.refiner_errors,
    );
    out.push_str("\n## Action Diagnostics\n\n");
    out.push_str(
        "| Round | Actions | Tool calls | Commands | Files | No-action round | Stop signal |\n",
    );
    out.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");
    for round in &analysis.rounds {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            round.iteration,
            round.actions_captured_count,
            round.tool_calls_captured_count,
            round.commands_captured_count,
            round.files_changed_count,
            round.no_action_round,
            stop_signal_status(round)
        ));
    }
    out.push_str("\n## Final Recommendation\n\n");
    out.push_str(match analysis.final_status.as_str() {
        "success" => "PASS\n",
        "partial" => "PARTIAL\n",
        "timeout" => "NEEDS_RERUN_WITH_LONGER_TIMEOUT\n",
        "interrupted" => "PARTIAL\n",
        "failed" => "FAIL\n",
        _ => "PARTIAL\n",
    });
    out
}

fn analyze_round_chaining(run: &YoloRunLog) -> RoundChainingAnalysis {
    let mut mismatches = Vec::new();
    for index in 1..run.iterations.len() {
        let previous = &run.iterations[index - 1];
        let current = &run.iterations[index];
        match previous.next_prompt_injected_into_agent.as_ref() {
            Some(prompt) if current.current_agent_input_prompt == *prompt => {}
            Some(_) => mismatches.push(format!(
                "round {} input did not match round {} nextPrompt",
                current.iteration, previous.iteration
            )),
            None => mismatches.push(format!(
                "round {} had no nextPrompt for round {}",
                previous.iteration, current.iteration
            )),
        }
    }
    RoundChainingAnalysis {
        checked: run.iterations.len() > 1,
        all_round_inputs_match_previous_next_prompt: mismatches.is_empty(),
        mismatches,
    }
}

fn analyze_project_artifacts(run: &YoloRunLog) -> ProjectArtifactsAnalysis {
    let files = run
        .iterations
        .iter()
        .flat_map(|iteration| iteration.changed_files.iter().cloned())
        .collect::<BTreeSet<_>>();
    let important_files_detected = files
        .iter()
        .filter(|file| {
            matches!(
                file.as_str(),
                "docker-compose.yml"
                    | "README.md"
                    | "frontend/package.json"
                    | "backend/package.json"
            ) || file.starts_with("frontend/")
                || file.starts_with("backend/")
        })
        .cloned()
        .collect::<Vec<_>>();
    ProjectArtifactsAnalysis {
        files_created_count: files.len(),
        docker_compose_detected: files.contains("docker-compose.yml"),
        readme_detected: files.contains("README.md"),
        important_files_detected,
    }
}

fn analyze_diagnostics(run: &YoloRunLog) -> DiagnosticsAnalysis {
    let mut diagnostics = DiagnosticsAnalysis {
        timeouts: Vec::new(),
        interruptions: Vec::new(),
        agent_errors: Vec::new(),
        provider_errors: Vec::new(),
        refiner_errors: Vec::new(),
        json_log_valid: true,
    };
    for iteration in &run.iterations {
        if iteration.timeout_occurred {
            diagnostics
                .timeouts
                .push(format!("iteration {}", iteration.iteration));
        }
        if iteration.interrupt_received {
            diagnostics
                .interruptions
                .push(format!("iteration {}", iteration.iteration));
        }
        if !iteration.agent_finished_normally
            && !iteration.timeout_occurred
            && !iteration.interrupt_received
        {
            diagnostics.agent_errors.push(format!(
                "iteration {} exit={:?} signal={:?}",
                iteration.iteration,
                iteration.agent_process_exit_code,
                iteration.agent_process_signal
            ));
        }
        if iteration.refiner_error.is_some() {
            diagnostics
                .refiner_errors
                .push(format!("iteration {}", iteration.iteration));
        }
        for error in &iteration.errors {
            let lower = error.to_ascii_lowercase();
            if lower.contains("responses")
                || lower.contains("vllm")
                || lower.contains("provider")
                || lower.contains("validation")
            {
                diagnostics.provider_errors.push(format!(
                    "iteration {}: {}",
                    iteration.iteration,
                    preview(error)
                ));
            }
        }
    }
    diagnostics
}

fn analyze_secret_redaction(run: &YoloRunLog) -> SecretRedactionAnalysis {
    let serialized = serde_json::to_string(run)
        .unwrap_or_default()
        .to_ascii_lowercase();
    SecretRedactionAnalysis {
        checked: true,
        leaks_detected: serialized.contains("local-dev-key")
            || serialized.contains("authorization:"),
        leak_patterns_checked: vec!["api key".to_string(), "authorization header".to_string()],
    }
}

fn final_status(run: &YoloRunLog) -> &'static str {
    match run.stop_reason.as_ref() {
        Some(YoloStopReason::IterationLimitReached | YoloStopReason::RefinerStopSignal)
            if run
                .iterations
                .iter()
                .all(|iteration| iteration.errors.is_empty()) =>
        {
            "success"
        }
        Some(YoloStopReason::IterationLimitReached | YoloStopReason::RefinerStopSignal) => {
            "partial"
        }
        Some(YoloStopReason::RoundTimeout) => "timeout",
        Some(YoloStopReason::Interrupted) => "interrupted",
        Some(
            YoloStopReason::RepeatedPromptGuard
            | YoloStopReason::FailureGuard
            | YoloStopReason::RefinerError
            | YoloStopReason::AgentError,
        ) => "failed",
        None => "partial",
    }
}

fn refiner_skipped_reason_name(reason: &RefinerSkippedReason) -> String {
    match reason {
        RefinerSkippedReason::Interrupted => "interrupted",
        RefinerSkippedReason::Timeout => "timeout",
        RefinerSkippedReason::AgentError => "agent_error",
        RefinerSkippedReason::MissingAgentOutput => "missing_agent_output",
    }
    .to_string()
}

fn duration_seconds(started_at: &str, completed_at: Option<&str>) -> u64 {
    let Some(completed_at) = completed_at else {
        return 0;
    };
    let Ok(started_at) = DateTime::parse_from_rfc3339(started_at) else {
        return 0;
    };
    let Ok(completed_at) = DateTime::parse_from_rfc3339(completed_at) else {
        return 0;
    };
    completed_at
        .with_timezone(&Utc)
        .signed_duration_since(started_at.with_timezone(&Utc))
        .num_seconds()
        .max(0) as u64
}

fn preview(value: &str) -> String {
    tail(
        &value
            .chars()
            .filter_map(|ch| match ch {
                '\n' | '\r' | '\t' => Some(' '),
                ch if ch.is_control() => None,
                ch => Some(ch),
            })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        PREVIEW_CHARS,
    )
}

fn list(out: &mut String, title: &str, values: &[String]) {
    out.push_str(&format!("### {title}\n\n"));
    if values.is_empty() {
        out.push_str("- None\n\n");
        return;
    }
    for value in values {
        out.push_str(&format!("- {}\n", preview(value)));
    }
    out.push('\n');
}

fn validation_status(round: &YoloRoundAnalysis) -> String {
    match round.next_prompt_validation_passed {
        Some(true) if round.next_prompt_repaired => "repaired".to_string(),
        Some(true) => "passed".to_string(),
        Some(false) => "failed".to_string(),
        None => "not run".to_string(),
    }
}

fn verification_status(round: &YoloRoundAnalysis) -> String {
    if round.external_verification.is_empty() {
        return "not configured".to_string();
    }
    let failures = round
        .external_verification
        .iter()
        .filter(|result| result.exit_code != Some(0))
        .count();
    if failures == 0 {
        format!("{} passed", round.external_verification.len())
    } else {
        format!("{failures}/{} failed", round.external_verification.len())
    }
}

fn stop_signal_status(round: &YoloRoundAnalysis) -> String {
    if !round.stop_signal_received {
        return "none".to_string();
    }
    if round.stop_signal_accepted {
        return "accepted".to_string();
    }
    if round.stop_signal_rejected {
        return format!(
            "rejected: {}",
            round
                .stop_signal_rejection_reason
                .as_deref()
                .unwrap_or("unknown reason")
        );
    }
    "received".to_string()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::yolo::types::RefinerSkippedReason;
    use crate::yolo::types::YoloIterationLog;

    fn iteration(iteration: u32, prompt: &str) -> YoloIterationLog {
        YoloIterationLog {
            run_id: "run".to_string(),
            session_id: Some("session".to_string()),
            iteration,
            timestamp: "2026-05-01T00:00:00Z".to_string(),
            original_user_prompt: "original".to_string(),
            current_agent_input_prompt: prompt.to_string(),
            agent_output_summary: "summary\n\u{0003}\u{1b}[31m".to_string(),
            actions_taken: Vec::new(),
            tool_calls_summary: Vec::new(),
            changed_files: Vec::new(),
            git_diff_summary: String::new(),
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
            current_git_status: String::new(),
            interrupt_received: false,
            timeout_occurred: false,
            agent_process_exit_code: Some(0),
            agent_process_signal: None,
            agent_round_duration_seconds: 2,
            agent_finished_normally: true,
            refiner_skipped_reason: None,
            refiner_input_summary: Some("summary".to_string()),
            refiner_raw_response: Some("next".to_string()),
            refiner_error: None,
            next_prompt_injected_into_agent: Some(format!("next {iteration}")),
            next_prompt_validation_passed: Some(true),
            next_prompt_validation_issues: Vec::new(),
            next_prompt_repaired: false,
            round_budget: RoundBudget::default(),
            stop_reason: None,
        }
    }

    fn run(iterations: Vec<YoloIterationLog>, stop_reason: YoloStopReason) -> YoloRunLog {
        YoloRunLog {
            run_id: "run".to_string(),
            started_at: "2026-05-01T00:00:00Z".to_string(),
            completed_at: Some("2026-05-01T00:01:00Z".to_string()),
            original_prompt: "original prompt".to_string(),
            base_url: "http://127.0.0.1:8002/v1".to_string(),
            model: "qwen35-local".to_string(),
            iteration_limit: Some(2),
            round_budget: RoundBudget::default(),
            allow_refiner_stop: false,
            verify_commands: Vec::new(),
            verify_timeout_secs: 15,
            acceptance_gate_enabled: false,
            acceptance_commands: Vec::new(),
            acceptance_max_seconds: 300,
            reject_stop_on_failed_acceptance: true,
            continue_after_timeout: false,
            max_repeated_prompts: 3,
            max_failures: 3,
            session_id: Some("session".to_string()),
            stop_reason: Some(stop_reason),
            iterations,
        }
    }

    #[test]
    fn analysis_reports_round_chaining_for_normal_run() {
        let mut first = iteration(1, "first");
        first.next_prompt_injected_into_agent = Some("second".to_string());
        let second = iteration(2, "second");

        let analysis = build_run_analysis(&run(
            vec![first, second],
            YoloStopReason::IterationLimitReached,
        ));

        assert_eq!(analysis.completed_iterations, 2);
        assert_eq!(
            analysis.round_chaining,
            RoundChainingAnalysis {
                checked: true,
                all_round_inputs_match_previous_next_prompt: true,
                mismatches: Vec::new(),
            }
        );
        assert_eq!(analysis.duration_seconds, 60);
        assert_eq!(analysis.final_status, "success");
        assert_eq!(analysis.round_budget, RoundBudget::default());
        assert!(!analysis.continue_after_timeout);
        assert_eq!(analysis.rounds[0].next_prompt_validation_passed, Some(true));
    }

    #[test]
    fn analysis_records_timeout_and_agent_error_status() {
        let mut timed_out = iteration(1, "first");
        timed_out.timeout_occurred = true;
        timed_out.agent_finished_normally = false;
        timed_out.refiner_skipped_reason = Some(RefinerSkippedReason::Timeout);
        timed_out.stop_reason = Some(YoloStopReason::RoundTimeout);
        timed_out.errors = vec!["timeout\n\u{0003}".to_string()];

        let analysis = build_run_analysis(&run(vec![timed_out], YoloStopReason::RoundTimeout));

        assert_eq!(analysis.final_status, "timeout");
        assert_eq!(analysis.diagnostics.timeouts, vec!["iteration 1"]);
        assert_eq!(
            analysis.rounds[0].refiner_skipped_reason,
            Some("timeout".to_string())
        );
    }

    #[test]
    fn analysis_markdown_is_readable_for_agent_error() {
        let mut failed = iteration(1, "first");
        failed.agent_process_exit_code = Some(1);
        failed.agent_finished_normally = false;
        failed.refiner_skipped_reason = Some(RefinerSkippedReason::AgentError);
        failed.stop_reason = Some(YoloStopReason::AgentError);

        let markdown = analysis_markdown(&build_run_analysis(&run(
            vec![failed],
            YoloStopReason::AgentError,
        )));

        assert!(markdown.contains("FAIL"));
        assert!(markdown.contains("Agent errors"));
    }
}
