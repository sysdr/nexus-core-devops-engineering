#!/usr/bin/env bash
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
echo "▶ Verifying eBPF probes..."
sudo bpftool prog list 2>/dev/null | grep -i nexuscore || echo "  ⚠ No nexuscore BPF programs found (run Go loader first)"

echo "▶ Checking SurrealDB health..."
curl -sf http://localhost:8000/health && echo " ✓ SurrealDB healthy" || echo "  ✗ SurrealDB not responding"

echo "▶ Checking Redis..."
if command -v redis-cli >/dev/null 2>&1; then
    redis-cli -h 127.0.0.1 -p 6379 ping 2>/dev/null && echo "  ✓ Redis healthy" || echo "  ✗ Redis not responding"
elif docker exec nexuscore-redis redis-cli ping 2>/dev/null | grep -q PONG; then
    echo "  ✓ Redis healthy (via docker exec)"
else
    echo "  ✗ Redis not responding"
fi

echo "▶ Checking PostgreSQL..."
if command -v pg_isready >/dev/null 2>&1; then
    pg_isready -h localhost -p 5432 -U postgres 2>/dev/null && echo "  ✓ Postgres healthy" || echo "  ✗ Postgres not responding"
elif docker exec nexuscore-postgres pg_isready -U postgres 2>/dev/null; then
    echo "  ✓ Postgres healthy (via docker exec)"
else
    echo "  ✗ Postgres not responding"
fi

echo "▶ Verifying WASI target..."
if [ -f "${ROOT}/target/wasm32-wasip2/release/orchestrator.wasm" ]; then
    echo "  ✓ WASI binary present"
else
    echo "  ✗ WASI binary missing (run: cd \"${ROOT}\" && make build-wasi)"
fi
