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
    pub yolo_max_repeated_prompts: Option<u32>,
    pub yolo_max_failures: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QwenCommand {
    Help,
    Version,
    Health,
    Normal { codex_args: Vec<String> },
    Yolo { prompt_parts: Vec<String> },
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
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--" {
            passthrough.extend(args[index + 1..].iter().cloned());
            break;
        }
        if arg == "--yolo" {
            yolo = true;
            index += 1;
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
        {
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
        {
            continue;
        }
        if consume_bool_flag(
            &mut index,
            arg,
            "--auto-tool-choice",
            &mut overrides.auto_tool_choice,
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

    let command = if health {
        QwenCommand::Health
    } else if yolo {
        QwenCommand::Yolo {
            prompt_parts: passthrough,
        }
    } else {
        QwenCommand::Normal {
            codex_args: normalize_normal_args(passthrough),
        }
    };

    Ok(ParsedQwenArgs { overrides, command })
}

fn normalize_normal_args(args: Vec<String>) -> Vec<String> {
    if args.is_empty() || starts_with_codex_subcommand(&args) {
        return args;
    }

    let mut normalized = vec!["exec".to_string(), "--skip-git-repo-check".to_string()];
    normalized.extend(args);
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
                prompt_parts: vec!["Improve docs".to_string()]
            }
        );
    }

    #[test]
    fn parses_optional_numeric_yolo_shorthand() {
        let parsed = parse_qwen_args(vec!["--yolo".to_string(), "--10".to_string()]).unwrap();

        assert_eq!(parsed.overrides.iterations, Some(10));
    }
}
