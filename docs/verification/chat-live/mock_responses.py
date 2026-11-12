"""A local stand-in for the Responses API, enough for `codex exec` to finish a turn.

Usage: python -I mock_responses.py <port-file> <log-file>
Every request is appended to the log (as one JSON line); every answer is one assistant message
that quotes the last user text it was sent, so a continuation prompt shows up in the output.
"""
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT_FILE, LOG_FILE = sys.argv[1], sys.argv[2]


def last_user_text(body):
    text = ""
    for item in body.get("input", []) or []:
        if item.get("role") == "user":
            for part in item.get("content", []) or []:
                if isinstance(part, dict) and part.get("type") in ("input_text", "text"):
                    text = part.get("text", "")
    return text


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_POST(self):
        length = int(self.headers.get("content-length", 0))
        body = json.loads(self.rfile.read(length) or b"{}")
        with open(LOG_FILE, "a", encoding="utf-8") as log:
            log.write(json.dumps({"path": self.path, "body": body}) + "\n")
        said = last_user_text(body)
        reply = "MOCK-REPLY to: " + said.replace("\n", " ")[:300]
        events = [
            {"type": "response.created", "response": {"id": "resp_1"}},
            {"type": "response.output_item.done", "item": {
                "type": "message", "role": "assistant", "id": "msg_1",
                "content": [{"type": "output_text", "text": reply}]}},
            {"type": "response.completed", "response": {"id": "resp_1", "usage": {
                "input_tokens": 1, "input_tokens_details": None, "output_tokens": 1,
                "output_tokens_details": None, "total_tokens": 2}}},
        ]
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.end_headers()
        for event in events:
            self.wfile.write(f"event: {event['type']}\ndata: {json.dumps(event)}\n\n".encode())
        self.wfile.flush()

    def do_GET(self):
        self.send_response(404)
        self.end_headers()


server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
with open(PORT_FILE, "w") as f:
    f.write(str(server.server_address[1]))
server.serve_forever()
