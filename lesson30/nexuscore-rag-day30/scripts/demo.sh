#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
echo "[NexusCore] Running demo simulation (no build required)..."
cargo run --manifest-path nexuscore-host/Cargo.toml --release -- \
  --corpus "$ROOT/data/corpus.jsonl" \
  --embeddings "$ROOT/data/embeddings.bin" \
  --tenants 5 --queries 3 \
  2>/dev/null || echo "Build nexuscore-host first with: ./scripts/start.sh"
