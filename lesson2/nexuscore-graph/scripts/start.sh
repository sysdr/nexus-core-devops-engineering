#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

echo "Building NexusCore Graph (native target)..."
cargo build --release --bin nexuscore-host
echo ""
echo "Starting benchmark demo..."
./target/release/nexuscore-host
