#!/usr/bin/env bash
# Lesson 5: stop local dashboard/load binaries; Docker nexuscore-* removed by repo-root cleanup.sh.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
pkill -f 'target/release/dashboard-web' 2>/dev/null || true
pkill -f 'target/release/load-gen' 2>/dev/null || true
rm -f "${ROOT}/results/dashboard-web.log" 2>/dev/null || true
