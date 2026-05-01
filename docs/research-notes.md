# Research Notes

Last updated: 2026-05-01T19:14:48Z

## Repository Structure

- Root workspace uses `pnpm` for repo maintenance scripts and a Rust workspace in `codex-rs/`.
- Native CLI code lives in `codex-rs/cli`.
- The npm package shim lives in `codex-cli/` and currently exposes the `codex` bin through `codex-cli/bin/codex.js`.
- Core agent/session/tool orchestration lives primarily in `codex-rs/core`, with app-server integration in `codex-rs/app-server*` and non-interactive execution in `codex-rs/exec`.
- Model provider metadata lives in `codex-rs/model-provider-info`; runtime provider behavior lives in `codex-rs/model-provider`.
- Tool specs are in `codex-rs/tools`; MCP runtime code is in `codex-rs/codex-mcp`.
- Skills live in `codex-rs/skills` and `codex-rs/core-skills`.
- Context compaction is implemented in `codex-rs/core/src/tasks/compact.rs`, `compact.rs`, and `compact_remote.rs`.
- Existing license files are `LICENSE`, `NOTICE`, and `docs/license.md`; license is Apache-2.0.

## Build And Test Commands

- Build native CLI: `cd codex-rs && cargo build -p codex-cli`.
- Run CLI from source: `cd codex-rs && cargo run --bin codex -- ...`.
- Format Rust: `cd codex-rs && just fmt`.
- Scoped Rust tests: `cd codex-rs && cargo test -p <crate>`.
- Full Rust test helper: `just test` from repo root if `cargo-nextest` is installed; otherwise use `cd codex-rs && cargo test`.
- Root formatting script: `pnpm run format` checks JSON, Markdown, workflow YAML, and JS.

## CLI Entrypoint

- Native binary entrypoint: `codex-rs/cli/src/main.rs`.
- Cargo binary name today: `codex` from `codex-rs/cli/Cargo.toml`.
- npm entrypoint today: `codex-cli/bin/codex.js`.
- `codex exec` uses `codex-rs/exec` and starts the same in-process app-server/agent architecture used by other non-interactive flows.

## Config And Provider Files

- CLI shared flags: `codex-rs/utils/cli/src/shared_options.rs`.
- Config TOML schema: `codex-rs/config/src/config_toml.rs`.
- Effective config builder: `codex-rs/core/src/config/mod.rs`.
- Provider metadata: `codex-rs/model-provider-info/src/lib.rs`.
- Provider runtime abstraction: `codex-rs/model-provider/src/provider.rs`.
- `.env` loading: `codex-rs/arg0/src/lib.rs` loads `$CODEX_HOME/.env` and filters `CODEX_` variables.

## Tool, Skill, And MCP Architecture

- Normal agent execution flows through the app-server client and core session machinery.
- `codex exec` uses `InProcessAppServerClient`, starts or resumes a thread, sends `turn/start`, and streams app-server notifications.
- Tool definitions are assembled from `codex-rs/tools`.
- MCP server configuration and runtime handling are centralized in `codex-rs/codex-mcp`, with connection management in `codex-rs/codex-mcp/src/mcp_connection_manager.rs`.
- YOLO mode should call back into `codex exec` or equivalent app-server execution instead of creating a separate reduced tool runner.

## Qwen Integration Placement

The least invasive integration is a Qwen wrapper layer that:

- Adds `qwen-codex` and `qwencodex` command entrypoints.
- Loads Qwen-specific `.env` values.
- Resolves Qwen CLI flags and environment variables.
- Injects normal Codex config overrides for a `qwen` OpenAI-compatible Responses provider.
- Reuses `codex exec` for one-shot prompts and YOLO iterations.

This keeps upstream provider and agent code mostly unchanged and makes future upstream merges easier.

## YOLO Hook Point

YOLO mode should be implemented outside `codex-core` and drive normal agent turns through `codex exec`.

The first iteration starts a Qwen Codex exec thread. Later iterations resume the latest matching Qwen thread so context, compaction, tools, skills, and MCP behavior stay owned by upstream Codex.

## Local Model Health Check

Requested initial endpoint:

```text
GET http://127.0.0.1:8000/v1/models
Result: HTTP JSON body {"detail":"Not Found"}
```

Corrected endpoint from user during the initial audit:

```text
GET http://127.0.0.1:8002/v1/models
Result: curl error 56, Recv failure: Connection reset by peer
```

Later verification after fixing the local vLLM compose:

```text
GET http://127.0.0.1:8002/v1/models
Result: healthy. The response reports model id qwen35-local and max_model_len 32768.
```

The healthy local server uses host base URL `http://127.0.0.1:8002/v1`, served model `qwen35-local`, GPU memory utilization `0.90`, and `TRITON_ATTN` for attention instead of FlashAttention.

## Normal CLI Implementation Findings

- The clean external branding layer is a new `codex-qwen` Rust crate plus `qwen-codex` and `qwencodex` native/npm command entrypoints. The wrapper delegates normal execution back to the upstream `codex` binary and passes config overrides instead of forking the agent architecture.
- Bare prompts are routed to `codex exec --skip-git-repo-check`, so normal mode continues to use upstream session, tool, MCP, shell, file-editing, and context compaction logic.
- Qwen config belongs in the wrapper crate. Runtime defaults are centralized in `codex-rs/qwen/src/config.rs`; `.env.example` and `modelo/` document the verified local values.
- vLLM `/v1/models` returns the OpenAI-compatible `{"object":"list","data":[...]}` shape, while Codex's model metadata manager expects the Codex model catalog shape. Custom providers with `requires_openai_auth = false` should not inherit the user's OpenAI auth manager; this avoids trying to refresh Codex backend model metadata for local vLLM while preserving first-party OpenAI behavior.
- The verified Qwen/vLLM Responses API still produced reasoning-only streams in this environment despite the server-side `enable_thinking=false` fix. Qwen Codex therefore adds a provider-name-scoped compatibility shim that moves `developer` messages into `instructions` and synthesizes final assistant text from Qwen reasoning-only output when needed.
- Direct Chat Completions against the same server returned `4` for the arithmetic smoke prompt. The final Qwen Codex normal-mode smoke also returned visible assistant text `4` through `qwen-codex`.

## External Source Notes

- Upstream Codex build instructions in this repo point developers to `codex-rs`, `cargo build`, `cargo run --bin codex`, `just fmt`, and crate-scoped `cargo test`.
- vLLM latest OpenAI-compatible server docs list `/v1/responses`, `/v1/chat/completions`, and `/v1/models`-adjacent APIs for text generation models. Source: https://docs.vllm.ai/en/latest/serving/openai_compatible_server/
- vLLM latest docs include Responses API compatibility with OpenAI's Responses API. Source: https://docs.vllm.ai/en/latest/serving/openai_compatible_server/
- vLLM Qwen3 reasoning parser docs describe a Qwen3/Qwen3.5 reasoning parser. Source: https://docs.vllm.ai/en/v0.19.1/api/vllm/reasoning/qwen3_reasoning_parser/
- vLLM tool-calling docs list parser flags such as `--tool-call-parser` and model-specific parser names. Source: https://docs.vllm.ai/en/latest/features/tool_calling/
