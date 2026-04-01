#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
cargo build --package nexuscore-host --release -q 2>/dev/null || true
RUST_LOG=warn ./target/release/nexuscore-host demo --tenants "${1:-20}" --rps "${2:-100}"
