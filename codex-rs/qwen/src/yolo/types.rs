use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone)]
pub(crate) struct YoloLoopConfig {
    pub original_prompt: String,
    pub iteration_limit: Option<u32>,
    pub log_root: PathBuf,
    pub round_timeout_secs: u64,
    pub max_repeated_prompts: u32,
    pub max_failures: u32,
    pub base_url: String,
    pub model: String,
    pub interrupt_flag: Option<Arc<AtomicBool>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentRoundRequest {
    pub iteration: u32,
    pub prompt: String,
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AgentRoundResult {
    pub session_id: Option<String>,
    pub exit_code: Option<i32>,
    pub final_response: Option<String>,
    pub actions_taken: Vec<String>,
    pub tool_calls: Vec<String>,
    pub changed_files: Vec<String>,
    pub commands_tests_run: Vec<String>,
    pub errors: Vec<String>,
    pub stdout_tail: String,
    pub stderr_tail: String,
    pub timed_out: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RefinerRequest {
    pub iteration: u32,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RefinerResponse {
    pub raw_response: String,
    pub next_prompt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RefinerFailureDiagnostic {
    pub endpoint: String,
    pub http_status: Option<u16>,
    pub response_body: Option<String>,
    pub request_summary_chars: usize,
    pub request_summary_preview: String,
    pub message_roles: Vec<String>,
    pub max_tokens: u32,
    pub temperature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum YoloStopReason {
    IterationLimitReached,
    RefinerStopSignal,
    RepeatedPromptGuard,
    FailureGuard,
    RefinerError,
    RoundTimeout,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct YoloRunOutcome {
    pub run_id: String,
    pub run_dir: PathBuf,
    pub session_id: Option<String>,
    pub iterations_completed: u32,
    pub stop_reason: YoloStopReason,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct YoloRunLog {
    pub run_id: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub original_prompt: String,
    pub base_url: String,
    pub model: String,
    pub iteration_limit: Option<u32>,
    pub max_repeated_prompts: u32,
    pub max_failures: u32,
    pub session_id: Option<String>,
    pub stop_reason: Option<YoloStopReason>,
    pub iterations: Vec<YoloIterationLog>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct YoloIterationLog {
    pub run_id: String,
    pub session_id: Option<String>,
    pub iteration: u32,
    pub timestamp: String,
    pub original_user_prompt: String,
    #[serde(rename = "agentInputPrompt")]
    pub current_agent_input_prompt: String,
    pub agent_output_summary: String,
    pub actions_taken: Vec<String>,
    pub tool_calls_summary: Vec<String>,
    #[serde(rename = "filesChanged")]
    pub changed_files: Vec<String>,
    pub git_diff_summary: String,
    pub commands_tests_run: Vec<String>,
    pub errors: Vec<String>,
    pub current_git_status: String,
    #[serde(rename = "refinerRequestSummary")]
    pub refiner_input_summary: Option<String>,
    #[serde(rename = "refinerResponse")]
    pub refiner_raw_response: Option<String>,
    pub refiner_error: Option<RefinerFailureDiagnostic>,
    #[serde(rename = "nextPrompt")]
    pub next_prompt_injected_into_agent: Option<String>,
    pub stop_reason: Option<YoloStopReason>,
}
