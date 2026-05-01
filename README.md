# Qwen Codex

Qwen Codex is an Apache-2.0-compatible fork/adaptation of the OpenAI Codex CLI that defaults to a local Qwen model served by vLLM through an OpenAI-compatible API.

It is not a rewrite of the Codex agent. Normal mode and YOLO mode delegate back into the upstream Codex execution path so shell tools, file editing, skills, MCP support, context compaction, and future upstream improvements remain available.

## Relationship To Upstream

- `origin` is the Qwen Codex repository.
- `upstream` is the original `openai/codex` repository.
- Qwen-specific code is kept in a small wrapper layer where practical so upstream merges remain manageable.
- The license remains Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).

## Quickstart

```sh
cp .env.example .env
docker compose -f modelo/docker-compose.qwen35-9b-awq.yml up -d
curl http://127.0.0.1:8002/v1/models
cd codex-rs
cargo build -p codex-cli
./target/debug/qwen-codex --health
./target/debug/qwen-codex "What is 2+2? Answer in one word."
```

The compatibility alias is also built:

```sh
cd codex-rs
./target/debug/qwencodex --help
```

## Local Qwen/vLLM Setup

The verified local baseline is:

- Base URL: `http://127.0.0.1:8002/v1`
- Container port: `8000`
- Host port: `8002`
- Served model: `qwen35-local`
- Model repository: `QuantTrio/Qwen3.5-9B-AWQ`
- Context window: `32768`
- GPU memory utilization: `0.90`
- Attention backend: `TRITON_ATTN`
- Reasoning parser: `qwen3`
- Default chat template kwargs: `{"enable_thinking": false}`

Start the model with:

```sh
docker compose -f modelo/docker-compose.qwen35-9b-awq.yml up -d
```

The compose template keeps tool calling configurable through `VLLM_TOOL_CALL_PARSER`. The current default is `qwen3_coder`; `qwen3_xml` is another Qwen parser to test when changing model families or vLLM versions.

## Configuration

Qwen Codex reads a local `.env` file in the current directory and the process environment. Precedence is CLI flags, official `QWEN_CODEX_*` environment variables, supported short aliases, then documented defaults.

Core variables:

```sh
QWEN_CODEX_BASE_URL=http://127.0.0.1:8002/v1
QWEN_CODEX_API_KEY=local-dev-key
QWEN_CODEX_MODEL=qwen35-local
QWEN_CODEX_CONTEXT_WINDOW=32768
QWEN_CODEX_REQUEST_TIMEOUT_MS=120000
QWEN_CODEX_LOG_LEVEL=info
```

Local vLLM accepts a placeholder API key unless you configure API-key enforcement on the server.

## Normal Usage

```sh
qwen-codex --help
qwen-codex --version
qwen-codex --health
qwen-codex "What is 2+2? Answer in one word."
qwen-codex exec "Summarize this repository."
```

A bare prompt is routed to `codex exec --skip-git-repo-check` with Qwen provider overrides. This preserves the upstream Codex agent architecture.

## YOLO Mode

YOLO mode runs repeated normal Codex agent rounds and asks a separate OpenAI-compatible refiner model for the next prompt between rounds.

```sh
qwen-codex --yolo "Create a minimal README for this project."
qwen-codex --yolo --iterations 10 "Refine this project."
qwen-codex --yolo -n 3 "Improve tests and docs."
```

YOLO stops when the iteration limit is reached, the refiner returns `YOLO_STOP`, the repeated-prompt guard triggers, the failure guard triggers, or Ctrl+C is received. Logs are written to `.qwen-codex/yolo-runs/<run-id>/` as both JSON and Markdown, with secrets redacted.

See [docs/yolo-mode.md](docs/yolo-mode.md).

## YOLO Configuration

```sh
QWEN_CODEX_YOLO_REFINER_BASE_URL=http://127.0.0.1:8002/v1
QWEN_CODEX_YOLO_REFINER_API_KEY=local-dev-key
QWEN_CODEX_YOLO_REFINER_MODEL=qwen35-local
QWEN_CODEX_YOLO_LOG_DIR=.qwen-codex/yolo-runs
QWEN_CODEX_YOLO_DEFAULT_ITERATIONS=
QWEN_CODEX_YOLO_MAX_REPEATED_PROMPTS=3
QWEN_CODEX_YOLO_MAX_FAILURES=3
```

Leave `QWEN_CODEX_YOLO_DEFAULT_ITERATIONS` blank for unlimited YOLO mode unless `--iterations` or `-n` is provided.

## Safety Notes

Qwen Codex can run shell commands and edit files through upstream Codex tools. Review generated changes, keep secrets out of prompts when possible, and use temporary workspaces for destructive experiments. YOLO logs redact common API keys, tokens, authorization headers, `.env` assignments, credentials, and private keys before writing to disk.

## Build And Test

```sh
cd codex-rs
just fmt
cargo test -p codex-qwen
cargo build -p codex-cli
just fix -p codex-qwen
```

If dependencies change, update and check Bazel locks from the repo root:

```sh
just bazel-lock-update
just bazel-lock-check
```

Root formatting uses:

```sh
pnpm run format
```

## Documentation

- [Modelo compose files](modelo/README.md)
- [YOLO mode](docs/yolo-mode.md)
- [Verification](docs/verification.md)
- [Research notes](docs/research-notes.md)
- [Upstream sync](docs/upstream-sync.md)
- [Roadmap](docs/roadmap.md)
- [Contributing](CONTRIBUTING.md)

Community model compose files belong under `modelo/` as `docker-compose.<model>.yml` with model, parser, GPU, and context-window notes.
