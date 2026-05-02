# Verification

Last updated: 2026-05-02T20:56:34Z

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

## Final Capability Verification

Capability verification was run on 2026-05-02 against the local Qwen/vLLM server at `http://127.0.0.1:8002/v1` with `QWEN_CODEX_MODEL=qwen35-local` and `QWEN_CODEX_CONTEXT_WINDOW=32768`.

Copied 10-round stress logs:

```text
Source: /tmp/qwen-yolo-stress/.qwen-codex/yolo-runs/20260502T112858Z-331475
Windows copy: /mnt/c/Users/eduar/Documents/qwen-codex-yolo-logs/stress-10rounds-20260502T112858Z-331475
```

10-round stress inspection:

- `completedIterations: 10`
- `stopReason: max_iterations`
- `finalStatus: success`
- Round chaining: pass, with every `agentInputPrompt` matching the previous `nextPrompt`.
- Every round had `noActionRound=false`, `filesChangedCount=1`, and nonzero command/action capture.
- Iteration action counts were `7, 4, 5, 3, 7, 5, 2, 2, 1, 2`.
- Iteration command counts matched action counts.
- `toolCallsSummary` was empty in every round, but that does not mean shell/file tools were unused. Shell commands were logged under `commandsTestsRun` and `actionsTaken`; changed files were logged under `filesChanged`.
- The stress project exists at `/tmp/qwen-yolo-stress/notes_cli` with `README.md`, `cli.py`, and `test_cli.py`. Running `python3 test_cli.py` from that directory passed, then `python3 cli.py add "Inspect"` and `python3 cli.py list` worked.

Focused tool/action diagnostic:

- Normal mode workspace: `/tmp/qwen-tool-diagnostic`.
- Normal mode created `tools-test.txt` containing `CONFIRMED`.
- YOLO workspace: `/tmp/qwen-yolo-tool-diagnostic`.
- YOLO run logs: `/tmp/qwen-yolo-tool-diagnostic/.qwen-codex/yolo-runs/20260502T203344Z-1262010`.
- YOLO created `tools-test.txt` containing `CONFIRMED`.
- YOLO diagnostics: `actionsCapturedCount=2`, `commandsCapturedCount=2`, `filesChangedCount=2`, `toolCallsCapturedCount=0`, `noActionRound=false`.

Capability table:

| Capability | Normal Mode | YOLO Mode | Status | Evidence | Limitation |
| --- | --- | --- | --- | --- | --- |
| Shell command execution | PASS | PASS | PASS | Normal `/tmp/qwen-capability-suite/add.py` prints `4`; YOLO diagnostic logged two shell commands and the 10-round stress logged commands every round. | Local model turns can still be slow. |
| File creation | PASS | PASS | PASS | Normal `hello.txt` and `add.py`; YOLO `tools-test.txt`; stress `README.md`, `cli.py`, `test_cli.py`. | None found for simple file creation. |
| File reading | PASS | PASS | PASS | Normal read prompt output referenced `HELLO_QWEN_CODEX`; YOLO diagnostic ran `cat /tmp/qwen-yolo-tool-diagnostic/tools-test.txt`. | In YOLO, file-read evidence is command/action logging, not `toolCallsSummary`. |
| File editing | PASS | PASS | PASS | Normal edited `hello.txt` to `HELLO_QWEN_CODEX_EDITED`; 10-round YOLO stress updated the notes project across all rounds. | No isolated YOLO edit-only test was run beyond project iteration. |
| Multi-file project | PARTIAL | PARTIAL | PARTIAL | Normal mode created `package.json`, `README.md`, and `src/server.js`; YOLO stress built a working notes CLI project. YOLO mini project created `README.md`, `notes.py`, and `test_notes.py`. | Normal Express test failed the exact check because `server.js` was under `src/` while `package.json` starts `node server.js`. YOLO mini run hit `round_timeout` and produced failing tests. |
| Web search | PARTIAL | NOT_TESTED | CAPABILITY_MISSING for native web search | Normal prompt used shell `curl` against `https://nodejs.org/dist/index.json` and found `v24.15.0` LTS `Krypton`. | No dedicated `web_search`/browser tool was exposed or used in this local environment. Shell network fetch works, but that is not native web-search parity. |
| PDF generation | PASS | NOT_TESTED | PASS | `/tmp/qwen-capability-suite/test.pdf`: `PDF document, version 1.4, 1 page(s)`. | Text extraction was not available because `pdftotext` is not installed. |
| DOCX generation | PASS | NOT_TESTED | PASS | `/tmp/qwen-capability-suite/test.docx`: `Microsoft Word 2007+`; `word/document.xml` contains the requested sentence. | Normal-mode shell/file workflow can create DOCX, but there is no dedicated document helper. |
| XLSX generation | PASS | NOT_TESTED | PASS | `/tmp/qwen-capability-suite/test.xlsx`: `Microsoft Excel 2007+`; `xl/worksheets/sheet1.xml` contains `Name`, `Age`, `City`, `Alice`, `30`, and `Lisbon`. | Normal-mode shell/file workflow can create XLSX, but there is no dedicated spreadsheet helper. |
| Docker compose workflow | NOT_TESTED in this pass | PASS | PASS | Prior six-round ecommerce acceptance-gated run passed `docker compose config`, `docker compose build`, backend `/health`, backend `/api/products`, and frontend `/`. | Docker rounds can still consume most of the timeout and need narrower prompts. |
| YOLO round chaining | N/A | PASS | PASS | 10-round stress and YOLO mini both report `allRoundInputsMatchPreviousNextPrompt=true`. | None found in logged handoffs. |
| Acceptance gate | N/A | PASS/PARTIAL | PARTIAL | Focused tests cover rejected `YOLO_STOP`; ecommerce acceptance gate passed real compose/build/runtime checks; fixed-iteration final acceptance behavior is covered. | Live impossible-acceptance run did not trigger a refiner `YOLO_STOP`, so the live rejected-stop path remains unobserved. |
| Infinite mode | N/A | PASS | PASS | Focused tests cover omitted `--iterations` as infinite config and safety stops. | Live infinite run was not left unbounded; stability was exercised with bounded stress. |
| Context/auto-compact | PASS config propagation | PASS config propagation | CONFIG-PROPAGATION-CONFIRMED-BUT-COMPACTION-NOT-TRIGGERED | Normal logs show `context_window=32768 auto_compact_token_limit=26214`. 10-round stress had no provider/context errors. | No compaction event was observed. A larger controlled token-growth stress test is still needed. |
| Tool/action logging semantics | PASS | PASS | PASS | Code inspection and YOLO diagnostics show shell/file actions in `actionsTaken`, `commandsTestsRun`, and `filesChanged`; `toolCallsSummary` tracks `mcp_tool_call`, `collab_tool_call`, and `web_search` items. | `toolCallsSummary=[]` is not a shell/file no-op signal. Consumers should use the canonical action fields. |

Normal-mode capability suite workspace:

```text
/tmp/qwen-capability-suite
```

Results:

- File creation: PASS, `hello.txt` contains `HELLO_QWEN_CODEX`.
- File reading: PASS, `normal-02-read.log` references `HELLO_QWEN_CODEX`.
- File editing: PASS, `hello.txt` contains exactly `HELLO_QWEN_CODEX_EDITED`.
- Command execution: PASS, `python3 add.py` outputs `4`.
- Multi-file Express project: PARTIAL. The model created `package.json`, `README.md`, and `src/server.js` with `GET /health` on port `2228`, but `package.json` starts `node server.js` and no root `server.js` exists.
- Web search: CAPABILITY_MISSING for native web-search/browser tools. The model used shell `curl` to fetch Node release data and found `v24.15.0` LTS `Krypton` from `nodejs.org`.
- PDF: PASS, `file test.pdf` reports a one-page PDF.
- DOCX: PASS, `file test.docx` reports Microsoft Word 2007+ and the requested text appears in `word/document.xml`.
- XLSX: PASS, `file test.xlsx` reports Microsoft Excel 2007+ and the requested row appears in `xl/worksheets/sheet1.xml`.

YOLO mini capability workspace:

```text
/tmp/qwen-yolo-capability
/tmp/qwen-yolo-capability/.qwen-codex/yolo-runs/20260502T204115Z-1279469
```

Result:

- `run.json` and `analysis.json`: valid JSON.
- `completedIterations: 3`.
- `stopReason: round_timeout`.
- `finalStatus: timeout`.
- Round chaining: pass.
- Secret redaction: pass; no `local-dev-key` or `Authorization` string found in YOLO logs.
- Project files: `notes_cli/README.md`, `notes_cli/notes.py`, and `notes_cli/test_notes.py`.
- Runtime quality: partial/fail. Round 3 timed out after 600 seconds, `notes.py` was incomplete, and `pytest -q` failed with `NameError` for missing imported functions in the generated tests.

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
- `cd codex-rs && ./target/debug/qwen-codex --yolo --iterations 0 --yolo-log-dir <tmpdir> "Smoke prompt"`: now expected to fail because zero is invalid; omit `--iterations` for infinite mode.
- `PATH="$HOME/.local/bin:$PATH" just bazel-lock-update`: passed.
- `PATH="$HOME/.local/bin:$PATH" just bazel-lock-check`: passed.
- `PATH="$HOME/.local/bin:$PATH" pnpm run format`: passed with the existing Node engine warning because this machine has Node `v20.20.0` while the repo asks for Node `>=22`.
- `git diff --check`: passed.

YOLO behavior covered by tests:

- CLI parsing for `--yolo`, `--iterations`, `-n`, and the optional `--10` shorthand.
- Fixed iteration limits.
- Omitted `--iterations` runs in infinite mode.
- `--iterations N`, `-n N`, and `--10` run at most `N` rounds.
- `--iterations 0` is invalid; zero is not used to mean infinite.
- `YOLO_STOP` is accepted by default unless the acceptance gate rejects it or refiner stop is explicitly disabled.
- External verification commands are recorded in iteration logs and included in refiner context before the next prompt is generated.
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

## YOLO Timeout And JSON Log Robustness

The 5-round ecommerce UX run was paused after it exposed two Qwen Codex issues: the first agent round created files but did not hand control back to YOLO after more than 10 minutes, and interrupted JSON logs became invalid when post-serialization redaction rewrote escaped `.env` content.

Fix verification:

- `cd codex-rs && cargo test -p codex-qwen yolo`: passed, 23 tests.
- `cd codex-rs && cargo build -p codex-cli`: passed.
- `cd codex-rs && just fix -p codex-qwen`: passed.
- `qwen-codex --help`: passed and documents `--yolo-round-timeout-secs <N>` plus `QWEN_CODEX_YOLO_ROUND_TIMEOUT_SECS`.
- Timeout smoke workspace: `/tmp/yolo-timeout-smoke`.
- Timeout smoke command: `qwen-codex --yolo-refiner --iterations 5 --yolo-round-timeout-secs 1 "Create a file called timeout.txt with the word TIMEOUT inside it, then explain what you did."`
- Timeout smoke result: process exited `0` after one iteration with `RoundTimeout`; no refiner response or next prompt was written.
- Log path: `/tmp/yolo-timeout-smoke/.qwen-codex/yolo-runs/20260501T220234Z-1617463`.
- JSON validation: `python3 -m json.tool run.json` passed; `python3 -m json.tool iteration-001.json` passed.
- Parsed stop reasons: `run.json.stopReason == "round_timeout"` and `iteration-001.json.stopReason == "round_timeout"`.
- Parsed error: `agent round timed out after 1 second(s)`.

The ecommerce project itself was not evaluated further in this pass. The next planned step is to rerun the five-round ecommerce test after this timeout/logging fix is committed and pushed.

## YOLO Agent Error Classification And Analysis Logs

Old ecommerce run inspected:

```text
Copied logs: /mnt/c/Users/eduar/Documents/qwen-codex-yolo-logs/ecommerce-interrupted-20260501T214039Z-1577146
Original logs: /tmp/qwen-yolo-ecommerce/.qwen-codex/yolo-runs/20260501T214039Z-1577146
```

Findings:

- The round generated real project files, then the child Codex process exited with code `1`.
- The old iteration log contained `resources/templates/list failed: unknown MCP server 'filesystem'`.
- The old iteration log contained `failed to parse function arguments: unknown field 'seed', expected 'step' or 'status'`.
- The wrapper also had an interrupt marker from the manual stop attempt, so the old `stopReason: "interrupted"` was ambiguous.
- The refiner was skipped because the run stopped before a clean agent handoff.

Fix verification:

- `cd codex-rs && cargo test -p codex-qwen yolo -- --nocapture`: passed, 31 tests.
- `cd codex-rs && cargo test -p codex-core unknown_field -- --nocapture`: passed, 1 matching test.
- `cd codex-rs && cargo build -p codex-cli`: passed.
- Timeout smoke workspace: `/tmp/qwen-yolo-timeout-analysis`.
- Timeout smoke log path: `/tmp/qwen-yolo-timeout-analysis/.qwen-codex/yolo-runs/20260501T235704Z-1822173`.
- Timeout smoke JSON validation: `run.json`, `iteration-001.json`, and `analysis.json` parsed with `python3 -m json.tool`.
- Timeout parsed result: `stopReason == "round_timeout"`, `refinerSkippedReason == "timeout"`, `analysis.finalStatus == "timeout"`.
- Agent-error smoke workspace: `/tmp/qwen-yolo-agent-error`.
- Agent-error smoke log path: `/tmp/qwen-yolo-agent-error/.qwen-codex/yolo-runs/20260501T235722Z-1822875`.
- Agent-error parsed result: `run.json.stopReason == "agent_error"`, `iteration-001.json.stopReason == "agent_error"`, `agentProcessExitCode == 1`, and `refinerSkippedReason == "agent_error"`.

MCP/filesystem conclusion:

- Normal Qwen Codex and YOLO both invoke the upstream Codex agent path through `codex exec`.
- The logged `filesystem` MCP name was not a default registered server in this configuration. It appears to be a model-requested MCP resource server name rather than a missing YOLO-only setup path.
- The fix does not add a separate filesystem MCP implementation. YOLO continues to rely on the same shell/file-editing tools as normal Codex mode.

Malformed `update_plan` argument handling:

- Extra model-generated fields such as `seed` in `update_plan.plan[]` are now ignored for that narrow tool parser.
- Required `step` and `status` fields still remain validated.
- Missing required fields still return a controlled tool error.

Unified analysis logs:

- New runs write `analysis.json` and `analysis.md` next to `run.json`, `run.md`, and per-iteration logs.
- `analysis.json` records whole-run status, round chaining, refiner call status, subprocess exit fields, timeouts, interruptions, agent errors, provider/refiner errors, and important project artifacts.
- The five-round ecommerce test is still pending after this fix commit.

## YOLO 5-Round Ecommerce UX Review

Command used:

```text
qwen-codex --yolo-refiner --iterations 5 --yolo-round-timeout-secs 1200 --dangerously-bypass-approvals-and-sandbox "<Dockerized ecommerce prompt>"
```

Run paths:

- Workspace: `/tmp/qwen-yolo-ecommerce`
- Logs: `/tmp/qwen-yolo-ecommerce/.qwen-codex/yolo-runs/20260501T235922Z-1827070`
- Analysis JSON: `/tmp/qwen-yolo-ecommerce/.qwen-codex/yolo-runs/20260501T235922Z-1827070/analysis.json`
- Analysis Markdown: `/tmp/qwen-yolo-ecommerce/.qwen-codex/yolo-runs/20260501T235922Z-1827070/analysis.md`

Result:

- Completed all 5 rounds: `NO`.
- Stop reason: `round_timeout`.
- Final status: `timeout`.
- Completed iterations logged: `4`.
- Rounds 1-3 completed with agent exit code `0`, called the refiner, and wrote `nextPrompt`.
- Round 4 timed out after 1200 seconds, skipped the refiner, and wrote valid final logs.
- Round chaining: `PASS` for all logged handoffs. Round 2 input matched round 1 `nextPrompt`, round 3 input matched round 2 `nextPrompt`, and round 4 input matched round 3 `nextPrompt`.
- Secret redaction: `PASS`; neither `local-dev-key` nor `authorization:` appeared in the run logs.
- JSON validation: `run.json`, `analysis.json`, and all `iteration-*.json` files parsed with `python3 -m json.tool`.

Round review:

| Round | Agent result | Refiner called? | Refiner next prompt summary | Files changed | Errors | Coherent improvement? |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Created initial Docker/backend scaffolding and compose files. | Yes | Asked for core Express/Mongo backend implementation. | Dockerfile/backend and compose artifacts. | None | Yes. It narrowed the next step to backend functionality. |
| 2 | Added backend code, routes, models, and Docker changes. | Yes | Asked for frontend package, Dockerfile, and React implementation. | Backend and compose artifacts. | None | Yes. It identified the missing frontend. |
| 3 | Added frontend scaffolding. | Yes | Asked for detailed React source, cart context, API client, pages, and routing. | Frontend plus previous artifacts. | None | Yes, but it was large for one round. |
| 4 | Began frontend implementation but did not finish before timeout. | No | None, because timeout skipped refiner. | Frontend/backend project files present. | `agent round timed out after 1200 second(s)` | Partial. The timeout prevented another hang and preserved logs. |

Project verification:

- `docker-compose.yml`: present.
- Root `README.md`: missing.
- `frontend/`: present with React source files.
- `backend/`: present with Express/Mongo source files.
- Host ports: `2225`, `2226`, and `2227` appear in `docker-compose.yml`.
- Forbidden host ports: none of the explicitly forbidden host ports were found in compose; `1200` only appeared as a CSS max-width value in frontend code.
- Backend endpoint code: `/health`, `/api/products`, auth register/login, cart routes, and orders routes are present in source.
- Frontend feature code: product listing/detail, cart UI, login/register, and checkout/order flow appear in source.

Docker validation:

- `docker compose config`: exited `0`, but only the backend service was active because generated frontend and mongo services were placed under an invalid-looking `-e` profile. Compose also warned that `version` is obsolete.
- `docker compose build`: `FAIL`. Backend build failed because `Dockerfile.backend` references `seed-data.js`, but that file is missing.
- `docker compose up`: not run because build failed.

UX conclusion:

- The intended autonomous handoff UX worked for three complete refiner cycles.
- The previous ambiguous interruption problem did not recur.
- The fourth round showed that realistic product tasks can still run too long for one agent round. The timeout now prevents indefinite hangs and produces valid logs, but the generated ecommerce project is not release-ready.
- The next product/UX improvement should reduce per-round task scope or add a progress/round-budget strategy so the refiner can split large implementation prompts before a 20-minute round timeout.

## YOLO Bounded Ecommerce Rerun

This section records the previous bounded ecommerce run before external verification and default `YOLO_STOP` suppression were added.

Command used:

```text
qwen-codex --yolo-refiner --iterations 5 --yolo-round-timeout-secs 1200 --dangerously-bypass-approvals-and-sandbox "<incremental Dockerized ecommerce prompt>"
```

Budget environment:

```text
QWEN_CODEX_YOLO_ROUND_GOAL_MAX_FILES=8
QWEN_CODEX_YOLO_ROUND_GOAL_MAX_ACTIONS=5
QWEN_CODEX_YOLO_ROUND_GOAL_MAX_TESTS=3
QWEN_CODEX_YOLO_REFINER_MAX_PROMPT_CHARS=3000
QWEN_CODEX_YOLO_REFINER_STYLE=incremental
```

Run paths:

- Workspace: `/tmp/qwen-yolo-ecommerce`
- Logs: `/tmp/qwen-yolo-ecommerce/.qwen-codex/yolo-runs/20260502T022211Z-2083254`
- Analysis JSON: `/tmp/qwen-yolo-ecommerce/.qwen-codex/yolo-runs/20260502T022211Z-2083254/analysis.json`
- Analysis Markdown: `/tmp/qwen-yolo-ecommerce/.qwen-codex/yolo-runs/20260502T022211Z-2083254/analysis.md`

Result:

- Completed all 5 requested rounds: `NO`; the refiner returned `YOLO_STOP` after 3 completed iterations.
- Stop reason: `refiner_stop_signal`.
- Final infrastructure status: `success`.
- Completed iterations logged: `3`.
- Timeouts: `0`.
- Agent errors: `0`.
- Refiner errors: `0`.
- Round chaining: `PASS`; rounds 2 and 3 inputs matched the previous iteration `nextPrompt`.
- Secret redaction: `PASS`; neither `local-dev-key` nor `authorization:` appeared in logs.
- JSON validation: `run.json`, `analysis.json`, and all `iteration-*.json` files parsed with `python3 -m json.tool`.
- Prompt validation fields: present. Rounds 1 and 2 had `nextPromptValidationPassed=true`, `nextPromptRepaired=false`, and no validation issues. Round 3 returned `YOLO_STOP`, so no next prompt was injected.

Round review:

| Round | Agent result | Refiner called? | Refiner next prompt summary | Duration | Prompt validation | Coherent improvement? |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Created a smaller baseline with root README, compose, backend, frontend, and Mongo service definitions. | Yes | Asked to verify the Docker Compose stack and API responses before adding scope. | 574s | Passed | Yes. This was more bounded than the previous broad frontend prompt. |
| 2 | Started the compose stack and performed verification work. | Yes | Asked to implement backend API logic and frontend fetch logic with a limited file set. | 359s | Passed | Mostly. It was actionable, but assumed some file names and did not catch the frontend runtime issue. |
| 3 | Updated backend/frontend code and verification artifacts. | Yes | Returned `YOLO_STOP`. | 348s | Not applicable | Partial. Infrastructure stopped cleanly, but the generated frontend still failed at runtime. |

Project verification:

- `README.md`: present.
- `docker-compose.yml`: present.
- `backend/`: present.
- `frontend/`: present.
- Services in `docker compose config`: backend, frontend, and mongo are all active.
- Host ports: frontend `2225`, backend `2226`, mongo `2227`.
- Forbidden host ports: none found in compose.
- `docker compose config`: `PASS`, with only the Docker Compose obsolete `version` warning.
- `docker compose build`: `PASS`.
- `docker compose ps`: all three containers were running during verification.
- Backend runtime: `PASS`; `GET http://localhost:2226/health` returned `{"status":"ok"}`, and `GET /api/products` returned a product JSON array.
- Frontend runtime: `FAIL`; `GET http://localhost:2225` returned HTTP 500 because Express called `res.send(indexHtml)` instead of rendering the HTML string. Browser-side `frontend/index.html` also hardcodes `http://backend:8000`, which is not a browser-reachable host URL.
- Cleanup: `docker compose down` was run after verification to free ports.

UX conclusion:

- The round-budget and prompt-validation strategy fixed the previous broad-prompt timeout for this rerun. The run stopped cleanly before the timeout and logs remained valid.
- Refiner prompts were smaller, structured, and contained acceptance criteria and verification commands.
- The generated project is more runnable than the previous attempt because compose config/build pass and backend endpoints work.
- The generated app is not complete production-quality because the frontend route fails at runtime and the refiner stopped too early.
- Next recommended improvement: feed external runtime checks into each refiner turn and prevent default refiner early stop. This is now implemented and needs a fresh live rerun.

## YOLO External Verification Rerun

Command used:

```text
qwen-codex --dangerously-bypass-approvals-and-sandbox --iterations 5 --yolo --yolo-verify-timeout-secs 5 --yolo-verify-commands "curl -sf http://localhost:2226/health;curl -sf http://localhost:2226/api/products;curl -sf http://localhost:2225" "<runnable ecommerce prompt>"
```

Run paths:

- Workspace: `/tmp/qwen-yolo-ecommerce`
- Windows log directory: `C:\Users\eduar\Documents\qwen-codex-yolo-logs\ecommerce-external-verify-final\20260502T042912Z-2327436`
- WSL log directory: `/mnt/c/Users/eduar/Documents/qwen-codex-yolo-logs/ecommerce-external-verify-final/20260502T042912Z-2327436`
- Analysis JSON: `/mnt/c/Users/eduar/Documents/qwen-codex-yolo-logs/ecommerce-external-verify-final/20260502T042912Z-2327436/analysis.json`

Result:

- Completed all 5 requested rounds: `NO`.
- Completed iterations: `2`.
- Stop reason: `round_timeout`.
- Final status: `timeout`.
- Round chaining: `PASS`; round 2 input matched round 1 `nextPrompt`.
- Secret redaction: `PASS`; `local-dev-key` was not present in logs.
- JSON validation: `run.json`, `analysis.json`, `iteration-001.json`, and `iteration-002.json` parsed with `python3 -m json.tool`.
- Refiner early stop: `PASS`; the refiner did not stop the run with `YOLO_STOP`.
- External verification timeout: `PASS`; failed curl checks in round 1 were bounded to roughly 5 seconds each and were logged under `externalVerification`.
- External verification in refiner context: `PASS`; round 1 refiner summary included `EXTERNAL VERIFICATION FAILED:` with the failing curl commands.
- Initial round budget guidance: present; round 1 `agentInputPrompt` included YOLO round 1 execution constraints.

Round review:

| Round | Agent result | Refiner called? | External verification | Stop reason | Notes |
| --- | --- | --- | --- | --- | --- |
| 1 | Created a partial Dockerized scaffold with backend/frontend files and compose. | Yes | All three curl checks timed out after the configured 5 seconds because services were not running yet. | None | The refiner generated a scoped next prompt and it was repaired into a bounded blocker-fix prompt. |
| 2 | Added/modified scaffold files but did not hand back before the per-round timeout. | No | Not run because the agent round timed out. | `round_timeout` | YOLO stopped cleanly because `QWEN_CODEX_YOLO_CONTINUE_AFTER_TIMEOUT=false` by default. |

Project verification after timeout:

- `docker-compose.yml`: present.
- `backend/`: present.
- `frontend/`: present.
- Host ports in compose: frontend `2225`, backend `2226`, mongo `2227`.
- `docker compose config`: `PASS`, with only Docker Compose's obsolete `version` warning.
- `docker compose build`: `FAIL`; frontend Dockerfile uses `pip3 install --break-system-packages`, but the base image's pip does not support that option.
- Runtime curls after build: not run because build failed.

Conclusion:

- Historical note: the requested iteration semantics at that point made `YOLO_STOP` ignored/repaired by default. This has since changed; current default YOLO mode accepts `YOLO_STOP`, and omitted `--iterations` means infinite mode.
- External verification is implemented and worked as intended in the live run.
- The live ecommerce run did not satisfy the desired `completedIterations=5` / `stopReason=max_iterations` acceptance result because the local Qwen agent timed out during round 2. This is now logged honestly as `round_timeout`, not as a refiner stop or interruption.
- Remaining product work: improve agent-round completion behavior for larger generated-app tasks, or provide a user-visible mode to continue after timeout when the partially completed round created useful files.

## YOLO Acceptance-Gated Ecommerce Verification

Primary six-round run:

- Workspace: `/tmp/qwen-yolo-ecommerce`
- Run directory: `/tmp/qwen-yolo-ecommerce/.qwen-codex/yolo-runs/20260502T093942Z-105960`
- Command shape: `qwen-codex --yolo-refiner --iterations 6 --yolo-round-timeout-secs 1200 --yolo-acceptance-gate --yolo-acceptance-command "docker compose config" --yolo-acceptance-command "docker compose build" --yolo-acceptance-command "<docker compose up/curl/down check>" --dangerously-bypass-approvals-and-sandbox "<incremental ecommerce prompt>"`
- JSON validity: `PASS`; `run.json`, `analysis.json`, and all iteration JSON files parsed.
- Completed iterations: `6`.
- Stop reason: `max_iterations`.
- Final status in that run: `success`.
- Round chaining: `PASS`; every continued round input matched the previous `nextPrompt`.
- Secret redaction: `PASS`; no `local-dev-key` or `authorization:` header text appeared in logs.
- Refiner stop signal: none observed in this run.
- Acceptance gate: enabled. Because the refiner did not emit `YOLO_STOP`, acceptance commands were not triggered during the primary run. This exposed a final-status gap, so Qwen Codex now also runs acceptance commands before a fixed-iteration `max_iterations` stop when the acceptance gate is enabled.

Round summary:

| Round | Duration | Agent result | Refiner result | Diagnostics |
| --- | ---: | --- | --- | --- |
| 1 | 145s | Created backend/frontend directories and compose scaffold. | Asked to fix Docker build/runtime. | Actions captured, no stop signal. |
| 2 | 1171s | Performed a large Docker/runtime fix round and added README. | Prompt was repaired because it was too similar/broad. | Long but completed before 1200s timeout. |
| 3 | 55s | Continued runtime fixes. | Asked to fix backend/frontend reachability. | Chained correctly. |
| 4 | 20s | No new captured commands, but existing changed files remained in git diff. | Prompt was repaired as scoped blocker work. | Not classified as no-action because files were still changed. |
| 5 | 151s | Ran more Docker/runtime work. | Asked for final Docker build/runtime blocker verification. | Chained correctly. |
| 6 | 16s | Final agent round completed. | Refiner skipped because iteration limit was reached. | Stopped with `max_iterations`. |

Manual project verification after the six-round run:

- Root `README.md`: present.
- `docker-compose.yml`: present.
- Services: backend, frontend, mongo.
- Host ports: frontend `2225`, backend `2226`, mongo internal-only via `expose: ["27017"]`.
- Browser-unreachable `http://backend:8000`: `PASS`; not found in generated files.
- `docker compose config`: `PASS`.
- `docker compose build`: `PASS`.
- `curl -fsS http://localhost:2226/health`: `PASS`, returned `{"status":"ok"}`.
- `curl -fsS http://localhost:2226/api/products`: `PASS`, returned a JSON product array.
- `curl -fsS http://localhost:2225 | grep -i product`: `PASS`, returned product HTML.
- Cleanup: `docker compose down` was run after verification.

Supplemental final-acceptance smoke after tightening `max_iterations` behavior:

- Run directory: `/tmp/qwen-yolo-ecommerce/.qwen-codex/yolo-runs/20260502T102028Z-198783`
- Command shape: one-iteration YOLO run over the generated ecommerce workspace with the same three acceptance commands.
- Stop reason: `max_iterations`.
- Final status: `success`.
- Completed iterations: `1`.
- Acceptance results: `PASS`; three commands were recorded as exactly three results, including the compound `docker compose up -d && ... && docker compose down` command as one command.
- JSON validity: `PASS`; `run.json`, `analysis.json`, and `iteration-001.json` parsed.
- Secret redaction: inherited from the same log redaction path; no secrets observed in the primary run.

Conclusion:

- Acceptance gating is implemented and tested.
- `YOLO_STOP` is rejected when acceptance commands fail.
- Failed acceptance output is converted into a repair prompt.
- Fixed-iteration runs now execute final acceptance checks before reporting success when the gate is enabled.
- The ecommerce project from this run is small but actually runnable.
- Remaining limitation: the main six-round run did not exercise a rejected `YOLO_STOP` in live mode because the refiner never emitted a stop signal; that path is covered by unit tests.

## YOLO Infinite Defaults And Acceptance Rejection Verification

Current iteration semantics:

- Omitting `--iterations` means infinite mode. The loop continues until accepted `YOLO_STOP` or a safety stop.
- `--iterations N`, `-n N`, and the `--10` shorthand run at most `N` rounds.
- `--iterations 0`, `-n 0`, and `--0` are invalid. The CLI exits with: `--iterations must be greater than 0; omit --iterations for infinite YOLO mode`.
- `YOLO_STOP` is accepted by default unless `QWEN_CODEX_YOLO_ALLOW_REFINER_STOP=false` or the acceptance gate rejects it.

Safety stops covered by code/tests:

- Accepted `YOLO_STOP`.
- Ctrl+C / interrupt flag between rounds.
- Per-round timeout.
- Repeated prompt guard in infinite mode.
- Consecutive failure guard.
- Agent subprocess errors.
- Refiner/provider fatal errors.
- Acceptance-gate rejection with no valid repair prompt.
- Delegated Codex context/compaction fatal errors.

Focused tests:

- `cargo test -p codex-qwen yolo -- --nocapture`: passed, 58 matching tests.
- `cargo test -p codex-qwen yolo_acceptance -- --nocapture`: passed, 3 tests.
- `cargo test -p codex-qwen yolo_timeout -- --nocapture`: passed, 1 test.

Live acceptance-rejection attempt:

- Workspace: `/tmp/qwen-yolo-stop-reject`
- Run directory: `/tmp/qwen-yolo-stop-reject/.qwen-codex/yolo-runs/20260502T112717Z-327102`
- Command shape: `qwen-codex --yolo-refiner --iterations 3 --yolo-round-timeout-secs 600 --yolo-acceptance-gate --yolo-acceptance-command "test -f MUST_EXIST_BEFORE_STOP.txt" --dangerously-bypass-approvals-and-sandbox "<tiny project prompt>"`
- Completed iterations: `3`.
- Stop reason: `max_iterations`.
- Final status: `partial`.
- JSON validity: `PASS`; `run.json`, `analysis.json`, and all iteration JSON files parsed.
- Round chaining: `PASS`.
- Secret redaction: `PASS`; no `local-dev-key` or `authorization:` text found.
- Live `YOLO_STOP` rejection: `NOT TRIGGERED`; the refiner did not emit `YOLO_STOP`.
- Final acceptance at max iterations: `PASS`; the impossible command ran, failed, was logged under `acceptanceResults`, and prevented success.

Because the live model did not emit `YOLO_STOP`, the actual rejected-stop continuation path remains proven by the focused `yolo_acceptance_rejects_stop_signal_and_injects_repair_prompt` test, which forces `YOLO_STOP`, records `stopSignalRejected=true`, captures failed acceptance output, injects a repair prompt, and verifies the next round receives that prompt.

## 10-Round YOLO Stress Verification

Workspace: `/tmp/qwen-yolo-stress`

Run directory: `/tmp/qwen-yolo-stress/.qwen-codex/yolo-runs/20260502T112858Z-331475`

Command shape: `qwen-codex --yolo-refiner --iterations 10 --yolo-round-timeout-secs 600 --dangerously-bypass-approvals-and-sandbox "<small Python notes CLI prompt>"` with small round budgets (`maxFiles=4`, `maxActions=3`, `maxTests=2`, `maxPromptChars=2200`).

Result:

- Completed iterations: `10`.
- Stop reason: `max_iterations`.
- Final status: `success`.
- JSON validity: `PASS`; `run.json`, `analysis.json`, and all iteration JSON files parsed.
- Round chaining: `PASS`; every continued round input matched the previous `nextPrompt`.
- Provider/context errors: `PASS`; no timeouts, interruptions, agent errors, provider errors, or refiner errors were reported.
- Repeated prompt failure: `PASS`; not triggered.
- Secret redaction: `PASS`; no `local-dev-key` or `authorization:` text found.
- Manual project check: `python test_cli.py` passed with `test_list_notes_empty PASSED` and `test_add_single_note PASSED`; `python cli.py add "Manual check" && python cli.py list` worked.
- Timeout utilization: highest observed round was 115s of 600s (`19%`); all `roundDurationNearTimeout` values were `false`.
- Auto-compact: no compaction marker or context error appeared in the logs. This run confirms 10-round chaining below the threshold, but it did not force an actual auto-compaction event.

## Context Management

Status: `PARTIALLY-CONFIRMED`.

Evidence:

- `codex-rs/qwen/src/config.rs` reads `QWEN_CODEX_CONTEXT_WINDOW` and emits `model_context_window=32768`.
- The same config path emits `model_auto_compact_token_limit=26214`.
- Qwen threshold formula: `min(context_window * 0.80, context_window - 4096)`.
- Unit coverage: `32768 -> 26214`, `8192 -> 4096`, `4096 -> 0`.
- `codex-rs/models-manager/src/model_info.rs` applies `model_context_window` and `model_auto_compact_token_limit` overrides to model metadata.
- `codex-rs/core/src/session/turn.rs` uses `model_info.auto_compact_token_limit()` for pre-turn and mid-turn compaction.
- `codex-rs/core/src/session/turn_context.rs` derives the effective context window from resolved model metadata.
- YOLO calls normal `codex exec --json` and then `codex exec --json resume <thread-id> <prompt>`, so it uses the same Qwen config and upstream compaction path as normal mode.
- The 10-round stress run completed cleanly with no provider/context errors and no repeated-prompt failure.
- No auto-compaction marker appeared in that stress run; the run stayed below the level needed to prove an actual compaction event.

Long-running/infinite YOLO context safety is partially confirmed: config propagation and 10-round chaining are verified, but live auto-compaction itself remains unexercised.

## Behavioral Tool Tests

Workspace: `/tmp/tool-test`.

| Capability      | Result               | Evidence                                                                                                                                                                 |
| --------------- | -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Web search      | `CAPABILITY_MISSING` | Prompt attempted `web_search`; upstream router logged `unsupported call: web_search`.                                                                                    |
| PDF generation  | `PASS`               | `test.pdf` exists; `file` reports `PDF document, version 1.4, 1 page(s), ASCII text`.                                                                                    |
| DOCX generation | `PASS`               | `test.docx` exists; `file` reports `Microsoft Word 2007+`; `word/document.xml` contains `Hello World`.                                                                   |
| XLSX generation | `FAIL`               | Prompt attempted package-based spreadsheet creation, but `pandas`/`openpyxl` were unavailable and network/package installation was blocked; `test.xlsx` was not created. |

The behavioral tests verify agent/tool execution through shell/file creation, not dedicated first-class PDF/DOCX/XLSX skills. Missing or failed capabilities are tracked in `docs/roadmap.md`.
