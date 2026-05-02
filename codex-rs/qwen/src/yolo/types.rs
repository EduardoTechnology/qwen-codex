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
    pub round_budget: RoundBudget,
    pub continue_after_timeout: bool,
    pub allow_refiner_stop: bool,
    pub verify_commands: Vec<String>,
    pub verify_timeout_secs: u64,
    pub acceptance_gate_enabled: bool,
    pub acceptance_commands: Vec<String>,
    pub acceptance_max_seconds: u64,
    pub reject_stop_on_failed_acceptance: bool,
    pub max_repeated_prompts: u32,
    pub max_failures: u32,
    pub base_url: String,
    pub model: String,
    pub interrupt_flag: Option<Arc<AtomicBool>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoundBudget {
    pub max_files: u32,
    pub max_actions: u32,
    pub max_tests: u32,
    pub max_next_prompt_chars: u32,
    pub style: String,
}

impl Default for RoundBudget {
    fn default() -> Self {
        Self {
            max_files: 8,
            max_actions: 5,
            max_tests: 3,
            max_next_prompt_chars: 3_000,
            style: "incremental".to_string(),
        }
    }
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
    pub exit_signal: Option<String>,
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExternalVerificationResult {
    pub command: String,
    pub exit_code: Option<i32>,
    pub output: String,
    pub duration_ms: u128,
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
    #[serde(rename = "max_iterations")]
    IterationLimitReached,
    RefinerStopSignal,
    RepeatedPromptGuard,
    FailureGuard,
    RefinerError,
    RoundTimeout,
    Interrupted,
    AgentError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RefinerSkippedReason {
    Interrupted,
    Timeout,
    AgentError,
    MissingAgentOutput,
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
    pub round_budget: RoundBudget,
    pub continue_after_timeout: bool,
    pub allow_refiner_stop: bool,
    pub verify_commands: Vec<String>,
    pub verify_timeout_secs: u64,
    pub acceptance_gate_enabled: bool,
    pub acceptance_commands: Vec<String>,
    pub acceptance_max_seconds: u64,
    pub reject_stop_on_failed_acceptance: bool,
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
    pub external_verification: Vec<ExternalVerificationResult>,
    pub acceptance_gate_enabled: bool,
    pub acceptance_commands: Vec<String>,
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
    pub current_git_status: String,
    pub interrupt_received: bool,
    pub timeout_occurred: bool,
    pub agent_process_exit_code: Option<i32>,
    pub agent_process_signal: Option<String>,
    pub agent_round_duration_seconds: u64,
    pub agent_finished_normally: bool,
    pub refiner_skipped_reason: Option<RefinerSkippedReason>,
    #[serde(rename = "refinerRequestSummary")]
    pub refiner_input_summary: Option<String>,
    #[serde(rename = "refinerResponse")]
    pub refiner_raw_response: Option<String>,
    pub refiner_error: Option<RefinerFailureDiagnostic>,
    #[serde(rename = "nextPrompt")]
    pub next_prompt_injected_into_agent: Option<String>,
    pub next_prompt_validation_passed: Option<bool>,
    pub next_prompt_validation_issues: Vec<String>,
    pub next_prompt_repaired: bool,
    pub round_budget: RoundBudget,
    pub stop_reason: Option<YoloStopReason>,
}
