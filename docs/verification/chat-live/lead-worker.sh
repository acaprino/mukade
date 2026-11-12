#!/usr/bin/env bash
# A real Claude Code (lead) gives a real Codex (worker) an instruction; the worker carries it out,
# finds it cannot send a message, and answers with a signal; the lead reads the signal.
#
# Usage: lead-worker.sh <mukade executable>
#
# - Throwaway service and folder (MUKADE_LIVE_SCRATCH, default TMPDIR), removed at the end.
# - No harness configuration is written (--mcp-config / -c overrides). Two real model turns.
# - Where HTTPS goes through a local TLS proxy Codex does not trust, Codex runs without
#   HTTPS_PROXY and NODE_EXTRA_CA_CERTS.
set -u
M="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
# MUKADE_LIVE_SCRATCH: a folder outside any project for the throwaway service (default: TMPDIR).
WORK="$(mktemp -d -p "${MUKADE_LIVE_SCRATCH:-${TMPDIR:-/tmp}}")"
export MUKADE_DATA_DIR="$(cd "$WORK" && pwd -W 2>/dev/null || pwd)/data"
cd "$WORK"
echo "status: READY" > notes.txt

"$M" chat start > start.json || exit 4
trap '"$M" chat stop > /dev/null; cd /; rm -rf "$WORK"' EXIT
ROOM=$("$M" chat open --name build --json 2>/dev/null | python -c "import json,sys;print(json.load(sys.stdin)['room_id'])")

python - "$MUKADE_DATA_DIR" <<'PYEOF'
import json, sys
start = json.load(open("start.json"))
token = open(sys.argv[1] + "/bus/credential").read()
headers = {"Authorization": "Bearer " + token, "X-Mukade-Service-Id": start["service_id"]}
json.dump({"mcpServers": {"mukade": {"type": "http", "url": start["endpoint"], "headers": headers}}},
          open("mcp.json", "w"))
open("codex.args", "w").write(
    'mcp_servers.mukade.url="%s"\n'
    'mcp_servers.mukade.http_headers={Authorization="Bearer %s","X-Mukade-Service-Id"="%s"}\n'
    "mcp_servers.mukade.tool_timeout_sec=330\n" % (start["endpoint"], token, start["service_id"]))
PYEOF
mapfile -t CX < codex.args

WORKER_PROMPT="You are a worker in a local chat room run by the MCP server named mukade.
1. Call chat_join with room_id \"$ROOM\", harness \"codex\", role \"worker\", and a fresh UUID as request_id.
2. Call chat_wait with room_id \"$ROOM\", your member_handle, addressed_only true, timeout_seconds 240. Call chat_ack with the returned batch_id.
3. Carry out the instruction addressed to you. You may read files in the current folder; do not change any file.
4. Try once to call chat_send with any short text and a fresh UUID request_id, and note the error code you get.
5. Call chat_signal with room_id \"$ROOM\", your member_handle, a fresh UUID request_id, instruction_event_id set to the event id of the instruction, and signal \"done\" if you completed it, or signal \"blocked\" with a one-line reason if you could not.
6. Finish with one line: the instruction, the chat_send error code, and the signal you sent."

env -u HTTPS_PROXY -u NODE_EXTRA_CA_CERTS codex exec --ephemeral --skip-git-repo-check -s read-only \
  -c 'approval_policy="never"' -c "${CX[0]}" -c "${CX[1]}" -c "${CX[2]}" "$WORKER_PROMPT" \
  < /dev/null > codex.out 2>&1 &
CODEX_PID=$!

for _ in $(seq 1 90); do
  "$M" chat members --room-id "$ROOM" --json 2>/dev/null | grep -q '"harness":"codex"' && break
  sleep 1
done

LEAD_PROMPT="You are the lead in a local chat room run by the MCP server named mukade. Use only its tools.
1. Call chat_join with room_id \"$ROOM\", harness \"claude\", role \"lead\", and a fresh UUID as request_id.
2. Call chat_members with room_id \"$ROOM\" and find the worker; note its name.
3. Call chat_send with a fresh UUID request_id and the text: @<that name> Check that notes.txt in your working folder says READY. Signal done if it does, blocked with the reason if it does not.
4. Call chat_wait with room_id \"$ROOM\", your member_handle, timeout_seconds 200, and call chat_ack with the returned batch_id. If what arrived is not the worker's signal, wait once more.
5. Finish with one line: the worker's signal and its reason if any."

MCP_TOOL_TIMEOUT=330000 env -u CLAUDECODE claude -p --model haiku --mcp-config mcp.json --strict-mcp-config \
  --allowedTools "mcp__mukade__chat_join mcp__mukade__chat_members mcp__mukade__chat_send mcp__mukade__chat_wait mcp__mukade__chat_ack" \
  --no-session-persistence "$LEAD_PROMPT" < /dev/null > claude.out 2>&1

wait "$CODEX_PID"
echo "== lead (Claude Code)"; tail -3 claude.out
echo "== worker (Codex)";     grep -E "mcp: mukade/.*\((completed|failed)\)" codex.out
awk '/^codex$/{buf="";cap=1;next} /^(hook:|tokens used)/{cap=0} cap{buf=buf $0 "\n"} END{printf "%s", buf}' codex.out
echo "== room events";        "$M" chat ledger --room-id "$ROOM" --json 2>/dev/null \
  | python -c "import json,sys;[print(e['event_id'],e['kind'],e['actor'].get('name','')) for e in json.load(sys.stdin)['items']]"
