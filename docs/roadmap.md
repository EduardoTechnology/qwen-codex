# Roadmap

This roadmap tracks work that remains after the normal CLI and initial YOLO milestones.

## Completed

- Qwen wrapper CLI commands: `qwen-codex` and `qwencodex`.
- Local Qwen/vLLM env configuration with official variables and supported aliases.
- Qwen provider overrides for the upstream Codex execution path.
- Public vLLM compose template for `QuantTrio/Qwen3.5-9B-AWQ`.
- Verified 32k local vLLM baseline on `http://127.0.0.1:8002/v1`.
- Qwen-only Responses compatibility for the verified vLLM behavior.
- YOLO loop controller, refiner client, logging, redaction, and guard tests.
- YOLO round budget config, structured refiner prompt guidance, and next-prompt validation/repair.
- YOLO acceptance gate with rejected stop signals, acceptance-result logging, final fixed-iteration acceptance checks, and repair prompts from failed acceptance output.
- YOLO infinite default semantics: omitted `--iterations` runs until accepted `YOLO_STOP` or safety stop; zero iteration limits are invalid.
- YOLO analysis timeout-utilization fields and refiner guidance for shrinking work after near-timeout rounds.
- Live normal-mode tool-call smoke against the local Qwen/vLLM server.
- Live two-iteration YOLO smoke that created `hello.txt` and then `README.md`.
- Live bounded ecommerce YOLO rerun with clean round chaining, no timeout, valid logs, compose config/build passing, and backend runtime checks passing.
- Live six-round ecommerce acceptance-gated run with `max_iterations`, valid logs, secret redaction, working round chaining, and a runnable Docker project verified by compose build plus backend/frontend curls.
- Live 10-round notes-CLI YOLO stress run with valid logs, clean chaining, no provider/context errors, and passing manual CLI/tests.
- Behavioral PDF and DOCX file generation through shell/file tools.

## Next

- Add or enable a real web search tool for local Qwen Codex. Current behavior test result: `CAPABILITY_MISSING` because `web_search` is unsupported by the active tool router.
- Improve spreadsheet generation. Current XLSX behavior test result: `FAIL` because package-based generation could not install or import `pandas`/`openpyxl`, and `test.xlsx` was not created.
- Add deterministic document-generation helpers or skills for PDF/DOCX/XLSX so local models do not need to improvise OOXML/PDF internals.
- Reduce long single-round agent turns on Docker projects. Guidance now tells the refiner to prefer `docker compose config`, use detached `up`, wrap long commands, and shrink scope after near-timeout rounds, but live Docker tasks can still produce long single turns.
- Force a larger context/auto-compaction YOLO stress test near the 32768-token Qwen context limit. The 10-round notes run stayed below the compaction threshold and did not exercise an actual compaction event.
- Exercise a live rejected-`YOLO_STOP` path. The focused test covers it, but the live model did not emit `YOLO_STOP` in the impossible-acceptance run.
- Expand integration coverage around Codex JSON event parsing if upstream event shapes change.
- Add more community model compose files under `modelo/`.
- Prepare release packaging and repository metadata for the Qwen Codex fork.

## Extension Points

- Add model compose files as `modelo/docker-compose.<model>.yml`.
- Keep model-specific parser flags in compose/docs, not in runtime Rust defaults.
- Add future YOLO summarizers under `codex-rs/qwen/src/yolo/` without touching `codex-core` unless upstream exposes a better public API.
- Prefer upstream Codex tool, skill, MCP, shell, and context-compaction APIs over fork-local alternatives.
