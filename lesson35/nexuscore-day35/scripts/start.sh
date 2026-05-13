#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

free_tcp_port() {
  local port="$1"
  if command -v fuser >/dev/null 2>&1; then
    # fuser -k prints killed PIDs on stdout — discard so it does not splice into script output.
    fuser -k "${port}/tcp" >/dev/null 2>&1 || true
  fi
}

echo "[nexuscore] Freeing ports (8765/9090/9876) if occupied..."
free_tcp_port 8765
free_tcp_port 9090
free_tcp_port 9876

echo "[nexuscore] Starting mock LLM server..."
PYTHONUNBUFFERED=1 python3 -u "$ROOT/scripts/mock_llm_server.py" &
echo $! > "$ROOT/.mock_llm.pid"
sleep 0.4

echo "[nexuscore] Starting mock search server..."
python3 "$ROOT/scripts/mock_search_server.py" &
echo $! > "$ROOT/.search_server.pid"
sleep 0.4

echo "[nexuscore] Starting visualizer (static page; live data from host :8080)..."
python3 -m http.server 9090 --directory "$ROOT/visualizer" >>"$ROOT/.viz_server.log" 2>&1 &
echo $! > "$ROOT/.viz_server.pid"

echo ""
echo "[nexuscore] Mock LLM:      http://127.0.0.1:9876/v1/messages"
echo "[nexuscore] Mock search:   http://127.0.0.1:8765/search"
echo "[nexuscore] Visualizer:    http://127.0.0.1:9090  (http.server log: $ROOT/.viz_server.log)"
echo "[nexuscore] When demo runs: http://127.0.0.1:8080/api/viz  +  /metrics"
echo ""
echo "[nexuscore] Run demo:"
echo "  cd \"$ROOT\" && ./scripts/demo.sh \"your query\""
echo "[nexuscore] Stop:"
echo "  \"$ROOT/scripts/cleanup.sh\""
