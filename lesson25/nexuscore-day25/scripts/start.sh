#!/usr/bin/env bash
set -euo pipefail

echo "Stopping any previous Redpanda container..."
if command -v docker >/dev/null 2>&1; then
  docker rm -f nexuscore-redpanda 2>/dev/null || true
else
  echo "(docker not found — skipping Redpanda)"
fi

echo "Starting Redpanda..."
if command -v docker >/dev/null 2>&1; then
  docker run -d --name nexuscore-redpanda \
    -p 9092:9092 -p 9644:9644 \
    docker.redpanda.com/redpandadata/redpanda:latest \
    redpanda start --overprovisioned --smp 1 --memory 512M \
    2>/dev/null || echo "(Redpanda already running)"
fi

echo "Waiting for Redpanda to be ready..."
if command -v docker >/dev/null 2>&1; then
  until docker exec nexuscore-redpanda rpk cluster health 2>/dev/null | grep -q "Healthy"; do
    sleep 1; printf "."
  done
  echo " Ready."
else
  echo "(skipped)"
fi

echo "Building NexusCore host..."
cargo build --release -p nexuscore-host 2>&1 | tail -5

echo "Ensuring metrics port 9090 is free..."
fuser -k 9090/tcp 2>/dev/null || true

echo "Starting NexusCore (simulator mode)..."
RUST_LOG=info cargo run --release -p nexuscore-host
