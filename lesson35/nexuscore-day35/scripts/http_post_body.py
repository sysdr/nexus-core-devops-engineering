"""Read POST bodies for stdlib ``http.server``: ``Content-Length`` or HTTP/1.1 chunked.

Wasmtime's WASI HTTP stack often emits ``Transfer-Encoding: chunked`` without
``Content-Length``. Reading only ``Content-Length`` bytes yields an empty body.
"""

from __future__ import annotations

import http.server


def read_chunked_body(rfile) -> bytes:
    out = bytearray()
    while True:
        line = rfile.readline()
        if not line:
            break
        chunk_meta = line.strip()
        if not chunk_meta:
            continue
        size_hex = chunk_meta.split(b";", 1)[0]
        try:
            chunk_len = int(size_hex, 16)
        except ValueError:
            break
        if chunk_len == 0:
            while True:
                trailer = rfile.readline()
                if trailer in (b"\r\n", b"\n", b""):
                    break
            break
        chunk = rfile.read(chunk_len)
        out.extend(chunk)
        rfile.readline()
    return bytes(out)


def read_post_body(handler: http.server.BaseHTTPRequestHandler) -> bytes:
    te = (handler.headers.get("Transfer-Encoding") or "").lower()
    if "chunked" in [x.strip() for x in te.split(",") if x.strip()]:
        return read_chunked_body(handler.rfile)
    length = int(handler.headers.get("Content-Length", "0") or "0")
    if length > 0:
        return handler.rfile.read(length)
    return b""
