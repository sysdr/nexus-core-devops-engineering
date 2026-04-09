#!/usr/bin/env bash
set -euo pipefail

kill_if_pidfile() {
  local pidfile="$1"
  if [ -f "$pidfile" ]; then
    local pid; pid="$(cat "$pidfile" 2>/dev/null || true)"
    if [ -n "${pid:-}" ] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      sleep 0.2
      kill -9 "$pid" 2>/dev/null || true
    fi
    rm -f "$pidfile"
  fi
}

kill_if_pidfile /tmp/nexuscore-host.pid
kill_if_pidfile /tmp/nexuscore-mockhost.pid
if [ -f /tmp/nexuscore-ingester.pid ]; then
  pid="$(cat /tmp/nexuscore-ingester.pid 2>/dev/null || true)"
  if [ -n "${pid:-}" ] && kill -0 "$pid" 2>/dev/null; then
    kill "$pid" 2>/dev/null || true
    sleep 0.2
    kill -9 "$pid" 2>/dev/null || true
  elif [ -n "${pid:-}" ] && sudo kill -0 "$pid" 2>/dev/null; then
    sudo kill "$pid" 2>/dev/null || true
    sleep 0.2
    sudo kill -9 "$pid" 2>/dev/null || true
  fi
  rm -f /tmp/nexuscore-ingester.pid
fi
kill_if_pidfile /tmp/nexuscore-sink.pid

# If PID files are stale, fall back to killing by bound ports.
for p in 9090 9091; do
  pid="$(ss -ltnp 2>/dev/null | awk -v port=":$p" '$4 ~ port {print $0}' | sed -n 's/.*pid=\\([0-9][0-9]*\\).*/\\1/p' | head -n 1)"
  if [ -n "${pid:-}" ] && kill -0 "$pid" 2>/dev/null; then
    kill "$pid" 2>/dev/null || true
    sleep 0.2
    kill -9 "$pid" 2>/dev/null || true
  fi
done

rm -f /tmp/nexuscore-host.sock /tmp/nexuscore-sink.log /tmp/nexuscore.pid /tmp/nexuscore-mockhost.log
echo "[cleanup] Done."
