# YOLO Mode

YOLO mode is an autonomous refinement loop for Qwen Codex. It does not implement a separate agent. Each iteration invokes the normal upstream Codex execution path through `codex exec`, then asks an OpenAI-compatible refiner model for the next prompt to inject into the same resumed Codex thread.

## Commands

```sh
qwen-codex --yolo "Create a minimal README for this project."
qwen-codex --yolo --iterations 10 "Refine this project."
qwen-codex --yolo -n 10 "Refine this project."
```

The optional numeric shorthand is supported when it is unambiguous:

```sh
qwen-codex --yolo --10 "Refine this project."
```

Prefer `--iterations 10` or `-n 10` in scripts and documentation.

## Configuration

YOLO refiner settings use official `QWEN_CODEX_*` variables first, then supported legacy aliases:

```sh
QWEN_CODEX_YOLO_REFINER_BASE_URL=http://127.0.0.1:8002/v1
QWEN_CODEX_YOLO_REFINER_API_KEY=local-dev-key
QWEN_CODEX_YOLO_REFINER_MODEL=qwen35-local
QWEN_CODEX_YOLO_LOG_DIR=.qwen-codex/yolo-runs
QWEN_CODEX_YOLO_DEFAULT_ITERATIONS=
QWEN_CODEX_YOLO_MAX_REPEATED_PROMPTS=3
QWEN_CODEX_YOLO_MAX_FAILURES=3
```

Aliases:

```sh
YOLO_REFINER_BASE_URL
YOLO_REFINER_API_KEY
YOLO_REFINER_MODEL
YOLO_LOG_DIR
YOLO_DEFAULT_ITERATIONS
```

CLI flags override environment values:

```sh
--refiner-base-url <URL>
--refiner-api-key <KEY>
--refiner-model <MODEL>
--yolo-log-dir <DIR>
--max-repeated-prompts <N>
--max-failures <N>
```

## Loop Behavior

For each iteration, Qwen Codex:

1. Runs one normal Codex agent round through the upstream `codex exec` path.
2. Captures the prompt, final output, structured JSON events, tool-call summaries when exposed, commands, changed files, git status, git diff summary, and errors.
3. Builds a redacted summary.
4. Sends that summary to the configured refiner model via `/v1/chat/completions`.
5. Uses the refiner response as the next prompt and resumes the same Codex thread.

The loop stops when:

- The fixed iteration limit is reached.
- The refiner returns `YOLO_STOP`.
- The repeated-prompt guard triggers.
- The consecutive-failure guard triggers.
- Ctrl+C is received; the process stops cleanly between rounds.

## Logging

Each run creates a directory under:

```text
.qwen-codex/yolo-runs/<run-id>/
```

Files written:

```text
run.json
run.md
iteration-001.json
iteration-001.md
iteration-002.json
iteration-002.md
```

Iteration logs include:

- Session ID
- Iteration number
- Timestamp
- Original user prompt
- Current injected prompt
- Agent output summary
- Actions and tool-call summaries
- Changed files
- Git diff summary
- Commands/tests run
- Errors
- Current git status
- Refiner input summary
- Refiner raw response
- Next prompt injected into the agent
- Stop reason when applicable

Secrets are redacted before logs are written. The redactor covers common API keys, tokens, authorization headers, `.env` style secret assignments, credential fields, and private key blocks.

## Refiner Prompt

The default refiner system prompt asks the model to act as a senior product and engineering refinement strategist. It instructs the refiner to produce only the next concrete coding-agent prompt, avoid repeating completed work, prefer small verifiable steps, include tests to run, and output `YOLO_STOP` when no useful work remains.

## Architecture Notes

YOLO mode lives in the Qwen wrapper crate and calls the upstream Codex binary. The first iteration starts a normal `codex exec --json --skip-git-repo-check` turn. Later iterations use `codex exec --json --skip-git-repo-check resume <thread-id> <prompt>`.

Because the normal Codex thread is resumed, future upstream changes to tools, skills, MCP handling, shell execution, file editing, and context compaction are automatically available to YOLO mode.

## Context Management

YOLO mode relies on upstream Codex context compaction. It does not implement a separate compactor.

For Qwen Codex, `QWEN_CODEX_CONTEXT_WINDOW` must match the vLLM `--max-model-len` value. The verified default is:

```text
QWEN_CODEX_CONTEXT_WINDOW=32768
```

Qwen Codex derives a conservative auto-compact threshold from the configured context window:

```text
threshold = min(context_window * 0.80, context_window - 4096)
```

For the verified 32768-token model server, the threshold is `26214` tokens.

This threshold is passed to upstream Codex as `model_auto_compact_token_limit`, while `model_context_window` is passed as `32768`. Long or unlimited YOLO runs depend on this propagation so upstream compaction starts before vLLM reaches the hard context limit.

Status: config propagation is confirmed and covered by tests, but long-running YOLO compaction has not yet been stress-tested.
