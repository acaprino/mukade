#!/usr/bin/env bash
# A real Codex, as an observer, explains a Claude Code session through the reduced view.
#
# Usage: observer.sh <mukade executable> <mukade repository>
#
# - The observed session is the repository's Claude Code fixture, staged in a scratch
#   CLAUDE_CONFIG_DIR: no real session is shown to a model. It read two files whose contents
#   are the canaries ALPHA-CONTENT-42 and BETA-CONTENT-17; the observer must not see them.
# - The peer member is joined and bound from the command line, as the user would; Codex joins as
#   observer through its MCP tools. One real Codex turn is paid for.
# - Codex's configuration is not written (-c overrides). Where HTTPS goes through a local TLS
#   proxy Codex does not trust, it runs without HTTPS_PROXY and NODE_EXTRA_CA_CERTS.
set -u
M="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
REPO="$(cd "$2" && pwd)"
ID=c45db7bc-542c-4201-89de-b590c6fdb797
# MUKADE_LIVE_SCRATCH: a folder outside any project for the throwaway service (default: TMPDIR).
WORK="$(mktemp -d -p "${MUKADE_LIVE_SCRATCH:-${TMPDIR:-/tmp}}")"
native() { (cd "$1" && pwd -W 2>/dev/null || pwd); }
mkdir -p "$WORK/claude/projects/scratch-observed"
cp "$REPO/fixtures/claude_code/source/$ID.jsonl" "$WORK/claude/projects/scratch-observed/"
export CLAUDE_CONFIG_DIR="$(native "$WORK/claude")"
export MUKADE_DATA_DIR="$(native "$WORK")/data"
cd "$WORK"

trap '"$M" chat stop > /dev/null; cd /; rm -rf "$WORK"' EXIT
"$M" chat start > start.json || exit 4
ROOM=$("$M" chat open --name explain --json 2>/dev/null | python -c "import json,sys;print(json.load(sys.stdin)['room_id'])")
"$M" chat join --room-id "$ROOM" --harness claude_code --role peer --session-id "$ID" --json 2>/dev/null > peer.json \
  || { echo "binding the fixture session failed"; exit 4; }

python - "$MUKADE_DATA_DIR" <<'PYEOF'
import json, sys
start = json.load(open("start.json"))
token = open(sys.argv[1] + "/bus/credential").read()
open("codex.args", "w").write(
    'mcp_servers.mukade.url="%s"\n'
    'mcp_servers.mukade.http_headers={Authorization="Bearer %s","X-Mukade-Service-Id"="%s"}\n'
    "mcp_servers.mukade.tool_timeout_sec=330\n" % (start["endpoint"], token, start["service_id"]))
PYEOF
mapfile -t CX < codex.args

PROMPT="You are the explainer in a local chat room run by the MCP server named mukade. Use only its tools; do not read files or run commands.
1. Call chat_join with room_id \"$ROOM\", harness \"codex\", role \"observer\", and a fresh UUID as request_id.
2. Call chat_members with room_id \"$ROOM\" and take the member_id of the member whose harness is claude_code.
3. Call chat_observe with room_id \"$ROOM\", your member_handle and that target_member_id. Then call chat_ack with the returned batch_id.
4. Explain to the user, in at most six short lines, what that session did and why. Mark what the session stated in its own words as (stated) and what you infer as (inferred).
5. Then one line starting with CONTENTS: quoting any file contents you were shown, or CONTENTS: none."

env -u HTTPS_PROXY -u NODE_EXTRA_CA_CERTS codex exec --ephemeral --skip-git-repo-check -s read-only \
  -c 'approval_policy="never"' -c "${CX[0]}" -c "${CX[1]}" -c "${CX[2]}" "$PROMPT" \
  < /dev/null > codex.out 2>&1

echo "== Codex tool calls"; grep -E "mcp: mukade/.*\(completed\)" codex.out
echo "== Codex answer";     sed -n '/^codex$/,/^tokens used/p' codex.out | tail -n +2 | head -n -1
echo "== canaries in anything Codex printed"
grep -c -E "ALPHA-CONTENT-42|BETA-CONTENT-17" codex.out
