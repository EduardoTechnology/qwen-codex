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
qwen-codex --yolo-refiner "Create a minimal README for this project."
qwen-codex --yolo-refiner --iterations 10 "Refine this project."
qwen-codex --yolo-refiner -n 3 "Improve tests and docs."
qwen-codex --yolo --iterations 5 --yolo-verify-commands "curl -sf http://localhost:2226/health;curl -sf http://localhost:2225" "Improve runtime health."
```

`--yolo-refiner` is the preferred explicit flag. `--yolo` remains supported for compatibility, but upstream Codex also uses `--yolo` as an alias for `--dangerously-bypass-approvals-and-sandbox`.

With `--iterations N`, YOLO runs exactly `N` rounds unless the user interrupts it, one round times out with timeout continuation disabled, the agent subprocess fails, or the failure guard triggers. The refiner cannot stop the run by default; `YOLO_STOP` is treated as an invalid next prompt and repaired into a focused continuation prompt. Optional early stop is available only with `--yolo-allow-refiner-stop` or `QWEN_CODEX_YOLO_ALLOW_REFINER_STOP=true`.

Without `--iterations`, or with `--iterations 0`, YOLO runs until Ctrl+C, timeout, the failure guard, or a fatal agent/refiner error. Logs are written to `.qwen-codex/yolo-runs/<run-id>/` as JSON and Markdown, with secrets redacted.

Each run also writes `analysis.json` and `analysis.md`, which summarize stop reason, round chaining, agent subprocess status, refiner calls, changed files, and diagnostics in one place.

For unattended local experiments, approvals and sandboxing are bypassed only when explicitly requested:

```sh
qwen-codex --yolo-refiner --iterations 5 --dangerously-bypass-approvals-and-sandbox "Refine this project."
```

See [docs/yolo-mode.md](docs/yolo-mode.md).

## YOLO Configuration

```sh
QWEN_CODEX_YOLO_REFINER_BASE_URL=http://127.0.0.1:8002/v1
QWEN_CODEX_YOLO_REFINER_API_KEY=local-dev-key
QWEN_CODEX_YOLO_REFINER_MODEL=qwen35-local
QWEN_CODEX_YOLO_LOG_DIR=.qwen-codex/yolo-runs
QWEN_CODEX_YOLO_ROUND_TIMEOUT_SECS=600
QWEN_CODEX_YOLO_ROUND_GOAL_MAX_FILES=8
QWEN_CODEX_YOLO_ROUND_GOAL_MAX_ACTIONS=5
QWEN_CODEX_YOLO_ROUND_GOAL_MAX_TESTS=3
QWEN_CODEX_YOLO_REFINER_MAX_PROMPT_CHARS=3000
QWEN_CODEX_YOLO_REFINER_STYLE=incremental
QWEN_CODEX_YOLO_CONTINUE_AFTER_TIMEOUT=false
QWEN_CODEX_YOLO_ALLOW_REFINER_STOP=false
QWEN_CODEX_YOLO_VERIFY_COMMANDS=
QWEN_CODEX_YOLO_VERIFY_TIMEOUT_SECS=15
QWEN_CODEX_YOLO_ACCEPTANCE_GATE=false
QWEN_CODEX_YOLO_ACCEPTANCE_COMMANDS=
QWEN_CODEX_YOLO_ACCEPTANCE_MAX_SECONDS=300
QWEN_CODEX_YOLO_REJECT_STOP_ON_FAILED_ACCEPTANCE=true
QWEN_CODEX_YOLO_DEFAULT_ITERATIONS=
QWEN_CODEX_YOLO_MAX_REPEATED_PROMPTS=3
QWEN_CODEX_YOLO_MAX_FAILURES=3
```

Leave `QWEN_CODEX_YOLO_DEFAULT_ITERATIONS` blank for unlimited YOLO mode unless `--iterations` or `-n` is provided.

`QWEN_CODEX_YOLO_ROUND_TIMEOUT_SECS` prevents a single Codex agent round from blocking the autonomous loop forever. The default is 600 seconds. On timeout, YOLO kills the round, writes valid JSON/Markdown/analysis logs, skips the refiner, and stops with `round_timeout`.

Round-budget settings guide the refiner toward smaller next prompts. By default the refiner should ask for at most 8 files, 5 concrete actions, 3 verification commands, and a 3000-character next prompt. Qwen Codex validates each refiner prompt before injecting it into the next agent round; broad prompts such as "finish everything" are repaired into a smaller scoped task or rejected.

`QWEN_CODEX_YOLO_CONTINUE_AFTER_TIMEOUT` defaults to `false`. The safe default is to stop on timeout rather than continue from incomplete work.

`QWEN_CODEX_YOLO_VERIFY_COMMANDS` and `--yolo-verify-commands` run semicolon-separated shell commands after each completed round and before the refiner call. Their exit codes and bounded output are logged in `externalVerification` and included in the refiner context, so runtime failures such as a failing frontend health check can drive the next prompt.

Each external verification command has its own timeout, configured by `QWEN_CODEX_YOLO_VERIFY_TIMEOUT_SECS` or `--yolo-verify-timeout-secs`. The default is 15 seconds so failed runtime checks do not stall the autonomous loop.

For release-style workflows, enable the acceptance gate so a refiner stop signal is accepted only after real checks pass:

```sh
qwen-codex --yolo-refiner --iterations 6 \
  --yolo-acceptance-gate \
  --yolo-acceptance-command "docker compose config" \
  --yolo-acceptance-command "docker compose build" \
  --yolo-acceptance-command "docker compose up -d && sleep 8 && curl -fsS http://localhost:2226/health && curl -fsS http://localhost:2225; status=$?; docker compose down; exit $status" \
  "Make this Dockerized app runnable."
```

When `QWEN_CODEX_YOLO_ACCEPTANCE_GATE=true`, `YOLO_STOP` is rejected if any acceptance command fails. The failed command output is logged under `acceptanceResults`, included in the refiner context, and converted into a bounded repair prompt. Fixed-iteration runs also execute acceptance commands before stopping at `max_iterations`, so failed acceptance checks mark the final status as partial instead of success.

## Safety Notes

Qwen Codex can run shell commands and edit files through upstream Codex tools. Review generated changes, keep secrets out of prompts when possible, and use temporary workspaces for destructive experiments. YOLO logs redact common API keys, tokens, authorization headers, `.env` assignments, credentials, and private keys before writing to disk.

## Context Window

Set `QWEN_CODEX_CONTEXT_WINDOW` to match the vLLM `--max-model-len` value. The provided compose file and verified local server use `32768`.

Qwen Codex derives a conservative auto-compact threshold from that value:

```text
threshold = min(context_window * 0.80, context_window - 4096)
```

For `32768`, the default Qwen auto-compact threshold is `26214` tokens. Long or unlimited YOLO runs depend on upstream Codex compaction and should be stress-tested before unattended production use.

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

## Development Notes

`pnpm run format` emits a Node.js engine warning on Node v20; Node v22+ is required for full compatibility but formatting still passes.

The provided compose maps host `http://127.0.0.1:8002/v1` to container port `8000`. Inside a Docker service network, use the service hostname and container port, for example `http://qwen35-vllm:8000/v1`. Do not use host port `8000` if another local service already owns it.

## Documentation

- [Modelo compose files](modelo/README.md)
- [YOLO mode](docs/yolo-mode.md)
- [Verification](docs/verification.md)
- [Research notes](docs/research-notes.md)
- [Upstream sync](docs/upstream-sync.md)
- [Roadmap](docs/roadmap.md)
- [Contributing](CONTRIBUTING.md)

Community model compose files belong under `modelo/` as `docker-compose.<model>.yml` with model, parser, GPU, and context-window notes.
