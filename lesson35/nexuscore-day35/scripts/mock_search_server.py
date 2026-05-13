#!/usr/bin/env python3
"""
NexusCore Day 35 — Mock Search Server
Simulates a search API so the agent can run without external dependencies.
Returns canned results for demo purposes.
Run: python3 mock_search_server.py
"""
import json
import http.server
import os
import sys
from datetime import datetime

_SCRIPTS = os.path.dirname(os.path.abspath(__file__))
if _SCRIPTS not in sys.path:
    sys.path.insert(0, _SCRIPTS)
from http_post_body import read_post_body  # noqa: E402

RESULTS = {
    "rust": "Rust 2024 Edition introduced async closures, precise capturing in closures, and stabilized the WASI 0.3 target (wasm32-wasip2). Key features include improved borrow checker diagnostics and const generics enhancements.",
    "wasi": "WASI Preview 3 (Component Model) defines composable Wasm modules with typed interfaces via WIT. It replaces POSIX-style syscalls with capability-based interface types, enabling shared-nothing multi-tenancy.",
    "ebpf": "eBPF CO-RE (Compile Once, Run Everywhere) uses BTF (BPF Type Format) to make eBPF programs portable across kernel versions. Programs compiled with Clang + libbpf auto-relocate struct offsets at load time.",
    "react agent": "ReAct (Reason + Act) agents interleave reasoning traces and tool calls. Each step produces a Thought, optional Action (tool call), and Observation (tool result), iterating until a Final Answer is reached.",
    "wasmtime": "Wasmtime 22+ added WASI Preview 2 component model support with async store semantics. Component instantiation with a warm AOT cache takes ~80-120 microseconds on modern hardware.",
    "default": "No specific result found. This is a mock search server for NexusCore Day 35 demo purposes."
}


class SearchHandler(http.server.BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        print(f"[mock-search] {datetime.now().strftime('%H:%M:%S')} {format % args}")

    def do_POST(self):
        if self.path != "/search":
            self.send_response(404)
            self.end_headers()
            return

        body_bytes = read_post_body(self)
        body = body_bytes.decode("utf-8", errors="replace")

        try:
            data = json.loads(body or "{}")
            query = data.get("q", "").lower()
        except Exception:
            query = ""

        # Find best matching result
        result = RESULTS["default"]
        for keyword, snippet in RESULTS.items():
            if keyword in query:
                result = snippet
                break

        response = json.dumps({"results": [{"snippet": result, "query": query}]})
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(response)))
        self.end_headers()
        self.wfile.write(response.encode())


if __name__ == "__main__":
    server = http.server.HTTPServer(("0.0.0.0", 8765), SearchHandler)
    print("[mock-search] Listening on http://0.0.0.0:8765/search")
    server.serve_forever()
