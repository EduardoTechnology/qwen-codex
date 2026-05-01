# Contributing To Qwen Codex

Qwen Codex tracks upstream OpenAI Codex while adding a small Qwen/vLLM wrapper layer. Keep fork-specific changes modular and document anything that affects local model setup, provider behavior, or YOLO mode.

## Development Setup

```sh
cp .env.example .env
docker compose -f modelo/docker-compose.qwen35-9b-awq.yml up -d
cd codex-rs
cargo build -p codex-cli
```

## Checks

For Qwen wrapper changes:

```sh
cd codex-rs
just fmt
cargo test -p codex-qwen
cargo build -p codex-cli
just fix -p codex-qwen
```

If Rust dependencies change, also run from the repo root:

```sh
just bazel-lock-update
just bazel-lock-check
```

Run `pnpm run format` before submitting documentation or package changes.

## Upstream Sync

See [docs/upstream-sync.md](docs/upstream-sync.md). Preserve Qwen-specific config, compose templates, the Qwen CLI wrapper, and YOLO mode when resolving upstream conflicts.

## Contribution Guidelines

- Write code, docs, commit messages, logs, and comments in English.
- Prefer upstream Codex abstractions over fork-local rewrites.
- Do not hardcode one user's local server values in runtime code; use centralized defaults and documented env vars.
- Keep model-specific vLLM flags in `modelo/` and docs unless runtime config genuinely needs them.
- Do not log secrets. Use the Qwen redaction helper for new YOLO or diagnostic logs.
