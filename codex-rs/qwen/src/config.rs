use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Context;

use crate::args::QwenCliOverrides;

pub const QWEN_PROVIDER_ID: &str = "qwen";

pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:8002/v1";
pub const DEFAULT_API_KEY: &str = "local-dev-key";
pub const DEFAULT_MODEL: &str = "qwen35-local";
pub const DEFAULT_CONTEXT_WINDOW: u64 = 32_768;
pub const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 600_000;
pub const DEFAULT_LOG_LEVEL: &str = "error";
pub const DEFAULT_REASONING_PARSER: &str = "qwen3";
pub const DEFAULT_TOOL_CALL_PARSER: &str = "qwen3_coder";
pub const DEFAULT_AUTO_TOOL_CHOICE: bool = true;
pub const DEFAULT_YOLO_LOG_DIR: &str = ".qwen-codex/yolo-runs";
pub const DEFAULT_YOLO_ROUND_TIMEOUT_SECS: u64 = 600;
pub const DEFAULT_YOLO_MAX_REPEATED_PROMPTS: u32 = 3;
pub const DEFAULT_YOLO_MAX_FAILURES: u32 = 3;
pub const DEFAULT_YOLO_ROUND_GOAL_MAX_FILES: u32 = 8;
pub const DEFAULT_YOLO_ROUND_GOAL_MAX_ACTIONS: u32 = 5;
pub const DEFAULT_YOLO_ROUND_GOAL_MAX_TESTS: u32 = 3;
pub const DEFAULT_YOLO_REFINER_MAX_PROMPT_CHARS: u32 = 3_000;
pub const DEFAULT_YOLO_REFINER_STYLE: &str = "incremental";
pub const DEFAULT_YOLO_CONTINUE_AFTER_TIMEOUT: bool = false;
pub const DEFAULT_YOLO_ALLOW_REFINER_STOP: bool = true;
pub const DEFAULT_YOLO_VERIFY_TIMEOUT_SECS: u64 = 15;
pub const DEFAULT_YOLO_ACCEPTANCE_GATE: bool = false;
pub const DEFAULT_YOLO_ACCEPTANCE_MAX_SECONDS: u64 = 300;
pub const DEFAULT_YOLO_REJECT_STOP_ON_FAILED_ACCEPTANCE: bool = true;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedQwenConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub context_window: u64,
    pub request_timeout_ms: u64,
    pub log_level: String,
    pub reasoning_parser: String,
    pub tool_call_parser: String,
    pub auto_tool_choice: bool,
    pub web_search_live: bool,
    pub yolo: ResolvedYoloConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedYoloConfig {
    pub refiner_base_url: String,
    pub refiner_api_key: String,
    pub refiner_model: String,
    pub log_dir: PathBuf,
    pub default_iterations: Option<u32>,
    pub round_timeout_secs: u64,
    pub max_repeated_prompts: u32,
    pub max_failures: u32,
    pub round_budget: ResolvedYoloRoundBudget,
    pub continue_after_timeout: bool,
    pub allow_refiner_stop: bool,
    pub verify_commands: Vec<String>,
    pub verify_timeout_secs: u64,
    pub acceptance_gate_enabled: bool,
    pub acceptance_commands: Vec<String>,
    pub acceptance_max_seconds: u64,
    pub reject_stop_on_failed_acceptance: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedYoloRoundBudget {
    pub max_files: u32,
    pub max_actions: u32,
    pub max_tests: u32,
    pub max_next_prompt_chars: u32,
    pub style: String,
}

pub trait EnvSource {
    fn get(&self, key: &str) -> Option<String>;
}

impl EnvSource for std::env::Vars {
    fn get(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

impl EnvSource for HashMap<String, String> {
    fn get(&self, key: &str) -> Option<String> {
        self.get(key).cloned()
    }
}

impl ResolvedQwenConfig {
    pub fn from_env(overrides: &QwenCliOverrides) -> anyhow::Result<Self> {
        Self::from_env_source(overrides, &std::env::vars())
    }

    pub fn from_env_source(
        overrides: &QwenCliOverrides,
        env: &dyn EnvSource,
    ) -> anyhow::Result<Self> {
        let base_url = resolve_string(
            overrides.base_url.clone(),
            env,
            "QWEN_CODEX_BASE_URL",
            Some("QWEN_BASE_URL"),
            DEFAULT_BASE_URL,
        );
        let api_key = resolve_string(
            overrides.api_key.clone(),
            env,
            "QWEN_CODEX_API_KEY",
            Some("QWEN_API_KEY"),
            DEFAULT_API_KEY,
        );
        let model = resolve_string(
            overrides.model.clone(),
            env,
            "QWEN_CODEX_MODEL",
            Some("QWEN_MODEL_NAME"),
            DEFAULT_MODEL,
        );
        let context_window = resolve_u64(
            overrides.context_window,
            env,
            "QWEN_CODEX_CONTEXT_WINDOW",
            Some("QWEN_MAX_MODEL_LEN"),
            DEFAULT_CONTEXT_WINDOW,
        )?;
        let request_timeout_ms = resolve_u64(
            overrides.request_timeout_ms,
            env,
            "QWEN_CODEX_REQUEST_TIMEOUT_MS",
            None,
            DEFAULT_REQUEST_TIMEOUT_MS,
        )?;
        let log_level = resolve_string(
            overrides.log_level.clone(),
            env,
            "QWEN_CODEX_LOG_LEVEL",
            None,
            DEFAULT_LOG_LEVEL,
        );
        let reasoning_parser = resolve_string(
            overrides.reasoning_parser.clone(),
            env,
            "QWEN_CODEX_REASONING_PARSER",
            None,
            DEFAULT_REASONING_PARSER,
        );
        let tool_call_parser = resolve_string(
            overrides.tool_call_parser.clone(),
            env,
            "QWEN_CODEX_TOOL_CALL_PARSER",
            None,
            DEFAULT_TOOL_CALL_PARSER,
        );
        let auto_tool_choice = resolve_bool(
            overrides.auto_tool_choice,
            env,
            "QWEN_CODEX_AUTO_TOOL_CHOICE",
            None,
            DEFAULT_AUTO_TOOL_CHOICE,
        )?;

        let refiner_base_url = resolve_string(
            overrides.yolo_refiner_base_url.clone(),
            env,
            "QWEN_CODEX_YOLO_REFINER_BASE_URL",
            Some("YOLO_REFINER_BASE_URL"),
            &base_url,
        );
        let refiner_api_key = resolve_string(
            overrides.yolo_refiner_api_key.clone(),
            env,
            "QWEN_CODEX_YOLO_REFINER_API_KEY",
            Some("YOLO_REFINER_API_KEY"),
            &api_key,
        );
        let refiner_model = resolve_string(
            overrides.yolo_refiner_model.clone(),
            env,
            "QWEN_CODEX_YOLO_REFINER_MODEL",
            Some("YOLO_REFINER_MODEL"),
            &model,
        );
        let log_dir = PathBuf::from(resolve_string(
            overrides.yolo_log_dir.clone(),
            env,
            "QWEN_CODEX_YOLO_LOG_DIR",
            Some("YOLO_LOG_DIR"),
            DEFAULT_YOLO_LOG_DIR,
        ));
        let default_iterations = resolve_optional_u32(
            overrides.iterations,
            env,
            "QWEN_CODEX_YOLO_DEFAULT_ITERATIONS",
            Some("YOLO_DEFAULT_ITERATIONS"),
        )?
        .map(|limit| {
            if limit == 0 {
                anyhow::bail!(
                    "QWEN_CODEX_YOLO_DEFAULT_ITERATIONS must be greater than 0; unset it for infinite YOLO mode"
                );
            }
            Ok(limit)
        })
        .transpose()?;
        let round_timeout_secs = resolve_u64(
            overrides.yolo_round_timeout_secs,
            env,
            "QWEN_CODEX_YOLO_ROUND_TIMEOUT_SECS",
            None,
            DEFAULT_YOLO_ROUND_TIMEOUT_SECS,
        )?;
        let max_repeated_prompts = resolve_u32(
            overrides.yolo_max_repeated_prompts,
            env,
            "QWEN_CODEX_YOLO_MAX_REPEATED_PROMPTS",
            None,
            DEFAULT_YOLO_MAX_REPEATED_PROMPTS,
        )?;
        let max_failures = resolve_u32(
            overrides.yolo_max_failures,
            env,
            "QWEN_CODEX_YOLO_MAX_FAILURES",
            None,
            DEFAULT_YOLO_MAX_FAILURES,
        )?;
        let round_budget = ResolvedYoloRoundBudget {
            max_files: resolve_u32(
                overrides.yolo_round_goal_max_files,
                env,
                "QWEN_CODEX_YOLO_ROUND_GOAL_MAX_FILES",
                None,
                DEFAULT_YOLO_ROUND_GOAL_MAX_FILES,
            )?,
            max_actions: resolve_u32(
                overrides.yolo_round_goal_max_actions,
                env,
                "QWEN_CODEX_YOLO_ROUND_GOAL_MAX_ACTIONS",
                None,
                DEFAULT_YOLO_ROUND_GOAL_MAX_ACTIONS,
            )?,
            max_tests: resolve_u32(
                overrides.yolo_round_goal_max_tests,
                env,
                "QWEN_CODEX_YOLO_ROUND_GOAL_MAX_TESTS",
                None,
                DEFAULT_YOLO_ROUND_GOAL_MAX_TESTS,
            )?,
            max_next_prompt_chars: resolve_u32(
                overrides.yolo_refiner_max_prompt_chars,
                env,
                "QWEN_CODEX_YOLO_REFINER_MAX_PROMPT_CHARS",
                None,
                DEFAULT_YOLO_REFINER_MAX_PROMPT_CHARS,
            )?,
            style: resolve_string(
                overrides.yolo_refiner_style.clone(),
                env,
                "QWEN_CODEX_YOLO_REFINER_STYLE",
                None,
                DEFAULT_YOLO_REFINER_STYLE,
            ),
        };
        let continue_after_timeout = resolve_bool(
            overrides.yolo_continue_after_timeout,
            env,
            "QWEN_CODEX_YOLO_CONTINUE_AFTER_TIMEOUT",
            None,
            DEFAULT_YOLO_CONTINUE_AFTER_TIMEOUT,
        )?;
        let allow_refiner_stop = resolve_bool(
            overrides.yolo_allow_refiner_stop,
            env,
            "QWEN_CODEX_YOLO_ALLOW_REFINER_STOP",
            None,
            DEFAULT_YOLO_ALLOW_REFINER_STOP,
        )?;
        let verify_commands = split_verify_commands(&resolve_string(
            overrides.yolo_verify_commands.clone(),
            env,
            "QWEN_CODEX_YOLO_VERIFY_COMMANDS",
            None,
            "",
        ));
        let verify_timeout_secs = resolve_u64(
            overrides.yolo_verify_timeout_secs,
            env,
            "QWEN_CODEX_YOLO_VERIFY_TIMEOUT_SECS",
            None,
            DEFAULT_YOLO_VERIFY_TIMEOUT_SECS,
        )?;
        let acceptance_gate_enabled = resolve_bool(
            overrides.yolo_acceptance_gate,
            env,
            "QWEN_CODEX_YOLO_ACCEPTANCE_GATE",
            None,
            DEFAULT_YOLO_ACCEPTANCE_GATE,
        )?;
        let acceptance_commands = resolve_command_list(
            &overrides.yolo_acceptance_commands,
            env,
            "QWEN_CODEX_YOLO_ACCEPTANCE_COMMANDS",
        );
        let acceptance_max_seconds = resolve_u64(
            overrides.yolo_acceptance_max_seconds,
            env,
            "QWEN_CODEX_YOLO_ACCEPTANCE_MAX_SECONDS",
            None,
            DEFAULT_YOLO_ACCEPTANCE_MAX_SECONDS,
        )?;
        let reject_stop_on_failed_acceptance = resolve_bool(
            None,
            env,
            "QWEN_CODEX_YOLO_REJECT_STOP_ON_FAILED_ACCEPTANCE",
            None,
            DEFAULT_YOLO_REJECT_STOP_ON_FAILED_ACCEPTANCE,
        )?;

        Ok(Self {
            base_url,
            api_key,
            model,
            context_window,
            request_timeout_ms,
            log_level,
            reasoning_parser,
            tool_call_parser,
            auto_tool_choice,
            web_search_live: overrides.web_search_live,
            yolo: ResolvedYoloConfig {
                refiner_base_url,
                refiner_api_key,
                refiner_model,
                log_dir,
                default_iterations,
                round_timeout_secs,
                max_repeated_prompts,
                max_failures,
                round_budget,
                continue_after_timeout,
                allow_refiner_stop,
                verify_commands,
                verify_timeout_secs,
                acceptance_gate_enabled,
                acceptance_commands,
                acceptance_max_seconds,
                reject_stop_on_failed_acceptance,
            },
        })
    }

    pub fn codex_config_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        for override_value in self.codex_overrides() {
            args.push("-c".to_string());
            args.push(override_value);
        }
        args
    }

    pub fn codex_overrides(&self) -> Vec<String> {
        let mut overrides = vec![
            format!("model_provider={}", toml_string_literal(QWEN_PROVIDER_ID)),
            format!("model={}", toml_string_literal(&self.model)),
            format!("model_reasoning_effort={}", toml_string_literal("medium")),
            format!("model_context_window={}", self.context_window),
            format!(
                "model_auto_compact_token_limit={}",
                qwen_auto_compact_threshold(self.context_window)
            ),
            self.provider_override(),
        ];
        if self.web_search_live {
            overrides.push("web_search=\"live\"".to_string());
        }
        overrides
    }

    pub fn child_env(&self) -> Vec<(String, String)> {
        let mut env = vec![("QWEN_CODEX_API_KEY".to_string(), self.api_key.clone())];
        if std::env::var_os("RUST_LOG").is_none() {
            env.push(("RUST_LOG".to_string(), self.log_level.clone()));
        }
        env
    }

    fn provider_override(&self) -> String {
        format!(
            "model_providers.{QWEN_PROVIDER_ID}={{ name = {}, base_url = {}, env_key = \"QWEN_CODEX_API_KEY\", wire_api = \"responses\", request_max_retries = 0, stream_max_retries = 0, stream_idle_timeout_ms = {}, supports_websockets = false }}",
            toml_string_literal("Qwen local OpenAI-compatible"),
            toml_string_literal(&self.base_url),
            self.request_timeout_ms
        )
    }
}

pub fn qwen_auto_compact_threshold(context_window: u64) -> u64 {
    let eighty_percent = ((context_window as f64) * 0.80).floor() as u64;
    let reserve_4096 = context_window.saturating_sub(4_096);
    eighty_percent.min(reserve_4096)
}

fn resolve_string(
    cli: Option<String>,
    env: &dyn EnvSource,
    official: &str,
    alias: Option<&str>,
    default: &str,
) -> String {
    cli.and_then(non_empty)
        .or_else(|| env.get(official).and_then(non_empty))
        .or_else(|| alias.and_then(|name| env.get(name).and_then(non_empty)))
        .unwrap_or_else(|| default.to_string())
}

fn resolve_u64(
    cli: Option<u64>,
    env: &dyn EnvSource,
    official: &str,
    alias: Option<&str>,
    default: u64,
) -> anyhow::Result<u64> {
    match cli {
        Some(value) => Ok(value),
        None => resolve_optional_string(env, official, alias)
            .map(|value| parse_u64(official, &value))
            .transpose()
            .map(|value| value.unwrap_or(default)),
    }
}

fn resolve_u32(
    cli: Option<u32>,
    env: &dyn EnvSource,
    official: &str,
    alias: Option<&str>,
    default: u32,
) -> anyhow::Result<u32> {
    match cli {
        Some(value) => Ok(value),
        None => resolve_optional_string(env, official, alias)
            .map(|value| parse_u32(official, &value))
            .transpose()
            .map(|value| value.unwrap_or(default)),
    }
}

fn resolve_optional_u32(
    cli: Option<u32>,
    env: &dyn EnvSource,
    official: &str,
    alias: Option<&str>,
) -> anyhow::Result<Option<u32>> {
    match cli {
        Some(value) => Ok(Some(value)),
        None => resolve_optional_string(env, official, alias)
            .map(|value| parse_u32(official, &value))
            .transpose(),
    }
}

fn resolve_bool(
    cli: Option<bool>,
    env: &dyn EnvSource,
    official: &str,
    alias: Option<&str>,
    default: bool,
) -> anyhow::Result<bool> {
    match cli {
        Some(value) => Ok(value),
        None => resolve_optional_string(env, official, alias)
            .map(|value| parse_bool(official, &value))
            .transpose()
            .map(|value| value.unwrap_or(default)),
    }
}

fn resolve_optional_string(
    env: &dyn EnvSource,
    official: &str,
    alias: Option<&str>,
) -> Option<String> {
    env.get(official)
        .and_then(non_empty)
        .or_else(|| alias.and_then(|name| env.get(name).and_then(non_empty)))
}

fn resolve_command_list(cli: &[String], env: &dyn EnvSource, official: &str) -> Vec<String> {
    if !cli.is_empty() {
        return cli
            .iter()
            .filter_map(|value| non_empty(value.clone()))
            .collect();
    }
    split_verify_commands(&resolve_string(None, env, official, None, ""))
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
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

fn split_verify_commands(value: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escaped = false;

    for ch in value.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' if !in_single_quote => {
                current.push(ch);
                escaped = true;
            }
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
                current.push(ch);
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
                current.push(ch);
            }
            ';' if !in_single_quote && !in_double_quote => {
                if let Some(command) = non_empty(current.clone()) {
                    commands.push(command);
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if let Some(command) = non_empty(current) {
        commands.push(command);
    }
    commands
}

fn toml_string_literal(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn resolves_official_env_before_aliases() {
        let env = HashMap::from([
            (
                "QWEN_CODEX_BASE_URL".to_string(),
                "http://official/v1".to_string(),
            ),
            ("QWEN_BASE_URL".to_string(), "http://alias/v1".to_string()),
            ("QWEN_CODEX_MODEL".to_string(), "official-model".to_string()),
            ("QWEN_MODEL_NAME".to_string(), "alias-model".to_string()),
        ]);

        let config =
            ResolvedQwenConfig::from_env_source(&QwenCliOverrides::default(), &env).unwrap();

        assert_eq!(config.base_url, "http://official/v1");
        assert_eq!(config.model, "official-model");
    }

    #[test]
    fn cli_overrides_env_values() {
        let env = HashMap::from([(
            "QWEN_CODEX_BASE_URL".to_string(),
            "http://env/v1".to_string(),
        )]);
        let overrides = QwenCliOverrides {
            base_url: Some("http://cli/v1".to_string()),
            ..Default::default()
        };

        let config = ResolvedQwenConfig::from_env_source(&overrides, &env).unwrap();

        assert_eq!(config.base_url, "http://cli/v1");
    }

    #[test]
    fn builds_provider_override_without_embedding_api_key() {
        let overrides = QwenCliOverrides {
            api_key: Some("secret-key".to_string()),
            ..Default::default()
        };

        let config = ResolvedQwenConfig::from_env_source(&overrides, &HashMap::new()).unwrap();
        let overrides = config.codex_overrides().join("\n");

        assert!(overrides.contains("model_providers.qwen"));
        assert!(overrides.contains("env_key = \"QWEN_CODEX_API_KEY\""));
        assert!(!overrides.contains("secret-key"));
    }

    #[test]
    fn qwen_auto_compact_threshold_uses_safe_context_window_reserve() {
        assert_eq!(qwen_auto_compact_threshold(32_768), 26_214);
        assert_eq!(qwen_auto_compact_threshold(8_192), 4_096);
        assert_eq!(qwen_auto_compact_threshold(4_096), 0);
    }

    #[test]
    fn codex_overrides_include_context_window_and_compact_threshold() {
        let env = HashMap::from([("QWEN_CODEX_CONTEXT_WINDOW".to_string(), "32768".to_string())]);

        let config =
            ResolvedQwenConfig::from_env_source(&QwenCliOverrides::default(), &env).unwrap();
        let overrides = config.codex_overrides().join("\n");

        assert!(overrides.contains("model_context_window=32768"));
        assert!(overrides.contains("model_auto_compact_token_limit=26214"));
        assert!(overrides.contains("model_reasoning_effort=\"medium\""));
    }

    #[test]
    fn resolves_yolo_refiner_and_guard_config() {
        let env = HashMap::from([
            (
                "QWEN_CODEX_YOLO_REFINER_BASE_URL".to_string(),
                "http://refiner/v1".to_string(),
            ),
            (
                "QWEN_CODEX_YOLO_REFINER_API_KEY".to_string(),
                "refiner-key".to_string(),
            ),
            (
                "QWEN_CODEX_YOLO_REFINER_MODEL".to_string(),
                "refiner-model".to_string(),
            ),
            (
                "QWEN_CODEX_YOLO_LOG_DIR".to_string(),
                ".custom-yolo".to_string(),
            ),
            (
                "QWEN_CODEX_YOLO_DEFAULT_ITERATIONS".to_string(),
                "7".to_string(),
            ),
            (
                "QWEN_CODEX_YOLO_ROUND_TIMEOUT_SECS".to_string(),
                "42".to_string(),
            ),
            (
                "QWEN_CODEX_YOLO_MAX_REPEATED_PROMPTS".to_string(),
                "4".to_string(),
            ),
            ("QWEN_CODEX_YOLO_MAX_FAILURES".to_string(), "2".to_string()),
        ]);

        let config =
            ResolvedQwenConfig::from_env_source(&QwenCliOverrides::default(), &env).unwrap();

        assert_eq!(
            config.yolo,
            ResolvedYoloConfig {
                refiner_base_url: "http://refiner/v1".to_string(),
                refiner_api_key: "refiner-key".to_string(),
                refiner_model: "refiner-model".to_string(),
                log_dir: PathBuf::from(".custom-yolo"),
                default_iterations: Some(7),
                round_timeout_secs: 42,
                max_repeated_prompts: 4,
                max_failures: 2,
                round_budget: ResolvedYoloRoundBudget {
                    max_files: DEFAULT_YOLO_ROUND_GOAL_MAX_FILES,
                    max_actions: DEFAULT_YOLO_ROUND_GOAL_MAX_ACTIONS,
                    max_tests: DEFAULT_YOLO_ROUND_GOAL_MAX_TESTS,
                    max_next_prompt_chars: DEFAULT_YOLO_REFINER_MAX_PROMPT_CHARS,
                    style: DEFAULT_YOLO_REFINER_STYLE.to_string(),
                },
                continue_after_timeout: DEFAULT_YOLO_CONTINUE_AFTER_TIMEOUT,
                allow_refiner_stop: DEFAULT_YOLO_ALLOW_REFINER_STOP,
                verify_commands: Vec::new(),
                verify_timeout_secs: DEFAULT_YOLO_VERIFY_TIMEOUT_SECS,
                acceptance_gate_enabled: DEFAULT_YOLO_ACCEPTANCE_GATE,
                acceptance_commands: Vec::new(),
                acceptance_max_seconds: DEFAULT_YOLO_ACCEPTANCE_MAX_SECONDS,
                reject_stop_on_failed_acceptance: DEFAULT_YOLO_REJECT_STOP_ON_FAILED_ACCEPTANCE,
            }
        );
    }

    #[test]
    fn resolves_yolo_round_budget_config() {
        let env = HashMap::from([
            (
                "QWEN_CODEX_YOLO_ROUND_GOAL_MAX_FILES".to_string(),
                "6".to_string(),
            ),
            (
                "QWEN_CODEX_YOLO_ROUND_GOAL_MAX_ACTIONS".to_string(),
                "4".to_string(),
            ),
            (
                "QWEN_CODEX_YOLO_ROUND_GOAL_MAX_TESTS".to_string(),
                "2".to_string(),
            ),
            (
                "QWEN_CODEX_YOLO_REFINER_MAX_PROMPT_CHARS".to_string(),
                "1800".to_string(),
            ),
            (
                "QWEN_CODEX_YOLO_REFINER_STYLE".to_string(),
                "incremental".to_string(),
            ),
            (
                "QWEN_CODEX_YOLO_CONTINUE_AFTER_TIMEOUT".to_string(),
                "true".to_string(),
            ),
            (
                "QWEN_CODEX_YOLO_ALLOW_REFINER_STOP".to_string(),
                "true".to_string(),
            ),
            (
                "QWEN_CODEX_YOLO_VERIFY_COMMANDS".to_string(),
                "echo ok;sh -c 'exit 7'".to_string(),
            ),
            (
                "QWEN_CODEX_YOLO_VERIFY_TIMEOUT_SECS".to_string(),
                "9".to_string(),
            ),
            (
                "QWEN_CODEX_YOLO_ACCEPTANCE_GATE".to_string(),
                "true".to_string(),
            ),
            (
                "QWEN_CODEX_YOLO_ACCEPTANCE_COMMANDS".to_string(),
                "docker compose config;curl -sf http://localhost:2225".to_string(),
            ),
            (
                "QWEN_CODEX_YOLO_ACCEPTANCE_MAX_SECONDS".to_string(),
                "120".to_string(),
            ),
            (
                "QWEN_CODEX_YOLO_REJECT_STOP_ON_FAILED_ACCEPTANCE".to_string(),
                "false".to_string(),
            ),
        ]);

        let config =
            ResolvedQwenConfig::from_env_source(&QwenCliOverrides::default(), &env).unwrap();

        assert_eq!(
            config.yolo.round_budget,
            ResolvedYoloRoundBudget {
                max_files: 6,
                max_actions: 4,
                max_tests: 2,
                max_next_prompt_chars: 1800,
                style: "incremental".to_string(),
            }
        );
        assert!(config.yolo.continue_after_timeout);
        assert!(config.yolo.allow_refiner_stop);
        assert_eq!(
            config.yolo.verify_commands,
            vec!["echo ok".to_string(), "sh -c 'exit 7'".to_string()]
        );
        assert_eq!(config.yolo.verify_timeout_secs, 9);
        assert!(config.yolo.acceptance_gate_enabled);
        assert_eq!(
            config.yolo.acceptance_commands,
            vec![
                "docker compose config".to_string(),
                "curl -sf http://localhost:2225".to_string()
            ]
        );
        assert_eq!(config.yolo.acceptance_max_seconds, 120);
        assert!(!config.yolo.reject_stop_on_failed_acceptance);
    }

    #[test]
    fn yolo_optional_safety_options_use_documented_defaults() {
        let config =
            ResolvedQwenConfig::from_env_source(&QwenCliOverrides::default(), &HashMap::new())
                .unwrap();

        assert!(!config.yolo.continue_after_timeout);
        assert!(config.yolo.allow_refiner_stop);
        assert!(config.yolo.verify_commands.is_empty());
        assert_eq!(
            config.yolo.verify_timeout_secs,
            DEFAULT_YOLO_VERIFY_TIMEOUT_SECS
        );
        assert!(!config.yolo.acceptance_gate_enabled);
        assert!(config.yolo.acceptance_commands.is_empty());
        assert_eq!(
            config.yolo.acceptance_max_seconds,
            DEFAULT_YOLO_ACCEPTANCE_MAX_SECONDS
        );
        assert!(config.yolo.reject_stop_on_failed_acceptance);
    }

    #[test]
    fn cli_acceptance_commands_override_env_commands() {
        let env = HashMap::from([(
            "QWEN_CODEX_YOLO_ACCEPTANCE_COMMANDS".to_string(),
            "echo env".to_string(),
        )]);
        let overrides = QwenCliOverrides {
            yolo_acceptance_commands: vec!["echo cli".to_string(), "sh -c 'echo a;b'".to_string()],
            ..Default::default()
        };

        let config = ResolvedQwenConfig::from_env_source(&overrides, &env).unwrap();

        assert_eq!(
            config.yolo.acceptance_commands,
            vec!["echo cli".to_string(), "sh -c 'echo a;b'".to_string()]
        );
    }

    #[test]
    fn yolo_iterations_are_unlimited_when_unset() {
        let config =
            ResolvedQwenConfig::from_env_source(&QwenCliOverrides::default(), &HashMap::new())
                .unwrap();

        assert_eq!(config.yolo.default_iterations, None);
    }

    #[test]
    fn yolo_zero_iterations_is_invalid() {
        let env = HashMap::from([(
            "QWEN_CODEX_YOLO_DEFAULT_ITERATIONS".to_string(),
            "0".to_string(),
        )]);

        let err = ResolvedQwenConfig::from_env_source(&QwenCliOverrides::default(), &env)
            .expect_err("zero iteration limit should be invalid");

        assert!(err.to_string().contains("unset it for infinite YOLO mode"));
    }

    #[test]
    fn split_verify_commands_respects_basic_quotes() {
        assert_eq!(
            split_verify_commands("echo ok;sh -c 'echo a;b';curl -sf \"http://x?a=b;c=d\""),
            vec![
                "echo ok".to_string(),
                "sh -c 'echo a;b'".to_string(),
                "curl -sf \"http://x?a=b;c=d\"".to_string(),
            ]
        );
    }

    #[test]
    fn yolo_refiner_config_falls_back_to_normal_qwen_config() {
        let env = HashMap::from([
            (
                "QWEN_CODEX_BASE_URL".to_string(),
                "http://normal/v1".to_string(),
            ),
            ("QWEN_CODEX_API_KEY".to_string(), "normal-key".to_string()),
            ("QWEN_CODEX_MODEL".to_string(), "normal-model".to_string()),
        ]);

        let config =
            ResolvedQwenConfig::from_env_source(&QwenCliOverrides::default(), &env).unwrap();

        assert_eq!(config.yolo.refiner_base_url, "http://normal/v1");
        assert_eq!(config.yolo.refiner_api_key, "normal-key");
        assert_eq!(config.yolo.refiner_model, "normal-model");
    }
}
