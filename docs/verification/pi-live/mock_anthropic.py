"""A mock Anthropic Messages endpoint for testing what a harness sends.

Listens on 127.0.0.1 only. Every POST /v1/messages body is appended to LOG (JSON Lines) and
checked against the provider rules a real API enforces (spec §7):
  A1  every tool_use is answered by a tool_result in the NEXT message
  A2  every tool_result answers a tool_use of the PREVIOUS assistant message
  A3  no tool_use is answered twice
  A4  a thinking block carries a signature
  ids match ^[a-zA-Z0-9_-]+$
A violation is answered with HTTP 400 and an Anthropic-style error, so the harness fails visibly.
Otherwise it streams a short text reply: MOCK-REPLY.

usage: mock_anthropic.py PORT_FILE LOG
"""
import json
import re
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ID = re.compile(r"^[a-zA-Z0-9_-]+$")
PORT_FILE, LOG = sys.argv[1], sys.argv[2]
LOCK = threading.Lock()


def blocks(message):
    content = message.get("content")
    if isinstance(content, str):
        return [{"type": "text", "text": content}]
    return content or []


def violations(body):
    found = []
    messages = body.get("messages", [])
    if messages and messages[0].get("role") != "user":
        found.append("first message is not a user message")
    for i, message in enumerate(messages):
        mine = blocks(message)
        if message.get("role") == "assistant":
            calls = [b for b in mine if b.get("type") == "tool_use"]
            nxt = blocks(messages[i + 1]) if i + 1 < len(messages) else []
            answered = [b.get("tool_use_id") for b in nxt if b.get("type") == "tool_result"]
            for call in calls:
                if not ID.match(call.get("id", "")):
                    found.append(f"A0 tool_use id {call.get('id')!r} outside the id pattern")
                if call.get("id") not in answered:
                    found.append(f"A1 tool_use {call.get('id')} not answered in the next message")
            for b in mine:
                if b.get("type") == "thinking" and not b.get("signature"):
                    found.append("A4 thinking block without a signature")
        for b in mine:
            if b.get("type") == "tool_result":
                prev = blocks(messages[i - 1]) if i > 0 else []
                called = [c.get("id") for c in prev if c.get("type") == "tool_use"]
                if b.get("tool_use_id") not in called:
                    found.append(f"A2 tool_result {b.get('tool_use_id')} answers no tool_use before it")
        results = [b.get("tool_use_id") for b in mine if b.get("type") == "tool_result"]
        if len(results) != len(set(results)):
            found.append("A3 a tool_use is answered twice")
    return found


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def _send(self, code, payload, ctype="application/json"):
        data = payload if isinstance(payload, bytes) else json.dumps(payload).encode()
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        raw = self.rfile.read(length)
        try:
            body = json.loads(raw)
        except ValueError:
            return self._send(400, {"type": "error", "error": {"type": "invalid_request_error", "message": "not JSON"}})
        problems = violations(body) if self.path.startswith("/v1/messages") else []
        with LOCK:
            with open(LOG, "a", encoding="utf-8") as f:
                f.write(json.dumps({"path": self.path, "violations": problems, "body": body}) + "\n")
        if problems:
            return self._send(
                400,
                {"type": "error", "error": {"type": "invalid_request_error", "message": "; ".join(problems)}},
            )
        events = [
            ("message_start", {"type": "message_start", "message": {
                "id": "msg_mock", "type": "message", "role": "assistant", "model": body.get("model", "mock"),
                "content": [], "stop_reason": None, "stop_sequence": None,
                "usage": {"input_tokens": 10, "output_tokens": 1}}}),
            ("content_block_start", {"type": "content_block_start", "index": 0,
                                     "content_block": {"type": "text", "text": ""}}),
            ("content_block_delta", {"type": "content_block_delta", "index": 0,
                                     "delta": {"type": "text_delta", "text": "MOCK-REPLY"}}),
            ("content_block_stop", {"type": "content_block_stop", "index": 0}),
            ("message_delta", {"type": "message_delta",
                               "delta": {"stop_reason": "end_turn", "stop_sequence": None},
                               "usage": {"output_tokens": 5}}),
            ("message_stop", {"type": "message_stop"}),
        ]
        out = b"".join(f"event: {name}\ndata: {json.dumps(data)}\n\n".encode() for name, data in events)
        self._send(200, out, "text/event-stream")

    def do_GET(self):
        self._send(404, {"type": "error", "error": {"type": "not_found_error", "message": "mock"}})


server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
with open(PORT_FILE, "w") as f:
    f.write(str(server.server_address[1]))
server.serve_forever()
