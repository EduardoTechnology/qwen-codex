mod args;
pub mod config;
mod health;
mod redaction;
mod runner;
mod yolo;

use codex_arg0::Arg0DispatchPaths;

pub const QWEN_ENTRYPOINT_ENV: &str = "QWEN_CODEX_ENTRYPOINT";

pub fn env_requests_qwen_entrypoint() -> bool {
    std::env::var(QWEN_ENTRYPOINT_ENV)
        .ok()
        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
}

pub async fn run_qwen_entrypoint(arg0_paths: Arg0DispatchPaths) -> anyhow::Result<()> {
    runner::run_qwen_entrypoint(arg0_paths).await
}

pub use redaction::redact_text;
