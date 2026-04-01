#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
cargo build --package nexuscore-host --release -q 2>/dev/null || true
./target/release/nexuscore-host verify
