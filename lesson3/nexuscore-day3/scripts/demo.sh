#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "[demo] Workspace tests (first 80 lines of output):"
cargo test --workspace -- --nocapture 2>&1 | head -80 || true

echo ""
echo "[demo] Regenerate small graph and show blob:"
python3 scripts/gen_graph.py 300
ls -lh data/graph.blob

echo ""
echo "[demo] Dashboard (with host running):"
echo "    http://127.0.0.1:9847/"
echo "[demo] Static file only (no live metrics):"
echo "    file://${ROOT}/data/visualizer.html"
