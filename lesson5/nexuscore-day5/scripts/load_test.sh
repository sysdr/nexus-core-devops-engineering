#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RPS=${1:-10000}
TENANTS=${2:-500}
DURATION=${3:-60}
TARGET=${4:-127.0.0.1:8000}

echo "▶ Stress test: ${RPS} RPS / ${TENANTS} tenants / ${DURATION}s → ${TARGET}"
cargo run --release --manifest-path "${ROOT}/load-gen/Cargo.toml" -- \
    --target "${TARGET}" \
    --rps "${RPS}" \
    --tenants "${TENANTS}" \
    --duration "${DURATION}" \
    --payload-bytes 1024 \
    --operation mixed
