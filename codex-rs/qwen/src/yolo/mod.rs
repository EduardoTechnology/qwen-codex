use std::collections::HashMap;
use std::future::Future;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

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
use crate::yolo::refiner::YOLO_STOP;
use crate::yolo::types::AgentRoundRequest;
use crate::yolo::types::AgentRoundResult;
use crate::yolo::types::RefinerRequest;
use crate::yolo::types::RefinerResponse;
use crate::yolo::types::YoloIterationLog;
use crate::yolo::types::YoloLoopConfig;
use crate::yolo::types::YoloRunLog;
use crate::yolo::types::YoloRunOutcome;
use crate::yolo::types::YoloStopReason;

mod agent;
mod logging;
mod refiner;
mod types;

pub async fn run_yolo_mode(
    arg0_paths: Arg0DispatchPaths,
    config: ResolvedQwenConfig,
    prompt_parts: Vec<String>,
) -> anyhow::Result<()> {
    let prompt = initial_prompt(prompt_parts).await?;
    let codex_exe = resolve_codex_executable(&arg0_paths)?;
    let agent = CodexAgentRunner::new(codex_exe, config.clone());
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
        max_repeated_prompts: config.max_repeated_prompts,
        max_failures: config.max_failures,
        session_id: None,
        stop_reason: None,
        iterations: Vec::new(),
    };
    logger.write_run(&run_log).await?;

    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    let mut current_prompt = config.original_prompt.clone();
    let mut session_id = None;
    let mut repeated_prompts = HashMap::<String, u32>::new();
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
        let agent_result = run_agent(AgentRoundRequest {
            iteration,
            prompt: current_prompt.clone(),
            thread_id: session_id.clone(),
        })
        .await;
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
            current_git_status,
            refiner_input_summary: None,
            refiner_raw_response: None,
            next_prompt_injected_into_agent: None,
            stop_reason: None,
        };

        if interrupted(&config) {
            iteration_log.stop_reason = Some(YoloStopReason::Interrupted);
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

        if consecutive_failures >= config.max_failures {
            iteration_log.stop_reason = Some(YoloStopReason::FailureGuard);
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

        let refiner_summary = build_refiner_summary(&iteration_log);
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
                iteration_log.errors.push(format!("refiner failed: {err}"));
                iteration_log.stop_reason = Some(reason.clone());
                iterations_completed = iteration;
                write_iteration(&logger, &mut run_log, iteration_log).await?;
                finish_run(&logger, &mut run_log, iterations_completed, reason).await?;
                break;
            }
        };

        let next_prompt = refiner_response.next_prompt.trim().to_string();
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

        if next_prompt.eq_ignore_ascii_case(YOLO_STOP) {
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

        if config
            .iteration_limit
            .is_some_and(|limit| iterations_completed >= limit)
        {
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

        let normalized_prompt = normalize_prompt_for_guard(&next_prompt);
        let count = repeated_prompts.entry(normalized_prompt).or_insert(0);
        *count = count.saturating_add(1);
        if *count >= config.max_repeated_prompts {
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
    logger.write_run(run_log).await
}

async fn finish_run(
    logger: &YoloLogger,
    run_log: &mut YoloRunLog,
    _iterations_completed: u32,
    reason: YoloStopReason,
) -> anyhow::Result<()> {
    run_log.completed_at = Some(now_timestamp());
    run_log.stop_reason = Some(reason);
    logger.write_run(run_log).await
}

fn build_refiner_summary(iteration: &YoloIterationLog) -> String {
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

Errors:
{errors}

Current git status:
{status}

Git diff summary:
{diff}

Produce the next concrete prompt for the coding agent, or output YOLO_STOP."#,
        original = iteration.original_user_prompt,
        input = iteration.current_agent_input_prompt,
        summary = iteration.agent_output_summary,
        actions = bullet_lines(&iteration.actions_taken),
        tools = bullet_lines(&iteration.tool_calls_summary),
        files = bullet_lines(&iteration.changed_files),
        commands = bullet_lines(&iteration.commands_tests_run),
        errors = bullet_lines(&iteration.errors),
        status = tail(&iteration.current_git_status, 4_000),
        diff = tail(&iteration.git_diff_summary, 6_000),
    );
    redact_text(&summary)
}

fn interrupted(config: &YoloLoopConfig) -> bool {
    config
        .interrupt_flag
        .as_ref()
        .is_some_and(|flag| flag.load(Ordering::SeqCst))
}

fn bullet_lines(values: &[String]) -> String {
    if values.is_empty() {
        return "- none".to_string();
    }
    values
        .iter()
        .map(|value| format!("- {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_prompt_for_guard(prompt: &str) -> String {
    prompt.split_whitespace().collect::<Vec<_>>().join(" ")
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
            final_response: Some("done".to_string()),
            ..AgentRoundResult::default()
        }
    }

    #[tokio::test]
    async fn fixed_iteration_limit_controls_agent_rounds() {
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
    async fn infinite_mode_can_stop_with_refiner_signal() {
        let temp = TempDir::new().unwrap();

        let outcome = run_yolo_loop(
            loop_config(&temp, None),
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
    }

    #[tokio::test]
    async fn repeated_prompt_guard_stops_loop() {
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
    async fn failure_guard_stops_after_agent_failures() {
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
    async fn interrupt_flag_stops_without_looping_forever() {
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
            current_git_status: String::new(),
            refiner_input_summary: None,
            refiner_raw_response: None,
            next_prompt_injected_into_agent: None,
            stop_reason: None,
        };

        let summary = build_refiner_summary(&iteration);

        assert!(summary.contains("[REDACTED]"));
        assert!(!summary.contains("sk_test_123456789abcdef"));
        assert!(!summary.contains("secret-value"));
    }
}
