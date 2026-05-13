#!/usr/bin/env bash
set -euo pipefail
QUERY="${1:-What is WASI 0.3 and how does it differ from WASI Preview 1?}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=/dev/null
source "$SCRIPT_DIR/_paths.sh"
cd "$ROOT"

echo "[nexuscore] Running single ReAct agent"
echo "[nexuscore] Query: $QUERY"
echo ""

if ! WASM="$(nexuscore_resolve_wasm "$ROOT")"; then
  echo "[nexuscore] Building Wasm component..."
  (cd agent-component && cargo component build --release)
  WASM="$(nexuscore_resolve_wasm "$ROOT")" || {
    echo "[nexuscore] ERROR: Could not find agent-component/target/*/release/nexuscore_agent.wasm after build."
    exit 1
  }
fi

if [ -n "${ANTHROPIC_API_KEY:-}" ]; then
  LLM_ENDPOINT="${LLM_ENDPOINT:-https://api.anthropic.com/v1/messages}"
  API_ARGS=(--api-key "$ANTHROPIC_API_KEY")
else
  LLM_ENDPOINT="${LLM_ENDPOINT:-http://127.0.0.1:9876/v1/messages}"
  API_ARGS=(--api-key "dummy")
fi

echo "[nexuscore] LLM endpoint: $LLM_ENDPOINT"
echo "[nexuscore] Wasm: $WASM"
echo "[nexuscore] Starting host orchestrator..."
HOST_BIN="$ROOT/target/release/nexuscore"
HOST_ARGS=(
  --wasm "$WASM"
  --query "$QUERY"
  --concurrency 1
  --llm-endpoint "$LLM_ENDPOINT"
  --search-endpoint "http://127.0.0.1:8765"
  --max-steps 8
  --viz-snapshot "$ROOT/visualizer/last-viz.json"
  "${API_ARGS[@]}"
)
if [[ -x "$HOST_BIN" ]]; then
  exec "$HOST_BIN" "${HOST_ARGS[@]}"
fi
(cd "$ROOT/host" && exec cargo run --release -- "${HOST_ARGS[@]}")
