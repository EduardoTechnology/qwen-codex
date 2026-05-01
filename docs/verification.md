# Verification

Last updated: 2026-05-01T19:14:48Z

## Verified Local Qwen/vLLM Server

The local vLLM server is healthy with the Qwen Codex baseline configuration:

- Host base URL: `http://127.0.0.1:8002/v1`
- Container port: `8000`
- Host port: `8002`
- Models endpoint: `http://127.0.0.1:8002/v1/models`
- Served model: `qwen35-local`
- Model repository: `QuantTrio/Qwen3.5-9B-AWQ`
- `max_model_len`: `32768`
- GPU memory utilization: `0.90`
- Attention backend: `TRITON_ATTN`
- Container status: healthy
- `/v1/models` reports `max_model_len: 32768`

Log lines observed from the healthy container:

```text
Using backend AttentionBackendEnum.TRITON_ATTN for vit attention
Using AttentionBackendEnum.TRITON_ATTN for MMEncoderAttention
Using AttentionBackendEnum.TRITON_ATTN backend
Starting vLLM server on http://0.0.0.0:8000
```

Health check command:

```sh
curl http://127.0.0.1:8002/v1/models
```

Observed result summary:

```text
data[0].id = qwen35-local
data[0].root = QuantTrio/Qwen3.5-9B-AWQ
data[0].max_model_len = 32768
```

Simple chat completion verification:

```sh
curl -s http://127.0.0.1:8002/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer local-dev-key' \
  -d '{"model":"qwen35-local","messages":[{"role":"user","content":"What is 2+2? Answer in one word."}],"max_tokens":256}'
```

Observed result: the model returned `4`. Very low token limits can return only reasoning content because the Qwen reasoning parser may consume the token budget before the final answer.

## Qwen Responses API Compatibility

The verified vLLM server works with Chat Completions and `/v1/models`. The vLLM Responses API for this Qwen build can still emit reasoning-only output even with:

```text
--reasoning-parser qwen3
--default-chat-template-kwargs '{"enable_thinking": false}'
```

Qwen Codex keeps the server-side thinking fix in the public compose file and adds the smallest runtime compatibility layer only for the `Qwen local OpenAI-compatible` provider:

- Developer-role messages are moved into the Responses `instructions` field because this vLLM build rejects `role: developer`.
- If a Qwen Responses stream completes with reasoning text but no final assistant message, Qwen Codex synthesizes a final assistant message from recognizable final-answer reasoning markers.
- The compatibility path does not run for upstream OpenAI/default providers.

## Normal CLI Milestone Status

The normal CLI milestone has been verified locally and is ready to commit.

Checks run:

- `cd codex-rs && just fmt`: passed.
- `cd codex-rs && cargo test -p codex-qwen`: passed, 14 tests.
- `cd codex-rs && cargo test -p codex-core qwen_compat`: passed, 6 focused tests.
- `cd codex-rs && cargo test -p codex-model-provider`: passed, 29 tests.
- `cd codex-rs && cargo build -p codex-cli`: passed.
- `cd codex-rs && just fix -p codex-qwen`: passed.
- `cd codex-rs && just fix -p codex-model-provider`: passed.
- `cd codex-rs && just fix -p codex-core`: passed.
- `cd codex-rs && just fix -p codex-cli`: passed.
- `PATH="$HOME/.local/bin:$PATH" just bazel-lock-update`: passed after installing Bazelisk locally.
- `PATH="$HOME/.local/bin:$PATH" just bazel-lock-check`: passed.
- `PATH="$HOME/.local/bin:$PATH" pnpm run format`: passed after installing pnpm dependencies. The command emitted the existing Node engine warning because this machine has Node `v20.20.0` while the repo asks for Node `>=22`.
- `cd codex-rs && ./target/debug/qwen-codex --help`: passed.
- `cd codex-rs && ./target/debug/qwen-codex --version`: passed.
- `cd codex-rs && ./target/debug/qwencodex --help`: passed.
- `cd codex-rs && ./target/debug/qwen-codex --health`: passed and returned `max_model_len: 32768`.
- `docker compose -f modelo/docker-compose.qwen35-9b-awq.yml config`: passed and shows host port `8002`, container port `8000`, `TRITON_ATTN`, `qwen3`, `qwen3_coder`, and `{"enable_thinking": false}`.
- `qwen-codex "What is 2+2? Answer in one word."`: passed against `http://127.0.0.1:8002/v1`; visible assistant text was `4`.

Not run in this milestone:

- Full Rust workspace test suite. This milestone used scoped tests for the changed crates and focused Qwen compatibility tests.
- YOLO behavioral checks. YOLO mode is intentionally deferred until the YOLO loop controller and logging modules are implemented.
