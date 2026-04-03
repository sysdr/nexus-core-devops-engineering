#!/usr/bin/env bash
# Terminal ASCII dashboard + same metrics on the web UI (starts dashboard-web if needed).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "${ROOT}/results"

DASH_BIN="${ROOT}/target/release/dashboard"
WEB_BIN="${ROOT}/target/release/dashboard-web"
PORT="${DASHBOARD_PORT:-3030}"
BASE="http://127.0.0.1:${PORT}"

if [ ! -x "$DASH_BIN" ] || [ ! -x "$WEB_BIN" ]; then
    echo "Building dashboard binaries (release)..."
    cargo build --release --manifest-path "${ROOT}/dashboard/Cargo.toml" --bin dashboard --bin dashboard-web
    DASH_BIN="${ROOT}/target/release/dashboard"
    WEB_BIN="${ROOT}/target/release/dashboard-web"
fi

SURREAL_PID=$(pgrep -f "surrealdb" | head -1 || true)
POSTGRES_PID=$(pgrep -f "postgres" | head -1 || true)
echo "▶ Dashboard demo (sample histograms — non-zero metrics)"
echo "  SurrealDB PID: ${SURREAL_PID:-<not found>}"
echo "  Postgres PID:  ${POSTGRES_PID:-<not found>}"

# Same JSON as piped to CLI — also POST to web API so the browser matches the terminal.
H1='{"ts":"2026-04-03T12:00:00Z","stack":"surrealdb","op":"vfs_read","p50_ns":18432,"p99_ns":72800,"p999_ns":215000,"total_count":50000,"buckets":{"14":120,"15":8200,"16":22000,"17":12000,"18":5000,"19":2000,"20":680}}'
H2='{"ts":"2026-04-03T12:00:01Z","stack":"polyglot","op":"vfs_read","p50_ns":22100,"p99_ns":91000,"p999_ns":280000,"total_count":42000,"buckets":{"15":400,"16":6000,"17":18000,"18":9000,"19":6000,"20":2800}}'
H3='{"ts":"2026-04-03T12:00:02Z","stack":"combined","op":"tcp_sendmsg","p50_ns":9500,"p99_ns":45200,"p999_ns":132000,"total_count":31000,"buckets":{"13":200,"14":5000,"15":12000,"16":8000,"17":3500,"18":2300}}'

if ! curl -sf "${BASE}/api/metrics" >/dev/null 2>&1; then
    echo "▶ Starting web dashboard in background (${BASE}/) ..."
    DASHBOARD_HOST="${DASHBOARD_HOST:-0.0.0.0}" DASHBOARD_PORT="${PORT}" \
        nohup "$WEB_BIN" >>"${ROOT}/results/dashboard-web.log" 2>&1 &
    disown || true
    ok=0
    for _ in $(seq 1 40); do
        if curl -sf "${BASE}/api/metrics" >/dev/null 2>&1; then
            ok=1
            break
        fi
        sleep 0.25
    done
    if [ "$ok" -eq 1 ]; then
        echo "  ✓ Web UI is up — open in browser: ${BASE}/"
        echo "  (logs: ${ROOT}/results/dashboard-web.log — stop server: pkill -f 'target/release/dashboard-web')"
    else
        echo "  ⚠ Web UI did not become ready — see ${ROOT}/results/dashboard-web.log"
        echo "  Try manually: bash scripts/dashboard-web.sh"
    fi
else
    echo "▶ Web dashboard already running — ${BASE}/"
fi

for payload in "$H1" "$H2" "$H3"; do
    curl -sf -X POST "${BASE}/api/ingest/histogram" -H 'Content-Type: application/json' -d "$payload" >/dev/null || true
done
echo "▶ Synced the same histograms to the web API (reload the browser if it was open)."

echo "▶ Terminal (ASCII) output:"
{
    echo "$H1"
    echo "$H2"
    echo "$H3"
} | "$DASH_BIN"
