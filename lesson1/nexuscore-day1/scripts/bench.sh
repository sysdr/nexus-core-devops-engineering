#!/usr/bin/env bash
# SurrealQL migration patterns benchmark
# Compares query patterns: SQL-style vs native SurrealDB multi-model
cd "$(dirname "$0")/.."

echo -e "\x1b[1;34m=== NexusCore Migration Benchmark ===\x1b[0m"
echo
echo "Benchmark categories:"
echo "  1. Relational (SQL-equivalent SELECT + WHERE)"
echo "  2. Graph traversal (RELATE + ->edge-> vs JOIN)"
echo "  3. Full-text BM25 (native @@ vs LIKE pattern)"
echo "  4. Document (schemaless nested vs normalized tables)"
echo
echo "Running stress test..."
cargo build --package nexuscore-host --release -q 2>/dev/null || true
./target/release/nexuscore-host stress --tenants "${1:-100}" --duration "${2:-10}"
