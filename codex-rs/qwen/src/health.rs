use std::time::Duration;

use anyhow::Context;

use crate::config::ResolvedQwenConfig;
use crate::redact_text;

pub async fn run_health_check(config: &ResolvedQwenConfig) -> anyhow::Result<()> {
    let url = models_url(&config.base_url);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(config.request_timeout_ms))
        .build()
        .context("failed to build Qwen health check client")?;

    let response = client
        .get(&url)
        .bearer_auth(&config.api_key)
        .send()
        .await
        .with_context(|| {
            format!(
                "failed to connect to local Qwen OpenAI-compatible server at {url}; \
                 check that vLLM is running and that QWEN_CODEX_BASE_URL is correct"
            )
        })?;

    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read Qwen health check response body")?;

    if !status.is_success() {
        anyhow::bail!(
            "Qwen health check failed: GET {url} returned {status}\n{}",
            redact_text(&body)
        );
    }

    println!("Qwen Codex health check OK");
    println!("GET {url}");
    println!("{}", redact_text(&body));
    Ok(())
}

fn models_url(base_url: &str) -> String {
    format!("{}/models", base_url.trim_end_matches('/'))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn builds_models_url_from_openai_compatible_base() {
        assert_eq!(
            models_url("http://127.0.0.1:8002/v1/"),
            "http://127.0.0.1:8002/v1/models"
        );
    }
}
