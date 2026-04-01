#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
echo "[nexuscore] cargo test (nexuscore-host)"
cargo test --package nexuscore-host
