# YOLO Mode

YOLO mode is an autonomous refinement loop for Qwen Codex. It does not implement a separate agent. Each iteration invokes the normal upstream Codex execution path through `codex exec`, then asks an OpenAI-compatible refiner model for the next prompt to inject into the same resumed Codex thread.

## Commands

```sh
qwen-codex --yolo-refiner "Create a minimal README for this project."
qwen-codex --yolo-refiner --iterations 10 "Refine this project."
qwen-codex --yolo-refiner -n 10 "Refine this project."
```

`--yolo-refiner` is the preferred explicit flag. `--yolorefiner` is accepted as a compatibility alias. `--yolo` is also accepted for backward compatibility with early Qwen Codex builds, but upstream Codex uses `--yolo` as an alias for `--dangerously-bypass-approvals-and-sandbox`, so new scripts should prefer `--yolo-refiner`.

The optional numeric shorthand is supported when it is unambiguous:

```sh
qwen-codex --yolo-refiner --10 "Refine this project."
```

Prefer `--iterations 10` or `-n 10` in scripts and documentation.

Autonomous runs can use upstream Codex's dangerous approval/sandbox bypass only when explicitly requested:

```sh
qwen-codex --yolo-refiner --iterations 5 --dangerously-bypass-approvals-and-sandbox "Refine this project."
```

Without that flag, YOLO agent rounds run through upstream Codex with the workspace-write sandbox. Qwen Codex does not silently bypass approvals in normal or YOLO mode.

## Configuration

YOLO refiner settings use official `QWEN_CODEX_*` variables first, then supported legacy aliases:

```sh
QWEN_CODEX_YOLO_REFINER_BASE_URL=http://127.0.0.1:8002/v1
QWEN_CODEX_YOLO_REFINER_API_KEY=local-dev-key
QWEN_CODEX_YOLO_REFINER_MODEL=qwen35-local
QWEN_CODEX_YOLO_LOG_DIR=.qwen-codex/yolo-runs
QWEN_CODEX_YOLO_ROUND_TIMEOUT_SECS=600
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
--yolo-round-timeout-secs <N>
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
- A single agent round exceeds `QWEN_CODEX_YOLO_ROUND_TIMEOUT_SECS`, which defaults to 600 seconds.
- The normal Codex agent subprocess exits nonzero without an explicit YOLO interrupt.
- The refiner returns `YOLO_STOP`.
- The repeated-prompt guard triggers.
- The consecutive-failure guard triggers.
- Ctrl+C is received; the process stops cleanly between rounds.

When a round times out, YOLO stops the run instead of trying to refine from an incomplete agent result. The iteration log records `stopReason: "round_timeout"` and an error such as `agent round timed out after 600 second(s)`. This prevents a stuck local model/tool turn from blocking unattended runs forever.

When the child Codex process exits nonzero without an explicit Ctrl+C/SIGTERM observed by the YOLO wrapper, YOLO records `stopReason: "agent_error"`, stores the exit code and a sanitized output/stderr excerpt, skips the refiner for that failed round, writes all logs, and stops cleanly. If an explicit interrupt was received, the run records `stopReason: "interrupted"` instead. The per-iteration fields `interruptReceived`, `timeoutOccurred`, `agentProcessExitCode`, `agentProcessSignal`, `agentRoundDurationSeconds`, `agentFinishedNormally`, and `refinerSkippedReason` make the distinction visible.

## Logging

Each run creates a directory under:

```text
.qwen-codex/yolo-runs/<run-id>/
```

Files written:

```text
run.json
run.md
analysis.json
analysis.md
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
- Agent subprocess exit code/signal
- Timeout and interrupt flags
- Refiner skipped reason
- Refiner input summary
- Refiner raw response
- Next prompt injected into the agent
- Stop reason when applicable

Secrets are redacted before logs are written. The redactor covers common API keys, tokens, authorization headers, `.env` style secret assignments, credential fields, and private key blocks.

JSON logs are redacted field-by-field before serialization, so raw newlines, ANSI escape sequences, interrupted output, and secret-like `.env` assignments remain valid JSON. Markdown logs are for human review only.

`analysis.json` is a compact whole-run summary designed for automation. It includes completed iteration count, final status, per-round previews, refiner call status, round-chaining checks, important project artifacts, secret-redaction checks, and diagnostics grouped as timeouts, interruptions, agent errors, provider errors, and refiner errors. `analysis.md` is the same review in a concise human-readable form.

The refiner receives bounded context derived from the latest iteration summary, changed files, errors, git status, and diff summary. Qwen Codex avoids passing full raw logs or large shell heredocs into the refiner request.

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
