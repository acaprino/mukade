#!/usr/bin/env bash
# `mukade chat launch codex` end to end, on a bench: a scratch CODEX_HOME with no plugins and a
# local mock of the Responses API (mock_responses.py), so no model is paid for and no real Codex
# configuration is read or written.
#
# Usage: codex-bench.sh <mukade executable>
#
# The bench's CODEX_HOME holds no hook but mukade's, so `--dangerously-bypass-hook-trust` stands
# in for the one-time `/hooks` review a real installation needs; outside such a bench that flag
# would also run every other untrusted hook, and mukade never passes it.
#
# Checks: the launched Codex joins the room; the model is given its introduction; it ends its
# turn and listens; a message addressed to it restarts it with the message; it stops after its
# one wake-up.
set -u
M="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
HERE="$(cd "$(dirname "$0")" && pwd)"
# MUKADE_LIVE_SCRATCH: a folder outside any project for the throwaway service (default: TMPDIR).
WORK="$(mktemp -d -p "${MUKADE_LIVE_SCRATCH:-${TMPDIR:-/tmp}}")"
native() { (cd "$1" && pwd -W 2>/dev/null || pwd); }
export MUKADE_DATA_DIR="$(native "$WORK")/data"
mkdir -p "$WORK/codex-home" "$WORK/project"
python -I "$HERE/mock_responses.py" "$WORK/port" "$WORK/requests.jsonl" &
MOCK=$!
trap '"$M" chat stop > /dev/null; kill $MOCK 2>/dev/null; cd /; rm -rf "$WORK"' EXIT
for _ in $(seq 1 50); do [ -s "$WORK/port" ] && break; sleep 0.1; done
cat > "$WORK/codex-home/config.toml" <<EOF
model = "mock-model"
model_provider = "mock"
[model_providers.mock]
name = "mock"
base_url = "http://127.0.0.1:$(cat "$WORK/port")/v1"
wire_api = "responses"
requires_openai_auth = false
[features]
plugins = false
EOF
export CODEX_HOME="$(native "$WORK/codex-home")"
cd "$WORK/project"

env -u HTTPS_PROXY -u NODE_EXTRA_CA_CERTS "$M" chat launch codex --room bench --listen 30 --max-wakes 1 -- \
  exec --ephemeral --skip-git-repo-check -s read-only -c 'approval_policy="never"' \
  --dangerously-bypass-hook-trust "Say hello." < /dev/null > "$WORK/codex.out" 2>&1 &
CODEX=$!

ROOM=""
for _ in $(seq 1 60); do
  ROOM=$("$M" chat rooms --json 2>/dev/null | python -c "import json,sys;r=json.load(sys.stdin)['items'];print(r[0]['room_id'] if r and r[0]['members'] else '')" 2>/dev/null)
  [ -n "$ROOM" ] && break
  sleep 1
done
echo "== room: ${ROOM:-none}"
# The tester writes to the room from the command line, once Codex has ended its first turn.
sleep 5
HANDLE=$("$M" chat join --room-id "$ROOM" --harness tester --role peer --json 2>/dev/null | python -c "import json,sys;print(json.load(sys.stdin)['member_handle'])")
"$M" chat send --room-id "$ROOM" --member-handle "$HANDLE" --text "@codex-1 are you there?" --json > /dev/null 2>&1
wait "$CODEX"
echo "== codex exit $?"
grep -E "^hook:" "$WORK/codex.out"
echo "== MCP startup (nothing below: the mukade server answered with the credential from the environment)"
grep -i -E "mcp|unauthori|401" "$WORK/codex.out"
echo "== requests the model got, and what in them came from mukade"
python - "$WORK/requests.jsonl" <<'PY'
import json, sys
for n, line in enumerate(open(sys.argv[1], encoding="utf-8")):
    body = json.loads(line)["body"]
    texts = []
    for item in body.get("input", []):
        for part in item.get("content", []) or []:
            text = part.get("text", "") if isinstance(part, dict) else ""
            if "[mukade chat]" in text:
                texts.append(f'{item.get("role")}: {text[:110]!r}')
    # Codex groups an MCP server's tools in a namespace named mcp__<server>.
    spaces = [t for t in body.get("tools", []) if t.get("name") == "mcp__mukade"]
    chat = sorted(t.get("name", "") for space in spaces for t in space.get("tools", []))
    print(f"request {n + 1}:", "; ".join(texts) or "(nothing from mukade)")
    print(f"  mukade tools offered to the model: {len(chat)} {chat[:4]}")
PY
# BENCH_KEEP: a folder to keep the requests and the Codex output in, for a closer look.
[ -n "${BENCH_KEEP:-}" ] && cp "$WORK/requests.jsonl" "$WORK/codex.out" "$BENCH_KEEP/"
echo "== members"; "$M" chat members --room-id "$ROOM" --json 2>/dev/null | python -c "import json,sys;print([m['name'] for m in json.load(sys.stdin)['items']])"
