#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import signal
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import unquote, urlparse


SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.dirname(SCRIPT_DIR)
DASHBOARD_FILE = os.path.join(REPO_ROOT, "visualizer", "index.html")


class Metrics:
    def __init__(self) -> None:
        self.lock = threading.Lock()
        self.cache_hits = 0
        self.cache_misses = 0
        self.rebuilds_ok = 0
        self.rebuild_errors = 0
        self.avg_rebuild_ns = 0

    def tick(self) -> None:
        with self.lock:
            self.cache_misses += 100
            self.cache_hits += 1000
            self.rebuilds_ok += 100
            self.avg_rebuild_ns = 2500 + (self.rebuilds_ok % 200) * 10
            if self.rebuilds_ok % 500 == 0:
                self.rebuild_errors += 1

    def snapshot(self) -> dict:
        with self.lock:
            total = self.cache_hits + self.cache_misses
            hit_rate = int(self.cache_hits * 100 / total) if total else 0
            return {
                "cache_hits": self.cache_hits,
                "cache_misses": self.cache_misses,
                "hit_rate_pct": hit_rate,
                "rebuilds_ok": self.rebuilds_ok,
                "rebuild_errors": self.rebuild_errors,
                "avg_rebuild_ns": self.avg_rebuild_ns,
            }


def _route_path(raw_path: str) -> str:
    p = urlparse(raw_path).path
    p = unquote(p) or "/"
    if len(p) > 1 and p.endswith("/"):
        p = p.rstrip("/") or "/"
    return p


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=8080)
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--pidfile", default="")
    args = ap.parse_args()

    m = Metrics()
    stop = threading.Event()

    def on_sig(_signum, _frame):
        stop.set()

    signal.signal(signal.SIGINT, on_sig)
    signal.signal(signal.SIGTERM, on_sig)

    dashboard_cache: list[bytes | None] = [None]

    def load_dashboard() -> bytes:
        if dashboard_cache[0] is None:
            with open(DASHBOARD_FILE, "rb") as f:
                dashboard_cache[0] = f.read()
        return dashboard_cache[0]

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:  # noqa: N802
            path = _route_path(self.path)
            if path == "/metrics":
                body = json.dumps(m.snapshot()).encode("utf-8")
                self.send_response(200)
                self.send_header("Content-Type", "application/json; charset=utf-8")
                self.send_header("Content-Length", str(len(body)))
                self.send_header("Access-Control-Allow-Origin", "*")
                self.end_headers()
                self.wfile.write(body)
                return
            if path in ("/", "/index.html", "/dashboard"):
                try:
                    body = load_dashboard()
                except OSError:
                    msg = b"Dashboard file missing: visualizer/index.html\n"
                    self.send_response(500)
                    self.send_header("Content-Type", "text/plain; charset=utf-8")
                    self.send_header("Content-Length", str(len(msg)))
                    self.end_headers()
                    self.wfile.write(msg)
                    return
                self.send_response(200)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.send_header("Content-Length", str(len(body)))
                self.send_header("Cache-Control", "no-store")
                self.end_headers()
                self.wfile.write(body)
                return
            self.send_response(404)
            self.send_header("Content-Type", "text/plain; charset=utf-8")
            self.end_headers()
            self.wfile.write(b"Not found\n")

        def log_message(self, _format: str, *_args) -> None:
            return

    httpd = ThreadingHTTPServer((args.host, args.port), Handler)

    if args.pidfile:
        try:
            with open(args.pidfile, "w", encoding="utf-8") as f:
                f.write(str(os.getpid()))
        except OSError:
            pass

    def serve():
        while not stop.is_set():
            httpd.handle_request()

    th = threading.Thread(target=serve, daemon=True)
    th.start()

    next_tick = time.time()
    try:
        while not stop.is_set():
            now = time.time()
            if now >= next_tick:
                m.tick()
                next_tick = now + 0.05
            time.sleep(0.01)
    finally:
        httpd.server_close()
        if args.pidfile:
            try:
                os.remove(args.pidfile)
            except OSError:
                pass

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
