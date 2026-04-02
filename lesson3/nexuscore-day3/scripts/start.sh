#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if pgrep -x nexuscore-host >/dev/null 2>&1; then
  echo "[start] Another nexuscore-host is already running (see: pgrep -ax nexuscore-host). Stop it first." >&2
  exit 1
fi

echo "[start] Building workspace (release)..."
cargo build --workspace --release

echo "[start] Generating test graph data..."
python3 scripts/gen_graph.py 2000

PORT="${NEXUSCORE_DASHBOARD_PORT:-9847}"
echo "[start] Launching host (Ctrl+C to stop)."
echo "[start] Dashboard: http://127.0.0.1:${PORT}/  (metrics: /api/metrics)"
echo "[start] Optional env: NEXUSCORE_RPS=100000 NEXUSCORE_DASHBOARD_PORT=${PORT}"
exec cargo run -p nexuscore-host --release
