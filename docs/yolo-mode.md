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
QWEN_CODEX_YOLO_ROUND_GOAL_MAX_FILES=8
QWEN_CODEX_YOLO_ROUND_GOAL_MAX_ACTIONS=5
QWEN_CODEX_YOLO_ROUND_GOAL_MAX_TESTS=3
QWEN_CODEX_YOLO_REFINER_MAX_PROMPT_CHARS=3000
QWEN_CODEX_YOLO_REFINER_STYLE=incremental
QWEN_CODEX_YOLO_CONTINUE_AFTER_TIMEOUT=false
QWEN_CODEX_YOLO_ALLOW_REFINER_STOP=false
QWEN_CODEX_YOLO_VERIFY_COMMANDS=
QWEN_CODEX_YOLO_VERIFY_TIMEOUT_SECS=15
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
--yolo-round-max-files <N>
--yolo-round-max-actions <N>
--yolo-round-max-tests <N>
--yolo-next-prompt-max-chars <N>
--yolo-refiner-style <STYLE>
--yolo-continue-after-timeout
--yolo-allow-refiner-stop
--yolo-verify-commands "cmd1;cmd2"
--yolo-verify-timeout-secs <N>
--yolo-acceptance-gate
--yolo-acceptance-command "cmd"
--yolo-acceptance-max-secs <N>
--max-repeated-prompts <N>
--max-failures <N>
```

## Loop Behavior

For each iteration, Qwen Codex:

1. Runs one normal Codex agent round through the upstream `codex exec` path.
2. Captures the prompt, final output, structured JSON events, tool-call summaries when exposed, commands, changed files, git status, git diff summary, and errors.
3. Builds a redacted summary.
4. Runs optional external verification commands configured by `--yolo-verify-commands` or `QWEN_CODEX_YOLO_VERIFY_COMMANDS`.
5. Sends the round summary and external verification results to the configured refiner model via `/v1/chat/completions`.
6. Uses the refiner response as the next prompt and resumes the same Codex thread.

With `--iterations N`, the normal stop reason is `max_iterations` after exactly `N` completed rounds. The refiner cannot stop the run by default. If it returns `YOLO_STOP`, Qwen Codex treats that as an invalid next prompt and locally repairs it into a focused prompt that asks the agent to verify and improve the most important remaining acceptance criterion.

Without `--iterations`, or with `--iterations 0`, YOLO runs indefinitely until Ctrl+C, timeout, the failure guard, or a fatal agent/refiner error.

The first agent round receives a small YOLO-only execution note appended to the user's original prompt. That note tells the agent it is in round 1 of a multi-round autonomous run, should prefer a minimal runnable baseline, should respect the configured round budget where practical, and should stop after summarizing verification. Later rounds get the same scoping through the refiner-generated next prompt.

The loop stops when:

- The fixed iteration limit is reached.
- A single agent round exceeds `QWEN_CODEX_YOLO_ROUND_TIMEOUT_SECS`, which defaults to 600 seconds.
- The normal Codex agent subprocess exits nonzero without an explicit YOLO interrupt.
- The repeated-prompt guard triggers.
- The consecutive-failure guard triggers.
- Ctrl+C is received; the process stops cleanly between rounds.

Optional early stop by refiner is disabled by default and is discouraged for product-generation workflows. It can be enabled explicitly with `--yolo-allow-refiner-stop` or `QWEN_CODEX_YOLO_ALLOW_REFINER_STOP=true`; only then can `YOLO_STOP` produce `stopReason: "refiner_stop_signal"` without acceptance gating.

For product-generation workflows, prefer the acceptance gate instead of unconditional early stop. With `--yolo-acceptance-gate` or `QWEN_CODEX_YOLO_ACCEPTANCE_GATE=true`, a refiner `YOLO_STOP` is accepted only after the configured acceptance commands pass. If any command fails, YOLO records `stopSignalReceived=true`, `stopSignalRejected=true`, writes `acceptanceResults`, asks the refiner for a bounded repair prompt using the failed command output, and continues if the iteration and failure guards allow it.

When a round times out, YOLO stops the run instead of trying to refine from an incomplete agent result. The iteration log records `stopReason: "round_timeout"` and an error such as `agent round timed out after 600 second(s)`. This prevents a stuck local model/tool turn from blocking unattended runs forever.

`QWEN_CODEX_YOLO_CONTINUE_AFTER_TIMEOUT` exists for future controlled retry workflows and defaults to `false`. With the safe default, the refiner is skipped after timeout and the run stops cleanly. If enabled, timeout counts as a failure and the refiner is asked for a smaller retry prompt subject to the same failure guard.

When the child Codex process exits nonzero without an explicit Ctrl+C/SIGTERM observed by the YOLO wrapper, YOLO records `stopReason: "agent_error"`, stores the exit code and a sanitized output/stderr excerpt, skips the refiner for that failed round, writes all logs, and stops cleanly. If an explicit interrupt was received, the run records `stopReason: "interrupted"` instead. The per-iteration fields `interruptReceived`, `timeoutOccurred`, `agentProcessExitCode`, `agentProcessSignal`, `agentRoundDurationSeconds`, `agentFinishedNormally`, and `refinerSkippedReason` make the distinction visible.

## Round Budget And Prompt Validation

YOLO round budgets are advisory limits that shape the refiner's next prompt:

- `QWEN_CODEX_YOLO_ROUND_GOAL_MAX_FILES`, default `8`.
- `QWEN_CODEX_YOLO_ROUND_GOAL_MAX_ACTIONS`, default `5`.
- `QWEN_CODEX_YOLO_ROUND_GOAL_MAX_TESTS`, default `3`.
- `QWEN_CODEX_YOLO_REFINER_MAX_PROMPT_CHARS`, default `3000`.
- `QWEN_CODEX_YOLO_REFINER_STYLE`, default `incremental`.

The refiner prompt must use a structured shape with `Title`, `Context`, `Task`, `Constraints`, `Acceptance criteria`, `Verification commands`, and `Stop condition`. Before YOLO injects the next prompt, Qwen Codex validates that it fits the configured length, has a clear objective, includes acceptance criteria or verification commands, avoids broad instructions such as "finish everything", is not a repeat of the previous prompt, and tells the agent when to stop.

If the prompt is too broad or incomplete, YOLO attempts a local repair into a smaller scoped task. The iteration log records `nextPromptValidationPassed`, `nextPromptValidationIssues`, `nextPromptRepaired`, and `roundBudget`. If repair cannot produce a valid prompt, the run stops with `refiner_error`.

## External Verification

External verification lets YOLO check real runtime acceptance criteria outside the model transcript before asking the refiner for the next prompt.

```sh
qwen-codex --yolo --iterations 5 \
  --yolo-verify-commands "curl -sf http://localhost:2226/health;curl -sf http://localhost:2226/api/products;curl -sf http://localhost:2225" \
  "Improve this Dockerized app."
```

Commands are separated by semicolons. Basic single and double quotes are respected when splitting the command string. Each command runs after a successful agent round and before the refiner call. Each command has an independent timeout, configured by `QWEN_CODEX_YOLO_VERIFY_TIMEOUT_SECS` or `--yolo-verify-timeout-secs`, defaulting to 15 seconds. Logs include:

```json
"externalVerification": [
  {
    "command": "curl -sf http://localhost:2225",
    "exitCode": 1,
    "output": "...",
    "durationMs": 1234
  }
]
```

If any command fails, the refiner context includes `EXTERNAL VERIFICATION FAILED:` with the failing command, exit code, and bounded output. The refiner prompt instructs the model to target those failures before adding unrelated scope. Without configured verification commands, YOLO behavior is unchanged.

## Acceptance Gate

The acceptance gate prevents a run from being marked complete while real runtime checks still fail.

```sh
qwen-codex --yolo-refiner --iterations 6 \
  --yolo-acceptance-gate \
  --yolo-acceptance-command "docker compose config" \
  --yolo-acceptance-command "docker compose build" \
  --yolo-acceptance-command "docker compose up -d && sleep 8 && curl -fsS http://localhost:2226/health && curl -fsS http://localhost:2225; status=$?; docker compose down; exit $status" \
  "Make this Dockerized app runnable."
```

Environment equivalents:

```text
QWEN_CODEX_YOLO_ACCEPTANCE_GATE=true
QWEN_CODEX_YOLO_ACCEPTANCE_COMMANDS=docker compose config;docker compose build
QWEN_CODEX_YOLO_ACCEPTANCE_MAX_SECONDS=300
QWEN_CODEX_YOLO_REJECT_STOP_ON_FAILED_ACCEPTANCE=true
```

Acceptance commands run when the refiner emits `YOLO_STOP`, and fixed-iteration runs also run them before stopping at `max_iterations`. Each command runs in the workspace, has a bounded timeout, and writes a redacted result under `acceptanceResults`. If a stop signal command fails and rejection is enabled, the stop signal is rejected and the next round receives a repair prompt focused on the failed acceptance output. If final acceptance fails at `max_iterations`, the run stops as requested but `finalStatus` is `partial`, not `success`. For browser-based Docker projects, the refiner is instructed to treat frontend HTTP 500s and browser-unreachable service hostnames such as `http://backend:8000` as blockers.

Docker builds can be slow. The default round timeout remains 600 seconds for general use; for Docker-heavy YOLO runs, pass `--yolo-round-timeout-secs 1200` so one build/fix round has enough time to hand control back cleanly.

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
- Action diagnostics: `actionsCapturedCount`, `toolCallsCapturedCount`, `commandsCapturedCount`, `filesChangedCount`, and `noActionRound`
- Changed files
- Git diff summary
- Commands/tests run
- Errors
- External verification results
- Acceptance gate results and stop-signal acceptance/rejection details
- Current git status
- Agent subprocess exit code/signal
- Timeout and interrupt flags
- Refiner skipped reason
- Refiner input summary
- Refiner raw response
- Next prompt injected into the agent
- Next prompt validation result, issues, and repair status
- Round budget used for that iteration
- Stop reason when applicable

Secrets are redacted before logs are written. The redactor covers common API keys, tokens, authorization headers, `.env` style secret assignments, credential fields, and private key blocks.

JSON logs are redacted field-by-field before serialization, so raw newlines, ANSI escape sequences, interrupted output, and secret-like `.env` assignments remain valid JSON. Markdown logs are for human review only.

`analysis.json` is a compact whole-run summary designed for automation. It includes completed iteration count, final status, per-round previews, refiner call status, acceptance-gate results, action/tool diagnostics, round-chaining checks, important project artifacts, secret-redaction checks, and diagnostics grouped as timeouts, interruptions, agent errors, provider errors, and refiner errors. `analysis.md` is the same review in a concise human-readable form.

The refiner receives bounded context derived from the latest iteration summary, changed files, errors, git status, and diff summary. Qwen Codex avoids passing full raw logs or large shell heredocs into the refiner request.

## Refiner Prompt

The default refiner system prompt asks the model to act as a senior product and engineering iteration planner. It instructs the refiner to produce only the next concrete coding-agent prompt, avoid repeating completed work, prefer one small verifiable step, fix build/runtime blockers before adding scope, include acceptance criteria and exact verification commands, and stop the agent after the scoped task. It may emit `YOLO_STOP` only when acceptance evidence proves the original goal and runtime checks are complete; otherwise it must return a focused next prompt.

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
