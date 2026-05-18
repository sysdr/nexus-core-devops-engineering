#!/usr/bin/env bash
set -euo pipefail

TENANT_COUNT=${1:-100}
UPDATE_ROUNDS=${2:-20}
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CTRL="$ROOT/control-plane/nexuscore-ctrl"

if [[ ! -f "$CTRL" ]]; then
  echo "[error] Control plane binary not found. Run start.sh first."
  exit 1
fi

echo "[load_test] Seeding $TENANT_COUNT tenants..."
for i in $(seq 1 "$TENANT_COUNT"); do
  TENANT_ID=$((1000 + i))
  TMP=$(mktemp /tmp/schema_XXXXXX.json)
  cat > "$TMP" << JSON
{
  "tenant_id": $TENANT_ID,
  "version":   1,
  "fields": [
    { "name": "timestamp", "type": "u64", "offset": 0 },
    { "name": "value",     "type": "f64", "offset": 8 }
  ]
}
JSON
  "$CTRL" push "$TMP" >/dev/null 2>&1 || true
  rm -f "$TMP"
done

echo "[load_test] Rolling updates: $UPDATE_ROUNDS rounds..."
for round in $(seq 1 "$UPDATE_ROUNDS"); do
  for i in $(seq 1 "$TENANT_COUNT"); do
    TENANT_ID=$((1000 + i))
    VERSION=$((round + 1))
    TMP=$(mktemp /tmp/schema_XXXXXX.json)
    cat > "$TMP" << JSON
{
  "tenant_id": $TENANT_ID,
  "version":   $VERSION,
  "fields": [
    { "name": "timestamp", "type": "u64", "offset": 0 },
    { "name": "value",     "type": "f64", "offset": 8 }
  ]
}
JSON
    "$CTRL" push "$TMP" >/dev/null 2>&1 || true
    rm -f "$TMP"
  done
  echo "[load_test] round $round/$UPDATE_ROUNDS complete"
done

"$CTRL" list 2>/dev/null || echo "[warn] list failed (BPF)"
