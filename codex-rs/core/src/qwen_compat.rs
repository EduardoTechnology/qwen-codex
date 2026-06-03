use codex_api::Provider;
use codex_protocol::models::ContentItem;
use codex_protocol::models::MessagePhase;
use codex_protocol::models::ReasoningItemContent;
use codex_protocol::models::ResponseItem;

const QWEN_PROVIDER_NAME: &str = "Qwen local OpenAI-compatible";
const QWEN_REASONING_ONLY_WARNING: &str = "Qwen Responses API returned reasoning-only output without a final assistant message. Verify vLLM is started with --default-chat-template-kwargs '{\"enable_thinking\": false}' and use a vLLM build/model whose Responses API honors Qwen chat template kwargs.";
const QWEN_TOOL_OUTPUT_ONLY_WARNING: &str =
    "Qwen Responses API returned tool output but no final assistant message.";
const QWEN_TOOL_OUTPUT_FALLBACK_MAX_CHARS: usize = 4_000;

pub(crate) fn apply_qwen_responses_compat(
    provider: &Provider,
    instructions: &mut String,
    input: &mut Vec<ResponseItem>,
) {
    if !requires_developer_role_compat(provider) {
        return;
    }

    remove_vllm_incompatible_history_items(input);
    normalize_message_content_for_vllm(input);
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

fn remove_vllm_incompatible_history_items(input: &mut Vec<ResponseItem>) {
    input.retain_mut(|item| match item {
        ResponseItem::Reasoning { .. } => false,
        ResponseItem::Message {
            role,
            content,
            phase,
            ..
        } => {
            *phase = None;
            !(role == "assistant" && (content.is_empty() || is_qwen_warning_message(content)))
        }
        _ => true,
    });
}

fn normalize_message_content_for_vllm(input: &mut [ResponseItem]) {
    for item in input {
        let ResponseItem::Message { content, .. } = item else {
            continue;
        };
        for content_item in content {
            if let ContentItem::OutputText { text } = content_item {
                *content_item = ContentItem::InputText { text: text.clone() };
            }
        }
    }
}

fn is_qwen_warning_message(content: &[ContentItem]) -> bool {
    matches!(
        content,
        [ContentItem::OutputText { text } | ContentItem::InputText { text }]
            if text == QWEN_REASONING_ONLY_WARNING
    )
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

pub(crate) fn synthesize_qwen_reasoning_only_message(
    reasoning_text: &str,
    needs_follow_up: bool,
    saw_tool_output: bool,
    last_tool_output_text: Option<&str>,
) -> Option<ResponseItem> {
    if needs_follow_up {
        return None;
    }
    let text = if reasoning_text.trim().is_empty() {
        if let Some(output) = last_tool_output_text.and_then(format_qwen_tool_output_fallback) {
            output
        } else if saw_tool_output {
            QWEN_TOOL_OUTPUT_ONLY_WARNING.to_string()
        } else {
            return None;
        }
    } else {
        extract_qwen_final_answer_from_reasoning(reasoning_text, saw_tool_output)
            .or_else(|| last_tool_output_text.and_then(format_qwen_tool_output_fallback))
            .unwrap_or_else(|| {
                if saw_tool_output {
                    QWEN_TOOL_OUTPUT_ONLY_WARNING.to_string()
                } else {
                    QWEN_REASONING_ONLY_WARNING.to_string()
                }
            })
    };
    Some(ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText { text }],
        phase: Some(MessagePhase::FinalAnswer),
    })
}

pub(crate) fn qwen_last_tool_output_text(input: &[ResponseItem]) -> Option<String> {
    input
        .iter()
        .rev()
        .filter_map(response_item_tool_output_text)
        .find(|text| !text.trim().is_empty())
}

fn response_item_tool_output_text(item: &ResponseItem) -> Option<String> {
    match item {
        ResponseItem::FunctionCallOutput { output, .. }
        | ResponseItem::CustomToolCallOutput { output, .. } => output.body.to_text(),
        ResponseItem::ToolSearchOutput { execution, .. } => Some(execution.clone()),
        ResponseItem::Message { .. }
        | ResponseItem::Reasoning { .. }
        | ResponseItem::LocalShellCall { .. }
        | ResponseItem::FunctionCall { .. }
        | ResponseItem::CustomToolCall { .. }
        | ResponseItem::ToolSearchCall { .. }
        | ResponseItem::WebSearchCall { .. }
        | ResponseItem::ImageGenerationCall { .. }
        | ResponseItem::Compaction { .. }
        | ResponseItem::Other => None,
    }
}

fn format_qwen_tool_output_fallback(output: &str) -> Option<String> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(truncate_chars(trimmed, QWEN_TOOL_OUTPUT_FALLBACK_MAX_CHARS))
}

fn extract_qwen_final_answer_from_reasoning(
    reasoning_text: &str,
    saw_tool_output: bool,
) -> Option<String> {
    let markers = [
        "Construct Output:",
        "Final Answer:",
        "Final answer:",
        "### Summary",
        "## Summary",
        "# Summary",
        "Summary:",
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
        .or_else(|| extract_concise_completion_line(reasoning_text))
        .or_else(|| {
            saw_tool_output
                .then(|| extract_user_facing_tool_answer(reasoning_text))
                .flatten()
        })
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

fn extract_concise_completion_line(reasoning_text: &str) -> Option<String> {
    for line in reasoning_text.lines() {
        let trimmed = line.trim();
        if let Some(sentence) = trimmed.strip_prefix("Files created.") {
            let suffix = sentence.trim();
            return Some(if suffix.is_empty() {
                "Files created.".to_string()
            } else {
                format!("Files created. {suffix}")
            });
        }
        if trimmed.starts_with("Created ")
            || trimmed.starts_with("Done.")
            || trimmed.starts_with("I created ")
        {
            return clean_answer_line(trimmed);
        }
    }
    None
}

fn extract_user_facing_tool_answer(reasoning_text: &str) -> Option<String> {
    let trimmed = reasoning_text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let first_line = trimmed
        .lines()
        .find(|line| !line.trim().is_empty())?
        .trim()
        .to_ascii_lowercase();
    let reasoning_prefixes = [
        "thinking",
        "thought",
        "analysis",
        "we need",
        "i need",
        "need to",
        "let's",
        "i will",
        "the user",
        "user asks",
    ];
    if reasoning_prefixes
        .iter()
        .any(|prefix| first_line.starts_with(prefix))
    {
        return None;
    }
    Some(truncate_chars(trimmed, QWEN_TOOL_OUTPUT_FALLBACK_MAX_CHARS))
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let mut end = text.len();
    let mut count = 0;
    for (index, _) in text.char_indices() {
        if count == max_chars {
            end = index;
            break;
        }
        count += 1;
    }
    if count <= max_chars {
        text.to_string()
    } else {
        format!("{}…", &text[..end])
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use codex_api::RetryConfig;
    use codex_protocol::models::FunctionCallOutputPayload;
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
    fn removes_vllm_incompatible_history_items_for_qwen_provider() {
        let provider = qwen_provider();
        let mut instructions = "base instructions".to_string();
        let mut input = vec![
            ResponseItem::Reasoning {
                id: "reasoning-1".to_string(),
                summary: Vec::new(),
                content: Some(vec![ReasoningItemContent::ReasoningText {
                    text: "thinking".to_string(),
                }]),
                encrypted_content: None,
            },
            ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: Vec::new(),
                phase: Some(MessagePhase::Commentary),
            },
            ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: "done".to_string(),
                }],
                phase: Some(MessagePhase::FinalAnswer),
            },
            message("user", "next"),
        ];

        apply_qwen_responses_compat(&provider, &mut instructions, &mut input);

        assert_eq!(
            input,
            vec![
                ResponseItem::Message {
                    id: None,
                    role: "assistant".to_string(),
                    content: vec![ContentItem::InputText {
                        text: "done".to_string()
                    }],
                    phase: None,
                },
                message("user", "next"),
            ]
        );
    }

    #[test]
    fn removes_synthetic_warning_and_preserves_tool_history_for_qwen_provider() {
        let provider = qwen_provider();
        let mut instructions = String::new();
        let mut input = vec![
            ResponseItem::FunctionCall {
                id: None,
                name: "exec_command".to_string(),
                namespace: None,
                arguments: r#"{"cmd":"echo HELLO > hello.txt"}"#.to_string(),
                call_id: "call-1".to_string(),
            },
            ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: QWEN_REASONING_ONLY_WARNING.to_string(),
                }],
                phase: Some(MessagePhase::FinalAnswer),
            },
            ResponseItem::FunctionCallOutput {
                call_id: "call-1".to_string(),
                output: FunctionCallOutputPayload::from_text("ok".to_string()),
            },
            message("user", "continue"),
        ];

        apply_qwen_responses_compat(&provider, &mut instructions, &mut input);

        assert_eq!(
            input,
            vec![
                ResponseItem::FunctionCall {
                    id: None,
                    name: "exec_command".to_string(),
                    namespace: None,
                    arguments: r#"{"cmd":"echo HELLO > hello.txt"}"#.to_string(),
                    call_id: "call-1".to_string(),
                },
                ResponseItem::FunctionCallOutput {
                    call_id: "call-1".to_string(),
                    output: FunctionCallOutputPayload::from_text("ok".to_string()),
                },
                message("user", "continue"),
            ]
        );
    }

    #[test]
    fn converts_output_text_messages_to_input_text_for_qwen_provider() {
        let provider = qwen_provider();
        let mut instructions = String::new();
        let mut input = vec![ResponseItem::Message {
            id: None,
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: "previous answer".to_string(),
            }],
            phase: Some(MessagePhase::FinalAnswer),
        }];

        apply_qwen_responses_compat(&provider, &mut instructions, &mut input);

        assert_eq!(
            input,
            vec![ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: vec![ContentItem::InputText {
                    text: "previous answer".to_string()
                }],
                phase: None,
            }]
        );
    }

    #[test]
    fn extracts_final_answer_from_qwen_reasoning_marker() {
        let item = synthesize_qwen_reasoning_only_message(
            "Thinking Process:\n\n5. Construct Output:\n    4\n",
            /*needs_follow_up*/ false,
            /*saw_tool_output*/ false,
            /*last_tool_output_text*/ None,
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
        let item = synthesize_qwen_reasoning_only_message(
            "Thinking Process:\n- still thinking",
            /*needs_follow_up*/ false,
            /*saw_tool_output*/ false,
            /*last_tool_output_text*/ None,
        )
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
    fn extracts_markdown_summary_from_qwen_reasoning() {
        let item = synthesize_qwen_reasoning_only_message(
            "### Summary\n\nI created `hello.py` and `README.md`.",
            /*needs_follow_up*/ false,
            /*saw_tool_output*/ false,
            /*last_tool_output_text*/ None,
        )
        .expect("message");

        assert_eq!(
            item,
            ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: "I created `hello.py` and `README.md`.".to_string()
                }],
                phase: Some(MessagePhase::FinalAnswer),
            }
        );
    }

    #[test]
    fn does_not_synthesize_reasoning_only_message_while_tool_follow_up_is_pending() {
        assert_eq!(
            synthesize_qwen_reasoning_only_message(
                "Thinking Process:\n- called a tool",
                /*needs_follow_up*/ true,
                /*saw_tool_output*/ true,
                /*last_tool_output_text*/ None,
            ),
            None
        );
    }

    #[test]
    fn extracts_short_answer_from_arithmetic_reasoning() {
        let item = synthesize_qwen_reasoning_only_message(
            "Calculate the Answer: 2 + 2 = 4.",
            /*needs_follow_up*/ false,
            /*saw_tool_output*/ false,
            /*last_tool_output_text*/ None,
        )
        .unwrap();

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
        let item = synthesize_qwen_reasoning_only_message(
            "The answer is simply 4.",
            /*needs_follow_up*/ false,
            /*saw_tool_output*/ false,
            /*last_tool_output_text*/ None,
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
    fn extracts_concise_completion_line_from_qwen_reasoning() {
        let item = synthesize_qwen_reasoning_only_message(
            "Files created. Now verify briefly and summarize.",
            /*needs_follow_up*/ false,
            /*saw_tool_output*/ true,
            /*last_tool_output_text*/ None,
        )
        .expect("message");

        assert_eq!(
            item,
            ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: "Files created. Now verify briefly and summarize.".to_string()
                }],
                phase: Some(MessagePhase::FinalAnswer),
            }
        );
    }

    #[test]
    fn extracts_user_facing_answer_from_qwen_reasoning_after_tool_output() {
        let answer = "Aqui estão as 3 últimas linhas de `README.md`:\n\n- [Contributing](CONTRIBUTING.md)\n\nCommunity model compose files belong under `modelo/`.";
        let item = synthesize_qwen_reasoning_only_message(
            answer, /*needs_follow_up*/ false, /*saw_tool_output*/ true,
            /*last_tool_output_text*/ None,
        )
        .expect("message");

        assert_eq!(
            item,
            ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: answer.to_string()
                }],
                phase: Some(MessagePhase::FinalAnswer),
            }
        );
    }

    #[test]
    fn uses_last_tool_output_without_final_reasoning_text() {
        let item = synthesize_qwen_reasoning_only_message(
            "",
            /*needs_follow_up*/ false,
            /*saw_tool_output*/ true,
            Some("- [Contributing](CONTRIBUTING.md)\n\nLast line."),
        )
        .expect("message");

        assert_eq!(
            item,
            ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: "- [Contributing](CONTRIBUTING.md)\n\nLast line.".to_string()
                }],
                phase: Some(MessagePhase::FinalAnswer),
            }
        );
    }

    #[test]
    fn finds_last_tool_output_text() {
        let input = vec![
            ResponseItem::FunctionCallOutput {
                call_id: "call-1".to_string(),
                output: FunctionCallOutputPayload::from_text("first".to_string()),
            },
            ResponseItem::CustomToolCallOutput {
                call_id: "call-2".to_string(),
                name: None,
                output: FunctionCallOutputPayload::from_text("second".to_string()),
            },
        ];

        assert_eq!(
            qwen_last_tool_output_text(&input),
            Some("second".to_string())
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
