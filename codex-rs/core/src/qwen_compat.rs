use codex_api::Provider;
use codex_protocol::models::ContentItem;
use codex_protocol::models::MessagePhase;
use codex_protocol::models::ReasoningItemContent;
use codex_protocol::models::ResponseItem;

const QWEN_PROVIDER_NAME: &str = "Qwen local OpenAI-compatible";
const QWEN_REASONING_ONLY_WARNING: &str = "Qwen Responses API returned reasoning-only output without a final assistant message. Verify vLLM is started with --default-chat-template-kwargs '{\"enable_thinking\": false}' and use a vLLM build/model whose Responses API honors Qwen chat template kwargs.";

pub(crate) fn apply_qwen_responses_compat(
    provider: &Provider,
    instructions: &mut String,
    input: &mut Vec<ResponseItem>,
) {
    if !requires_developer_role_compat(provider) {
        return;
    }

    let developer_messages = take_developer_messages(input);
    if developer_messages.is_empty() {
        return;
    }

    let developer_text = developer_messages.join("\n\n");
    if instructions.trim().is_empty() {
        *instructions = developer_text;
    } else {
        instructions.push_str("\n\n");
        instructions.push_str(&developer_text);
    }
}

fn requires_developer_role_compat(provider: &Provider) -> bool {
    is_qwen_provider_name(&provider.name)
}

pub(crate) fn is_qwen_provider_name(name: &str) -> bool {
    name.eq_ignore_ascii_case(QWEN_PROVIDER_NAME)
}

fn take_developer_messages(input: &mut Vec<ResponseItem>) -> Vec<String> {
    let mut developer_messages = Vec::new();
    input.retain(|item| match item {
        ResponseItem::Message { role, content, .. } if role == "developer" => {
            developer_messages.push(content_to_text(content));
            false
        }
        _ => true,
    });
    developer_messages
}

fn content_to_text(content: &[ContentItem]) -> String {
    content
        .iter()
        .map(|item| match item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => text.clone(),
            ContentItem::InputImage { image_url, .. } => format!("[image: {image_url}]"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn qwen_reasoning_text_from_item(item: &ResponseItem) -> Option<String> {
    let ResponseItem::Reasoning {
        content: Some(content),
        ..
    } = item
    else {
        return None;
    };

    let text = content
        .iter()
        .map(|item| match item {
            ReasoningItemContent::ReasoningText { text } | ReasoningItemContent::Text { text } => {
                text.as_str()
            }
        })
        .collect::<String>();
    (!text.trim().is_empty()).then_some(text)
}

pub(crate) fn synthesize_qwen_reasoning_only_message(reasoning_text: &str) -> Option<ResponseItem> {
    if reasoning_text.trim().is_empty() {
        return None;
    }
    let text = extract_qwen_final_answer_from_reasoning(reasoning_text)
        .unwrap_or_else(|| QWEN_REASONING_ONLY_WARNING.to_string());
    Some(ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText { text }],
        phase: Some(MessagePhase::FinalAnswer),
    })
}

fn extract_qwen_final_answer_from_reasoning(reasoning_text: &str) -> Option<String> {
    let markers = [
        "Construct Output:",
        "Final Answer:",
        "Final answer:",
        "Output:",
    ];
    for marker in markers {
        if let Some((_, after_marker)) = reasoning_text.rsplit_once(marker)
            && let Some(answer) = first_clean_answer_line(after_marker)
        {
            return Some(answer);
        }
    }
    extract_after_last_pattern(reasoning_text, "answer is")
        .or_else(|| extract_after_last_pattern(reasoning_text, "Answer is"))
        .or_else(|| extract_after_last_pattern(reasoning_text, "="))
}

fn first_clean_answer_line(text: &str) -> Option<String> {
    text.lines().find_map(clean_answer_line)
}

fn clean_answer_line(line: &str) -> Option<String> {
    let trimmed = line
        .trim()
        .trim_start_matches(['*', '-'])
        .trim()
        .trim_matches('`')
        .trim();
    (!trimmed.is_empty() && !trimmed.ends_with(':')).then(|| trimmed.to_string())
}

fn extract_after_last_pattern(reasoning_text: &str, pattern: &str) -> Option<String> {
    let (_, after_pattern) = reasoning_text.rsplit_once(pattern)?;
    let line = after_pattern
        .trim()
        .trim_start_matches([':', '-', '>'])
        .trim()
        .lines()
        .next()?;
    let candidates = line
        .split_whitespace()
        .filter_map(clean_pattern_answer_token)
        .collect::<Vec<_>>();
    if let Some(numeric) = candidates.iter().rev().find(|candidate| {
        candidate.chars().any(|ch| ch.is_ascii_digit())
            && candidate
                .chars()
                .all(|ch| ch.is_ascii_digit() || matches!(ch, '.' | '-' | '/' | '+'))
    }) {
        return Some(numeric.clone());
    }
    candidates
        .into_iter()
        .find(|candidate| !matches!(candidate.as_str(), "simply" | "just" | "only"))
}

fn clean_pattern_answer_token(token: &str) -> Option<String> {
    let candidate = token
        .trim_matches(|ch: char| {
            ch == '`'
                || ch == '"'
                || ch == '\''
                || ch == '.'
                || ch == ','
                || ch == ';'
                || ch == ':'
                || ch == ')'
                || ch == ']'
        })
        .trim();
    let is_reasonable = !candidate.is_empty()
        && candidate.len() <= 40
        && candidate
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_' | '/'));
    is_reasonable.then(|| candidate.to_string())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use codex_api::RetryConfig;
    use http::HeaderMap;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn moves_developer_messages_into_instructions_for_qwen_provider() {
        let provider = qwen_provider();
        let mut instructions = "base instructions".to_string();
        let mut input = vec![
            message("developer", "developer one"),
            message("user", "hello"),
            message("developer", "developer two"),
        ];

        apply_qwen_responses_compat(&provider, &mut instructions, &mut input);

        assert_eq!(
            instructions,
            "base instructions\n\ndeveloper one\n\ndeveloper two"
        );
        assert_eq!(input, vec![message("user", "hello")]);
    }

    #[test]
    fn leaves_non_qwen_provider_unchanged() {
        let provider = Provider {
            name: "OpenAI".to_string(),
            ..qwen_provider()
        };
        let mut instructions = "base instructions".to_string();
        let mut input = vec![
            message("developer", "developer one"),
            message("user", "hello"),
        ];

        apply_qwen_responses_compat(&provider, &mut instructions, &mut input);

        assert_eq!(instructions, "base instructions");
        assert_eq!(
            input,
            vec![
                message("developer", "developer one"),
                message("user", "hello")
            ]
        );
    }

    #[test]
    fn extracts_final_answer_from_qwen_reasoning_marker() {
        let item = synthesize_qwen_reasoning_only_message(
            "Thinking Process:\n\n5. Construct Output:\n    4\n",
        )
        .expect("message");

        assert_eq!(
            item,
            ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: "4".to_string()
                }],
                phase: Some(MessagePhase::FinalAnswer),
            }
        );
    }

    #[test]
    fn returns_warning_for_qwen_reasoning_without_final_answer() {
        let item = synthesize_qwen_reasoning_only_message("Thinking Process:\n- still thinking")
            .expect("message");

        assert_eq!(
            item,
            ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: QWEN_REASONING_ONLY_WARNING.to_string()
                }],
                phase: Some(MessagePhase::FinalAnswer),
            }
        );
    }

    #[test]
    fn extracts_short_answer_from_arithmetic_reasoning() {
        let item =
            synthesize_qwen_reasoning_only_message("Calculate the Answer: 2 + 2 = 4.").unwrap();

        assert_eq!(
            item,
            ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: "4".to_string()
                }],
                phase: Some(MessagePhase::FinalAnswer),
            }
        );
    }

    #[test]
    fn extracts_numeric_answer_after_filler_word() {
        let item =
            synthesize_qwen_reasoning_only_message("The answer is simply 4.").expect("message");

        assert_eq!(
            item,
            ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: "4".to_string()
                }],
                phase: Some(MessagePhase::FinalAnswer),
            }
        );
    }

    fn qwen_provider() -> Provider {
        Provider {
            name: QWEN_PROVIDER_NAME.to_string(),
            base_url: "http://127.0.0.1:8002/v1".to_string(),
            query_params: None,
            headers: HeaderMap::new(),
            retry: RetryConfig {
                max_attempts: 1,
                base_delay: Duration::from_millis(1),
                retry_429: false,
                retry_5xx: false,
                retry_transport: false,
            },
            stream_idle_timeout: Duration::from_millis(1),
        }
    }

    fn message(role: &str, text: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: role.to_string(),
            content: vec![ContentItem::InputText {
                text: text.to_string(),
            }],
            phase: None,
        }
    }
}
