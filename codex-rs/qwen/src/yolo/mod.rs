use std::future::Future;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use codex_arg0::Arg0DispatchPaths;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;

use crate::config::ResolvedQwenConfig;
use crate::redaction::redact_text;
use crate::runner::resolve_codex_executable;
use crate::yolo::agent::CodexAgentRunner;
use crate::yolo::agent::changed_files_from_git_status;
use crate::yolo::agent::git_diff_summary;
use crate::yolo::agent::git_status;
use crate::yolo::agent::summarize_agent_result;
use crate::yolo::agent::tail;
use crate::yolo::logging::YoloLogger;
use crate::yolo::logging::new_run_id;
use crate::yolo::logging::now_timestamp;
use crate::yolo::refiner::RefinerClient;
use crate::yolo::refiner::RefinerClientError;
use crate::yolo::refiner::YOLO_STOP;
use crate::yolo::types::AgentRoundRequest;
use crate::yolo::types::AgentRoundResult;
use crate::yolo::types::ExternalVerificationResult;
use crate::yolo::types::RefinerRequest;
use crate::yolo::types::RefinerResponse;
use crate::yolo::types::RefinerSkippedReason;
use crate::yolo::types::RoundBudget;
use crate::yolo::types::YoloIterationLog;
use crate::yolo::types::YoloLoopConfig;
use crate::yolo::types::YoloRunLog;
use crate::yolo::types::YoloRunOutcome;
use crate::yolo::types::YoloStopReason;
use crate::yolo::verification::failed_verification_summary;
use crate::yolo::verification::run_external_verification;
use crate::yolo::verification::verification_summary;

mod agent;
mod analysis;
mod flow_trace;
mod logging;
mod prompt_validation;
mod refiner;
mod types;
mod verification;

const REFINER_SUMMARY_MAX_CHARS: usize = 12_000;
const REFINER_AGENT_SUMMARY_MAX_CHARS: usize = 2_500;
const REFINER_LIST_ITEM_MAX_CHARS: usize = 700;
const REFINER_ERRORS_TOTAL_MAX_CHARS: usize = 2_100;

pub async fn run_yolo_mode(
    arg0_paths: Arg0DispatchPaths,
    config: ResolvedQwenConfig,
    prompt_parts: Vec<String>,
    dangerously_bypass_approvals_and_sandbox: bool,
) -> anyhow::Result<()> {
    let prompt = initial_prompt(prompt_parts).await?;
    let codex_exe = resolve_codex_executable(&arg0_paths)?;
    let agent = CodexAgentRunner::new(
        codex_exe,
        config.clone(),
        dangerously_bypass_approvals_and_sandbox,
    );
    let refiner = RefinerClient::new(&config)?;
    let interrupt_flag = Arc::new(AtomicBool::new(false));
    let interrupt_flag_for_signal = interrupt_flag.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            interrupt_flag_for_signal.store(true, Ordering::SeqCst);
        }
    });
    let loop_config = YoloLoopConfig {
        original_prompt: prompt,
        iteration_limit: config.yolo.default_iterations,
        log_root: config.yolo.log_dir.clone(),
        round_timeout_secs: config.yolo.round_timeout_secs,
        round_budget: RoundBudget {
            max_files: config.yolo.round_budget.max_files,
            max_actions: config.yolo.round_budget.max_actions,
            max_tests: config.yolo.round_budget.max_tests,
            max_next_prompt_chars: config.yolo.round_budget.max_next_prompt_chars,
            style: config.yolo.round_budget.style.clone(),
        },
        continue_after_timeout: config.yolo.continue_after_timeout,
        allow_refiner_stop: config.yolo.allow_refiner_stop,
        verify_commands: config.yolo.verify_commands.clone(),
        verify_timeout_secs: config.yolo.verify_timeout_secs,
        acceptance_gate_enabled: config.yolo.acceptance_gate_enabled,
        acceptance_commands: config.yolo.acceptance_commands.clone(),
        acceptance_max_seconds: config.yolo.acceptance_max_seconds,
        reject_stop_on_failed_acceptance: config.yolo.reject_stop_on_failed_acceptance,
        max_repeated_prompts: config.yolo.max_repeated_prompts,
        max_failures: config.yolo.max_failures,
        base_url: config.base_url.clone(),
        model: config.model.clone(),
        interrupt_flag: Some(interrupt_flag),
    };

    eprintln!(
        "Starting YOLO mode. Logs will be written under {}",
        loop_config.log_root.display()
    );

    let outcome = run_yolo_loop(
        loop_config,
        |request| agent.run_round(request),
        |request| refiner.refine(request),
    )
    .await?;

    eprintln!(
        "YOLO stopped after {} iteration(s): {:?}. Logs: {}",
        outcome.iterations_completed,
        outcome.stop_reason,
        outcome.run_dir.display()
    );
    Ok(())
}

pub(crate) async fn run_yolo_loop<A, AFut, R, RFut>(
    config: YoloLoopConfig,
    mut run_agent: A,
    mut refine: R,
) -> anyhow::Result<YoloRunOutcome>
where
    A: FnMut(AgentRoundRequest) -> AFut,
    AFut: Future<Output = anyhow::Result<AgentRoundResult>>,
    R: FnMut(RefinerRequest) -> RFut,
    RFut: Future<Output = anyhow::Result<RefinerResponse>>,
{
    if config.iteration_limit == Some(0) {
        anyhow::bail!(
            "--iterations must be greater than 0; omit --iterations for infinite YOLO mode"
        );
    }

    let run_id = new_run_id();
    let logger = YoloLogger::new(&config.log_root, &run_id).await?;
    let started_at = now_timestamp();
    let mut run_log = YoloRunLog {
        run_id: run_id.clone(),
        started_at,
        completed_at: None,
        original_prompt: config.original_prompt.clone(),
        base_url: config.base_url.clone(),
        model: config.model.clone(),
        iteration_limit: config.iteration_limit,
        round_timeout_secs: config.round_timeout_secs,
        round_budget: config.round_budget.clone(),
        continue_after_timeout: config.continue_after_timeout,
        allow_refiner_stop: config.allow_refiner_stop,
        verify_commands: config.verify_commands.clone(),
        verify_timeout_secs: config.verify_timeout_secs,
        acceptance_gate_enabled: config.acceptance_gate_enabled,
        acceptance_commands: config.acceptance_commands.clone(),
        acceptance_max_seconds: config.acceptance_max_seconds,
        reject_stop_on_failed_acceptance: config.reject_stop_on_failed_acceptance,
        max_repeated_prompts: config.max_repeated_prompts,
        max_failures: config.max_failures,
        session_id: None,
        stop_reason: None,
        iterations: Vec::new(),
    };
    write_run_and_analysis(&logger, &run_log).await?;

    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    let mut current_prompt = initial_round_prompt(&config.original_prompt, &config.round_budget);
    let mut session_id = None;
    let mut last_refiner_prompt = None::<String>;
    let mut repeated_prompt_count = 0_u32;
    let mut consecutive_failures = 0_u32;
    let mut iterations_completed = 0_u32;

    loop {
        if interrupted(&config) {
            finish_run(
                &logger,
                &mut run_log,
                iterations_completed,
                YoloStopReason::Interrupted,
            )
            .await?;
            break;
        }

        if config
            .iteration_limit
            .is_some_and(|limit| iterations_completed >= limit)
        {
            finish_run(
                &logger,
                &mut run_log,
                iterations_completed,
                YoloStopReason::IterationLimitReached,
            )
            .await?;
            break;
        }

        let iteration = iterations_completed + 1;
        let current_git_status_before = git_status(&cwd).await;
        let agent_request = AgentRoundRequest {
            iteration,
            prompt: current_prompt.clone(),
            thread_id: session_id.clone(),
        };
        let agent_started_at = Instant::now();
        let agent_result = match tokio::time::timeout(
            Duration::from_secs(config.round_timeout_secs),
            run_agent(agent_request),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Ok(agent_round_timeout_result(config.round_timeout_secs)),
        };
        let agent_round_duration_seconds = agent_started_at.elapsed().as_secs();
        let current_git_status = git_status(&cwd).await;
        let mut git_changed_files = changed_files_from_git_status(&current_git_status);
        if git_changed_files.is_empty() {
            git_changed_files = changed_files_from_git_status(&current_git_status_before);
        }
        let git_diff_summary = git_diff_summary(&cwd).await;

        let (agent_summary, mut agent_result, agent_failed) = match agent_result {
            Ok(result) => {
                let summary = summarize_agent_result(&result);
                (summary, result, false)
            }
            Err(err) => {
                let message = format!("agent round failed: {err}");
                let result = AgentRoundResult {
                    errors: vec![message.clone()],
                    ..AgentRoundResult::default()
                };
                (message, result, true)
            }
        };
        if agent_failed || !agent_result.errors.is_empty() {
            consecutive_failures = consecutive_failures.saturating_add(1);
        } else {
            consecutive_failures = 0;
        }
        let interrupt_received = interrupted(&config);
        let timeout_occurred = agent_result.timed_out;
        let agent_process_exit_code = agent_result.exit_code;
        let agent_process_signal = agent_result.exit_signal.clone();
        let agent_exited_with_error =
            agent_result.exit_code.is_some_and(|code| code != 0) || agent_process_signal.is_some();
        let agent_finished_normally = agent_result.exit_code == Some(0)
            && agent_process_signal.is_none()
            && !timeout_occurred
            && !agent_failed;

        if session_id.is_none() {
            session_id = agent_result.session_id.clone();
            run_log.session_id = session_id.clone();
        }
        for file in git_changed_files {
            if !agent_result.changed_files.contains(&file) {
                agent_result.changed_files.push(file);
            }
        }
        agent_result.changed_files.sort();
        agent_result.changed_files.dedup();

        let mut iteration_log = YoloIterationLog {
            run_id: run_id.clone(),
            session_id: session_id.clone(),
            iteration,
            timestamp: now_timestamp(),
            original_user_prompt: config.original_prompt.clone(),
            current_agent_input_prompt: current_prompt.clone(),
            agent_output_summary: agent_summary,
            actions_taken: agent_result.actions_taken.clone(),
            tool_calls_summary: agent_result.tool_calls.clone(),
            changed_files: agent_result.changed_files.clone(),
            git_diff_summary,
            commands_tests_run: agent_result.commands_tests_run.clone(),
            errors: agent_result.errors.clone(),
            external_verification: Vec::new(),
            acceptance_gate_enabled: config.acceptance_gate_enabled,
            acceptance_commands: config.acceptance_commands.clone(),
            acceptance_results: Vec::new(),
            stop_signal_received: false,
            stop_signal_accepted: false,
            stop_signal_rejected: false,
            stop_signal_rejection_reason: None,
            repair_prompt_after_failed_acceptance: None,
            actions_captured_count: agent_result.actions_taken.len(),
            tool_calls_captured_count: agent_result.tool_calls.len(),
            commands_captured_count: agent_result.commands_tests_run.len(),
            files_changed_count: agent_result.changed_files.len(),
            no_action_round: false,
            no_action_round_reason: None,
            unproductive_round: false,
            unproductive_round_reason: None,
            current_git_status,
            interrupt_received,
            timeout_occurred,
            agent_process_exit_code,
            agent_process_signal,
            agent_round_duration_seconds,
            agent_finished_normally,
            refiner_skipped_reason: None,
            refiner_input_summary: None,
            refiner_raw_response: None,
            refiner_error: None,
            next_prompt_injected_into_agent: None,
            next_prompt_validation_passed: None,
            next_prompt_validation_issues: Vec::new(),
            next_prompt_repaired: false,
            round_budget: config.round_budget.clone(),
            stop_reason: None,
        };
        apply_action_diagnostics(&mut iteration_log, &agent_result);
        if iteration_log.no_action_round {
            if let Some(reason) = &iteration_log.no_action_round_reason {
                iteration_log.errors.push(reason.clone());
            }
            consecutive_failures = consecutive_failures.saturating_add(1);
        }
        if iteration_log.unproductive_round && !iteration_log.no_action_round {
            if let Some(reason) = &iteration_log.unproductive_round_reason {
                iteration_log.errors.push(reason.clone());
            }
            consecutive_failures = consecutive_failures.saturating_add(1);
        }

        if agent_result.timed_out && !config.continue_after_timeout {
            iteration_log.stop_reason = Some(YoloStopReason::RoundTimeout);
            iteration_log.refiner_skipped_reason = Some(RefinerSkippedReason::Timeout);
            iterations_completed = iteration;
            write_iteration(&logger, &mut run_log, iteration_log).await?;
            finish_run(
                &logger,
                &mut run_log,
                iterations_completed,
                YoloStopReason::RoundTimeout,
            )
            .await?;
            break;
        }

        if interrupt_received {
            iteration_log.stop_reason = Some(YoloStopReason::Interrupted);
            iteration_log.refiner_skipped_reason = Some(RefinerSkippedReason::Interrupted);
            iterations_completed = iteration;
            write_iteration(&logger, &mut run_log, iteration_log).await?;
            finish_run(
                &logger,
                &mut run_log,
                iterations_completed,
                YoloStopReason::Interrupted,
            )
            .await?;
            break;
        }

        if agent_exited_with_error {
            iteration_log.stop_reason = Some(YoloStopReason::AgentError);
            iteration_log.refiner_skipped_reason = Some(RefinerSkippedReason::AgentError);
            iterations_completed = iteration;
            write_iteration(&logger, &mut run_log, iteration_log).await?;
            finish_run(
                &logger,
                &mut run_log,
                iterations_completed,
                YoloStopReason::AgentError,
            )
            .await?;
            break;
        }

        if consecutive_failures >= config.max_failures {
            iteration_log.stop_reason = Some(YoloStopReason::FailureGuard);
            iteration_log.refiner_skipped_reason = Some(RefinerSkippedReason::AgentError);
            iterations_completed = iteration;
            write_iteration(&logger, &mut run_log, iteration_log).await?;
            finish_run(
                &logger,
                &mut run_log,
                iterations_completed,
                YoloStopReason::FailureGuard,
            )
            .await?;
            break;
        }

        iteration_log.external_verification =
            run_external_verification(&config.verify_commands, config.verify_timeout_secs).await;
        if has_external_verification_failure(&iteration_log.external_verification) {
            iteration_log.errors.push(format!(
                "external verification failed: {}",
                failed_verification_summary(&iteration_log.external_verification)
            ));
        }

        if config
            .iteration_limit
            .is_some_and(|limit| iterations_completed + 1 >= limit)
        {
            run_final_acceptance_if_configured(&config, &mut iteration_log).await;
            iterations_completed = iteration;
            iteration_log.stop_reason = Some(YoloStopReason::IterationLimitReached);
            write_iteration(&logger, &mut run_log, iteration_log).await?;
            finish_run(
                &logger,
                &mut run_log,
                iterations_completed,
                YoloStopReason::IterationLimitReached,
            )
            .await?;
            break;
        }

        let refiner_summary = build_refiner_summary(&iteration_log, config.round_timeout_secs);
        iteration_log.refiner_input_summary = Some(refiner_summary.clone());
        let refiner_result = refine(RefinerRequest {
            iteration,
            summary: refiner_summary,
        })
        .await;

        let refiner_response = match refiner_result {
            Ok(response) => {
                iteration_log.refiner_raw_response = Some(response.raw_response.clone());
                response
            }
            Err(err) => {
                consecutive_failures = consecutive_failures.saturating_add(1);
                let reason = if consecutive_failures >= config.max_failures {
                    YoloStopReason::FailureGuard
                } else {
                    YoloStopReason::RefinerError
                };
                if let Some(refiner_error) = err.downcast_ref::<RefinerClientError>() {
                    iteration_log.refiner_error = Some(refiner_error.diagnostic());
                }
                iteration_log.errors.push(format!("refiner failed: {err}"));
                iteration_log.stop_reason = Some(reason.clone());
                iterations_completed = iteration;
                write_iteration(&logger, &mut run_log, iteration_log).await?;
                finish_run(&logger, &mut run_log, iterations_completed, reason).await?;
                break;
            }
        };

        let mut next_prompt = refiner_response.next_prompt.trim().to_string();
        iterations_completed = iteration;
        if interrupted(&config) {
            iteration_log.stop_reason = Some(YoloStopReason::Interrupted);
            write_iteration(&logger, &mut run_log, iteration_log).await?;
            finish_run(
                &logger,
                &mut run_log,
                iterations_completed,
                YoloStopReason::Interrupted,
            )
            .await?;
            break;
        }

        if is_yolo_stop_signal(&next_prompt) {
            iteration_log.stop_signal_received = true;
            if config.acceptance_gate_enabled {
                iteration_log.acceptance_results = run_external_verification(
                    &config.acceptance_commands,
                    config.acceptance_max_seconds,
                )
                .await;
                if acceptance_gate_passed(&config, &iteration_log.acceptance_results) {
                    iteration_log.stop_signal_accepted = true;
                    iteration_log.stop_reason = Some(YoloStopReason::RefinerStopSignal);
                    write_iteration(&logger, &mut run_log, iteration_log).await?;
                    finish_run(
                        &logger,
                        &mut run_log,
                        iterations_completed,
                        YoloStopReason::RefinerStopSignal,
                    )
                    .await?;
                    break;
                }

                let rejection_reason =
                    acceptance_rejection_reason(&config, &iteration_log.acceptance_results);
                iteration_log.stop_signal_rejected = true;
                iteration_log.stop_signal_rejection_reason = Some(rejection_reason.clone());
                iteration_log.errors.push(rejection_reason);
                if config.reject_stop_on_failed_acceptance {
                    let repair_summary = build_acceptance_repair_summary(&iteration_log);
                    let repair_response = refine(RefinerRequest {
                        iteration,
                        summary: repair_summary,
                    })
                    .await;
                    next_prompt = match repair_response {
                        Ok(response) => {
                            let repaired = response.next_prompt.trim().to_string();
                            iteration_log.repair_prompt_after_failed_acceptance =
                                Some(repaired.clone());
                            repaired
                        }
                        Err(err) => {
                            if let Some(refiner_error) = err.downcast_ref::<RefinerClientError>() {
                                iteration_log.refiner_error = Some(refiner_error.diagnostic());
                            }
                            iteration_log
                                .errors
                                .push(format!("refiner failed while repairing stop signal: {err}"));
                            let repaired = failed_acceptance_fallback_prompt(&iteration_log);
                            iteration_log.repair_prompt_after_failed_acceptance =
                                Some(repaired.clone());
                            repaired
                        }
                    };
                } else {
                    iteration_log.stop_signal_accepted = true;
                    iteration_log.stop_reason = Some(YoloStopReason::RefinerStopSignal);
                    write_iteration(&logger, &mut run_log, iteration_log).await?;
                    finish_run(
                        &logger,
                        &mut run_log,
                        iterations_completed,
                        YoloStopReason::RefinerStopSignal,
                    )
                    .await?;
                    break;
                }
            } else if config.allow_refiner_stop {
                iteration_log.stop_signal_accepted = true;
                iteration_log.stop_reason = Some(YoloStopReason::RefinerStopSignal);
                write_iteration(&logger, &mut run_log, iteration_log).await?;
                finish_run(
                    &logger,
                    &mut run_log,
                    iterations_completed,
                    YoloStopReason::RefinerStopSignal,
                )
                .await?;
                break;
            }
        }

        let validated_prompt = prompt_validation::validate_and_repair_next_prompt(
            &next_prompt,
            &current_prompt,
            &config.round_budget,
        );
        iteration_log.next_prompt_validation_passed = Some(validated_prompt.passed);
        iteration_log.next_prompt_validation_issues = validated_prompt.issues.clone();
        iteration_log.next_prompt_repaired = validated_prompt.repaired;
        if !validated_prompt.passed {
            iteration_log.errors.push(format!(
                "refiner produced an invalid next prompt: {}",
                validated_prompt.issues.join("; ")
            ));
            iteration_log.stop_reason = Some(YoloStopReason::RefinerError);
            iterations_completed = iteration;
            write_iteration(&logger, &mut run_log, iteration_log).await?;
            finish_run(
                &logger,
                &mut run_log,
                iterations_completed,
                YoloStopReason::RefinerError,
            )
            .await?;
            break;
        }
        let next_prompt = validated_prompt.prompt;

        let normalized_prompt = normalize_prompt_for_guard(&next_prompt);
        if last_refiner_prompt.as_deref() == Some(normalized_prompt.as_str()) {
            repeated_prompt_count = repeated_prompt_count.saturating_add(1);
        } else {
            last_refiner_prompt = Some(normalized_prompt);
            repeated_prompt_count = 1;
        }
        if config.iteration_limit.is_none() && repeated_prompt_count >= config.max_repeated_prompts
        {
            iteration_log.stop_reason = Some(YoloStopReason::RepeatedPromptGuard);
            write_iteration(&logger, &mut run_log, iteration_log).await?;
            finish_run(
                &logger,
                &mut run_log,
                iterations_completed,
                YoloStopReason::RepeatedPromptGuard,
            )
            .await?;
            break;
        }

        iteration_log.next_prompt_injected_into_agent = Some(next_prompt.clone());
        write_iteration(&logger, &mut run_log, iteration_log).await?;
        current_prompt = next_prompt;
    }

    Ok(YoloRunOutcome {
        run_id,
        run_dir: logger.run_dir().to_path_buf(),
        session_id,
        iterations_completed,
        stop_reason: run_log
            .stop_reason
            .clone()
            .unwrap_or(YoloStopReason::Interrupted),
    })
}

async fn write_iteration(
    logger: &YoloLogger,
    run_log: &mut YoloRunLog,
    iteration_log: YoloIterationLog,
) -> anyhow::Result<()> {
    logger.write_iteration(&iteration_log).await?;
    run_log.iterations.push(iteration_log);
    write_run_and_analysis(logger, run_log).await
}

async fn finish_run(
    logger: &YoloLogger,
    run_log: &mut YoloRunLog,
    _iterations_completed: u32,
    reason: YoloStopReason,
) -> anyhow::Result<()> {
    run_log.completed_at = Some(now_timestamp());
    run_log.stop_reason = Some(reason);
    write_run_and_analysis(logger, run_log).await
}

async fn write_run_and_analysis(logger: &YoloLogger, run_log: &YoloRunLog) -> anyhow::Result<()> {
    logger.write_run(run_log).await?;
    logger.write_analysis(run_log).await?;
    logger.write_flow_trace(run_log).await
}

fn build_refiner_summary(iteration: &YoloIterationLog, round_timeout_secs: u64) -> String {
    let timeout_utilization_percent =
        timeout_utilization_percent(iteration.agent_round_duration_seconds, round_timeout_secs);
    let round_duration_near_timeout =
        round_duration_near_timeout(iteration.agent_round_duration_seconds, round_timeout_secs);
    let summary = format!(
        r#"Original user prompt:
{original}

Current agent input:
{input}

Agent output summary:
{summary}

Actions taken:
{actions}

Tool calls:
{tools}

Changed files:
{files}

Commands/tests run:
{commands}

External verification:
{external_verification}

EXTERNAL VERIFICATION FAILED:
{external_verification_failures}

Errors:
{errors}

Action diagnostics:
- actions captured: {actions_count}
- tool calls captured: {tools_count}
- commands captured: {commands_count}
- files changed: {files_count}
- no-action round: {no_action_round}
- unproductive round: {unproductive_round}

Unproductive round warning:
{unproductive_warning}

Round timing:
- duration seconds: {duration_seconds}
- timeout seconds: {round_timeout_secs}
- timeout utilization percent: {timeout_utilization_percent}
- roundDurationNearTimeout: {round_duration_near_timeout}

Current git status:
{status}

Git diff summary:
{diff}

Round budget:
- style: {style}
- modify at most {max_files} files
- aim for at most {max_actions} concrete actions
- run at most {max_tests} verification commands
- next prompt must be at most {max_prompt_chars} characters

Next prompt contract:
- Use sections: Title, Context, Task, Constraints, Acceptance criteria, Verification commands, Stop condition.
- Choose one highest-impact next step that fits this budget.
- Prefer making the project runnable and fixing known build/runtime blockers before adding scope.
- Do not ask the agent to finish everything, implement all remaining features, or continue indefinitely.
- If any external verification failed, target that failure before adding unrelated scope.
- If roundDurationNearTimeout is true, make the next prompt smaller than the prior round and avoid combining large implementation work with heavy verification.
- For Docker projects, prefer docker compose config before build, run docker compose build only when necessary, use docker compose up -d instead of foreground up, wrap long commands with timeout where appropriate, and run docker compose down after runtime checks.
- If a Docker/web project has frontend HTTP 500, backend health failure, products endpoint failure, Docker build failure, Docker compose config failure, missing README/run instructions, missing required endpoints, or browser JavaScript fetching an unreachable Docker service hostname such as http://backend:8000, treat it as an incomplete runtime blocker.
- Browser JavaScript running on the host cannot fetch http://backend:8000. Use a host-reachable URL such as http://localhost:2226, a relative/proxy URL, or documented environment configuration.
- Treat unverified acceptance criteria and unrun verification commands as incomplete.
- Include exact verification commands and tell the agent to stop after the scoped task is verified.
- Only emit YOLO_STOP when all original goals, explicit acceptance criteria, verification commands, and any configured acceptance-gate checks are known to have passed and there are no known runtime failures or TODO blockers.
- If configured acceptance gate results are missing or failed, produce a focused next prompt instead of YOLO_STOP.

Your role is to inspect the latest round, identify the most impactful remaining improvement, and produce a focused prompt for the next round unless the acceptance evidence proves the project is complete."#,
        original = iteration.original_user_prompt,
        input = iteration.current_agent_input_prompt,
        summary = tail(
            &iteration.agent_output_summary,
            REFINER_AGENT_SUMMARY_MAX_CHARS
        ),
        actions =
            bullet_lines_limited(&iteration.actions_taken, REFINER_LIST_ITEM_MAX_CHARS, 1_200),
        tools = bullet_lines_limited(
            &iteration.tool_calls_summary,
            REFINER_LIST_ITEM_MAX_CHARS,
            1_000
        ),
        files = bullet_lines_limited(&iteration.changed_files, 300, 1_000),
        commands = bullet_lines_limited(
            &iteration.commands_tests_run,
            REFINER_LIST_ITEM_MAX_CHARS,
            1_200
        ),
        external_verification = verification_summary(&iteration.external_verification),
        external_verification_failures =
            failed_verification_summary(&iteration.external_verification),
        errors = bullet_lines_limited(
            &iteration.errors,
            REFINER_LIST_ITEM_MAX_CHARS,
            REFINER_ERRORS_TOTAL_MAX_CHARS
        ),
        actions_count = iteration.actions_captured_count,
        tools_count = iteration.tool_calls_captured_count,
        commands_count = iteration.commands_captured_count,
        files_count = iteration.files_changed_count,
        no_action_round = iteration.no_action_round,
        unproductive_round = iteration.unproductive_round,
        unproductive_warning = unproductive_round_warning(iteration),
        duration_seconds = iteration.agent_round_duration_seconds,
        round_timeout_secs = round_timeout_secs,
        timeout_utilization_percent = timeout_utilization_percent,
        round_duration_near_timeout = round_duration_near_timeout,
        status = tail(&iteration.current_git_status, 1_000),
        diff = tail(&iteration.git_diff_summary, 1_500),
        style = &iteration.round_budget.style,
        max_files = iteration.round_budget.max_files,
        max_actions = iteration.round_budget.max_actions,
        max_tests = iteration.round_budget.max_tests,
        max_prompt_chars = iteration.round_budget.max_next_prompt_chars,
    );
    redact_text(&limit_text(&summary, REFINER_SUMMARY_MAX_CHARS))
}

fn timeout_utilization_percent(duration_seconds: u64, round_timeout_secs: u64) -> u32 {
    if round_timeout_secs == 0 {
        return 0;
    }
    let percent = duration_seconds.saturating_mul(100) / round_timeout_secs;
    percent.min(u32::MAX as u64) as u32
}

fn round_duration_near_timeout(duration_seconds: u64, round_timeout_secs: u64) -> bool {
    round_timeout_secs > 0
        && duration_seconds.saturating_mul(100) >= round_timeout_secs.saturating_mul(80)
}

fn agent_round_timeout_result(round_timeout_secs: u64) -> AgentRoundResult {
    AgentRoundResult {
        timed_out: true,
        errors: vec![format!(
            "agent round timed out after {round_timeout_secs} second(s)"
        )],
        ..AgentRoundResult::default()
    }
}

fn interrupted(config: &YoloLoopConfig) -> bool {
    config
        .interrupt_flag
        .as_ref()
        .is_some_and(|flag| flag.load(Ordering::SeqCst))
}

fn bullet_lines_limited(
    values: &[String],
    max_chars_per_value: usize,
    max_total_chars: usize,
) -> String {
    if values.is_empty() {
        return "- none".to_string();
    }
    let joined = values
        .iter()
        .map(|value| format!("- {}", tail(value, max_chars_per_value)))
        .collect::<Vec<_>>()
        .join("\n");
    limit_text(&joined, max_total_chars)
}

fn limit_text(value: &str, max_chars: usize) -> String {
    let total = value.chars().count();
    if total <= max_chars {
        return value.to_string();
    }
    let marker = "\n[truncated to fit YOLO refiner context]\n";
    let marker_len = marker.chars().count();
    if max_chars <= marker_len {
        return tail(value, max_chars);
    }
    let remaining = max_chars - marker_len;
    let head_len = remaining * 2 / 3;
    let tail_len = remaining - head_len;
    let head = value.chars().take(head_len).collect::<String>();
    let tail = value
        .chars()
        .skip(total.saturating_sub(tail_len))
        .collect::<String>();
    format!("{head}{marker}{tail}")
}

fn normalize_prompt_for_guard(prompt: &str) -> String {
    prompt.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_yolo_stop_signal(prompt: &str) -> bool {
    prompt
        .trim_start()
        .to_ascii_uppercase()
        .starts_with(YOLO_STOP)
}

fn has_external_verification_failure(results: &[ExternalVerificationResult]) -> bool {
    results.iter().any(|result| result.exit_code != Some(0))
}

async fn run_final_acceptance_if_configured(
    config: &YoloLoopConfig,
    iteration_log: &mut YoloIterationLog,
) {
    if !config.acceptance_gate_enabled || config.acceptance_commands.is_empty() {
        return;
    }
    iteration_log.acceptance_results =
        run_external_verification(&config.acceptance_commands, config.acceptance_max_seconds).await;
    if !acceptance_gate_passed(config, &iteration_log.acceptance_results) {
        iteration_log.errors.push(format!(
            "final acceptance failed before max_iterations stop: {}",
            failed_verification_summary(&iteration_log.acceptance_results)
        ));
    }
}

fn acceptance_gate_passed(config: &YoloLoopConfig, results: &[ExternalVerificationResult]) -> bool {
    config.acceptance_gate_enabled
        && !config.acceptance_commands.is_empty()
        && !has_external_verification_failure(results)
        && results.len() == config.acceptance_commands.len()
}

fn acceptance_rejection_reason(
    config: &YoloLoopConfig,
    results: &[ExternalVerificationResult],
) -> String {
    if config.acceptance_commands.is_empty() {
        return "refiner stop signal rejected: acceptance gate enabled but no acceptance commands are configured"
            .to_string();
    }
    format!(
        "refiner stop signal rejected: acceptance checks failed or were incomplete: {}",
        failed_verification_summary(results)
    )
}

fn build_acceptance_repair_summary(iteration: &YoloIterationLog) -> String {
    let summary = format!(
        r#"The refiner emitted YOLO_STOP, but Qwen Codex rejected that stop signal because acceptance checks did not pass.

Original user prompt:
{original}

Latest agent input:
{input}

Latest agent output summary:
{summary}

Failed acceptance checks:
{acceptance}

Known errors:
{errors}

Changed files:
{files}

Commands/tests run:
{commands}

Task:
Generate one bounded repair prompt for the coding agent. The prompt must target the failed acceptance checks first and must not broaden scope.

Required next-prompt sections:
Title
Context
Task
Constraints
Acceptance criteria
Verification commands
Stop condition

Constraints for the next prompt:
- Modify at most {max_files} files.
- Use at most {max_actions} concrete actions.
- Run at most {max_tests} verification commands.
- Keep Docker/web checks focused and bounded.
- Do not emit YOLO_STOP."#,
        original = iteration.original_user_prompt,
        input = iteration.current_agent_input_prompt,
        summary = tail(
            &iteration.agent_output_summary,
            REFINER_AGENT_SUMMARY_MAX_CHARS
        ),
        acceptance = failed_verification_summary(&iteration.acceptance_results),
        errors = bullet_lines_limited(
            &iteration.errors,
            REFINER_LIST_ITEM_MAX_CHARS,
            REFINER_ERRORS_TOTAL_MAX_CHARS
        ),
        files = bullet_lines_limited(&iteration.changed_files, 300, 1_000),
        commands = bullet_lines_limited(
            &iteration.commands_tests_run,
            REFINER_LIST_ITEM_MAX_CHARS,
            1_200
        ),
        max_files = iteration.round_budget.max_files,
        max_actions = iteration.round_budget.max_actions,
        max_tests = iteration.round_budget.max_tests,
    );
    redact_text(&limit_text(&summary, REFINER_SUMMARY_MAX_CHARS))
}

fn failed_acceptance_fallback_prompt(iteration: &YoloIterationLog) -> String {
    let acceptance = failed_verification_summary(&iteration.acceptance_results);
    format!(
        r#"Title:
Fix failed acceptance checks.

Context:
The refiner attempted to stop, but acceptance checks failed or were incomplete:
{acceptance}

Task:
Fix the smallest concrete runtime or build blocker shown in the failed acceptance output. Do not add unrelated features.

Constraints:
- Modify at most {max_files} files.
- Use at most {max_actions} concrete actions.
- Run at most {max_tests} verification commands.
- Keep Docker ports and existing project shape unchanged.
- For browser JavaScript, use host-reachable API URLs such as http://localhost:2226, a relative/proxy URL, or documented environment configuration.

Acceptance criteria:
- The failed acceptance command is addressed.
- The changed area remains small and focused.
- Verification commands are run and summarized.

Verification commands:
{commands}

Stop condition:
Stop this agent round after the failed acceptance check is fixed or the remaining blocker is clearly summarized."#,
        max_files = iteration.round_budget.max_files,
        max_actions = iteration.round_budget.max_actions,
        max_tests = iteration.round_budget.max_tests,
        commands = acceptance_commands_for_prompt(iteration),
    )
}

fn acceptance_commands_for_prompt(iteration: &YoloIterationLog) -> String {
    if iteration.acceptance_commands.is_empty() {
        return "- git status --short\n- Run the smallest relevant build or runtime check"
            .to_string();
    }
    iteration
        .acceptance_commands
        .iter()
        .map(|command| format!("- {command}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn apply_action_diagnostics(iteration: &mut YoloIterationLog, agent_result: &AgentRoundResult) {
    iteration.actions_captured_count = iteration.actions_taken.len();
    iteration.tool_calls_captured_count = iteration.tool_calls_summary.len();
    iteration.commands_captured_count = iteration.commands_tests_run.len();
    iteration.files_changed_count = iteration.changed_files.len();

    let captured_any_work = iteration.actions_captured_count > 0
        || iteration.tool_calls_captured_count > 0
        || iteration.commands_captured_count > 0
        || iteration.files_changed_count > 0;
    let claimed_completion = agent_result
        .final_response
        .as_deref()
        .is_some_and(claims_completion);
    iteration.no_action_round =
        iteration.agent_finished_normally && claimed_completion && !captured_any_work;
    iteration.no_action_round_reason = iteration.no_action_round.then(|| {
        "agent completed the round without captured actions, commands, tool calls, or file changes"
            .to_string()
    });
    iteration.unproductive_round_reason = classify_unproductive_round(iteration);
    iteration.unproductive_round = iteration.unproductive_round_reason.is_some();
}

fn classify_unproductive_round(iteration: &YoloIterationLog) -> Option<String> {
    if !iteration.agent_finished_normally
        || verification_only_expected(&iteration.current_agent_input_prompt)
    {
        return None;
    }
    if !iteration.changed_files.is_empty() || !iteration.git_diff_summary.trim().is_empty() {
        return None;
    }
    let commands_empty_or_trivial = iteration.commands_tests_run.is_empty()
        || iteration
            .commands_tests_run
            .iter()
            .all(|command| is_trivial_inspection_command(command));
    let no_meaningful_actions = iteration.actions_captured_count == 0
        || iteration
            .actions_taken
            .iter()
            .all(|action| !meaningful_action(action));
    (commands_empty_or_trivial && no_meaningful_actions).then(|| {
        "agent finished normally but made no file changes, produced no git diff, and captured no meaningful commands or actions"
            .to_string()
    })
}

fn verification_only_expected(prompt: &str) -> bool {
    let lower = prompt.to_ascii_lowercase();
    let asks_for_verification = [
        "verify",
        "verification",
        "inspect",
        "review",
        "read",
        "check",
        "summarize",
    ]
    .into_iter()
    .any(|word| lower.contains(word));
    let asks_for_change = [
        "create",
        "edit",
        "modify",
        "implement",
        "build",
        "add ",
        "fix",
        "delete",
        "update",
        "write ",
    ]
    .into_iter()
    .any(|word| lower.contains(word));
    asks_for_verification && !asks_for_change
}

fn meaningful_action(action: &str) -> bool {
    if action.starts_with("file change:") {
        return true;
    }
    if let Some(command) = action.strip_prefix("command:") {
        return !is_trivial_inspection_command(command);
    }
    action.starts_with("web_search:")
        || action.starts_with("mcp:")
        || action.starts_with("collab:")
        || !action.starts_with("event:")
}

fn is_trivial_inspection_command(command: &str) -> bool {
    let command = command.trim();
    let lower = command.to_ascii_lowercase();
    let first = lower.split(['|', '&', ';']).next().unwrap_or("").trim();
    first == "pwd"
        || first == "ls"
        || first.starts_with("ls ")
        || first == "tree"
        || first.starts_with("tree ")
        || first == "git status"
        || first.starts_with("git status ")
        || first == "git diff --stat"
        || first == "git diff --name-status"
        || first == "rg --files"
        || first.starts_with("rg --files ")
        || first.starts_with("find ")
        || first.starts_with("cat ")
        || first.starts_with("sed ")
        || first.starts_with("head ")
        || first.starts_with("tail ")
        || first.starts_with("wc ")
        || first.starts_with("grep ")
        || first.starts_with("rg ")
}

fn unproductive_round_warning(iteration: &YoloIterationLog) -> String {
    if !iteration.unproductive_round {
        return "- none".to_string();
    }
    "WARNING: previous round was unproductive. It created no files and made no meaningful changes. The next prompt must request a concrete, small file/code change and verification command."
        .to_string()
}

fn claims_completion(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "completed",
        "complete",
        "done",
        "created",
        "implemented",
        "finished",
    ]
    .into_iter()
    .any(|word| lower.contains(word))
}

fn initial_round_prompt(original_prompt: &str, budget: &RoundBudget) -> String {
    format!(
        r#"{original_prompt}

YOLO round 1 execution constraints:
- Treat this as the first bounded round of a multi-round autonomous run, not the whole project.
- Prefer a minimal runnable baseline before optional features.
- Modify at most {max_files} files if practical.
- Aim for at most {max_actions} concrete implementation actions.
- Run at most {max_tests} verification commands.
- Do not broaden scope beyond the requested project.
- Stop after completing this bounded round and summarizing verification results."#,
        max_files = budget.max_files,
        max_actions = budget.max_actions,
        max_tests = budget.max_tests,
    )
}

async fn initial_prompt(prompt_parts: Vec<String>) -> anyhow::Result<String> {
    let prompt = prompt_parts.join(" ").trim().to_string();
    if !prompt.is_empty() {
        return Ok(prompt);
    }
    if atty_stdin() {
        print!("YOLO prompt> ");
        std::io::stdout()
            .flush()
            .context("failed to flush prompt")?;
    }
    let mut line = String::new();
    let mut reader = BufReader::new(tokio::io::stdin());
    reader
        .read_line(&mut line)
        .await
        .context("failed to read YOLO prompt from stdin")?;
    let prompt = line.trim().to_string();
    if prompt.is_empty() {
        anyhow::bail!("YOLO mode requires an initial prompt");
    }
    Ok(prompt)
}

fn atty_stdin() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::*;

    fn loop_config(temp: &TempDir, iteration_limit: Option<u32>) -> YoloLoopConfig {
        YoloLoopConfig {
            original_prompt: "Build a demo".to_string(),
            iteration_limit,
            log_root: temp.path().join("logs"),
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
            base_url: "http://127.0.0.1:8002/v1".to_string(),
            model: "qwen35-local".to_string(),
            interrupt_flag: None,
        }
    }

    fn agent_result(session_id: &str) -> AgentRoundResult {
        AgentRoundResult {
            session_id: Some(session_id.to_string()),
            exit_code: Some(0),
            final_response: Some("done".to_string()),
            actions_taken: vec!["shell: git status --short".to_string()],
            commands_tests_run: vec!["git status --short".to_string()],
            ..AgentRoundResult::default()
        }
    }

    fn bounded_prompt(title: &str) -> String {
        format!(
            r#"Title:
{title}

Context:
The previous round identified one scoped issue to fix.

Task:
Complete only this focused next step.

Constraints:
- Modify at most 2 files.
- Do not rewrite unrelated files.

Acceptance criteria:
- The focused issue is addressed.
- Verification is run and summarized.

Verification commands:
- git status --short

Stop condition:
Stop after the scoped task is complete and verification is summarized."#
        )
    }

    #[test]
    fn yolo_initial_round_prompt_adds_bounded_round_guidance() {
        let prompt = initial_round_prompt("Build an app", &RoundBudget::default());

        assert!(prompt.contains("Build an app"));
        assert!(prompt.contains("first bounded round"));
        assert!(prompt.contains("Modify at most 8 files"));
        assert!(prompt.contains("Stop after completing this bounded round"));
    }

    #[tokio::test]
    async fn yolo_timeout_stops_cleanly_without_refiner() {
        let temp = TempDir::new().unwrap();
        let mut config = loop_config(&temp, None);
        config.round_timeout_secs = 0;
        let refiner_calls = Rc::new(RefCell::new(0_u32));
        let refiner_calls_for_refiner = refiner_calls.clone();

        let outcome = run_yolo_loop(
            config,
            |_| async {
                tokio::time::sleep(Duration::from_secs(60)).await;
                Ok(agent_result("session-1"))
            },
            move |_| {
                let refiner_calls = refiner_calls_for_refiner.clone();
                async move {
                    *refiner_calls.borrow_mut() += 1;
                    Ok(RefinerResponse {
                        raw_response: "unused".to_string(),
                        next_prompt: "unused".to_string(),
                    })
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(outcome.iterations_completed, 1);
        assert_eq!(outcome.stop_reason, YoloStopReason::RoundTimeout);
        assert_eq!(*refiner_calls.borrow(), 0);

        let iteration_json =
            std::fs::read_to_string(outcome.run_dir.join("iteration-001.json")).unwrap();
        let iteration_log = serde_json::from_str::<YoloIterationLog>(&iteration_json).unwrap();
        assert_eq!(
            iteration_log.stop_reason,
            Some(YoloStopReason::RoundTimeout)
        );
        assert_eq!(
            iteration_log.refiner_skipped_reason,
            Some(RefinerSkippedReason::Timeout)
        );
        assert!(iteration_log.timeout_occurred);
        assert!(!iteration_log.agent_finished_normally);
        assert_eq!(
            iteration_log.errors,
            vec!["agent round timed out after 0 second(s)".to_string()]
        );

        let flow_trace_json =
            std::fs::read_to_string(outcome.run_dir.join("flow_trace.json")).unwrap();
        let flow_trace: serde_json::Value = serde_json::from_str(&flow_trace_json).unwrap();
        assert_eq!(flow_trace["finalStatus"], "timeout");
        assert_eq!(flow_trace["stopReason"], "round_timeout");
        assert_eq!(
            flow_trace["rounds"][0]["agentInputPrompt"],
            initial_round_prompt("Build a demo", &RoundBudget::default())
        );
        assert_eq!(flow_trace["rounds"][0]["refinerRequestSummary"], "");
        assert_eq!(flow_trace["rounds"][0]["refinerResponse"], "");
    }

    #[tokio::test]
    async fn yolo_loop_exit_zero_reaches_iteration_limit_without_refiner() {
        let temp = TempDir::new().unwrap();
        let refiner_calls = Rc::new(RefCell::new(0_u32));
        let refiner_calls_for_refiner = refiner_calls.clone();

        let outcome = run_yolo_loop(
            loop_config(&temp, Some(1)),
            |_| async { Ok(agent_result("session-1")) },
            move |_| {
                let refiner_calls = refiner_calls_for_refiner.clone();
                async move {
                    *refiner_calls.borrow_mut() += 1;
                    Ok(RefinerResponse {
                        raw_response: "next".to_string(),
                        next_prompt: "Keep going".to_string(),
                    })
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(outcome.stop_reason, YoloStopReason::IterationLimitReached);
        assert_eq!(*refiner_calls.borrow(), 0);
    }

    #[tokio::test]
    async fn yolo_loop_exit_one_is_agent_error_not_interrupted() {
        let temp = TempDir::new().unwrap();
        let refiner_calls = Rc::new(RefCell::new(0_u32));
        let refiner_calls_for_refiner = refiner_calls.clone();

        let outcome = run_yolo_loop(
            loop_config(&temp, None),
            |_| async {
                Ok(AgentRoundResult {
                    exit_code: Some(1),
                    errors: vec!["agent process exited with code 1".to_string()],
                    ..AgentRoundResult::default()
                })
            },
            move |_| {
                let refiner_calls = refiner_calls_for_refiner.clone();
                async move {
                    *refiner_calls.borrow_mut() += 1;
                    Ok(RefinerResponse {
                        raw_response: "unused".to_string(),
                        next_prompt: "unused".to_string(),
                    })
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(outcome.stop_reason, YoloStopReason::AgentError);
        assert_eq!(*refiner_calls.borrow(), 0);

        let iteration_json =
            std::fs::read_to_string(outcome.run_dir.join("iteration-001.json")).unwrap();
        let iteration_log = serde_json::from_str::<YoloIterationLog>(&iteration_json).unwrap();
        assert_eq!(iteration_log.stop_reason, Some(YoloStopReason::AgentError));
        assert_eq!(
            iteration_log.refiner_skipped_reason,
            Some(RefinerSkippedReason::AgentError)
        );
        assert_eq!(iteration_log.agent_process_exit_code, Some(1));
        assert!(!iteration_log.interrupt_received);
    }

    #[tokio::test]
    async fn yolo_loop_interrupt_after_round_is_classified_as_interrupted() {
        let temp = TempDir::new().unwrap();
        let interrupt = Arc::new(AtomicBool::new(false));
        let interrupt_for_agent = interrupt.clone();
        let mut config = loop_config(&temp, None);
        config.interrupt_flag = Some(interrupt);

        let outcome = run_yolo_loop(
            config,
            move |_| {
                let interrupt = interrupt_for_agent.clone();
                async move {
                    interrupt.store(true, Ordering::SeqCst);
                    Ok(agent_result("session-1"))
                }
            },
            |_| async {
                Ok(RefinerResponse {
                    raw_response: "unused".to_string(),
                    next_prompt: "unused".to_string(),
                })
            },
        )
        .await
        .unwrap();

        assert_eq!(outcome.stop_reason, YoloStopReason::Interrupted);
        let iteration_json =
            std::fs::read_to_string(outcome.run_dir.join("iteration-001.json")).unwrap();
        let iteration_log = serde_json::from_str::<YoloIterationLog>(&iteration_json).unwrap();
        assert_eq!(
            iteration_log.refiner_skipped_reason,
            Some(RefinerSkippedReason::Interrupted)
        );
        assert!(iteration_log.interrupt_received);
    }

    #[tokio::test]
    async fn yolo_loop_guard_iteration_limit_controls_agent_rounds() {
        let temp = TempDir::new().unwrap();
        let calls = Rc::new(RefCell::new(0_u32));
        let calls_for_agent = calls.clone();

        let outcome = run_yolo_loop(
            loop_config(&temp, Some(2)),
            move |_| {
                let calls = calls_for_agent.clone();
                async move {
                    *calls.borrow_mut() += 1;
                    Ok(agent_result("session-1"))
                }
            },
            |_| async {
                Ok(RefinerResponse {
                    raw_response: "next".to_string(),
                    next_prompt: "Keep going".to_string(),
                })
            },
        )
        .await
        .unwrap();

        assert_eq!(outcome.iterations_completed, 2);
        assert_eq!(outcome.stop_reason, YoloStopReason::IterationLimitReached);
        assert_eq!(*calls.borrow(), 2);
    }

    #[tokio::test]
    async fn yolo_loop_rejects_zero_iteration_limit() {
        let temp = TempDir::new().unwrap();
        let err = run_yolo_loop(
            loop_config(&temp, Some(0)),
            |_| async { Ok(agent_result("session-1")) },
            |_| async {
                Ok(RefinerResponse {
                    raw_response: "unused".to_string(),
                    next_prompt: "unused".to_string(),
                })
            },
        )
        .await
        .expect_err("zero iteration limit should be invalid");

        assert!(
            err.to_string()
                .contains("omit --iterations for infinite YOLO mode")
        );
    }

    #[tokio::test]
    async fn yolo_next_prompt_validation_repairs_before_injection() {
        let temp = TempDir::new().unwrap();

        let outcome = run_yolo_loop(
            loop_config(&temp, Some(2)),
            |_| async { Ok(agent_result("session-1")) },
            |_| async {
                Ok(RefinerResponse {
                    raw_response: "broad".to_string(),
                    next_prompt: "Please finish the entire project and do everything.".to_string(),
                })
            },
        )
        .await
        .unwrap();

        assert_eq!(outcome.stop_reason, YoloStopReason::IterationLimitReached);
        let iteration_json =
            std::fs::read_to_string(outcome.run_dir.join("iteration-001.json")).unwrap();
        let iteration_log = serde_json::from_str::<YoloIterationLog>(&iteration_json).unwrap();
        assert_eq!(iteration_log.next_prompt_validation_passed, Some(true));
        assert!(iteration_log.next_prompt_repaired);
        assert!(
            iteration_log
                .next_prompt_validation_issues
                .iter()
                .any(|issue| issue.contains("too broad"))
        );
        assert!(
            iteration_log
                .next_prompt_injected_into_agent
                .as_deref()
                .unwrap()
                .contains("Acceptance criteria:")
        );
    }

    #[tokio::test]
    async fn yolo_loop_can_disable_stop_signal_and_reach_max_iterations() {
        let temp = TempDir::new().unwrap();
        let mut config = loop_config(&temp, Some(2));
        config.allow_refiner_stop = false;
        let calls = Rc::new(RefCell::new(0_u32));
        let calls_for_agent = calls.clone();

        let outcome = run_yolo_loop(
            config,
            move |_| {
                let calls = calls_for_agent.clone();
                async move {
                    *calls.borrow_mut() += 1;
                    Ok(agent_result("session-1"))
                }
            },
            |_| async {
                Ok(RefinerResponse {
                    raw_response: YOLO_STOP.to_string(),
                    next_prompt: YOLO_STOP.to_string(),
                })
            },
        )
        .await
        .unwrap();

        assert_eq!(outcome.iterations_completed, 2);
        assert_eq!(outcome.stop_reason, YoloStopReason::IterationLimitReached);
        assert_eq!(*calls.borrow(), 2);

        let iteration_json =
            std::fs::read_to_string(outcome.run_dir.join("iteration-001.json")).unwrap();
        let iteration_log = serde_json::from_str::<YoloIterationLog>(&iteration_json).unwrap();
        assert_eq!(iteration_log.next_prompt_validation_passed, Some(true));
        assert!(iteration_log.next_prompt_repaired);
        assert!(
            iteration_log
                .next_prompt_validation_issues
                .iter()
                .any(|issue| issue.contains("YOLO_STOP"))
        );
    }

    #[tokio::test]
    async fn yolo_loop_accepts_stop_signal_before_max_iterations() {
        let temp = TempDir::new().unwrap();
        let calls = Rc::new(RefCell::new(0_u32));
        let calls_for_agent = calls.clone();

        let outcome = run_yolo_loop(
            loop_config(&temp, Some(5)),
            move |_| {
                let calls = calls_for_agent.clone();
                async move {
                    *calls.borrow_mut() += 1;
                    Ok(agent_result("session-1"))
                }
            },
            |_| async {
                Ok(RefinerResponse {
                    raw_response: "stop".to_string(),
                    next_prompt: "YOLO_STOP no further work remains".to_string(),
                })
            },
        )
        .await
        .unwrap();

        assert_eq!(outcome.iterations_completed, 1);
        assert_eq!(outcome.stop_reason, YoloStopReason::RefinerStopSignal);
        assert_eq!(*calls.borrow(), 1);
    }

    #[tokio::test]
    async fn yolo_loop_infinite_mode_stops_on_yolo_stop() {
        let temp = TempDir::new().unwrap();

        let outcome = run_yolo_loop(
            loop_config(&temp, None),
            |_| async { Ok(agent_result("session-1")) },
            |_| async {
                Ok(RefinerResponse {
                    raw_response: "stop".to_string(),
                    next_prompt: "YOLO_STOP no further work remains".to_string(),
                })
            },
        )
        .await
        .unwrap();

        assert_eq!(outcome.iterations_completed, 1);
        assert_eq!(outcome.stop_reason, YoloStopReason::RefinerStopSignal);

        let run_json = std::fs::read_to_string(outcome.run_dir.join("run.json")).unwrap();
        let run_log = serde_json::from_str::<YoloRunLog>(&run_json).unwrap();
        assert_eq!(run_log.iteration_limit, None);
        assert_eq!(run_log.stop_reason, Some(YoloStopReason::RefinerStopSignal));

        let analysis_json = std::fs::read_to_string(outcome.run_dir.join("analysis.json")).unwrap();
        let analysis: serde_json::Value = serde_json::from_str(&analysis_json).unwrap();
        assert_eq!(analysis["iterationLimit"], serde_json::Value::Null);
        assert_eq!(analysis["finalStatus"], "success");

        let iteration_json =
            std::fs::read_to_string(outcome.run_dir.join("iteration-001.json")).unwrap();
        let iteration_log = serde_json::from_str::<YoloIterationLog>(&iteration_json).unwrap();
        assert!(iteration_log.stop_signal_accepted);
    }

    #[tokio::test]
    async fn yolo_acceptance_rejects_stop_signal_and_injects_repair_prompt() {
        let temp = TempDir::new().unwrap();
        let mut config = loop_config(&temp, Some(2));
        config.acceptance_gate_enabled = true;
        config.acceptance_commands = vec!["echo ok".to_string(), "sh -c 'exit 7'".to_string()];
        config.acceptance_max_seconds = 5;
        let refiner_calls = Rc::new(RefCell::new(0_u32));
        let refiner_calls_for_refiner = refiner_calls.clone();
        let repair_prompt = bounded_prompt("Fix failed frontend acceptance check");

        let outcome = run_yolo_loop(
            config,
            |_| async { Ok(agent_result("session-1")) },
            move |_| {
                let refiner_calls = refiner_calls_for_refiner.clone();
                let repair_prompt = repair_prompt.clone();
                async move {
                    let mut calls = refiner_calls.borrow_mut();
                    *calls += 1;
                    if *calls == 1 {
                        Ok(RefinerResponse {
                            raw_response: YOLO_STOP.to_string(),
                            next_prompt: YOLO_STOP.to_string(),
                        })
                    } else {
                        Ok(RefinerResponse {
                            raw_response: repair_prompt.clone(),
                            next_prompt: repair_prompt,
                        })
                    }
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(outcome.iterations_completed, 2);
        assert_eq!(outcome.stop_reason, YoloStopReason::IterationLimitReached);
        assert_eq!(*refiner_calls.borrow(), 2);

        let first_json =
            std::fs::read_to_string(outcome.run_dir.join("iteration-001.json")).unwrap();
        let first = serde_json::from_str::<YoloIterationLog>(&first_json).unwrap();
        assert!(first.stop_signal_received);
        assert!(first.stop_signal_rejected);
        assert!(!first.stop_signal_accepted);
        assert_eq!(first.acceptance_results.len(), 2);
        assert_eq!(first.acceptance_results[1].exit_code, Some(7));
        assert!(first.repair_prompt_after_failed_acceptance.is_some());

        let second_json =
            std::fs::read_to_string(outcome.run_dir.join("iteration-002.json")).unwrap();
        let second = serde_json::from_str::<YoloIterationLog>(&second_json).unwrap();
        assert_eq!(
            second.current_agent_input_prompt,
            first.next_prompt_injected_into_agent.unwrap()
        );
    }

    #[tokio::test]
    async fn yolo_acceptance_accepts_stop_signal_when_checks_pass() {
        let temp = TempDir::new().unwrap();
        let mut config = loop_config(&temp, Some(5));
        config.acceptance_gate_enabled = true;
        config.acceptance_commands = vec!["echo ok".to_string()];
        config.acceptance_max_seconds = 5;

        let outcome = run_yolo_loop(
            config,
            |_| async { Ok(agent_result("session-1")) },
            |_| async {
                Ok(RefinerResponse {
                    raw_response: YOLO_STOP.to_string(),
                    next_prompt: YOLO_STOP.to_string(),
                })
            },
        )
        .await
        .unwrap();

        assert_eq!(outcome.iterations_completed, 1);
        assert_eq!(outcome.stop_reason, YoloStopReason::RefinerStopSignal);

        let first_json =
            std::fs::read_to_string(outcome.run_dir.join("iteration-001.json")).unwrap();
        let first = serde_json::from_str::<YoloIterationLog>(&first_json).unwrap();
        assert!(first.stop_signal_received);
        assert!(first.stop_signal_accepted);
        assert!(!first.stop_signal_rejected);
        assert_eq!(first.acceptance_results.len(), 1);
        assert_eq!(first.acceptance_results[0].exit_code, Some(0));
    }

    #[tokio::test]
    async fn yolo_acceptance_runs_before_fixed_iteration_success() {
        let temp = TempDir::new().unwrap();
        let mut config = loop_config(&temp, Some(1));
        config.acceptance_gate_enabled = true;
        config.acceptance_commands = vec!["sh -c 'exit 7'".to_string()];
        config.acceptance_max_seconds = 5;

        let outcome = run_yolo_loop(
            config,
            |_| async { Ok(agent_result("session-1")) },
            |_| async {
                Ok(RefinerResponse {
                    raw_response: "unused".to_string(),
                    next_prompt: "unused".to_string(),
                })
            },
        )
        .await
        .unwrap();

        assert_eq!(outcome.iterations_completed, 1);
        assert_eq!(outcome.stop_reason, YoloStopReason::IterationLimitReached);

        let first_json =
            std::fs::read_to_string(outcome.run_dir.join("iteration-001.json")).unwrap();
        let first = serde_json::from_str::<YoloIterationLog>(&first_json).unwrap();
        assert_eq!(first.acceptance_results.len(), 1);
        assert_eq!(first.acceptance_results[0].exit_code, Some(7));
        assert!(
            first
                .errors
                .iter()
                .any(|error| error.contains("final acceptance failed before max_iterations stop"))
        );

        let analysis_json = std::fs::read_to_string(outcome.run_dir.join("analysis.json")).unwrap();
        let analysis: serde_json::Value = serde_json::from_str(&analysis_json).unwrap();
        assert_eq!(analysis["finalStatus"], "partial");
    }

    #[tokio::test]
    async fn yolo_external_verification_is_recorded_and_passed_to_refiner() {
        let temp = TempDir::new().unwrap();
        let mut config = loop_config(&temp, Some(2));
        config.verify_commands = vec!["echo ok".to_string(), "sh -c 'exit 7'".to_string()];
        let summaries = Rc::new(RefCell::new(Vec::<String>::new()));
        let summaries_for_refiner = summaries.clone();

        let outcome = run_yolo_loop(
            config,
            |_| async { Ok(agent_result("session-1")) },
            move |request| {
                let summaries = summaries_for_refiner.clone();
                async move {
                    summaries.borrow_mut().push(request.summary);
                    Ok(RefinerResponse {
                        raw_response: "next".to_string(),
                        next_prompt: bounded_prompt("Fix the failing external verification"),
                    })
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(outcome.iterations_completed, 2);
        assert_eq!(outcome.stop_reason, YoloStopReason::IterationLimitReached);
        assert_eq!(summaries.borrow().len(), 1);
        assert!(summaries.borrow()[0].contains("EXTERNAL VERIFICATION FAILED:"));
        assert!(summaries.borrow()[0].contains("sh -c 'exit 7' -> exitCode=Some(7)"));

        let iteration_json =
            std::fs::read_to_string(outcome.run_dir.join("iteration-001.json")).unwrap();
        let iteration_log = serde_json::from_str::<YoloIterationLog>(&iteration_json).unwrap();
        assert_eq!(iteration_log.external_verification.len(), 2);
        assert_eq!(iteration_log.external_verification[0].exit_code, Some(0));
        assert_eq!(iteration_log.external_verification[1].exit_code, Some(7));
    }

    #[test]
    fn yolo_action_diagnostics_do_not_treat_actions_as_missing_tools() {
        let mut iteration = YoloIterationLog {
            run_id: "run".to_string(),
            session_id: Some("session".to_string()),
            iteration: 1,
            timestamp: now_timestamp(),
            original_user_prompt: "prompt".to_string(),
            current_agent_input_prompt: "prompt".to_string(),
            agent_output_summary: "summary".to_string(),
            actions_taken: vec!["shell: touch README.md".to_string()],
            tool_calls_summary: Vec::new(),
            changed_files: vec!["README.md".to_string()],
            git_diff_summary: String::new(),
            commands_tests_run: vec!["touch README.md".to_string()],
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
        };
        let result = AgentRoundResult {
            final_response: Some("Completed".to_string()),
            actions_taken: iteration.actions_taken.clone(),
            changed_files: iteration.changed_files.clone(),
            commands_tests_run: iteration.commands_tests_run.clone(),
            exit_code: Some(0),
            ..AgentRoundResult::default()
        };

        apply_action_diagnostics(&mut iteration, &result);

        assert_eq!(iteration.actions_captured_count, 1);
        assert_eq!(iteration.tool_calls_captured_count, 0);
        assert_eq!(iteration.commands_captured_count, 1);
        assert_eq!(iteration.files_changed_count, 1);
        assert!(!iteration.no_action_round);
        assert!(!iteration.unproductive_round);
    }

    #[test]
    fn yolo_action_diagnostics_mark_completed_empty_rounds() {
        let mut iteration = YoloIterationLog {
            run_id: "run".to_string(),
            session_id: Some("session".to_string()),
            iteration: 1,
            timestamp: now_timestamp(),
            original_user_prompt: "prompt".to_string(),
            current_agent_input_prompt: "prompt".to_string(),
            agent_output_summary: "summary".to_string(),
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
        };
        let result = AgentRoundResult {
            final_response: Some("Completed".to_string()),
            exit_code: Some(0),
            ..AgentRoundResult::default()
        };

        apply_action_diagnostics(&mut iteration, &result);

        assert!(iteration.no_action_round);
        assert!(iteration.no_action_round_reason.is_some());
        assert!(iteration.unproductive_round);
        assert!(iteration.unproductive_round_reason.is_some());
    }

    #[test]
    fn yolo_action_diagnostics_do_not_mark_expected_verification_round_unproductive() {
        let mut iteration = YoloIterationLog {
            run_id: "run".to_string(),
            session_id: Some("session".to_string()),
            iteration: 1,
            timestamp: now_timestamp(),
            original_user_prompt: "prompt".to_string(),
            current_agent_input_prompt: "Verify the project state and summarize the result."
                .to_string(),
            agent_output_summary: "summary".to_string(),
            actions_taken: vec!["command: ls".to_string()],
            tool_calls_summary: Vec::new(),
            changed_files: Vec::new(),
            git_diff_summary: String::new(),
            commands_tests_run: vec!["ls".to_string()],
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
        };
        let result = AgentRoundResult {
            final_response: Some("Verified".to_string()),
            actions_taken: iteration.actions_taken.clone(),
            commands_tests_run: iteration.commands_tests_run.clone(),
            exit_code: Some(0),
            ..AgentRoundResult::default()
        };

        apply_action_diagnostics(&mut iteration, &result);

        assert!(!iteration.unproductive_round);
        assert_eq!(iteration.unproductive_round_reason, None);
    }

    #[test]
    fn refiner_summary_warns_about_unproductive_round() {
        let mut iteration = YoloIterationLog {
            run_id: "run".to_string(),
            session_id: Some("session".to_string()),
            iteration: 1,
            timestamp: now_timestamp(),
            original_user_prompt: "prompt".to_string(),
            current_agent_input_prompt: "Build a project".to_string(),
            agent_output_summary: "summary".to_string(),
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
        };
        let result = AgentRoundResult {
            final_response: Some("Done".to_string()),
            exit_code: Some(0),
            ..AgentRoundResult::default()
        };
        apply_action_diagnostics(&mut iteration, &result);

        let summary = build_refiner_summary(&iteration, 600);

        assert!(iteration.unproductive_round);
        assert!(summary.contains("WARNING: previous round was unproductive"));
    }

    #[tokio::test]
    async fn yolo_loop_guard_repeated_prompt_stops_loop() {
        let temp = TempDir::new().unwrap();
        let mut config = loop_config(&temp, None);
        config.max_repeated_prompts = 2;

        let outcome = run_yolo_loop(
            config,
            |_| async { Ok(agent_result("session-1")) },
            |_| async {
                Ok(RefinerResponse {
                    raw_response: "repeat".to_string(),
                    next_prompt: "Repeat this prompt".to_string(),
                })
            },
        )
        .await
        .unwrap();

        assert_eq!(outcome.iterations_completed, 2);
        assert_eq!(outcome.stop_reason, YoloStopReason::RepeatedPromptGuard);
    }

    #[tokio::test]
    async fn yolo_loop_guard_failure_guard_stops_after_agent_failures() {
        let temp = TempDir::new().unwrap();
        let mut config = loop_config(&temp, None);
        config.max_failures = 1;

        let outcome = run_yolo_loop(
            config,
            |_| async { anyhow::bail!("agent failed") },
            |_| async {
                Ok(RefinerResponse {
                    raw_response: "unused".to_string(),
                    next_prompt: "unused".to_string(),
                })
            },
        )
        .await
        .unwrap();

        assert_eq!(outcome.iterations_completed, 1);
        assert_eq!(outcome.stop_reason, YoloStopReason::FailureGuard);
    }

    #[tokio::test]
    async fn yolo_loop_guard_interrupt_flag_stops_without_looping_forever() {
        let temp = TempDir::new().unwrap();
        let mut config = loop_config(&temp, None);
        config.interrupt_flag = Some(Arc::new(AtomicBool::new(true)));
        let calls = Rc::new(RefCell::new(0_u32));
        let calls_for_agent = calls.clone();

        let outcome = run_yolo_loop(
            config,
            move |_| {
                let calls = calls_for_agent.clone();
                async move {
                    *calls.borrow_mut() += 1;
                    Ok(agent_result("session-1"))
                }
            },
            |_| async {
                Ok(RefinerResponse {
                    raw_response: "unused".to_string(),
                    next_prompt: "unused".to_string(),
                })
            },
        )
        .await
        .unwrap();

        assert_eq!(outcome.iterations_completed, 0);
        assert_eq!(outcome.stop_reason, YoloStopReason::Interrupted);
        assert_eq!(*calls.borrow(), 0);
    }

    #[test]
    fn refiner_summary_redacts_secrets() {
        let iteration = YoloIterationLog {
            run_id: "run".to_string(),
            session_id: Some("session".to_string()),
            iteration: 1,
            timestamp: now_timestamp(),
            original_user_prompt: "prompt".to_string(),
            current_agent_input_prompt: "Authorization: Bearer sk_test_123456789abcdef".to_string(),
            agent_output_summary: "QWEN_CODEX_API_KEY=secret-value".to_string(),
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
        };

        let summary = build_refiner_summary(&iteration, 600);

        assert!(summary.contains("[REDACTED]"));
        assert!(!summary.contains("sk_test_123456789abcdef"));
        assert!(!summary.contains("secret-value"));
    }

    #[test]
    fn refiner_summary_is_bounded_for_large_error_payloads() {
        let iteration = YoloIterationLog {
            run_id: "run".to_string(),
            session_id: Some("session".to_string()),
            iteration: 1,
            timestamp: now_timestamp(),
            original_user_prompt: "prompt".to_string(),
            current_agent_input_prompt: "prompt".to_string(),
            agent_output_summary: "x".repeat(100_000),
            actions_taken: vec!["action".repeat(10_000)],
            tool_calls_summary: Vec::new(),
            changed_files: Vec::new(),
            git_diff_summary: "diff".repeat(10_000),
            commands_tests_run: Vec::new(),
            errors: vec!["error".repeat(100_000)],
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
            commands_captured_count: 0,
            files_changed_count: 0,
            no_action_round: false,
            no_action_round_reason: None,
            unproductive_round: false,
            unproductive_round_reason: None,
            current_git_status: "status".repeat(10_000),
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
        };

        let summary = build_refiner_summary(&iteration, 600);

        assert!(summary.chars().count() <= REFINER_SUMMARY_MAX_CHARS);
        assert!(summary.contains("[truncated"));
    }
}
