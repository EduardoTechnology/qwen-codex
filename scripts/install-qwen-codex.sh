#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/install-qwen-codex.sh [BASE_URL] [--release]

Builds the local source tree, stores the Qwen /v1 URL, and installs commands into:
  ${QWEN_CODEX_INSTALL_DIR:-$HOME/.local/bin}

Example:
  scripts/install-qwen-codex.sh http://127.0.0.1:8002/v1

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
base_url="${QWEN_CODEX_BASE_URL:-http://127.0.0.1:8002/v1}"
model="${QWEN_CODEX_MODEL:-}"
api_key="${QWEN_CODEX_API_KEY:-local-dev-key}"
context_window="${QWEN_CODEX_CONTEXT_WINDOW:-}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --release)
      cargo_args+=(--release)
      profile_dir="release"
      ;;
    http://* | https://*)
      base_url="$1"
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
  shift
done

base_url="${base_url%/}"

detect_model_metadata() {
  if [[ -n "$model" && -n "$context_window" ]]; then
    return
  fi
  if ! command -v curl >/dev/null 2>&1 || ! command -v python3 >/dev/null 2>&1; then
    model="${model:-qwen35-local}"
    context_window="${context_window:-32768}"
    return
  fi

  local metadata
  metadata="$(
    curl -fsS --connect-timeout 2 --max-time 10 "$base_url/models" 2>/dev/null |
      python3 -c 'import json,sys; data=json.load(sys.stdin).get("data", []); item=data[0] if data else {}; print(item.get("id","")); print(item.get("max_model_len",""))' 2>/dev/null || true
  )"
  model="${model:-$(printf '%s\n' "$metadata" | sed -n '1p')}"
  context_window="${context_window:-$(printf '%s\n' "$metadata" | sed -n '2p')}"
  model="${model:-qwen35-local}"
  context_window="${context_window:-32768}"
}

write_qwen_wrapper() {
  local name="$1"
  local real="$2"
  local path="$install_dir/$name"
  cat >"$path" <<EOF
#!/usr/bin/env bash
export QWEN_CODEX_BASE_URL="\${QWEN_CODEX_BASE_URL:-$base_url}"
export QWEN_CODEX_MODEL="\${QWEN_CODEX_MODEL:-$model}"
export QWEN_CODEX_API_KEY="\${QWEN_CODEX_API_KEY:-$api_key}"
export QWEN_CODEX_CONTEXT_WINDOW="\${QWEN_CODEX_CONTEXT_WINDOW:-$context_window}"
exec "$install_dir/$real" "\$@"
EOF
  chmod +x "$path"
}

detect_model_metadata

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
done
cp -f "$target_dir/codex" "$install_dir/codex"
cp -f "$target_dir/qwen-codex" "$install_dir/qwen-codex-real"
cp -f "$target_dir/qwencodex" "$install_dir/qwencodex-real"
chmod +x "$install_dir/codex" "$install_dir/qwen-codex-real" "$install_dir/qwencodex-real"
write_qwen_wrapper "qwen-codex" "qwen-codex-real"
write_qwen_wrapper "qwencodex" "qwencodex-real"

echo "Installed qwen-codex commands into: $install_dir"
echo "Configured Qwen endpoint: $base_url"
echo "Configured Qwen model: $model"
if [[ ":$PATH:" != *":$install_dir:"* ]]; then
  echo "Add this directory to PATH, then open a new shell:"
  echo "  export PATH=\"$install_dir:\$PATH\""
fi
echo "Run: qwen-codex"
