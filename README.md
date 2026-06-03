# Qwen Codex

Qwen Codex is an Apache-2.0-compatible fork/adaptation of the OpenAI Codex CLI that defaults to a local Qwen model served by vLLM through an OpenAI-compatible API.

It is not a rewrite of the Codex agent. Normal mode and YOLO mode delegate back into the upstream Codex execution path so shell tools, file editing, skills, MCP support, context compaction, and future upstream improvements remain available.

## Relationship To Upstream

- `origin` is the Qwen Codex repository.
- `upstream` is the original `openai/codex` repository.
- Qwen-specific code is kept in a small wrapper layer where practical so upstream merges remain manageable.
- The license remains Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).

## Quickstart

### Plug-and-play

For a class or demo, students only need to start the bundled local Qwen/vLLM model, build/install Qwen Codex, then run `qwen-codex`. The repository already includes a safe default `.env` for `http://127.0.0.1:8002/v1`.

WSL2/Linux/macOS:

```sh
docker compose up -d
cargo install --path codex-rs/cli --bin qwen-codex --locked --force
qwen-codex
```

Windows PowerShell:

```powershell
docker compose up -d
cargo install --path codex-rs/cli --bin qwen-codex --locked --force
qwen-codex
```

If your model is not running at `http://127.0.0.1:8002/v1`, edit `.env` and change only `QWEN_CODEX_BASE_URL`.

The bundled compose downloads `QuantTrio/Qwen3.5-9B-AWQ`, but exposes it to the agent as the stable local id `qwen35-local`. This is intentional for classes: students can change the downloaded model in `docker-compose.yml` and keep `--served-model-name qwen35-local`, so no extra `.env` setting is needed.

If a student also changes `--served-model-name`, uncomment `QWEN_CODEX_MODEL` in `.env` and set it to the exact id returned by `/v1/models`.

Optional checks:

```sh
curl http://127.0.0.1:8002/v1/models
qwen-codex --health
```

`qwen-codex` reads `.env` from the current directory when it starts. Run it from a folder that contains `.env`, or set `QWEN_CODEX_BASE_URL` in your shell environment.

If a student already has another OpenAI-compatible Qwen server, change `QWEN_CODEX_BASE_URL` in `.env`, then run `qwen-codex`. Set `QWEN_CODEX_MODEL` only if that server does not expose `qwen35-local`.

```dotenv
QWEN_CODEX_BASE_URL=http://STUDENT_MODEL_HOST:PORT/v1
# QWEN_CODEX_MODEL=the-model-id-shown-by-v1-models
```

Classroom demo prompt:

```text
Create a tiny static website for a coding class. Create index.html and styles.css using safe file writing methods. The page must visibly show the title Qwen Codex Aula, exactly three bullet points about local AI coding, and a button labeled Testar agente. Then validate with a local command that reads the files and asserts the required text exists. Answer only with PASS and the files created if validation passes.
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
- Served model alias: `qwen35-local`
- Downloaded model repository: `QuantTrio/Qwen3.5-9B-AWQ`
- Context window: `32768`
- GPU memory utilization: `0.90`
- Attention backend: `TRITON_ATTN`
- Auto tool choice: enabled
- Tool-call parser: `qwen3_coder`
- Reasoning parser: `qwen3`
- Default chat template kwargs: `{"enable_thinking": false}`

Start the model with:

```sh
docker compose up -d
```

The compose intentionally uses fixed values instead of shell-style environment defaults so it is easier to teach. The agent-critical vLLM flags for Qwen Codex are `--enable-auto-tool-choice`, `--tool-call-parser qwen3_coder`, `--reasoning-parser qwen3`, and `--default-chat-template-kwargs '{"enable_thinking": false}'`. The `TRITON_ATTN` and language-only flags are kept because this 9B AWQ setup needs the extra KV-cache headroom for the 32768-token context window.
`--enforce-eager` is also kept because this local WSL/GPU setup otherwise triggered a KV-cache startup failure before eventually restarting.

## Configuration

Qwen Codex reads a local `.env` file in the current directory and the process environment. Precedence is CLI flags, official `QWEN_CODEX_*` environment variables, supported short aliases, then documented defaults.

Core variables:

```sh
QWEN_CODEX_BASE_URL=http://127.0.0.1:8002/v1
QWEN_CODEX_API_KEY=local-dev-key
QWEN_CODEX_MODEL=qwen35-local
QWEN_CODEX_CONTEXT_WINDOW=32768
QWEN_CODEX_REQUEST_TIMEOUT_MS=600000
QWEN_CODEX_LOG_LEVEL=error
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

Native web search/browser tools depend on the active Codex tool environment. In the verified local Qwen setup, native `web_search` was not available; shell network commands such as `curl` can still work when the sandbox/network policy allows them.

## YOLO Mode

YOLO mode runs repeated normal Codex agent rounds and asks a separate OpenAI-compatible refiner model for the next prompt between rounds.

```sh
qwen-codex --yolo-refiner "Create a minimal README for this project."
qwen-codex --yolo "Keep improving until YOLO_STOP or a safety stop."
qwen-codex --yolo --iterations 5 "Refine this project."
qwen-codex --yolo-refiner --iterations 10 "Refine this project."
qwen-codex --yolo-refiner -n 3 "Improve tests and docs."
qwen-codex --yolo --iterations 5 --yolo-verify-commands "curl -sf http://localhost:2226/health;curl -sf http://localhost:2225" "Improve runtime health."
```

`--yolo-refiner` is the preferred explicit flag. `--yolo` remains supported for compatibility, but upstream Codex also uses `--yolo` as an alias for `--dangerously-bypass-approvals-and-sandbox`.

Without `--iterations`, YOLO runs in infinite mode until an accepted `YOLO_STOP` signal or a safety stop. Safety stops include Ctrl+C, round timeout, repeated prompt guard, repeated failure guard, agent/refiner/provider fatal errors, failed acceptance repair, and context/compaction fatal errors from the delegated Codex process.

With `--iterations N`, `-n N`, or the optional `--10` shorthand, YOLO runs at most `N` rounds. An accepted `YOLO_STOP` may stop earlier. If the acceptance gate is enabled and the run reaches the max iteration limit, acceptance commands run before the final status is marked successful. `--iterations 0` is invalid; omit `--iterations` for infinite mode.

Logs are written to `.qwen-codex/yolo-runs/<run-id>/` as JSON and Markdown, with secrets redacted.

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
QWEN_CODEX_YOLO_ALLOW_REFINER_STOP=true
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

Leave `QWEN_CODEX_YOLO_DEFAULT_ITERATIONS` blank for infinite YOLO mode unless `--iterations` or `-n` is provided. A value of `0` is invalid.

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

For Docker projects, prefer `docker compose config` before build, run `docker compose build` only when necessary, use `docker compose up -d` instead of foreground `up`, wrap long commands with `timeout` where appropriate, and run `docker compose down` after runtime checks. Avoid combining large feature work and heavy Docker verification in one round; if a round uses more than 80% of its timeout, the next refiner prompt is instructed to shrink scope.

## Safety Notes

Qwen Codex can run shell commands and edit files through upstream Codex tools. Review generated changes, keep secrets out of prompts when possible, and use temporary workspaces for destructive experiments. YOLO logs redact common API keys, tokens, authorization headers, `.env` assignments, credentials, and private keys before writing to disk.

## Context Window

Set `QWEN_CODEX_CONTEXT_WINDOW` to match the vLLM `--max-model-len` value. The provided compose file and verified local server use `32768`.

Qwen Codex derives a conservative auto-compact threshold from that value:

```text
threshold = min(context_window * 0.80, context_window - 4096)
```

For `32768`, the default Qwen auto-compact threshold is `26214` tokens. Long or infinite YOLO runs depend on upstream Codex compaction and should be stress-tested before unattended production use.

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

The provided compose maps host `http://127.0.0.1:8002/v1` to container port `8000`. Inside that Docker Compose network, use the service hostname and container port, for example `http://qwen-codex-model:8000/v1`. Do not use host port `8000` if another local service already owns it.

When syncing from upstream, prefer merging `upstream/main` into this fork's `main` so Qwen-specific history remains visible:

```sh
git fetch upstream
git checkout main
git merge upstream/main
```

See [docs/upstream-sync.md](docs/upstream-sync.md) for conflict priorities and validation steps.

## Documentation

- [Classroom Qwen/vLLM compose](docker-compose.yml)
- [YOLO mode](docs/yolo-mode.md)
- [Verification](docs/verification.md)
- [Research notes](docs/research-notes.md)
- [Upstream sync](docs/upstream-sync.md)
- [Roadmap](docs/roadmap.md)
- [Contributing](CONTRIBUTING.md)

The plug-and-play local demo compose lives at `docker-compose.yml`.
