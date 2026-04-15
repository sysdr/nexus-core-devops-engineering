#!/usr/bin/env bash
set -euo pipefail
# Demo: run the host (simulator) long enough to populate metrics,
# then print key counters to prove the dashboard will be non-zero.
echo "Starting NexusCore host (simulator) in background..."
fuser -k 9090/tcp 2>/dev/null || true
echo "Building host (release)..."
cargo build --release -p nexuscore-host >/tmp/nexuscore-build.log 2>&1 || (tail -80 /tmp/nexuscore-build.log && exit 1)

echo "Launching host..."
RUST_LOG=info ./target/release/nexuscore >/tmp/nexuscore-host.log 2>&1 &
HOST_PID=$!

echo "Waiting for metrics endpoint to become ready..."
ready=0
for i in $(seq 1 180); do
  if curl -s --max-time 1 http://localhost:9090/metrics | grep -q '^nexuscore_tags_total'; then
    ready=1
    break
  fi
  sleep 1
done

echo
echo "=== Metrics sample (should be non-zero) ==="
if [[ "$ready" -eq 1 ]]; then
  curl -s --max-time 2 http://localhost:9090/metrics \
    | grep -E '^(nexuscore_tags_total|nexuscore_classify_latency_ns_bucket|nexuscore_wasm_instances)' \
    | head -40 || true
else
  echo "[metrics unavailable] host did not expose metrics within timeout"
  echo "Last host log lines:"
  tail -40 /tmp/nexuscore-host.log || true
fi
echo "=== End metrics sample ==="
echo

kill "$HOST_PID" 2>/dev/null || true
wait "$HOST_PID" 2>/dev/null || true
echo "Demo complete."
