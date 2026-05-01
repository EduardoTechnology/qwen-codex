use std::time::Duration;

use anyhow::Context;
use reqwest::header::AUTHORIZATION;
use reqwest::header::CONTENT_TYPE;
use serde::Deserialize;
use serde::Serialize;

use crate::config::ResolvedQwenConfig;
use crate::yolo::types::RefinerRequest;
use crate::yolo::types::RefinerResponse;

pub(crate) const YOLO_STOP: &str = "YOLO_STOP";

const DEFAULT_REFINER_SYSTEM_PROMPT: &str = r#"You are a senior product and engineering refinement strategist supervising an autonomous coding agent.

You receive the previous round's user goal, agent actions, files changed, tests, errors, and current repository state.

Your job is to produce the next high-leverage instruction for the coding agent.

Improve:
- product quality
- correctness
- maintainability
- tests
- documentation
- developer experience
- release readiness
- security
- error handling
- architecture

Rules:
- Output only the next prompt for the coding agent.
- Be concrete and actionable.
- Do not repeat completed work.
- Prefer small, verifiable next steps.
- Include tests to run.
- Reference files/functions when possible.
- Keep the prompt concise but sufficiently detailed.
- If the project appears complete, ask the agent to perform final verification, cleanup, release notes, and documentation review.
- If no further useful work remains, output: YOLO_STOP"#;

#[derive(Clone)]
pub(crate) struct RefinerClient {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl RefinerClient {
    pub(crate) fn new(config: &ResolvedQwenConfig) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.request_timeout_ms))
            .build()
            .context("failed to build YOLO refiner HTTP client")?;
        Ok(Self {
            client,
            base_url: config.yolo.refiner_base_url.clone(),
            api_key: config.yolo.refiner_api_key.clone(),
            model: config.yolo.refiner_model.clone(),
        })
    }

    pub(crate) async fn refine(&self, request: RefinerRequest) -> anyhow::Result<RefinerResponse> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let response = self
            .client
            .post(url)
            .header(CONTENT_TYPE, "application/json")
            .header(AUTHORIZATION, format!("Bearer {}", self.api_key))
            .json(&ChatCompletionRequest {
                model: self.model.clone(),
                messages: vec![
                    ChatMessage {
                        role: "system",
                        content: DEFAULT_REFINER_SYSTEM_PROMPT,
                    },
                    ChatMessage {
                        role: "user",
                        content: &request.summary,
                    },
                ],
                temperature: 0.2,
                max_tokens: 1_024,
            })
            .send()
            .await
            .context("failed to call YOLO refiner model")?;

        let status = response.status();
        let body = response
            .text()
            .await
            .context("failed to read YOLO refiner response body")?;
        if !status.is_success() {
            anyhow::bail!("YOLO refiner returned HTTP {status}: {body}");
        }

        let parsed = serde_json::from_str::<ChatCompletionResponse>(&body)
            .with_context(|| format!("failed to parse YOLO refiner response: {body}"))?;
        let next_prompt = parsed
            .choices
            .first()
            .map(|choice| choice.message.content.trim().to_string())
            .filter(|content| !content.is_empty())
            .context("YOLO refiner response did not include message content")?;

        Ok(RefinerResponse {
            raw_response: body,
            next_prompt,
        })
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest<'a> {
    model: String,
    messages: Vec<ChatMessage<'a>>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Debug, Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChatChoiceMessage {
    content: String,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use pretty_assertions::assert_eq;
    use serde_json::json;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::header;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    use crate::args::QwenCliOverrides;
    use crate::config::ResolvedQwenConfig;

    use super::*;

    #[tokio::test]
    async fn refiner_client_calls_chat_completions() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer refiner-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"message": {"content": "Run tests next."}}]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let env = HashMap::from([
            (
                "QWEN_CODEX_YOLO_REFINER_BASE_URL".to_string(),
                format!("{}/v1", server.uri()),
            ),
            (
                "QWEN_CODEX_YOLO_REFINER_API_KEY".to_string(),
                "refiner-key".to_string(),
            ),
        ]);
        let config =
            ResolvedQwenConfig::from_env_source(&QwenCliOverrides::default(), &env).unwrap();
        let client = RefinerClient::new(&config).unwrap();

        let response = client
            .refine(RefinerRequest {
                iteration: 1,
                summary: "summary".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(response.next_prompt, "Run tests next.");
    }
}
