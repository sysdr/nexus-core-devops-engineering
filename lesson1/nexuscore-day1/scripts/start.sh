#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Override with e.g. NEXUSCORE_SURREAL_HOST=host.docker.internal if Docker is only reachable there.
SURREAL_HOST="${NEXUSCORE_SURREAL_HOST:-127.0.0.1}"
SURREAL_PORT="${NEXUSCORE_SURREAL_PORT:-8000}"

echo -e "\x1b[1;34m[nexuscore] Starting SurrealDB...\x1b[0m"
if command -v docker &>/dev/null; then
    if docker ps -a --format '{{.Names}}' 2>/dev/null | grep -qx 'nexuscore-surreal'; then
        if docker ps --format '{{.Names}}' 2>/dev/null | grep -qx 'nexuscore-surreal'; then
            echo "[ok ] SurrealDB container nexuscore-surreal already running"
        else
            docker start nexuscore-surreal 2>/dev/null && echo "[ok ] Started existing nexuscore-surreal" \
                || echo "[warn] Could not start nexuscore-surreal"
        fi
    else
        docker run -d \
            --name nexuscore-surreal \
            -p 8000:8000 \
            surrealdb/surrealdb:v2 \
            start --log trace --user root --pass root memory \
            && echo "[ok ] SurrealDB started (in-memory mode)" \
            || echo "[warn] docker run failed — start SurrealDB manually"
    fi
else
    echo "[warn] docker not found — ensure SurrealDB is listening on :8000"
fi

echo -e "\x1b[1;34m[nexuscore] Waiting for SurrealDB TCP on ${SURREAL_HOST}:${SURREAL_PORT}...\x1b[0m"
if command -v nc >/dev/null 2>&1; then
    i=0
    until nc -z "$SURREAL_HOST" "$SURREAL_PORT" 2>/dev/null; do
        i=$((i + 1))
        if [ "$i" -ge 120 ]; then
            echo "[warn] ${SURREAL_HOST}:${SURREAL_PORT} not open after ~60s — check Docker (-p 8000:8000)."
            break
        fi
        sleep 0.5
    done
    if nc -z "$SURREAL_HOST" "$SURREAL_PORT" 2>/dev/null; then
        echo "[ok ] ${SURREAL_HOST}:${SURREAL_PORT} accepts TCP"
    fi
else
    echo "[info] Install netcat (nc) to wait for the port before starting the host."
fi

if [ -z "${NEXUSCORE_SURREAL_URL:-}" ]; then
    export NEXUSCORE_SURREAL_URL="ws://${SURREAL_HOST}:${SURREAL_PORT}"
fi

echo -e "\x1b[1;34m[nexuscore] Building host runtime...\x1b[0m"
cargo build --package nexuscore-host --release

echo -e "\x1b[1;34m[nexuscore] Starting host (cwd: $ROOT)...\x1b[0m"
echo "[info] Using NEXUSCORE_SURREAL_URL=${NEXUSCORE_SURREAL_URL}"
RUST_LOG=nexuscore=info,warn ./target/release/nexuscore-host start --surreal-url "${NEXUSCORE_SURREAL_URL}"
