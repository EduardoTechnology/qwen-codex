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
- Qwen YOLO safe-write guidance for multi-line JSON/JS/TS/HTML/CSS/YAML plus automatic refiner feedback for package JSON, Docker Compose, backend JS syntax, frontend `http://backend:` URLs, and acceptance status.
- Qwen YOLO unavailable-MCP handling: attempts such as unknown `git` or `filesystem` MCP resources are recorded as non-blocking infrastructure warnings when project acceptance passes, and refiner context recommends shell fallbacks.
- Live normal-mode tool-call smoke against the local Qwen/vLLM server.
- Live two-iteration YOLO smoke that created `hello.txt` and then `README.md`.
- Live bounded ecommerce YOLO rerun with clean round chaining, no timeout, valid logs, compose config/build passing, and backend runtime checks passing.
- Live six-round ecommerce acceptance-gated run with `max_iterations`, valid logs, secret redaction, working round chaining, and a runnable Docker project verified by compose build plus backend/frontend curls.
- Live four-round safe-write Docker rerun with coherent flow trace and passing manual package JSON, JS syntax, compose config/build/up, backend `/health`, backend `/api/items`, frontend `/`, and frontend URL checks.
- Live final full-stack Docker validation with passing manual compose config/build/up, backend `/health`, backend `/api/items`, frontend `/`, frontend URL checks, and secret checks after an incomplete four-logged-round YOLO run.
- Live 10-round notes-CLI YOLO stress run with valid logs, clean chaining, no provider/context errors, and passing manual CLI/tests.
- Behavioral PDF, DOCX, and XLSX file generation through shell/file tools.
- Final capability verification pass covering normal shell/file/code workflows, YOLO action logging, 10-round stress logs, document generation, and context propagation.

## Next

- Add or enable a real web search/browser tool for local Qwen Codex via upstream native support or an MCP/search provider bridge. Current behavior test result: `CAPABILITY_MISSING` for native web search because `web_search` was not exposed or used by the active tool router. Shell `curl` network fetches work and retrieved Node.js release data from `nodejs.org`, but that is not native web-search parity.
- Decide whether to add deterministic document-generation helpers or skills for PDF/DOCX/XLSX. Behavioral creation passed for all three formats through shell/file tools, but local models currently improvise PDF/OOXML internals or depend on whatever libraries happen to be installed.
- Improve scaffold self-checking prompts so generated package scripts match generated file paths. The final normal-mode Express test created real project files but placed `server.js` under `src/` while `package.json` starts `node server.js`, so exact runnable scaffolding still needs prompt/model hardening.
- Improve YOLO mini-project stability under local Qwen. The final three-round notes CLI run preserved logs and chaining but hit `round_timeout` in round 3 and produced incomplete Python/tests.
- Reduce long single-round agent turns on Docker projects. Guidance now tells the refiner to prefer `docker compose config`, use detached `up`, wrap long commands, shrink scope after near-timeout rounds, and repair invalid files narrowly, but live Docker tasks can still produce long single turns and excessive rewrite loops.
- Harden incomplete YOLO run lifecycle handling. The final full-stack validation left a manually passing app, but the controller process was gone with only four of six iterations logged and `run.json.completedAt=null` / `stopReason=null`.
- Continue reducing local-model attempts to call unavailable MCP resources during YOLO runs. These attempts are now warning-classified when acceptance passes, but they can still waste model turns.
- Design a safer context/auto-compaction YOLO stress test near the 32768-token Qwen context limit. The 10-round notes run stayed below the compaction threshold, and the 15-round compact-pressure attempt was blocked by a long first agent turn that hit `round_timeout` before resumed-turn compaction pressure was reached.
- Exercise a live rejected-`YOLO_STOP` path. The focused test covers it, but the live model did not emit `YOLO_STOP` in the impossible-acceptance run.
- Expand integration coverage around Codex JSON event parsing if upstream event shapes change.
- Add more community model compose files under `modelo/`.
- Prepare release packaging and repository metadata for the Qwen Codex fork.

## Extension Points

- Add model compose files as `modelo/docker-compose.<model>.yml`.
- Keep model-specific parser flags in compose/docs, not in runtime Rust defaults.
- Add future YOLO summarizers under `codex-rs/qwen/src/yolo/` without touching `codex-core` unless upstream exposes a better public API.
- Prefer upstream Codex tool, skill, MCP, shell, and context-compaction APIs over fork-local alternatives.
