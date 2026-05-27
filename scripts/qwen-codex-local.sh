#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/qwen-codex-local.sh [BASE_URL] [PROMPT...]

Examples:
  scripts/qwen-codex-local.sh http://127.0.0.1:8002/v1
  scripts/qwen-codex-local.sh http://127.0.0.1:8002/v1 "Create a README for this repo."

Optional environment overrides:
  QWEN_CODEX_MODEL=qwen35-local
  QWEN_CODEX_API_KEY=local-dev-key
  QWEN_CODEX_CONTEXT_WINDOW=32768
  QWEN_CODEX_HEALTH_TIMEOUT_SECS=10
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"

base_url="${1:-${QWEN_CODEX_BASE_URL:-http://127.0.0.1:8002/v1}}"
if [[ $# -gt 0 ]]; then
  shift
fi
prompt="${*:-What is 2+2? Answer in one word.}"

trimmed_base_url="${base_url%/}"
models_url="${trimmed_base_url}/models"
health_timeout="${QWEN_CODEX_HEALTH_TIMEOUT_SECS:-10}"

detect_model() {
  if [[ -n "${QWEN_CODEX_MODEL:-}" ]]; then
    printf '%s\n' "$QWEN_CODEX_MODEL"
    return
  fi
  if command -v curl >/dev/null 2>&1 && command -v python3 >/dev/null 2>&1; then
    local detected
    detected="$(
      curl -fsS --connect-timeout 2 --max-time "$health_timeout" "$models_url" 2>/dev/null | python3 -c 'import json,sys; data=json.load(sys.stdin).get("data", []); print(data[0].get("id", "") if data else "")' 2>/dev/null || true
    )"
    if [[ -n "$detected" ]]; then
      printf '%s\n' "$detected"
      return
    fi
  fi
  printf '%s\n' "qwen35-local"
}

model="$(detect_model)"
api_key="${QWEN_CODEX_API_KEY:-local-dev-key}"
context_window="${QWEN_CODEX_CONTEXT_WINDOW:-32768}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo was not found. Install Rust with rustup, then rerun this script." >&2
  echo "https://rustup.rs/" >&2
  exit 1
fi

echo "Building qwen-codex..."
(
  cd "$repo_root/codex-rs"
  cargo build -p codex-cli
)

export QWEN_CODEX_BASE_URL="$trimmed_base_url"
export QWEN_CODEX_MODEL="$model"
export QWEN_CODEX_API_KEY="$api_key"
export QWEN_CODEX_CONTEXT_WINDOW="$context_window"
export QWEN_CODEX_YOLO_REFINER_BASE_URL="${QWEN_CODEX_YOLO_REFINER_BASE_URL:-$trimmed_base_url}"
export QWEN_CODEX_YOLO_REFINER_MODEL="${QWEN_CODEX_YOLO_REFINER_MODEL:-$model}"
export QWEN_CODEX_YOLO_REFINER_API_KEY="${QWEN_CODEX_YOLO_REFINER_API_KEY:-$api_key}"

qwen_bin="$repo_root/codex-rs/target/debug/qwen-codex"

echo "Using model endpoint: $QWEN_CODEX_BASE_URL"
echo "Using model name: $QWEN_CODEX_MODEL"
echo "Checking model health..."
"$qwen_bin" --health

echo
echo "Running qwen-codex prompt..."
"$qwen_bin" "$prompt"
