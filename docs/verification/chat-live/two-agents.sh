#!/usr/bin/env bash
# Two real harness instances talk through the mukade chat service: Codex joins a room and waits,
# Claude Code joins, asks Codex a question by tag, Codex answers, Claude Code reads the answer.
#
# Usage: two-agents.sh <mukade executable>
#
# - The service runs in a fresh data folder under the system temporary folder, removed at the end.
# - Neither harness's own configuration is written: Claude Code gets --mcp-config with
#   --strict-mcp-config, Codex gets -c overrides for this one run. The credential of this
#   throwaway service is visible on the Codex command line while it runs.
# - Two real model turns are paid for: Claude Code with `haiku`, Codex with its configured model.
# - Where HTTPS goes through a local TLS proxy that Codex does not trust, Codex runs without
#   HTTPS_PROXY and NODE_EXTRA_CA_CERTS (docs/verification/source-model-summary-live.md).
set -u
M="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
# MUKADE_LIVE_SCRATCH: a folder outside any project for the throwaway service (default: TMPDIR).
WORK="$(mktemp -d -p "${MUKADE_LIVE_SCRATCH:-${TMPDIR:-/tmp}}")"
export MUKADE_DATA_DIR="$(cd "$WORK" && pwd -W 2>/dev/null || pwd)/data"
cd "$WORK"

"$M" chat start > start.json || exit 4
trap '"$M" chat stop > /dev/null; cd /; rm -rf "$WORK"' EXIT
ROOM=$("$M" chat open --name duo --json 2>/dev/null | python -c "import json,sys;print(json.load(sys.stdin)['room_id'])")

python - "$MUKADE_DATA_DIR" <<'PYEOF'
import json, sys
start = json.load(open("start.json"))
token = open(sys.argv[1] + "/bus/credential").read()
headers = {"Authorization": "Bearer " + token, "X-Mukade-Service-Id": start["service_id"]}
# The same entries `mukade chat connect claude_code|codex` writes.
json.dump({"mcpServers": {"mukade": {"type": "http", "url": start["endpoint"], "headers": headers}}},
          open("mcp.json", "w"))
open("codex.args", "w").write(
    'mcp_servers.mukade.url="%s"\n'
    'mcp_servers.mukade.http_headers={Authorization="Bearer %s","X-Mukade-Service-Id"="%s"}\n'
    "mcp_servers.mukade.tool_timeout_sec=330\n" % (start["endpoint"], token, start["service_id"]))
PYEOF
mapfile -t CX < codex.args

CODEX_PROMPT="You are a participant in a local chat room run by the MCP server named mukade. Use only its tools.
1. Call chat_join with room_id \"$ROOM\", harness \"codex\", role \"peer\", and a fresh UUID as request_id. Remember the member_handle and your name from the result.
2. Call chat_wait with room_id \"$ROOM\", your member_handle, addressed_only true, timeout_seconds 240.
3. If it returns events, read the newest message addressed to you, then call chat_ack with the returned batch_id.
4. Call chat_send with room_id \"$ROOM\", your member_handle, a fresh UUID request_id, and a short text that answers that message and starts with @ followed by the sender's name.
5. Finish with one line: the text you received and the text you sent."

env -u HTTPS_PROXY -u NODE_EXTRA_CA_CERTS codex exec --ephemeral --skip-git-repo-check -s read-only \
  -c 'approval_policy="never"' -c "${CX[0]}" -c "${CX[1]}" -c "${CX[2]}" "$CODEX_PROMPT" \
  < /dev/null > codex.out 2>&1 &
CODEX_PID=$!

# Codex joins first, so Claude Code can address it by name.
for _ in $(seq 1 90); do
  "$M" chat members --room-id "$ROOM" --json 2>/dev/null | grep -q '"harness":"codex"' && break
  sleep 1
done

CLAUDE_PROMPT="You are a participant in a local chat room run by the MCP server named mukade. Use only its tools.
1. Call chat_join with room_id \"$ROOM\", harness \"claude\", role \"peer\", and a fresh UUID as request_id. Remember the member_handle.
2. Call chat_members with room_id \"$ROOM\" and find the member whose harness is codex; note its name.
3. Call chat_send with room_id \"$ROOM\", your member_handle, a fresh UUID request_id, and the text: @<that name> What is the capital of Italy? Answer in one word.
4. Call chat_wait with room_id \"$ROOM\", your member_handle, addressed_only true, timeout_seconds 200. Then call chat_ack with the returned batch_id.
5. Finish with one line: the reply you received, verbatim."

MCP_TOOL_TIMEOUT=330000 env -u CLAUDECODE claude -p --model haiku --mcp-config mcp.json --strict-mcp-config \
  --allowedTools "mcp__mukade__chat_join mcp__mukade__chat_members mcp__mukade__chat_send mcp__mukade__chat_wait mcp__mukade__chat_ack" \
  --no-session-persistence "$CLAUDE_PROMPT" < /dev/null > claude.out 2>&1

wait "$CODEX_PID"
echo "== Claude Code";  tail -3 claude.out
echo "== Codex";        grep -E "mcp: mukade/.*\(completed\)" codex.out; tail -1 codex.out
echo "== room members"; "$M" chat members --room-id "$ROOM" --json 2>/dev/null
