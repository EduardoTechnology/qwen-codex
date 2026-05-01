#!/usr/bin/env node
// Qwen Codex compatibility entry point.

process.env.QWEN_CODEX_ENTRYPOINT = "1";
await import("./codex.js");
