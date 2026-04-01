#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

PORT="${NEXUSCORE_DASHBOARD_PORT:-8000}"
PID_FILE="${NEXUSCORE_DASHBOARD_PID_FILE:-/tmp/nexuscore_dashboard_${PORT}.pid}"

start_server() {
  # Serve project root so /web/index.html works
  nohup python3 -m http.server "$PORT" --bind 127.0.0.1 >/tmp/nexuscore_dashboard_"$PORT".log 2>&1 &
  echo "$!" > "$PID_FILE"
}

wait_ready() {
  local url="http://127.0.0.1:${PORT}/web/index.html"
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    if python3 - <<PY >/dev/null 2>&1
import urllib.request
urllib.request.urlopen("${url}", timeout=0.5).read(1)
PY
    then
      return 0
    fi
    sleep 0.2
  done
  return 1
}

is_running() {
  local pid=""
  pid="$(cat "$PID_FILE" 2>/dev/null || true)"
  [[ -n "$pid" ]] && ps -p "$pid" >/dev/null 2>&1
}

if is_running; then
  :
else
  rm -f "$PID_FILE" 2>/dev/null || true
  start_server
fi

URL="http://127.0.0.1:${PORT}/web/index.html"
if wait_ready; then
  echo "Dashboard URL: $URL"
  echo "Log: /tmp/nexuscore_dashboard_${PORT}.log"
else
  echo "Dashboard server started but is not responding yet."
  echo "URL (try again in a moment): $URL"
  echo "Log: /tmp/nexuscore_dashboard_${PORT}.log"
  exit 1
fi
