#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "[cleanup] cargo clean..."
cargo clean 2>/dev/null || true
rm -f data/graph.blob data/perf_stress.txt
echo "[cleanup] Done."
