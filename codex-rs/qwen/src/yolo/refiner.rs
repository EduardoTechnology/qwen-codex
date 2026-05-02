use std::time::Duration;

use anyhow::Context;
use reqwest::header::AUTHORIZATION;
use reqwest::header::CONTENT_TYPE;
use serde::Deserialize;
use serde::Serialize;

use crate::config::ResolvedQwenConfig;
use crate::redaction::redact_text;
use crate::yolo::agent::tail;
use crate::yolo::types::RefinerFailureDiagnostic;
use crate::yolo::types::RefinerRequest;
use crate::yolo::types::RefinerResponse;

pub(crate) const YOLO_STOP: &str = "YOLO_STOP";

const REFINER_MAX_TOKENS: u32 = 512;
const REFINER_TEMPERATURE: f32 = 0.2;
const REFINER_DIAGNOSTIC_BODY_LIMIT: usize = 4_000;
const REFINER_DIAGNOSTIC_REQUEST_LIMIT: usize = 1_000;

const DEFAULT_REFINER_SYSTEM_PROMPT: &str = r#"You are a senior product and engineering refinement strategist supervising an autonomous coding agent.

You receive the previous round's user goal, agent actions, files changed, tests, errors, and current repository state.

Your job is to produce the next high-leverage instruction for the coding agent as a small, bounded product/engineering iteration.

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
- Prefer one small, verifiable next step over broad project completion.
- Prefer fixing build/runtime blockers and making the project runnable before expanding scope.
- Respect the round budget in the user context for maximum files, actions, tests, and prompt length.
- Include explicit acceptance criteria.
- Include exact verification commands to run.
- Tell the coding agent to stop after the scoped task is complete and verification is summarized.
- Reference files/functions when possible.
- Keep the prompt concise but sufficiently detailed.
- Avoid broad instructions such as "finish everything", "implement all remaining features", "do everything", or "continue until complete".
- If the previous round timed out, generate a smaller retry prompt focused on one blocker.
- If an acceptance criterion was not verified, treat it as incomplete.
- If a verification command was not run, treat it as incomplete.
- If external verification failed, target that failure in the next prompt.
- If acceptance-gate checks failed or are missing, target that failure in the next prompt.
- Do not add unrelated features while runtime acceptance criteria fail.
- If Docker config/build failed, ask for the smallest fix for that failure before adding features.
- For Docker/web projects, frontend HTTP 500, backend health failure, products endpoint failure, Docker build failure, Docker Compose config failure, missing README/run instructions, missing required endpoints, and browser-unreachable service hostnames are blockers.
- Browser JavaScript running on the host cannot fetch http://backend:8000. Use a host-reachable URL such as http://localhost:2226, a relative/proxy URL, or documented environment configuration.
- Use this structure exactly:
  Title:
  One concise objective.

  Context:
  Brief current state and known blocker.

  Task:
  One bounded task for this round.

  Constraints:
  Budget and project constraints.

  Acceptance criteria:
  Three to six concrete checks.

  Verification commands:
  Exact commands to run.

  Stop condition:
  Stop condition for the agent's current round only, not for YOLO mode.
- Emit YOLO_STOP only when the original goal, explicit acceptance criteria, verification commands, and acceptance-gate checks are known to have passed and there are no known runtime failures or TODO blockers.
- If acceptance-gate results are missing, failed, or disabled, return a focused next-prompt instead of YOLO_STOP.
- Your role is to inspect the latest round, identify the most impactful remaining improvement, and produce a focused prompt for the next round unless acceptance evidence proves the project is complete."#;

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
        let diagnostic = self.failure_diagnostic(&url, &request.summary, None, None);
        let response = self
            .client
            .post(&url)
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
                temperature: REFINER_TEMPERATURE,
                max_tokens: REFINER_MAX_TOKENS,
            })
            .send()
            .await
            .map_err(|err| {
                RefinerClientError::new(
                    format!("failed to call YOLO refiner model: {err}"),
                    diagnostic.clone(),
                )
            })?;

        let status = response.status();
        let body = response
            .text()
            .await
            .context("failed to read YOLO refiner response body")?;
        if !status.is_success() {
            return Err(RefinerClientError::new(
                format!(
                    "YOLO refiner returned HTTP {status}: {}",
                    tail(&body, 1_000)
                ),
                self.failure_diagnostic(&url, &request.summary, Some(status.as_u16()), Some(&body)),
            )
            .into());
        }

        let parsed = serde_json::from_str::<ChatCompletionResponse>(&body).map_err(|err| {
            RefinerClientError::new(
                format!(
                    "failed to parse YOLO refiner response: {err}: {}",
                    tail(&body, 1_000)
                ),
                self.failure_diagnostic(&url, &request.summary, Some(status.as_u16()), Some(&body)),
            )
        })?;
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

    fn failure_diagnostic(
        &self,
        endpoint: &str,
        request_summary: &str,
        http_status: Option<u16>,
        response_body: Option<&str>,
    ) -> RefinerFailureDiagnostic {
        RefinerFailureDiagnostic {
            endpoint: endpoint.to_string(),
            http_status,
            response_body: response_body
                .map(|body| redact_text(&tail(body, REFINER_DIAGNOSTIC_BODY_LIMIT))),
            request_summary_chars: request_summary.chars().count(),
            request_summary_preview: redact_text(&tail(
                request_summary,
                REFINER_DIAGNOSTIC_REQUEST_LIMIT,
            )),
            message_roles: vec!["system".to_string(), "user".to_string()],
            max_tokens: REFINER_MAX_TOKENS,
            temperature: REFINER_TEMPERATURE.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RefinerClientError {
    message: String,
    diagnostic: RefinerFailureDiagnostic,
}

impl RefinerClientError {
    fn new(message: String, diagnostic: RefinerFailureDiagnostic) -> Self {
        Self {
            message: redact_text(&message),
            diagnostic,
        }
    }

    pub(crate) fn diagnostic(&self) -> RefinerFailureDiagnostic {
        self.diagnostic.clone()
    }
}

impl std::fmt::Display for RefinerClientError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(formatter)
    }
}

impl std::error::Error for RefinerClientError {}

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

    #[tokio::test]
    async fn refiner_client_reports_sanitized_http_failure_diagnostics() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error": {
                    "message": "context limit exceeded with Authorization: Bearer sk_test_123456789abcdef",
                    "type": "BadRequestError"
                }
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

        let err = client
            .refine(RefinerRequest {
                iteration: 1,
                summary: "Authorization: Bearer sk_test_123456789abcdef".to_string(),
            })
            .await
            .unwrap_err();

        let err = err.downcast_ref::<RefinerClientError>().unwrap();
        let diagnostic = err.diagnostic();
        assert_eq!(diagnostic.http_status, Some(400));
        assert_eq!(diagnostic.message_roles, vec!["system", "user"]);
        assert_eq!(diagnostic.max_tokens, REFINER_MAX_TOKENS);
        assert!(diagnostic.endpoint.ends_with("/v1/chat/completions"));
        assert!(diagnostic.response_body.unwrap().contains("[REDACTED]"));
        assert!(diagnostic.request_summary_preview.contains("[REDACTED]"));
    }
}
