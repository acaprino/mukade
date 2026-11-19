#!/usr/bin/env bash
# Two Claude Code instances started with `mukade chat launch`: nobody tells them to wait. The
# adapter's hooks make each a room member; the listening Stop hook wakes the first when the
# second asks it something, and wakes the second when the answer comes.
#
# Usage: launch-listen.sh <mukade executable>
#
# - Throwaway service and folder (MUKADE_LIVE_SCRATCH, default TMPDIR), removed at the end.
# - The user's Claude Code settings are not written: each instance gets a one-session plugin.
# - Two `claude -p` runs with `haiku` are paid for, a few turns each.
set -u
M="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
# MUKADE_LIVE_SCRATCH: a folder outside any project for the throwaway service (default: TMPDIR).
WORK="$(mktemp -d -p "${MUKADE_LIVE_SCRATCH:-${TMPDIR:-/tmp}}")"
export MUKADE_DATA_DIR="$(cd "$WORK" && pwd -W 2>/dev/null || pwd)/data"
cd "$WORK"
trap '"$M" chat stop > /dev/null; cd /; rm -rf "$WORK"' EXIT

TOOLS="mcp__plugin_mukade_mukade mcp__mukade"
launch() { env -u CLAUDECODE "$M" chat launch claude_code --room live "$@"; }

# A: greets the room, then listens; one wake-up is enough for this check.
launch --listen 150 --max-wakes 1 -- -p --model haiku --allowedTools "$TOOLS" \
  --no-session-persistence \
  "Greet the room in one short message with the chat_send tool, then end your turn. If a member asks you something later, answer it with chat_send, addressed to that member." \
  < /dev/null > a.out 2> a.err &
A=$!

# B joins once A is a member.
for _ in $(seq 1 60); do
  "$M" chat rooms --json 2>/dev/null | grep -q '"members":1' && break
  sleep 1
done

launch --listen 150 --max-wakes 1 -- -p --model haiku --allowedTools "$TOOLS" \
  --no-session-persistence \
  "Find the other member's name in the messages you were given. Ask it, with one chat_send message addressed to it, what 6 times 7 is, then end your turn. When its answer reaches you, reply to the user with one line: the answer you received." \
  < /dev/null > b.out 2> b.err
wait "$A"

echo "== A (stdout)"; tail -5 a.out; echo "== A (stderr)"; tail -3 a.err
echo "== B (stdout)"; tail -5 b.out; echo "== B (stderr)"; tail -3 b.err
ROOM=$("$M" chat rooms --json 2>/dev/null | python -c "import json,sys;print(json.load(sys.stdin)['items'][0]['room_id'])")
echo "== room events"
"$M" chat ledger --room-id "$ROOM" --json 2>/dev/null | python -c "
import json,sys
for e in json.load(sys.stdin)['items']:
    p=e['payload']; print(e['event_id'], e['kind'], e['actor'].get('name',''), (p.get('text') or p.get('role') or '')[:100])"
