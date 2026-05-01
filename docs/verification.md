# Verification

Last updated: 2026-05-01T21:16:00Z

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
- Qwen-bound history strips reasoning items, empty assistant messages, phase metadata, synthetic warning messages, and vLLM-incompatible assistant `output_text` message content before sending history back to vLLM.
- If a Qwen Responses stream completes with reasoning text or tool output but no final assistant message, Qwen Codex synthesizes a final assistant message from recognizable final-answer/summary markers or a short completion acknowledgement.
- The compatibility path does not run for upstream OpenAI/default providers.

Live tool-call compatibility verification:

- `/tmp/qwen-normal-tool-test`: `qwen-codex` created `README.md` and `hello.py` in the current directory, ran `python hello.py`, printed `HELLO`, exited `0`, and emitted visible assistant text. No Qwen/vLLM validation errors and no `local-dev-key` leak were found in `output.log`.
- `/tmp/yolo-smoke`: `qwen-codex --yolo --iterations 2` created `hello.txt` in iteration 1 and `README.md` in iteration 2. `iteration-002.json.agentInputPrompt == iteration-001.json.nextPrompt`, both iteration `errors` arrays were empty, and log redaction did not leak `local-dev-key`.

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

## YOLO Mode Milestone Status

YOLO mode has been implemented as a Qwen wrapper feature that delegates each agent round to the normal upstream `codex exec` path and resumes the same Codex thread between refiner prompts.

Checks run for this milestone:

- `cd codex-rs && just fmt`: passed.
- `cd codex-rs && cargo test -p codex-qwen`: passed, 24 tests.
- `cd codex-rs && just fix -p codex-qwen`: passed.
- `cd codex-rs && cargo build -p codex-cli`: passed.
- `cd codex-rs && ./target/debug/qwen-codex --help`: passed.
- `cd codex-rs && ./target/debug/qwen-codex --version`: passed.
- `cd codex-rs && ./target/debug/qwencodex --help`: passed.
- `cd codex-rs && ./target/debug/qwen-codex --yolo --iterations 0 --yolo-log-dir <tmpdir> "Smoke prompt"`: passed and wrote `run.json` plus `run.md` without entering the agent/refiner loop.
- `PATH="$HOME/.local/bin:$PATH" just bazel-lock-update`: passed.
- `PATH="$HOME/.local/bin:$PATH" just bazel-lock-check`: passed.
- `PATH="$HOME/.local/bin:$PATH" pnpm run format`: passed with the existing Node engine warning because this machine has Node `v20.20.0` while the repo asks for Node `>=22`.
- `git diff --check`: passed.

YOLO behavior covered by tests:

- CLI parsing for `--yolo`, `--iterations`, `-n`, and the optional `--10` shorthand.
- Fixed iteration limits.
- Infinite-mode setup without looping forever by stopping on `YOLO_STOP`.
- `YOLO_STOP` refiner stop signal.
- Repeated prompt guard.
- Consecutive failure guard.
- Ctrl+C interrupt flag behavior between rounds.
- YOLO log writing and secret redaction.
- Refiner client call against a mocked OpenAI-compatible `/v1/chat/completions` endpoint.
- YOLO refiner env/config loading.

Live YOLO verification:

```text
Run directory: /tmp/yolo-smoke/.qwen-codex/yolo-runs/20260501T210832Z-1510775
Exit: 0
Stop reason: RefinerStopSignal after 2 iterations
Created files: hello.txt, README.md
Round 2 prompt injection: iteration-002.agentInputPrompt == iteration-001.nextPrompt
Errors: []
Secret redaction: PASS
```

## Context Management

Status: `CONFIG-PROPAGATION-CONFIRMED-BUT-LONG-RUN-NOT-STRESS-TESTED`.

Evidence:

- `codex-rs/qwen/src/config.rs` reads `QWEN_CODEX_CONTEXT_WINDOW` and emits `model_context_window=32768`.
- The same config path emits `model_auto_compact_token_limit=26214`.
- Qwen threshold formula: `min(context_window * 0.80, context_window - 4096)`.
- Unit coverage: `32768 -> 26214`, `8192 -> 4096`, `4096 -> 0`.
- `codex-rs/models-manager/src/model_info.rs` applies `model_context_window` and `model_auto_compact_token_limit` overrides to model metadata.
- `codex-rs/core/src/session/turn.rs` uses `model_info.auto_compact_token_limit()` for pre-turn and mid-turn compaction.
- `codex-rs/core/src/session/turn_context.rs` derives the effective context window from resolved model metadata.
- YOLO calls normal `codex exec --json` and then `codex exec --json resume <thread-id> <prompt>`, so it uses the same Qwen config and upstream compaction path as normal mode.

Long-running/infinite YOLO compaction has not been stress-tested beyond config propagation and unit coverage.

## Behavioral Tool Tests

Workspace: `/tmp/tool-test`.

| Capability      | Result               | Evidence                                                                                                                                                                 |
| --------------- | -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Web search      | `CAPABILITY_MISSING` | Prompt attempted `web_search`; upstream router logged `unsupported call: web_search`.                                                                                    |
| PDF generation  | `PASS`               | `test.pdf` exists; `file` reports `PDF document, version 1.4, 1 page(s), ASCII text`.                                                                                    |
| DOCX generation | `PASS`               | `test.docx` exists; `file` reports `Microsoft Word 2007+`; `word/document.xml` contains `Hello World`.                                                                   |
| XLSX generation | `FAIL`               | Prompt attempted package-based spreadsheet creation, but `pandas`/`openpyxl` were unavailable and network/package installation was blocked; `test.xlsx` was not created. |

The behavioral tests verify agent/tool execution through shell/file creation, not dedicated first-class PDF/DOCX/XLSX skills. Missing or failed capabilities are tracked in `docs/roadmap.md`.
