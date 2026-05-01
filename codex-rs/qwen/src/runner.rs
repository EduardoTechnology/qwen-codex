use std::collections::HashMap;
use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;

use anyhow::Context;
use codex_arg0::Arg0DispatchPaths;
use tokio::process::Command;

use crate::QWEN_ENTRYPOINT_ENV;
use crate::args::QwenCommand;
use crate::args::parse_qwen_args;
use crate::config::DEFAULT_API_KEY;
use crate::config::EnvSource;
use crate::config::ResolvedQwenConfig;
use crate::health::run_health_check;
use crate::yolo::run_yolo_mode;

pub async fn run_qwen_entrypoint(arg0_paths: Arg0DispatchPaths) -> anyhow::Result<()> {
    let parsed = parse_qwen_args(env::args().skip(1).collect())?;
    let runtime_env = RuntimeEnv::load()?;
    let config = ResolvedQwenConfig::from_env_source(&parsed.overrides, &runtime_env)?;

    match parsed.command {
        QwenCommand::Help => {
            print_help(&config);
            Ok(())
        }
        QwenCommand::Version => {
            println!("qwen-codex {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        QwenCommand::Health => run_health_check(&config).await,
        QwenCommand::Normal { codex_args } => run_codex(arg0_paths, config, codex_args).await,
        QwenCommand::Yolo {
            prompt_parts,
            dangerously_bypass_approvals_and_sandbox,
        } => {
            run_yolo_mode(
                arg0_paths,
                config,
                prompt_parts,
                dangerously_bypass_approvals_and_sandbox,
            )
            .await
        }
    }
}

async fn run_codex(
    arg0_paths: Arg0DispatchPaths,
    config: ResolvedQwenConfig,
    codex_args: Vec<String>,
) -> anyhow::Result<()> {
    let codex_exe = resolve_codex_executable(&arg0_paths)?;
    let mut command = Command::new(&codex_exe);
    command
        .args(config.codex_config_args())
        .args(codex_args)
        .env_remove(QWEN_ENTRYPOINT_ENV)
        .envs(config.child_env())
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let status = command.status().await.with_context(|| {
        format!(
            "failed to launch upstream Codex binary at {}",
            display_path(&codex_exe)
        )
    })?;

    match status.code() {
        Some(code) => std::process::exit(code),
        None => std::process::exit(1),
    }
}

pub(crate) fn resolve_codex_executable(arg0_paths: &Arg0DispatchPaths) -> anyhow::Result<PathBuf> {
    let current = arg0_paths
        .codex_self_exe
        .clone()
        .or_else(|| env::current_exe().ok())
        .context("failed to resolve current executable path")?;

    if is_codex_binary(&current) {
        return Ok(current);
    }

    if let Some(sibling) = sibling_codex_binary(&current)
        && sibling.exists()
    {
        return Ok(sibling);
    }

    if let Some(on_path) = find_codex_on_path()
        && !same_path(&on_path, &current)
    {
        return Ok(on_path);
    }

    anyhow::bail!(
        "could not find the upstream `codex` binary for Qwen Codex to delegate to; \
         run `cargo build -p codex-cli` or install the package so `codex` is available next to `qwen-codex`"
    );
}

fn is_codex_binary(path: &Path) -> bool {
    path.file_stem()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "codex")
}

fn sibling_codex_binary(path: &Path) -> Option<PathBuf> {
    let executable_name = if cfg!(windows) { "codex.exe" } else { "codex" };
    path.parent().map(|parent| parent.join(executable_name))
}

fn find_codex_on_path() -> Option<PathBuf> {
    let executable_name = if cfg!(windows) { "codex.exe" } else { "codex" };
    env::var_os("PATH").and_then(|path| {
        env::split_paths(&path)
            .map(|dir| dir.join(executable_name))
            .find(|candidate| candidate.exists())
    })
}

fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn print_help(config: &ResolvedQwenConfig) {
    println!(
        r#"Qwen Codex {version}

USAGE:
    qwen-codex [OPTIONS] [PROMPT]
    qwen-codex [OPTIONS] <CODEX_SUBCOMMAND> [ARGS]
    qwencodex [OPTIONS] [PROMPT]

NORMAL MODE:
    Runs the upstream Codex CLI with a Qwen OpenAI-compatible provider override.
    A bare prompt is routed to `codex exec --skip-git-repo-check`.

OPTIONS:
    --base-url <URL>              Local OpenAI-compatible base URL
    --api-key <KEY>               API key sent as bearer auth; local vLLM accepts a placeholder
    --model, -m <MODEL>           Served model name
    --context-window <TOKENS>     Model context window
    --request-timeout-ms <MS>     Request and stream idle timeout
    --log-level <LEVEL>           Default RUST_LOG level for child Codex process
    --search                      Enable Codex live web search config override
    --health                      Check <base-url>/models and print the response
    --help, -h                    Show this help
    --version, -V                 Show qwen-codex version

YOLO OPTIONS:
    --yolo-refiner                Start autonomous refinement mode
    --yolorefiner                 Compatibility alias for --yolo-refiner
    --yolo                        Backward-compatible alias; upstream Codex also uses this as a dangerous bypass alias
    --iterations, -n <COUNT>      Run a fixed number of refinement iterations
    --refiner-base-url <URL>      OpenAI-compatible refiner endpoint
    --refiner-api-key <KEY>       Refiner API key
    --refiner-model <MODEL>       Refiner model name
    --yolo-log-dir <DIR>          YOLO run log directory
    --yolo-round-timeout-secs <N> Per-agent-round timeout; default is 600 seconds
    --max-repeated-prompts <N>    Repeated prompt guard
    --max-failures <N>            Repeated failure guard
    --dangerously-bypass-approvals-and-sandbox
                                  Forward upstream Codex dangerous bypass to each YOLO agent round

ENVIRONMENT:
    QWEN_CODEX_BASE_URL           default: {base_url}
    QWEN_CODEX_MODEL              default: {model}
    QWEN_CODEX_API_KEY            default: {api_key}
    QWEN_CODEX_CONTEXT_WINDOW     default: {context_window}
    QWEN_CODEX_REQUEST_TIMEOUT_MS default: {timeout_ms}
    QWEN_CODEX_LOG_LEVEL          default: {log_level}
    QWEN_CODEX_YOLO_ROUND_TIMEOUT_SECS default: {round_timeout_secs}

    A local .env file in the current directory is loaded for QWEN_CODEX_* and supported alias variables.
    Precedence is CLI flags, QWEN_CODEX_* environment, short aliases, documented defaults.

EXAMPLES:
    qwen-codex --health
    qwen-codex "What is 2+2? Answer in one word."
    qwen-codex --model qwen35-local exec "Summarize this repository."
    qwen-codex --yolo-refiner --iterations 10 "Refine this project."
    qwen-codex --yolo-refiner --iterations 5 --dangerously-bypass-approvals-and-sandbox "Autonomously refine this project."
"#,
        version = env!("CARGO_PKG_VERSION"),
        base_url = config.base_url,
        model = config.model,
        api_key = DEFAULT_API_KEY,
        context_window = config.context_window,
        timeout_ms = config.request_timeout_ms,
        log_level = config.log_level,
        round_timeout_secs = config.yolo.round_timeout_secs
    );
}

struct RuntimeEnv {
    dotenv: HashMap<String, String>,
}

impl RuntimeEnv {
    fn load() -> anyhow::Result<Self> {
        let dotenv = match std::fs::read_to_string(".env") {
            Ok(contents) => parse_dotenv_contents(&contents),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
            Err(error) => return Err(error).context("failed to read local .env file"),
        };

        Ok(Self { dotenv })
    }
}

impl EnvSource for RuntimeEnv {
    fn get(&self, key: &str) -> Option<String> {
        env::var(key)
            .ok()
            .and_then(non_empty)
            .or_else(|| self.dotenv.get(key).cloned().and_then(non_empty))
    }
}

fn parse_dotenv_contents(contents: &str) -> HashMap<String, String> {
    contents
        .lines()
        .filter_map(parse_dotenv_line)
        .filter(|(key, _)| is_allowed_dotenv_key(key))
        .collect()
}

fn parse_dotenv_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }

    let without_export = trimmed.strip_prefix("export ").unwrap_or(trimmed);
    let (key, value) = without_export.split_once('=')?;
    let key = key.trim().to_string();
    let value = strip_dotenv_quotes(value.trim()).to_string();
    Some((key, value))
}

fn strip_dotenv_quotes(value: &str) -> &str {
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

fn is_allowed_dotenv_key(key: &str) -> bool {
    matches!(
        key,
        "QWEN_BASE_URL"
            | "QWEN_API_KEY"
            | "QWEN_MODEL_NAME"
            | "QWEN_MAX_MODEL_LEN"
            | "YOLO_REFINER_BASE_URL"
            | "YOLO_REFINER_API_KEY"
            | "YOLO_REFINER_MODEL"
            | "YOLO_LOG_DIR"
            | "YOLO_DEFAULT_ITERATIONS"
    ) || key.starts_with("QWEN_CODEX_")
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn parses_allowed_dotenv_values_only() {
        let env = parse_dotenv_contents(
            r#"
            QWEN_CODEX_BASE_URL=http://127.0.0.1:8002/v1
            export QWEN_MODEL_NAME="qwen35-local"
            OPENAI_API_KEY=should-not-load
            YOLO_LOG_DIR='.qwen-codex/yolo-runs'
            "#,
        );

        assert_eq!(
            env,
            HashMap::from([
                (
                    "QWEN_CODEX_BASE_URL".to_string(),
                    "http://127.0.0.1:8002/v1".to_string()
                ),
                ("QWEN_MODEL_NAME".to_string(), "qwen35-local".to_string()),
                (
                    "YOLO_LOG_DIR".to_string(),
                    ".qwen-codex/yolo-runs".to_string()
                ),
            ])
        );
    }

    #[test]
    fn documented_defaults_match_verified_local_server() {
        assert_eq!(crate::config::DEFAULT_BASE_URL, "http://127.0.0.1:8002/v1");
        assert_eq!(crate::config::DEFAULT_MODEL, "qwen35-local");
        assert_eq!(crate::config::DEFAULT_CONTEXT_WINDOW, 32_768);
        assert_eq!(crate::config::DEFAULT_REQUEST_TIMEOUT_MS, 120_000);
        assert_eq!(crate::config::DEFAULT_LOG_LEVEL, "info");
    }
}
