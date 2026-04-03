#!/usr/bin/env bash
# Metrics UI — listens on 0.0.0.0 (all interfaces) so WSL2 + Windows browser works.
# Keep this terminal open while using the site. Stop with Ctrl+C.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${ROOT}/target/release/dashboard-web"
if [ ! -x "$BIN" ]; then
  echo "Building dashboard-web (release)..."
  cargo build --release --manifest-path "${ROOT}/dashboard/Cargo.toml" --bin dashboard-web
  BIN="${ROOT}/target/release/dashboard-web"
fi
export DASHBOARD_PORT="${DASHBOARD_PORT:-3030}"
export DASHBOARD_HOST="${DASHBOARD_HOST:-0.0.0.0}"
echo "Starting web dashboard — http://127.0.0.1:${DASHBOARD_PORT}/ (leave this running)"
exec "$BIN"
