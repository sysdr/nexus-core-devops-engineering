#!/usr/bin/env bash
set -euo pipefail
# Workspace root (so this works when run as ./scripts/start.sh from anywhere)
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "[NexusCore] Building Wasm component..."
(cd rag-component && cargo build --target wasm32-wasip2 --release 2>&1 | grep -E "(Compiling|Finished|error)" || true)
echo "[NexusCore] Building host runtime..."
(cd nexuscore-host && cargo build --release 2>&1 | grep -E "(Compiling|Finished|error)" || true)

# Cargo workspace: the binary is at ./target/release/, not nexuscore-host/target/release/
HOST_BIN="$ROOT/target/release/nexuscore-host"
if [[ ! -x "$HOST_BIN" ]]; then
  echo "[NexusCore] ERROR: expected host at $HOST_BIN (Cargo workspace output)." >&2
  exit 1
fi

echo "[NexusCore] Starting host (simulation mode)..."
exec "$HOST_BIN" \
  --corpus data/corpus.jsonl \
  --embeddings data/embeddings.bin \
  --tenants 20 --queries 5 --max-instances 50
