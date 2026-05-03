use serde::Serialize;

use crate::yolo::analysis::RoundChainingAnalysis;
use crate::yolo::analysis::UnavailableToolAttempt;
use crate::yolo::analysis::build_run_analysis;
use crate::yolo::types::ExternalVerificationResult;
use crate::yolo::types::YoloRunLog;
use crate::yolo::types::YoloStopReason;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct YoloFlowTrace {
    pub run_id: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub model: String,
    pub base_url: String,
    pub iteration_limit: Option<u32>,
    pub original_user_prompt: String,
    pub final_status: String,
    pub stop_reason: Option<YoloStopReason>,
    pub infrastructure_warnings: Vec<String>,
    pub unavailable_tool_attempts: Vec<UnavailableToolAttempt>,
    pub rounds: Vec<YoloFlowRound>,
    pub round_chaining: RoundChainingAnalysis,
    pub quality_signals: YoloFlowQualitySignals,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct YoloFlowRound {
    pub iteration: u32,
    pub agent_input_prompt: String,
    pub agent_output_summary: String,
    pub actions_taken: Vec<String>,
    pub commands_tests_run: Vec<String>,
    pub files_changed: Vec<String>,
    pub git_diff_summary: String,
    pub file_validation_summary: Vec<String>,
    pub errors: Vec<String>,
    pub agent_finished_normally: bool,
    pub refiner_request_summary: String,
    pub refiner_response: String,
    pub next_prompt: String,
    pub next_prompt_validation_passed: bool,
    pub next_prompt_injected_into_following_round: bool,
    pub unproductive_round: bool,
    pub unproductive_round_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct YoloFlowQualitySignals {
    pub unproductive_rounds: Vec<u32>,
    pub rounds_with_errors: Vec<u32>,
    pub rounds_near_timeout: Vec<u32>,
    pub acceptance_results: Vec<YoloFlowAcceptanceResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct YoloFlowAcceptanceResult {
    pub iteration: u32,
    pub results: Vec<ExternalVerificationResult>,
}

pub(crate) fn build_flow_trace(run: &YoloRunLog) -> YoloFlowTrace {
    let analysis = build_run_analysis(run);
    let rounds = run
        .iterations
        .iter()
        .enumerate()
        .map(|(index, iteration)| {
            let next_prompt_injected_into_following_round =
                run.iterations.get(index + 1).is_some_and(|next| {
                    iteration
                        .next_prompt_injected_into_agent
                        .as_ref()
                        .is_some_and(|prompt| next.current_agent_input_prompt == *prompt)
                });

            YoloFlowRound {
                iteration: iteration.iteration,
                agent_input_prompt: iteration.current_agent_input_prompt.clone(),
                agent_output_summary: iteration.agent_output_summary.clone(),
                actions_taken: iteration.actions_taken.clone(),
                commands_tests_run: iteration.commands_tests_run.clone(),
                files_changed: iteration.changed_files.clone(),
                git_diff_summary: iteration.git_diff_summary.clone(),
                file_validation_summary: iteration.file_validation_summary.clone(),
                errors: iteration.errors.clone(),
                agent_finished_normally: iteration.agent_finished_normally,
                refiner_request_summary: iteration
                    .refiner_input_summary
                    .clone()
                    .unwrap_or_default(),
                refiner_response: iteration.refiner_raw_response.clone().unwrap_or_default(),
                next_prompt: iteration
                    .next_prompt_injected_into_agent
                    .clone()
                    .unwrap_or_default(),
                next_prompt_validation_passed: iteration
                    .next_prompt_validation_passed
                    .unwrap_or(false),
                next_prompt_injected_into_following_round,
                unproductive_round: iteration.unproductive_round,
                unproductive_round_reason: iteration.unproductive_round_reason.clone(),
            }
        })
        .collect::<Vec<_>>();

    let quality_signals = YoloFlowQualitySignals {
        unproductive_rounds: run
            .iterations
            .iter()
            .filter(|iteration| iteration.unproductive_round)
            .map(|iteration| iteration.iteration)
            .collect(),
        rounds_with_errors: run
            .iterations
            .iter()
            .filter(|iteration| !iteration.errors.is_empty())
            .map(|iteration| iteration.iteration)
            .collect(),
        rounds_near_timeout: analysis
            .rounds
            .iter()
            .filter(|round| round.round_duration_near_timeout)
            .map(|round| round.iteration)
            .collect(),
        acceptance_results: run
            .iterations
            .iter()
            .filter(|iteration| !iteration.acceptance_results.is_empty())
            .map(|iteration| YoloFlowAcceptanceResult {
                iteration: iteration.iteration,
                results: iteration.acceptance_results.clone(),
            })
            .collect(),
    };

    YoloFlowTrace {
        run_id: run.run_id.clone(),
        started_at: run.started_at.clone(),
        completed_at: run.completed_at.clone(),
        model: run.model.clone(),
        base_url: run.base_url.clone(),
        iteration_limit: run.iteration_limit,
        original_user_prompt: run.original_prompt.clone(),
        final_status: analysis.final_status.clone(),
        stop_reason: run.stop_reason.clone(),
        infrastructure_warnings: analysis.infrastructure_warnings.clone(),
        unavailable_tool_attempts: analysis.unavailable_tool_attempts.clone(),
        rounds,
        round_chaining: analysis.round_chaining,
        quality_signals,
    }
}

pub(crate) fn flow_trace_markdown(trace: &YoloFlowTrace) -> String {
    let mut out = String::new();
    out.push_str(&format!("# YOLO Flow Trace {}\n\n", trace.run_id));
    out.push_str(&format!("- Started: `{}`\n", trace.started_at));
    if let Some(completed_at) = &trace.completed_at {
        out.push_str(&format!("- Completed: `{completed_at}`\n"));
    }
    out.push_str(&format!(
        "- Model: `{}`\n- Base URL: `{}`\n- Iteration limit: `{:?}`\n- Stop reason: `{:?}`\n- Final status: `{}`\n\n",
        trace.model,
        trace.base_url,
        trace.iteration_limit,
        trace.stop_reason,
        trace.final_status
    ));
    list(
        &mut out,
        "Infrastructure Warnings",
        &trace.infrastructure_warnings,
    );
    let unavailable_tool_attempts = trace
        .unavailable_tool_attempts
        .iter()
        .map(|attempt| {
            format!(
                "round {} `{}` blocking={} {}",
                attempt.round, attempt.tool_or_server, attempt.blocking, attempt.message
            )
        })
        .collect::<Vec<_>>();
    list(
        &mut out,
        "Unavailable Tool Attempts",
        &unavailable_tool_attempts,
    );
    out.push_str("## Original User Prompt\n\n```text\n");
    out.push_str(&trace.original_user_prompt);
    out.push_str("\n```\n\n");
    out.push_str("## Timeline\n\n");
    for round in &trace.rounds {
        out.push_str(&format!("### Round {}\n\n", round.iteration));
        out.push_str(&format!(
            "- Agent finished normally: `{}`\n- Files changed: `{}`\n- Commands/tests run: `{}`\n- Errors: `{}`\n- Unproductive: `{}`\n- Next prompt injected into following round: `{}`\n\n",
            round.agent_finished_normally,
            round.files_changed.len(),
            round.commands_tests_run.len(),
            round.errors.len(),
            round.unproductive_round,
            round.next_prompt_injected_into_following_round
        ));
        section(&mut out, "Agent Input Prompt", &round.agent_input_prompt);
        section(
            &mut out,
            "Agent Output Summary",
            &round.agent_output_summary,
        );
        list(&mut out, "Actions Taken", &round.actions_taken);
        list(
            &mut out,
            "Commands And Tests Run",
            &round.commands_tests_run,
        );
        list(&mut out, "Files Changed", &round.files_changed);
        list(
            &mut out,
            "File Validation Summary",
            &round.file_validation_summary,
        );
        if !round.refiner_request_summary.is_empty() {
            section(
                &mut out,
                "Refiner Request Summary",
                &round.refiner_request_summary,
            );
        }
        if !round.refiner_response.is_empty() {
            section(&mut out, "Refiner Response", &round.refiner_response);
        }
        if !round.next_prompt.is_empty() {
            section(&mut out, "Next Prompt", &round.next_prompt);
        }
    }
    out.push_str("## Round Chaining\n\n");
    out.push_str(&format!(
        "- Checked: `{}`\n- All round inputs match previous nextPrompt: `{}`\n",
        trace.round_chaining.checked,
        trace
            .round_chaining
            .all_round_inputs_match_previous_next_prompt
    ));
    if !trace.round_chaining.mismatches.is_empty() {
        list(
            &mut out,
            "Round Chaining Mismatches",
            &trace.round_chaining.mismatches,
        );
    }
    out
}

fn section(out: &mut String, title: &str, body: &str) {
    out.push_str(&format!("#### {title}\n\n```text\n"));
    out.push_str(body);
    out.push_str("\n```\n\n");
}

fn list(out: &mut String, title: &str, values: &[String]) {
    out.push_str(&format!("#### {title}\n\n"));
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
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::yolo::types::RoundBudget;
    use crate::yolo::types::YoloIterationLog;

    fn iteration(iteration: u32, input: &str, next_prompt: Option<&str>) -> YoloIterationLog {
        YoloIterationLog {
            run_id: "run".to_string(),
            session_id: Some("session".to_string()),
            iteration,
            timestamp: "2026-05-02T00:00:00Z".to_string(),
            original_user_prompt: "original".to_string(),
            current_agent_input_prompt: input.to_string(),
            agent_output_summary: "summary".to_string(),
            actions_taken: vec!["command: git status --short".to_string()],
            tool_calls_summary: Vec::new(),
            changed_files: Vec::new(),
            git_diff_summary: String::new(),
            file_validation_summary: Vec::new(),
            commands_tests_run: vec!["git status --short".to_string()],
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
            actions_captured_count: 1,
            tool_calls_captured_count: 0,
            commands_captured_count: 1,
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
            refiner_input_summary: Some("refiner summary".to_string()),
            refiner_raw_response: Some("refiner raw".to_string()),
            refiner_error: None,
            next_prompt_injected_into_agent: next_prompt.map(str::to_string),
            next_prompt_validation_passed: next_prompt.map(|_| true),
            next_prompt_validation_issues: Vec::new(),
            next_prompt_repaired: false,
            round_budget: RoundBudget::default(),
            stop_reason: None,
        }
    }

    fn run(iterations: Vec<YoloIterationLog>) -> YoloRunLog {
        YoloRunLog {
            run_id: "run".to_string(),
            started_at: "2026-05-02T00:00:00Z".to_string(),
            completed_at: Some("2026-05-02T00:01:00Z".to_string()),
            original_prompt: "Build a project".to_string(),
            base_url: "http://127.0.0.1:8002/v1".to_string(),
            model: "qwen35-local".to_string(),
            iteration_limit: Some(2),
            round_timeout_secs: 600,
            round_budget: RoundBudget::default(),
            continue_after_timeout: false,
            allow_refiner_stop: true,
            verify_commands: Vec::new(),
            verify_timeout_secs: 15,
            acceptance_gate_enabled: false,
            acceptance_commands: Vec::new(),
            acceptance_max_seconds: 300,
            reject_stop_on_failed_acceptance: true,
            max_repeated_prompts: 3,
            max_failures: 3,
            session_id: Some("session".to_string()),
            stop_reason: Some(YoloStopReason::IterationLimitReached),
            iterations,
        }
    }

    #[test]
    fn flow_trace_records_round_chaining_fields() {
        let trace = build_flow_trace(&run(vec![
            iteration(1, "first", Some("second")),
            iteration(2, "second", None),
        ]));

        assert_eq!(trace.original_user_prompt, "Build a project");
        assert_eq!(trace.rounds[0].agent_input_prompt, "first");
        assert_eq!(trace.rounds[0].next_prompt, "second");
        assert!(trace.rounds[0].next_prompt_injected_into_following_round);
        assert!(
            trace
                .round_chaining
                .all_round_inputs_match_previous_next_prompt
        );

        let json = serde_json::to_string_pretty(&trace).expect("flow trace serializes");
        serde_json::from_str::<serde_json::Value>(&json).expect("flow trace is valid JSON");
    }

    #[test]
    fn flow_trace_records_unavailable_tool_attempts() {
        let mut first = iteration(1, "first", None);
        first.errors = vec!["resources/read failed: unknown MCP server 'git'".to_string()];

        let trace = build_flow_trace(&run(vec![first]));

        assert_eq!(trace.final_status, "success_with_warnings");
        assert_eq!(trace.infrastructure_warnings.len(), 1);
        assert_eq!(
            trace.unavailable_tool_attempts,
            vec![UnavailableToolAttempt {
                round: 1,
                tool_or_server: "git".to_string(),
                message: "resources/read failed: unknown MCP server 'git'".to_string(),
                blocking: false,
            }]
        );

        let json = serde_json::to_string_pretty(&trace).expect("flow trace serializes");
        let value =
            serde_json::from_str::<serde_json::Value>(&json).expect("flow trace is valid JSON");
        assert!(value.get("infrastructureWarnings").is_some());
        assert!(value.get("unavailableToolAttempts").is_some());
    }
}
