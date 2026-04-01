#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

echo "=== NexusCore Day 2 Verification ==="
echo ""
echo "--- Running unit + integration tests ---"
cargo test -- --nocapture
echo ""
echo "=== Verification complete ==="
