#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WASM="${ROOT}/target/wasm32-wasip2/release/orchestrator.wasm"
echo "▶ WASI orchestrator (optional — requires wasmtime + built .wasm)..."
if command -v wasmtime >/dev/null 2>&1 && [ -f "$WASM" ]; then
    wasmtime run --wasi preview2 "$WASM" -- --dry-run 2>/dev/null || echo "  (wasm run skipped — component CLI may differ)"
else
    echo "  (skip: install wasmtime and run: cd \"${ROOT}\" && make build-wasi)"
fi

echo "▶ Running benchmark scenario: SurrealDB, 50 tenants, 1000 req/tenant"
cargo run --release --manifest-path "${ROOT}/load-gen/Cargo.toml" -- \
    --target 127.0.0.1:8000 \
    --rps 2000 \
    --tenants 50 \
    --duration 15 \
    --payload-bytes 512 \
    --operation mixed
