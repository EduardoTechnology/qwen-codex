# Upstream Sync Workflow

Qwen Codex keeps two Git remotes:

- `origin`: the Qwen Codex fork, `git@github.com:EduardoTechnology/qwen-codex.git`.
- `upstream`: the original OpenAI Codex repository, `https://github.com/openai/codex.git`.

Recommended sync strategy: merge upstream `main` into this fork's `main`.

This repository intentionally carries Qwen-specific integration code, local model defaults, documentation, and YOLO mode. A merge workflow preserves the fork's published history and makes it easier for contributors to see where upstream changed versus where Qwen Codex changed. Rebasing `main` would create a cleaner linear history, but it rewrites shared branch history and is less friendly for an open-source fork.

## Sync `main`

```bash
git fetch upstream
git checkout main
git merge upstream/main
# resolve conflicts
# run tests
git push origin main
```

## Resolve Conflicts

When conflicts happen, preserve upstream architecture first and re-apply the smallest Qwen-specific layer needed afterward.

Priority areas to protect:

- `qwen-codex` and `qwencodex` command entrypoints.
- Qwen environment variable mapping and local OpenAI-compatible provider overrides.
- YOLO mode CLI parsing, refiner loop, logging, and secret redaction.
- `modelo/` compose examples.
- Documentation that explains the fork relationship and local vLLM usage.

For Rust conflicts, keep Qwen-specific code in dedicated modules or crates where possible. Avoid editing central upstream files unless the integration point requires it.

## Validate After Sync

Run the same scoped checks used for development:

```bash
cd codex-rs
just fmt
cargo test -p codex-qwen
cargo test -p codex-cli
```

If upstream changes common, core, protocol, or model-provider behavior, also run broader tests after the scoped tests pass.
