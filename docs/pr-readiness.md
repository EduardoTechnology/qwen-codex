# PR Readiness

Last updated: 2026-05-03T04:40:30Z

## Branch

`feature/qwen-codex-local-yolo`

## Summary

This branch adapts Codex for a local Qwen/vLLM workflow while keeping the upstream Codex agent path as the execution core.

Major features:

- `qwen-codex` and `qwencodex` wrapper entrypoints.
- Local Qwen OpenAI-compatible provider defaults and `.env` support.
- Public vLLM compose file for `QuantTrio/Qwen3.5-9B-AWQ`.
- Qwen-only Responses API compatibility for the verified vLLM behavior.
- YOLO refiner mode with finite and infinite iteration semantics.
- YOLO safety stops: accepted `YOLO_STOP`, Ctrl+C, round timeout, repeated prompts, repeated failures, agent/refiner/provider fatal errors, acceptance repair failure, and context/compaction fatal errors.
- YOLO round chaining, structured JSON/Markdown logs, `analysis.json`, action diagnostics, timeout-utilization diagnostics, and secret redaction.
- YOLO `flow_trace.json` and `flow_trace.md` for full user-prompt to agent/refiner/nextPrompt review.
- Acceptance gate for real verification commands before accepting stop or final fixed-iteration success.
- Qwen YOLO safe-write, file-validation, and unavailable-MCP fallback guidance.
- Analysis/flow-trace fields for non-blocking unavailable MCP attempts when acceptance passes.

## Verified Model Setup

Verified local server:

- Base URL: `http://127.0.0.1:8002/v1`
- Served model: `qwen35-local`
- Model repository: `QuantTrio/Qwen3.5-9B-AWQ`
- Context window: `32768`
- Auto-compact threshold propagated to Codex: `26214`
- Compose file: `docker-compose.yml`
- `/v1/models`: healthy in prior verification and reports `max_model_len=32768`

## Capability Matrix

See [docs/verification.md](verification.md#final-capability-verification).

Release classification:

- PASS: shell command execution, file creation, file reading, file editing, PDF generation, DOCX generation, XLSX generation, Docker compose workflow, YOLO chaining, YOLO infinite mode, acceptance-gate focused tests, and 10-round YOLO stress without provider/context errors.
- PARTIAL: larger multi-file scaffolds can still have local-model consistency bugs; the YOLO mini capability run timed out in round 3 and generated failing Python tests; the 10-round full-stack refiner evaluation completed six logged rounds and then stopped safely with `round_timeout`; auto-compact config propagation is confirmed but the best-effort pressure run was blocked before compaction; live rejected-`YOLO_STOP` was not triggered by the model and is covered by focused tests.
- CAPABILITY_MISSING: native `web_search`/browser tooling is not available in this local environment. Shell `curl` can work when network is allowed, but it is not native web-search parity.

## Manual Evidence

Key run paths:

- 10-round stress logs: `/tmp/qwen-yolo-stress/.qwen-codex/yolo-runs/20260502T112858Z-331475`
- Windows copy of 10-round stress logs: `/mnt/c/Users/eduar/Documents/qwen-codex-yolo-logs/stress-10rounds-20260502T112858Z-331475`
- Normal capability suite: `/tmp/qwen-capability-suite`
- YOLO tool diagnostic: `/tmp/qwen-yolo-tool-diagnostic/.qwen-codex/yolo-runs/20260502T203344Z-1262010`
- YOLO mini capability: `/tmp/qwen-yolo-capability/.qwen-codex/yolo-runs/20260502T204115Z-1279469`
- Compact-pressure attempt: `/tmp/qwen-yolo-compact/.qwen-codex/yolo-runs/20260502T213530Z-1376298`
- Windows copy of compact-pressure logs: `/mnt/c/Users/eduar/Documents/qwen-codex-yolo-logs/compact-pressure/20260502T213530Z-1376298`
- Full-stack 10-round refiner evaluation: `/tmp/qwen-yolo-fullstack-10/.qwen-codex/yolo-runs/20260502T232422Z-1560909`
- Windows copy of full-stack refiner logs: `/mnt/c/Users/eduar/Documents/qwen-codex-yolo-logs/fullstack-10/20260502T232422Z-1560909`
- Final full-stack validation: `/tmp/qwen-yolo-fullstack-final/.qwen-codex/yolo-runs/20260503T025110Z-1945177`
- Windows copy of final full-stack logs: `/mnt/c/Users/eduar/Documents/qwen-codex-yolo-logs/fullstack-final/20260503T025110Z-1945177`
- Round-by-round flow review: [docs/yolo-refiner-flow-review.md](yolo-refiner-flow-review.md)
- Ecommerce acceptance-gated verification: documented in [docs/verification.md](verification.md)

## Commands Passed

Final PR-readiness pass:

```sh
cd codex-rs
just fmt
cargo test -p codex-qwen yolo
cargo test -p codex-qwen yolo_acceptance
cargo test -p codex-qwen yolo_timeout
cargo build -p codex-cli
./target/debug/qwen-codex --help
./target/debug/qwen-codex --version
./target/debug/qwencodex --help
cd ..
git diff --check
```

Node formatting was not rerun in this final pass. Prior `pnpm run format` checks passed with the documented Node v20 engine warning because the repo expects Node v22+.

## Milestone Commits

- Normal CLI milestone: `720cc740f04c3e5fbb6de9a0e7d1bb3c8d7e6cb9`
- Initial YOLO milestone: `2e9648aa5699c7fad420ffc7530a77ce883644d4`
- YOLO timeout/logging fix: `02af23468c765c7dd78e0666fc70cf40820a2642`
- YOLO round budget and prompt validation: `6b813bb719662d1d32f797873248ecb0079ebfd2`
- YOLO acceptance gate: `23fa04ab5d61a7ed186e978f56a9fe5d3a9038ff`
- YOLO infinite mode: `c8a204b8d05d423fa609b60ab781df3f8cf7f051`
- Final capability verification: `73f48f4031f0411b74c2d111e5dc8a2401afa44d`
- Final capability hash log: `e6c62ef587db477fa8e38a3a7375f9deacd3c9e7`

## Upstream Sync Status

`upstream` is configured as `https://github.com/openai/codex.git`.

Latest fetched upstream main during this pass:

```text
upstream/main: 35aaa5d9fc Bound websocket request sends with idle timeout (#20751)
```

No upstream merge was attempted in this PR-readiness pass. Use the documented merge workflow in [docs/upstream-sync.md](upstream-sync.md) before or after this PR depending on review scope.

This branch currently has no common merge-base with the fetched `upstream/main` in this local checkout, so upstream sync should be handled deliberately as a separate task rather than folded into this Qwen feature PR.

## Repo Hygiene

- Tracked tree should be clean after the PR-readiness commit.
- `.qwen-codex/` is ignored for local YOLO logs.
- `/tmp` run artifacts are outside the repository.
- The pre-existing untracked `deploy/` directory is left untracked and is not part of this PR.
- Placeholder `local-dev-key` values appear only in docs/examples and `.env.example`.

## Known Limitations

- Native `web_search`/browser tooling is not available in the verified local environment.
- Shell `curl` network access is a useful fallback but not native web-search parity.
- Larger local-Qwen scaffold tasks can still produce small consistency bugs, such as package scripts pointing to the wrong generated entrypoint.
- The latest full-stack YOLO/refiner evaluation proved flow tracing and round chaining through six rounds, but stopped with `round_timeout` during a long Docker/runtime agent turn. The generated backend then failed runtime checks because it used ESM `import` syntax without `"type": "module"`.
- The final full-stack validation generated a manually passing Docker app, but the YOLO run did not finish cleanly: only four of six requested iterations were logged, `completedAt=null`, and `stopReason=null`.
- Unavailable MCP attempts such as `git`/`filesystem` are now non-blocking warnings when project acceptance passes, but local Qwen can still waste turns requesting unavailable MCP resources.
- The final YOLO mini capability run timed out in round 3 and generated failing Python tests.
- Auto-compact config propagation is confirmed, but no actual compaction event was triggered. The best-effort 15-round pressure attempt created 12 text files in one agent turn and then stopped safely with `round_timeout` before resumed-turn compaction pressure was reached.
- Live rejected-`YOLO_STOP` was not triggered by the model, though focused tests cover acceptance-gate rejection and repair prompting.
- Docker projects can still produce long single-agent turns; refiner guidance narrows future prompts but does not guarantee short local-model turns or a clean terminal run record.

## Current Recommendation

Open a PR for review only if the remaining limitations are explicit in the PR body. I do not recommend merging to `main` yet because the latest final validation exposed an incomplete YOLO lifecycle record even though the generated app passed manual acceptance.

## Remaining Roadmap

See [docs/roadmap.md](roadmap.md).
