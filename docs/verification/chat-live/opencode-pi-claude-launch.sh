#!/usr/bin/env bash
# A real OpenCode, a real Pi and a real Claude Code, all started with `mukade chat launch` and
# listening: Claude Code asks each of the other two one question, neither is told to wait.
#
# Usage: opencode-pi-claude-launch.sh <mukade executable> [opencode model]
#
# - Throwaway service and folder (MUKADE_LIVE_SCRATCH, default TMPDIR), removed at the end. It
#   must lie outside any git repository: the chat service refuses a data folder inside a project.
# - OpenCode writes its session into a scratch database (OPENCODE_DB), exported at the end to
#   show where the room's synthetic message sits; its own configuration is read, not written.
#   Default model: openrouter/~anthropic/claude-haiku-latest (OPENROUTER_API_KEY).
# - Pi runs with `--no-session` and its configured model; Claude Code with `haiku`. PI_ARGS adds
#   Pi arguments: `PI_ARGS="-ne -e builtin:mcp"` loads Pi's own MCP and no installed extension, for
#   a Pi whose settings turn the built-in MCP off in favour of an MCP package.
# - MUKADE_LIVE_KEEP=1 keeps the work folder (its path is printed first).
# - Paid for: a few turns of each.
set -u
M="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
OC_MODEL="${2:-openrouter/~anthropic/claude-haiku-latest}"
WORK="$(mktemp -d -p "${MUKADE_LIVE_SCRATCH:-${TMPDIR:-/tmp}}")"
echo "== work folder: $WORK"
native() { (cd "$1" && pwd -W 2>/dev/null || pwd); }
export MUKADE_DATA_DIR="$(native "$WORK")/data"
# A folder of its own: the service refuses a data folder inside a harness's store.
mkdir -p "$WORK/opencode-store"
OC_DB="$(native "$WORK/opencode-store")/opencode.db"
if [ "${MUKADE_LIVE_KEEP:-0}" = 1 ]; then
  trap '"$M" chat stop > /dev/null' EXIT
else
  trap '"$M" chat stop > /dev/null; cd /; rm -rf "$WORK"' EXIT
fi
cd "$WORK"
"$M" chat start > /dev/null || exit 4

ASK_LATER="If a member asks you something later, answer it with chat_send, addressed to that member."

# OpenCode: greets, then listens; one wake-up is enough.
OPENCODE_DB="$OC_DB" "$M" chat launch opencode --room r --listen 120 --max-wakes 1 -- \
  run -m "$OC_MODEL" --format json \
  "Greet the room in one short message with the chat_send tool, then end your turn. $ASK_LATER" \
  < /dev/null > opencode.out 2> opencode.err &
OC=$!

# Pi: the same.
# shellcheck disable=SC2086 # PI_ARGS is a list of arguments.
"$M" chat launch pi --room r --listen 120 --max-wakes 1 -- \
  ${PI_ARGS:-} -p --no-session \
  "Greet the room in one short message with the chat_send tool, then end your turn. $ASK_LATER" \
  < /dev/null > pi.out 2> pi.err &
PI=$!

joined=0
for _ in $(seq 1 120); do
  "$M" chat rooms --json 2>/dev/null | grep -q '"members":2' && { joined=1; break; }
  sleep 1
done
echo "== both joined before Claude Code: $joined"

# Claude Code: asks each one question, then listens for both answers.
env -u CLAUDECODE "$M" chat launch claude_code --room r --listen 120 --max-wakes 2 -- \
  -p --model haiku --allowedTools "mcp__plugin_mukade_mukade mcp__mukade" --no-session-persistence \
  "Find the members of your room with the chat_members tool: one name starts with opencode, one with pi. With chat_send, ask the opencode member which planet is the largest in the solar system, and ask the pi member what the capital of France is: one message to each, addressed to it. Then end your turn. When both answers reach you, reply to the user with one line per member: its name and the answer you received." \
  < /dev/null > claude.out 2>&1
wait "$OC"; echo "== OpenCode exit $?"
wait "$PI"; echo "== Pi exit $?"

echo "== OpenCode stderr (last lines)"; tail -5 opencode.err
echo "== OpenCode tool calls"
python -c "
import json,sys
for line in open('opencode.out',encoding='utf-8',errors='replace'):
    try: e=json.loads(line)
    except Exception: continue
    p=e.get('part') or {}
    if p.get('type')=='tool': print(' ', p.get('tool'), (p.get('state') or {}).get('status'))
    if p.get('type')=='text' and p.get('text'): print('  text:', p['text'][:120].replace(chr(10),' '))
"
echo "== Pi stdout (last lines)";  tail -4 pi.out
echo "== Pi stderr (last lines)";  tail -4 pi.err
echo "== Claude Code";             tail -3 claude.out

ROOM=$("$M" chat rooms --json 2>/dev/null | python -c "import json,sys;print(json.load(sys.stdin)['items'][0]['room_id'])")
echo "== room events"
"$M" chat ledger --room-id "$ROOM" --json 2>/dev/null | python -c "
import json,sys
for e in json.load(sys.stdin)['items']:
    p=e['payload']; print(e['event_id'], e['kind'], e['actor'].get('name',''), (p.get('text') or p.get('role') or '')[:110])"

# Where the room's text sits in OpenCode's session: the synthetic message, then the typed prompt.
echo "== OpenCode session, message order"
SID=$(python -c "
import json
for line in open('opencode.out',encoding='utf-8',errors='replace'):
    try: e=json.loads(line)
    except Exception: continue
    s=e.get('sessionID') or (e.get('part') or {}).get('sessionID')
    if s: print(s); break
")
echo "  session: ${SID:-none found}"
if [ -n "$SID" ]; then
  OPENCODE_DB="$OC_DB" opencode session export "$SID" --standalone > export.json 2> export.err \
    || { echo "  export failed:"; tail -3 export.err; }
  # OpenCode 2 export: messages[] with type user | synthetic | assistant | idle | ...
  python -c "
import json
d=json.load(open('export.json',encoding='utf-8'))
for m in d.get('messages',[]):
    t=m.get('type'); src=(m.get('metadata') or {}).get('source','')
    if t=='assistant':
        calls=[c.get('name') or c.get('tool') for c in m.get('content',[]) if c.get('type') in ('tool','tool-call','toolCall')]
        texts=[c.get('text','') for c in m.get('content',[]) if c.get('type')=='text']
        print(' ', t, 'tools:', calls, (' '.join(texts))[:70].replace(chr(10),' '))
    else:
        print(' ', t, src, (m.get('text') or m.get('outcome') or '')[:90].replace(chr(10),' '))
"
fi
