use super::*;
use crate::ModelsManagerConfig;
use codex_protocol::openai_models::ReasoningEffort;
use pretty_assertions::assert_eq;

#[test]
fn reasoning_summaries_override_true_enables_support() {
    let model = model_info_from_slug("unknown-model");
    let config = ModelsManagerConfig {
        model_supports_reasoning_summaries: Some(true),
        ..Default::default()
    };

    let updated = with_config_overrides(model.clone(), &config);
    let mut expected = model;
    expected.supports_reasoning_summaries = true;

    assert_eq!(updated, expected);
}

#[test]
fn qwen_local_model_instructions_include_tool_guidance() {
    let model = model_info_from_slug("qwen35-local");
    let instructions =
        model.get_model_instructions(Some(codex_protocol::config_types::Personality::Pragmatic));

    assert!(!model.used_fallback_model_metadata);
    assert_eq!(model.context_window, Some(32_768));
    assert_eq!(model.default_reasoning_level, Some(ReasoningEffort::Medium));
    assert!(instructions.contains("Qwen local tool guidance"));
    assert!(instructions.contains("Do not invent MCP servers"));
    assert!(instructions.contains("Never call MCP resources for local file reads"));
    assert!(instructions.contains("simple file read/transcription requests"));
    assert!(instructions.contains("Path.write_text"));
    assert!(instructions.contains("Avoid wrapping heredoc commands in double quotes"));
    assert!(instructions.contains("Do not call image-view tools"));
    assert!(instructions.contains("node --check"));
    assert!(instructions.contains("repair the file and rerun validation"));
}

#[test]
fn reasoning_summaries_override_false_does_not_disable_support() {
    let mut model = model_info_from_slug("unknown-model");
    model.supports_reasoning_summaries = true;
    let config = ModelsManagerConfig {
        model_supports_reasoning_summaries: Some(false),
        ..Default::default()
    };

    let updated = with_config_overrides(model.clone(), &config);

    assert_eq!(updated, model);
}

#[test]
fn reasoning_summaries_override_false_is_noop_when_model_is_false() {
    let model = model_info_from_slug("unknown-model");
    let config = ModelsManagerConfig {
        model_supports_reasoning_summaries: Some(false),
        ..Default::default()
    };

    let updated = with_config_overrides(model.clone(), &config);

    assert_eq!(updated, model);
}

#[test]
fn model_context_window_override_clamps_to_max_context_window() {
    let mut model = model_info_from_slug("unknown-model");
    model.context_window = Some(273_000);
    model.max_context_window = Some(400_000);
    let config = ModelsManagerConfig {
        model_context_window: Some(500_000),
        ..Default::default()
    };

    let updated = with_config_overrides(model.clone(), &config);
    let mut expected = model;
    expected.context_window = Some(400_000);

    assert_eq!(updated, expected);
}

#[test]
fn model_context_window_uses_model_value_without_override() {
    let mut model = model_info_from_slug("unknown-model");
    model.context_window = Some(273_000);
    model.max_context_window = Some(400_000);
    let config = ModelsManagerConfig::default();

    let updated = with_config_overrides(model.clone(), &config);

    assert_eq!(updated, model);
}
