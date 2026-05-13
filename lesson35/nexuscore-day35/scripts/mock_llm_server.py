#!/usr/bin/env python3
"""
Minimal Anthropic-compatible /v1/messages endpoint for offline demos.
Returns deterministic ReAct-formatted text based on whether the prompt already
contains an Observation line from the mock search server.
"""
from __future__ import annotations

import json
import http.server
import os
import sys
from datetime import datetime
from typing import Optional

_SCRIPTS = os.path.dirname(os.path.abspath(__file__))
if _SCRIPTS not in sys.path:
    sys.path.insert(0, _SCRIPTS)
from http_post_body import read_post_body  # noqa: E402


def extract_user_prompt(body: dict) -> str:
    msgs = body.get("messages") or []
    parts: list[str] = []
    for m in msgs:
        if m.get("role") != "user":
            continue
        c = m.get("content")
        if isinstance(c, str):
            parts.append(c)
        elif isinstance(c, list):
            for block in c:
                if isinstance(block, dict) and block.get("type") == "text":
                    parts.append(str(block.get("text") or ""))
    return "\n".join(parts)


def extract_question_line(prompt: str) -> str:
    for line in prompt.splitlines():
        s = line.strip()
        if s.lower().startswith("question:"):
            return s.split(":", 1)[1].strip()
    return ""


def final_answer_for_topic(topic: str) -> str:
    """Short canned finals aligned with mock_search_server keywords."""
    t = topic.lower()
    if "ebpf" in t or "e-bpf" in t or "bpf" in t:
        fa = (
            "eBPF CO-RE (Compile Once, Run Everywhere) uses BTF (BPF Type Format) to make eBPF programs "
            "portable across kernel versions. Programs compiled with Clang + libbpf auto-relocate struct "
            "offsets at load time."
        )
    elif "wasi" in t or "wasm" in t:
        fa = (
            "WASI 0.3 refers to the evolving component-model surface (typed WIT imports, including wasi:http). "
            "WASI Preview 1 historically mapped POSIX-like capabilities to wasm32-wasi; Preview 2+ replaces that "
            "flat syscall style with composable, capability-scoped interfaces."
        )
    elif "rust" in t:
        fa = (
            "Rust 2024 Edition introduced async closures, precise capturing in closures, and stabilized the "
            "WASI 0.3 target (wasm32-wasip2), with improved borrow checker diagnostics."
        )
    else:
        fa = (
            f"Summary based on the search results above for: {topic[:200]}"
            if topic
            else "Summary based on the search results above."
        )
    return (
        "Thought: I can answer from the search observations above.\n"
        f"Final Answer: {fa}"
    )


def react_text(prompt: str, react_step: Optional[int]) -> str:
    """Mock Anthropic assistant text.

    The Wasm agent embeds ``nexuscore_step`` (0-based ReAct iteration) in the JSON body. Step 0
    asks for a Search action; step >= 1 returns a Final Answer so the demo completes without relying
    on the prompt containing ``Observation:`` when traces are omitted.

    The first Search uses the ``Question:`` line from the agent prompt so demos like ``What is eBPF?``
    match mock search keywords.

    Manual curls without ``nexuscore_step`` still work: respond with Final Answer when
    ``Observation:`` appears in the prompt.
    """
    topic = extract_question_line(prompt) or "the user question"
    q_short = topic if len(topic) <= 120 else topic[:117].rstrip() + "..."
    search_action = (
        "Thought: I should search for information relevant to the question.\n"
        f"Action: Search[{q_short}]"
    )

    if react_step is not None and react_step >= 1:
        return final_answer_for_topic(topic)
    if "Observation:" in prompt:
        return final_answer_for_topic(topic)
    return search_action


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, fmt: str, *args) -> None:
        print(f"[mock-llm] {datetime.now().strftime('%H:%M:%S')} {fmt % args}")

    def header_ci(self, name: str) -> Optional[str]:
        want = name.lower()
        for k, v in self.headers.items():
            if k.lower() == want:
                return v
        return None

    def do_GET(self) -> None:
        # Browsers and health checks use GET; this API is POST-only. Avoid 501 noise in logs.
        if self.path in ("/favicon.ico",):
            self.send_response(204)
            self.end_headers()
            return
        body = (
            "Mock LLM: send POST with JSON to /v1/messages (Anthropic-style). "
            "This server has no UI; use ./scripts/demo.sh to run the agent.\n"
        ).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "text/plain; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self) -> None:
        if self.path.rstrip("/") != "/v1/messages":
            self.send_response(404)
            self.end_headers()
            return

        raw = read_post_body(self).decode("utf-8", errors="replace")
        try:
            body = json.loads(raw or "{}")
        except json.JSONDecodeError:
            body = {}

        prompt = extract_user_prompt(body)
        raw_step = body.get("nexuscore_step")
        if isinstance(raw_step, bool) or raw_step is None:
            react_step = None
        elif isinstance(raw_step, int):
            react_step = raw_step
        else:
            try:
                react_step = int(raw_step)
            except (TypeError, ValueError):
                react_step = None
        step_hdr = self.header_ci("x-nexuscore-react-step")
        if react_step is None and step_hdr is not None and step_hdr.isdigit():
            react_step = int(step_hdr)

        text = react_text(prompt, react_step)

        resp = {
            "id": "msg_mock",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": text}],
            "model": "mock",
            "stop_reason": "end_turn",
        }

        payload = json.dumps(resp).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


if __name__ == "__main__":
    server = http.server.HTTPServer(("0.0.0.0", 9876), Handler)
    print("[mock-llm] Listening on http://0.0.0.0:9876/v1/messages")
    server.serve_forever()
