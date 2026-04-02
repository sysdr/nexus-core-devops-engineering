#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "=== NexusCore Day 3 Verification ==="
cargo test --workspace -- --nocapture
echo "=== Verification complete ==="
