use anyhow::Context;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QwenCliOverrides {
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub context_window: Option<u64>,
    pub request_timeout_ms: Option<u64>,
    pub log_level: Option<String>,
    pub reasoning_parser: Option<String>,
    pub tool_call_parser: Option<String>,
    pub auto_tool_choice: Option<bool>,
    pub web_search_live: bool,
    pub iterations: Option<u32>,
    pub yolo_refiner_base_url: Option<String>,
    pub yolo_refiner_api_key: Option<String>,
    pub yolo_refiner_model: Option<String>,
    pub yolo_log_dir: Option<String>,
    pub yolo_round_timeout_secs: Option<u64>,
    pub yolo_max_repeated_prompts: Option<u32>,
    pub yolo_max_failures: Option<u32>,
    pub yolo_round_goal_max_files: Option<u32>,
    pub yolo_round_goal_max_actions: Option<u32>,
    pub yolo_round_goal_max_tests: Option<u32>,
    pub yolo_refiner_max_prompt_chars: Option<u32>,
    pub yolo_refiner_style: Option<String>,
    pub yolo_continue_after_timeout: Option<bool>,
    pub yolo_allow_refiner_stop: Option<bool>,
    pub yolo_verify_commands: Option<String>,
    pub yolo_verify_timeout_secs: Option<u64>,
    pub yolo_acceptance_gate: Option<bool>,
    pub yolo_acceptance_commands: Vec<String>,
    pub yolo_acceptance_max_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QwenCommand {
    Help,
    Version,
    Health,
    Normal {
        codex_args: Vec<String>,
    },
    Yolo {
        prompt_parts: Vec<String>,
        dangerously_bypass_approvals_and_sandbox: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedQwenArgs {
    pub overrides: QwenCliOverrides,
    pub command: QwenCommand,
}

const CODEX_SUBCOMMANDS: &[&str] = &[
    "exec",
    "e",
    "review",
    "login",
    "logout",
    "mcp",
    "plugin",
    "mcp-server",
    "app-server",
    "app",
    "completion",
    "update",
    "sandbox",
    "debug",
    "execpolicy",
    "apply",
    "a",
    "resume",
    "fork",
    "cloud",
    "cloud-tasks",
    "responses-api-proxy",
    "stdio-to-uds",
    "exec-server",
    "features",
];
const DANGEROUS_BYPASS_FLAG: &str = "--dangerously-bypass-approvals-and-sandbox";

pub fn parse_qwen_args(args: Vec<String>) -> anyhow::Result<ParsedQwenArgs> {
    if matches!(args.first().map(String::as_str), Some("--help" | "-h")) {
        return Ok(ParsedQwenArgs {
            overrides: QwenCliOverrides::default(),
            command: QwenCommand::Help,
        });
    }
    if matches!(args.first().map(String::as_str), Some("--version" | "-V")) {
        return Ok(ParsedQwenArgs {
            overrides: QwenCliOverrides::default(),
            command: QwenCommand::Version,
        });
    }

    let mut overrides = QwenCliOverrides::default();
    let mut passthrough = Vec::new();
    let mut yolo = false;
    let mut health = false;
    let mut dangerously_bypass_approvals_and_sandbox = false;
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--" {
            passthrough.extend(args[index + 1..].iter().cloned());
            break;
        }
        if matches!(arg.as_str(), "--yolo" | "--yolo-refiner" | "--yolorefiner") {
            yolo = true;
            index += 1;
            continue;
        }
        if consume_dangerous_bypass_flag(
            &mut index,
            arg,
            &mut dangerously_bypass_approvals_and_sandbox,
        )? {
            continue;
        }
        if arg == "--health" {
            health = true;
            index += 1;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--")
            && !value.is_empty()
            && value.chars().all(|ch| ch.is_ascii_digit())
        {
            overrides.iterations = Some(parse_u32("--iterations", value)?);
            index += 1;
            continue;
        }
        if consume_string_flag(
            &args,
            &mut index,
            arg,
            "--base-url",
            &mut overrides.base_url,
        )? || consume_string_flag(&args, &mut index, arg, "--api-key", &mut overrides.api_key)?
            || consume_string_flag(&args, &mut index, arg, "--model", &mut overrides.model)?
            || consume_string_flag(&args, &mut index, arg, "-m", &mut overrides.model)?
            || consume_string_flag(
                &args,
                &mut index,
                arg,
                "--log-level",
                &mut overrides.log_level,
            )?
            || consume_string_flag(
                &args,
                &mut index,
                arg,
                "--reasoning-parser",
                &mut overrides.reasoning_parser,
            )?
            || consume_string_flag(
                &args,
                &mut index,
                arg,
                "--tool-call-parser",
                &mut overrides.tool_call_parser,
            )?
            || consume_string_flag(
                &args,
                &mut index,
                arg,
                "--refiner-base-url",
                &mut overrides.yolo_refiner_base_url,
            )?
            || consume_string_flag(
                &args,
                &mut index,
                arg,
                "--refiner-api-key",
                &mut overrides.yolo_refiner_api_key,
            )?
            || consume_string_flag(
                &args,
                &mut index,
                arg,
                "--refiner-model",
                &mut overrides.yolo_refiner_model,
            )?
            || consume_string_flag(
                &args,
                &mut index,
                arg,
                "--yolo-log-dir",
                &mut overrides.yolo_log_dir,
            )?
            || consume_string_flag(
                &args,
                &mut index,
                arg,
                "--yolo-refiner-style",
                &mut overrides.yolo_refiner_style,
            )?
            || consume_string_flag(
                &args,
                &mut index,
                arg,
                "--yolo-verify-commands",
                &mut overrides.yolo_verify_commands,
            )?
        {
            continue;
        }
        if consume_string_list_flag(
            &args,
            &mut index,
            arg,
            "--yolo-acceptance-command",
            &mut overrides.yolo_acceptance_commands,
        )? {
            continue;
        }
        if consume_u64_flag(
            &args,
            &mut index,
            arg,
            "--context-window",
            &mut overrides.context_window,
        )? || consume_u64_flag(
            &args,
            &mut index,
            arg,
            "--request-timeout-ms",
            &mut overrides.request_timeout_ms,
        )? || consume_u64_flag(
            &args,
            &mut index,
            arg,
            "--yolo-round-timeout-secs",
            &mut overrides.yolo_round_timeout_secs,
        )? || consume_u64_flag(
            &args,
            &mut index,
            arg,
            "--yolo-verify-timeout-secs",
            &mut overrides.yolo_verify_timeout_secs,
        )? || consume_u64_flag(
            &args,
            &mut index,
            arg,
            "--yolo-acceptance-max-secs",
            &mut overrides.yolo_acceptance_max_seconds,
        )? {
            continue;
        }
        if consume_u32_flag(
            &args,
            &mut index,
            arg,
            "--iterations",
            &mut overrides.iterations,
        )? || consume_u32_flag(&args, &mut index, arg, "-n", &mut overrides.iterations)?
            || consume_u32_flag(
                &args,
                &mut index,
                arg,
                "--max-repeated-prompts",
                &mut overrides.yolo_max_repeated_prompts,
            )?
            || consume_u32_flag(
                &args,
                &mut index,
                arg,
                "--max-failures",
                &mut overrides.yolo_max_failures,
            )?
            || consume_u32_flag(
                &args,
                &mut index,
                arg,
                "--yolo-round-max-files",
                &mut overrides.yolo_round_goal_max_files,
            )?
            || consume_u32_flag(
                &args,
                &mut index,
                arg,
                "--yolo-round-max-actions",
                &mut overrides.yolo_round_goal_max_actions,
            )?
            || consume_u32_flag(
                &args,
                &mut index,
                arg,
                "--yolo-round-max-tests",
                &mut overrides.yolo_round_goal_max_tests,
            )?
            || consume_u32_flag(
                &args,
                &mut index,
                arg,
                "--yolo-next-prompt-max-chars",
                &mut overrides.yolo_refiner_max_prompt_chars,
            )?
        {
            continue;
        }
        if consume_bool_flag(
            &mut index,
            arg,
            "--auto-tool-choice",
            &mut overrides.auto_tool_choice,
        )? || consume_bool_flag(
            &mut index,
            arg,
            "--yolo-continue-after-timeout",
            &mut overrides.yolo_continue_after_timeout,
        )? || consume_bool_flag(
            &mut index,
            arg,
            "--yolo-allow-refiner-stop",
            &mut overrides.yolo_allow_refiner_stop,
        )? || consume_bool_flag(
            &mut index,
            arg,
            "--yolo-acceptance-gate",
            &mut overrides.yolo_acceptance_gate,
        )? {
            continue;
        }
        if arg == "--search" {
            overrides.web_search_live = true;
            index += 1;
            continue;
        }

        passthrough.push(arg.clone());
        index += 1;
    }

    if yolo && overrides.iterations == Some(0) {
        anyhow::bail!(
            "--iterations must be greater than 0; omit --iterations for infinite YOLO mode"
        );
    }

    let command = if health {
        QwenCommand::Health
    } else if yolo {
        QwenCommand::Yolo {
            prompt_parts: passthrough,
            dangerously_bypass_approvals_and_sandbox,
        }
    } else {
        QwenCommand::Normal {
            codex_args: normalize_normal_args(
                passthrough,
                dangerously_bypass_approvals_and_sandbox,
            ),
        }
    };

    Ok(ParsedQwenArgs { overrides, command })
}

fn normalize_normal_args(
    args: Vec<String>,
    dangerously_bypass_approvals_and_sandbox: bool,
) -> Vec<String> {
    if args.is_empty() {
        return if dangerously_bypass_approvals_and_sandbox {
            vec![DANGEROUS_BYPASS_FLAG.to_string()]
        } else {
            args
        };
    }

    if starts_with_codex_subcommand(&args) {
        return add_dangerous_bypass_to_subcommand(args, dangerously_bypass_approvals_and_sandbox);
    }

    let mut normalized = vec!["exec".to_string(), "--skip-git-repo-check".to_string()];
    if dangerously_bypass_approvals_and_sandbox {
        normalized.push(DANGEROUS_BYPASS_FLAG.to_string());
    } else {
        normalized.extend(["--sandbox".to_string(), "workspace-write".to_string()]);
    }
    normalized.extend(args);
    normalized
}

fn add_dangerous_bypass_to_subcommand(
    args: Vec<String>,
    dangerously_bypass_approvals_and_sandbox: bool,
) -> Vec<String> {
    if !dangerously_bypass_approvals_and_sandbox {
        return args;
    }

    let mut normalized = Vec::with_capacity(args.len() + 1);
    let mut iter = args.into_iter();
    if let Some(subcommand) = iter.next() {
        normalized.push(subcommand);
        normalized.push(DANGEROUS_BYPASS_FLAG.to_string());
        normalized.extend(iter);
    }
    normalized
}

fn starts_with_codex_subcommand(args: &[String]) -> bool {
    args.first()
        .is_some_and(|arg| CODEX_SUBCOMMANDS.contains(&arg.as_str()))
}

fn consume_string_flag(
    args: &[String],
    index: &mut usize,
    arg: &str,
    name: &str,
    target: &mut Option<String>,
) -> anyhow::Result<bool> {
    if arg == name {
        *target = Some(next_value(args, *index, name)?);
        *index += 2;
        return Ok(true);
    }

    let Some(value) = arg.strip_prefix(&format!("{name}=")) else {
        return Ok(false);
    };
    *target = Some(value.to_string());
    *index += 1;
    Ok(true)
}

fn consume_string_list_flag(
    args: &[String],
    index: &mut usize,
    arg: &str,
    name: &str,
    target: &mut Vec<String>,
) -> anyhow::Result<bool> {
    if arg == name {
        let value = args
            .get(*index + 1)
            .with_context(|| format!("{name} requires a value"))?;
        target.push(value.clone());
        *index += 2;
        return Ok(true);
    }

    let Some(value) = arg.strip_prefix(&format!("{name}=")) else {
        return Ok(false);
    };
    target.push(value.to_string());
    *index += 1;
    Ok(true)
}

fn consume_u64_flag(
    args: &[String],
    index: &mut usize,
    arg: &str,
    name: &str,
    target: &mut Option<u64>,
) -> anyhow::Result<bool> {
    let mut raw = None;
    if arg == name {
        raw = Some(next_value(args, *index, name)?);
        *index += 2;
    } else if let Some(value) = arg.strip_prefix(&format!("{name}=")) {
        raw = Some(value.to_string());
        *index += 1;
    }

    match raw {
        Some(value) => {
            *target = Some(parse_u64(name, &value)?);
            Ok(true)
        }
        None => Ok(false),
    }
}

fn consume_u32_flag(
    args: &[String],
    index: &mut usize,
    arg: &str,
    name: &str,
    target: &mut Option<u32>,
) -> anyhow::Result<bool> {
    let mut raw = None;
    if arg == name {
        raw = Some(next_value(args, *index, name)?);
        *index += 2;
    } else if let Some(value) = arg.strip_prefix(&format!("{name}=")) {
        raw = Some(value.to_string());
        *index += 1;
    }

    match raw {
        Some(value) => {
            *target = Some(parse_u32(name, &value)?);
            Ok(true)
        }
        None => Ok(false),
    }
}

fn consume_bool_flag(
    index: &mut usize,
    arg: &str,
    name: &str,
    target: &mut Option<bool>,
) -> anyhow::Result<bool> {
    if arg == name {
        *target = Some(true);
        *index += 1;
        return Ok(true);
    }

    let Some(value) = arg.strip_prefix(&format!("{name}=")) else {
        return Ok(false);
    };
    *target = Some(parse_bool(name, value)?);
    *index += 1;
    Ok(true)
}

fn consume_dangerous_bypass_flag(
    index: &mut usize,
    arg: &str,
    target: &mut bool,
) -> anyhow::Result<bool> {
    if arg == DANGEROUS_BYPASS_FLAG {
        *target = true;
        *index += 1;
        return Ok(true);
    }

    let Some(value) = arg.strip_prefix(&format!("{DANGEROUS_BYPASS_FLAG}=")) else {
        return Ok(false);
    };
    *target = parse_bool(DANGEROUS_BYPASS_FLAG, value)?;
    *index += 1;
    Ok(true)
}

fn next_value(args: &[String], index: usize, name: &str) -> anyhow::Result<String> {
    args.get(index + 1)
        .cloned()
        .with_context(|| format!("{name} requires a value"))
}

fn parse_u64(name: &str, value: &str) -> anyhow::Result<u64> {
    value
        .parse::<u64>()
        .with_context(|| format!("{name} must be an unsigned integer"))
}

fn parse_u32(name: &str, value: &str) -> anyhow::Result<u32> {
    value
        .parse::<u32>()
        .with_context(|| format!("{name} must be an unsigned integer"))
}

fn parse_bool(name: &str, value: &str) -> anyhow::Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => anyhow::bail!("{name} must be a boolean"),
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn one_shot_prompt_is_routed_to_exec() {
        let parsed = parse_qwen_args(vec!["What is 2+2?".to_string()]).unwrap();

        assert_eq!(
            parsed.command,
            QwenCommand::Normal {
                codex_args: vec![
                    "exec".to_string(),
                    "--skip-git-repo-check".to_string(),
                    "--sandbox".to_string(),
                    "workspace-write".to_string(),
                    "What is 2+2?".to_string()
                ]
            }
        );
    }

    #[test]
    fn existing_subcommand_is_preserved() {
        let parsed = parse_qwen_args(vec!["exec".to_string(), "hello".to_string()]).unwrap();

        assert_eq!(
            parsed.command,
            QwenCommand::Normal {
                codex_args: vec!["exec".to_string(), "hello".to_string()]
            }
        );
    }

    #[test]
    fn parses_yolo_iterations_and_prompt() {
        let parsed = parse_qwen_args(vec![
            "--yolo".to_string(),
            "-n".to_string(),
            "10".to_string(),
            "Improve docs".to_string(),
        ])
        .unwrap();

        assert_eq!(parsed.overrides.iterations, Some(10));
        assert_eq!(
            parsed.command,
            QwenCommand::Yolo {
                prompt_parts: vec!["Improve docs".to_string()],
                dangerously_bypass_approvals_and_sandbox: false,
            }
        );
    }

    #[test]
    fn yolo_without_iterations_leaves_limit_unset() {
        let parsed =
            parse_qwen_args(vec!["--yolo".to_string(), "Improve docs".to_string()]).unwrap();

        assert_eq!(parsed.overrides.iterations, None);
        assert_eq!(
            parsed.command,
            QwenCommand::Yolo {
                prompt_parts: vec!["Improve docs".to_string()],
                dangerously_bypass_approvals_and_sandbox: false,
            }
        );
    }

    #[test]
    fn rejects_zero_yolo_iterations() {
        for args in [
            vec!["--yolo", "--iterations", "0", "Improve docs"],
            vec!["--yolo", "-n", "0", "Improve docs"],
            vec!["--yolo", "--0", "Improve docs"],
        ] {
            let err = parse_qwen_args(args.into_iter().map(str::to_string).collect())
                .expect_err("zero iteration limit should be invalid");

            assert!(
                err.to_string()
                    .contains("omit --iterations for infinite YOLO mode")
            );
        }
    }

    #[test]
    fn parses_optional_numeric_yolo_shorthand() {
        let parsed = parse_qwen_args(vec!["--yolo".to_string(), "--10".to_string()]).unwrap();

        assert_eq!(parsed.overrides.iterations, Some(10));
    }

    #[test]
    fn parses_yolo_round_timeout() {
        let parsed = parse_qwen_args(vec![
            "--yolo-refiner".to_string(),
            "--yolo-round-timeout-secs".to_string(),
            "30".to_string(),
            "Improve".to_string(),
        ])
        .unwrap();

        assert_eq!(parsed.overrides.yolo_round_timeout_secs, Some(30));
    }

    #[test]
    fn parses_yolo_round_budget_flags() {
        let parsed = parse_qwen_args(vec![
            "--yolo-refiner".to_string(),
            "--yolo-round-max-files".to_string(),
            "6".to_string(),
            "--yolo-round-max-actions=4".to_string(),
            "--yolo-round-max-tests".to_string(),
            "2".to_string(),
            "--yolo-next-prompt-max-chars".to_string(),
            "1800".to_string(),
            "--yolo-refiner-style".to_string(),
            "incremental".to_string(),
            "--yolo-continue-after-timeout=true".to_string(),
            "--yolo-allow-refiner-stop".to_string(),
            "--yolo-verify-commands".to_string(),
            "echo ok;sh -c 'exit 7'".to_string(),
            "--yolo-verify-timeout-secs".to_string(),
            "9".to_string(),
            "--yolo-acceptance-gate".to_string(),
            "--yolo-acceptance-command".to_string(),
            "docker compose config".to_string(),
            "--yolo-acceptance-command=curl -sf http://localhost:2225".to_string(),
            "--yolo-acceptance-max-secs".to_string(),
            "120".to_string(),
            "Improve".to_string(),
        ])
        .unwrap();

        assert_eq!(parsed.overrides.yolo_round_goal_max_files, Some(6));
        assert_eq!(parsed.overrides.yolo_round_goal_max_actions, Some(4));
        assert_eq!(parsed.overrides.yolo_round_goal_max_tests, Some(2));
        assert_eq!(parsed.overrides.yolo_refiner_max_prompt_chars, Some(1800));
        assert_eq!(
            parsed.overrides.yolo_refiner_style,
            Some("incremental".to_string())
        );
        assert_eq!(parsed.overrides.yolo_continue_after_timeout, Some(true));
        assert_eq!(parsed.overrides.yolo_allow_refiner_stop, Some(true));
        assert_eq!(
            parsed.overrides.yolo_verify_commands,
            Some("echo ok;sh -c 'exit 7'".to_string())
        );
        assert_eq!(parsed.overrides.yolo_verify_timeout_secs, Some(9));
        assert_eq!(parsed.overrides.yolo_acceptance_gate, Some(true));
        assert_eq!(
            parsed.overrides.yolo_acceptance_commands,
            vec![
                "docker compose config".to_string(),
                "curl -sf http://localhost:2225".to_string()
            ]
        );
        assert_eq!(parsed.overrides.yolo_acceptance_max_seconds, Some(120));
    }

    #[test]
    fn parses_yolo_refiner_alias_and_dangerous_bypass() {
        let parsed = parse_qwen_args(vec![
            "--yolo-refiner".to_string(),
            DANGEROUS_BYPASS_FLAG.to_string(),
            "Improve docs".to_string(),
        ])
        .unwrap();

        assert_eq!(
            parsed.command,
            QwenCommand::Yolo {
                prompt_parts: vec!["Improve docs".to_string()],
                dangerously_bypass_approvals_and_sandbox: true,
            }
        );
    }

    #[test]
    fn parses_yolorefiner_compat_alias() {
        let parsed =
            parse_qwen_args(vec!["--yolorefiner".to_string(), "Improve".to_string()]).unwrap();

        assert_eq!(
            parsed.command,
            QwenCommand::Yolo {
                prompt_parts: vec!["Improve".to_string()],
                dangerously_bypass_approvals_and_sandbox: false,
            }
        );
    }

    #[test]
    fn normal_prompt_forwards_explicit_dangerous_bypass_without_workspace_sandbox() {
        let parsed = parse_qwen_args(vec![
            DANGEROUS_BYPASS_FLAG.to_string(),
            "What is 2+2?".to_string(),
        ])
        .unwrap();

        assert_eq!(
            parsed.command,
            QwenCommand::Normal {
                codex_args: vec![
                    "exec".to_string(),
                    "--skip-git-repo-check".to_string(),
                    DANGEROUS_BYPASS_FLAG.to_string(),
                    "What is 2+2?".to_string(),
                ]
            }
        );
    }
}
