#!/usr/bin/env python3
import json
import os
import socket
import sys

SOCK = os.environ.get("NEXUS_HOST_SOCK", "/tmp/nexuscore-host.sock")

try:
    os.unlink(SOCK)
except FileNotFoundError:
    pass

srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
srv.bind(SOCK)
srv.listen(128)
sys.stderr.write(f"[mock-host] Listening on unix://{SOCK}\n")
sys.stderr.flush()

while True:
    conn, _ = srv.accept()
    try:
        data = b""
        while not data.endswith(b"\n") and len(data) < 1024 * 1024:
            chunk = conn.recv(4096)
            if not chunk:
                break
            data += chunk
        # best-effort parse; we don't need to respond for ingest accounting
        try:
            _ = json.loads(data.decode("utf-8").strip() or "{}")
        except Exception:
            pass
        # keep compatible with possible future callers expecting a JSON line
        try:
            conn.sendall(b'{"ok":true}\n')
        except Exception:
            pass
    finally:
        try:
            conn.close()
        except Exception:
            pass

