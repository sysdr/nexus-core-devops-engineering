#!/usr/bin/env bash
set -euo pipefail

rm_container_if_exists() {
    local name="$1"
    if docker ps -a --format '{{.Names}}' | grep -qx "$name"; then
        echo "  Removing existing container: $name"
        docker rm -f "$name" >/dev/null 2>&1 || true
    fi
}

echo "▶ Starting SurrealDB..."
rm_container_if_exists nexuscore-surreal
docker run -d --name nexuscore-surreal \
    -p 8000:8000 \
    surrealdb/surrealdb:latest \
    start --log trace --user root --pass root memory \
    >/dev/null

echo "▶ Starting polyglot stack (Postgres + Redis)..."
rm_container_if_exists nexuscore-postgres
docker run -d --name nexuscore-postgres \
    -p 5432:5432 \
    -e POSTGRES_PASSWORD=nexuscore \
    postgres:16-alpine >/dev/null

rm_container_if_exists nexuscore-redis
docker run -d --name nexuscore-redis \
    -p 6379:6379 \
    redis:7-alpine >/dev/null

echo "✓ All services started. Allow 5s for SurrealDB to initialize."
sleep 5
echo "✓ Ready."
