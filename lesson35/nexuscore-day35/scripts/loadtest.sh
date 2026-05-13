#!/usr/bin/env bash
set -euo pipefail
N="${1:-50}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=/dev/null
source "$SCRIPT_DIR/_paths.sh"
cd "$ROOT"

if ! WASM="$(nexuscore_resolve_wasm "$ROOT")"; then
    echo "[loadtest] Building Wasm component first..."
    (cd agent-component && cargo component build --release)
    WASM="$(nexuscore_resolve_wasm "$ROOT")" || {
      echo "[loadtest] ERROR: Could not find nexuscore_agent.wasm under agent-component/target/"
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

echo "[loadtest] Launching $N concurrent agents..."
echo "[loadtest] Wasm: $WASM"
START_NS=$(date +%s%N)

HOST_BIN="$ROOT/target/release/nexuscore"
HOST_ARGS=(
  --wasm "$WASM"
  --query "What are the key performance characteristics of eBPF CO-RE programs?"
  --concurrency "$N"
  --llm-endpoint "$LLM_ENDPOINT"
  --search-endpoint "http://127.0.0.1:8765"
  --max-steps 6
  --viz-snapshot "$ROOT/visualizer/last-viz.json"
  "${API_ARGS[@]}"
)
if [[ -x "$HOST_BIN" ]]; then
  "$HOST_BIN" "${HOST_ARGS[@]}"
else
  (cd "$ROOT/host" && cargo run --release -- "${HOST_ARGS[@]}")
fi

END_NS=$(date +%s%N)
ELAPSED_MS=$(( (END_NS - START_NS) / 1000000 ))
echo ""
echo "[loadtest] ── Results ──────────────────────────────"
echo "[loadtest]  Agents: $N"
echo "[loadtest]  Total time: ${ELAPSED_MS}ms"
echo "[loadtest]  Avg per agent: $(( ELAPSED_MS / N ))ms"
echo "[loadtest] ─────────────────────────────────────────"
echo "[loadtest] Check Prometheus metrics: curl http://localhost:8080/metrics"
