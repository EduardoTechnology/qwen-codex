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
- Live normal-mode tool-call smoke against the local Qwen/vLLM server.
- Live two-iteration YOLO smoke that created `hello.txt` and then `README.md`.
- Behavioral PDF and DOCX file generation through shell/file tools.

## Next

- Add or enable a real web search tool for local Qwen Codex. Current behavior test result: `CAPABILITY_MISSING` because `web_search` is unsupported by the active tool router.
- Improve spreadsheet generation. Current XLSX behavior test result: `FAIL` because package-based generation could not install or import `pandas`/`openpyxl`, and `test.xlsx` was not created.
- Add deterministic document-generation helpers or skills for PDF/DOCX/XLSX so local models do not need to improvise OOXML/PDF internals.
- Stress-test auto-compaction in long YOLO runs near the 32768-token Qwen context limit.
- Expand integration coverage around Codex JSON event parsing if upstream event shapes change.
- Add more community model compose files under `modelo/`.
- Prepare release packaging and repository metadata for the Qwen Codex fork.

## Extension Points

- Add model compose files as `modelo/docker-compose.<model>.yml`.
- Keep model-specific parser flags in compose/docs, not in runtime Rust defaults.
- Add future YOLO summarizers under `codex-rs/qwen/src/yolo/` without touching `codex-core` unless upstream exposes a better public API.
- Prefer upstream Codex tool, skill, MCP, shell, and context-compaction APIs over fork-local alternatives.
