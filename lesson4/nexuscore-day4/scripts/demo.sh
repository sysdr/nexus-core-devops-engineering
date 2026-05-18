#!/usr/bin/env bash
# Demo: tolerates missing pinned BPF map (no set -e on control-plane calls)
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CTRL="$ROOT/control-plane/nexuscore-ctrl"

if [[ ! -f "$CTRL" ]]; then
  echo "[error] Control plane not built. Run start.sh first."
  exit 1
fi

echo "[demo] Listing current schemas in BPF map..."
"$CTRL" list || echo "[warn] list failed (BPF map not pinned — expected without root/BPF setup)"

echo ""
echo "[demo] Pushing initial schema for tenant 1001..."
"$CTRL" push "$ROOT/control-plane/example_schema.json" || echo "[warn] push failed (requires pinned map at /sys/fs/bpf/nexuscore/schemas)"

echo ""
echo "[demo] Simulating 5 live schema updates for tenant 1001..."
"$CTRL" simulate 1001 5 || echo "[warn] simulate failed (requires working pinned map)"

echo ""
echo "[demo] Final schema state:"
"$CTRL" list || echo "[warn] list failed"

echo "[demo] Done (warnings OK when BPF is not configured)."
exit 0
