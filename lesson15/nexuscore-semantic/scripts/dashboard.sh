#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PORT="${DASH_PORT:-8080}"

cd "$ROOT/visualizer"
echo "[dashboard] Serving dashboard on http://localhost:${PORT}/"
echo "[dashboard] Metrics endpoint expected at http://localhost:9091/metrics"
python3 -m http.server "$PORT" --bind 127.0.0.1

