# Add Qwen Codex local provider and YOLO refiner mode

## Summary

Adds Qwen Codex wrapper entrypoints for local Qwen/vLLM usage while preserving the upstream Codex agent execution path.

Includes:

- `qwen-codex` and `qwencodex` entrypoints
- local Qwen OpenAI-compatible provider configuration
- `.env.example` and vLLM compose setup
- Qwen Responses compatibility fixes
- YOLO refiner mode with finite and infinite iteration semantics
- round timeout, failure guards, repeated prompt guard, and Ctrl+C handling
- round budget controls and next-prompt validation/repair
- acceptance gate with focused tests
- JSON/Markdown YOLO logs, `analysis.json`, `flow_trace.json`, action diagnostics, and redaction
- docs for upstream sync and local model setup

## Verification

See:

- `docs/pr-readiness.md`
- `docs/verification.md`
- `docs/yolo-mode.md`

Final checks passed:

- `just fmt`
- `cargo test -p codex-qwen yolo`
- `cargo test -p codex-qwen yolo_acceptance`
- `cargo test -p codex-qwen yolo_timeout`
- `cargo build -p codex-cli`
- `qwen-codex --help`
- `qwen-codex --version`
- `qwencodex --help`
- `git diff --check`

Verified capabilities:

- shell command execution: PASS
- file creation/reading/editing: PASS
- PDF/DOCX/XLSX generation: PASS
- Docker compose workflow: PASS
- YOLO finite mode: PASS
- YOLO infinite mode: PASS
- acceptance gate: PASS
- 10-round YOLO stress: PASS
- full-stack YOLO/refiner flow trace: PARTIAL/PASS for logging and chaining, timeout before all 10 rounds
- context config propagation: PASS

## Known Limitations

- Native `web_search`/`browser` tool is unavailable in the tested local environment. Shell `curl` can access known URLs, but it is not native search/browser parity.
- Larger local-Qwen scaffold tasks can still produce small consistency bugs, such as package scripts pointing to the wrong generated entrypoint.
- The latest full-stack YOLO/refiner evaluation wrote valid `flow_trace.json`, proved chaining through six rounds, then stopped safely with `round_timeout`; generated backend runtime checks failed because the app used ESM `import` syntax without `"type": "module"`.
- Auto-compact config propagation is confirmed. The latest best-effort context-pressure run did not trigger actual compaction: `/tmp/qwen-yolo-compact/.qwen-codex/yolo-runs/20260502T213530Z-1376298` stopped with `round_timeout` in the first YOLO round after creating 12 bounded text files, with valid logs and no compaction marker.
- Live rejected `YOLO_STOP` was not triggered by the model during live runs; focused tests cover the rejection and repair path.
- Docker projects can produce long single-agent turns, especially when builds pull images.

## Local Qwen/vLLM setup tested

- Base URL: `http://127.0.0.1:8002/v1`
- Model: `qwen35-local`
- Context window: `32768`
- Auto-compact threshold: `26214`
- Attention backend: `TRITON_ATTN`

## How to run

Finite YOLO:

```bash
qwen-codex --yolo --iterations 5 "your task"
```

Infinite YOLO:

```bash
qwen-codex --yolo "your task"
```

Acceptance-gated YOLO:

```bash
qwen-codex --yolo --iterations 6 \
  --yolo-acceptance-gate \
  --yolo-acceptance-command "docker compose config" \
  --yolo-acceptance-command "docker compose build" \
  "Make this Docker project runnable."
```
