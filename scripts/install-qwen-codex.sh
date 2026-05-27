#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/install-qwen-codex.sh [--release]

Builds the local source tree and installs command shims into:
  ${QWEN_CODEX_INSTALL_DIR:-$HOME/.local/bin}

After this, run:
  qwen-codex
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
install_dir="${QWEN_CODEX_INSTALL_DIR:-$HOME/.local/bin}"
profile_dir="debug"
cargo_args=(build -p codex-cli --bins)

if [[ "${1:-}" == "--release" ]]; then
  cargo_args+=(--release)
  profile_dir="release"
elif [[ $# -gt 0 ]]; then
  usage >&2
  exit 2
fi

echo "Building qwen-codex from local source..."
(
  cd "$repo_root/codex-rs"
  cargo "${cargo_args[@]}"
)

mkdir -p "$install_dir"

target_dir="$repo_root/codex-rs/target/$profile_dir"
for bin in codex qwen-codex qwencodex; do
  src="$target_dir/$bin"
  if [[ ! -x "$src" ]]; then
    echo "expected binary not found: $src" >&2
    exit 1
  fi
  ln -sf "$src" "$install_dir/$bin"
done

echo "Installed qwen-codex commands into: $install_dir"
if [[ ":$PATH:" != *":$install_dir:"* ]]; then
  echo "Add this directory to PATH, then open a new shell:"
  echo "  export PATH=\"$install_dir:\$PATH\""
fi
echo "Run: qwen-codex"
