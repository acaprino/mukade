#!/usr/bin/env bash
# A real Codex and a real Claude Code, both started with `mukade chat launch` and listening:
# Claude Code asks, Codex answers, neither is told to wait.
#
# Usage: codex-claude-launch.sh <mukade executable>
#
# - Throwaway service and folder (MUKADE_LIVE_SCRATCH, default TMPDIR), removed at the end.
# - No harness configuration is written. Codex uses its own CODEX_HOME (its login) with
#   `-c features.plugins=false`, so its plugins' hooks are not loaded; with no other hook left,
#   `--dangerously-bypass-hook-trust` runs only mukade's three hooks and stands in for the
#   one-time `/hooks` review. Check that first: with a mock model the same command starts
#   exactly three hooks (docs/verification/2026-11-12-chat-live.md, run 7).
# - Where HTTPS goes through a local TLS proxy that Codex does not trust, Codex runs without
#   HTTPS_PROXY and NODE_EXTRA_CA_CERTS.
# - Paid for: a few Codex turns (its configured model) and a few Claude Code `haiku` turns.
set -u
M="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
# MUKADE_LIVE_SCRATCH: a folder outside any project for the throwaway service (default: TMPDIR).
WORK="$(mktemp -d -p "${MUKADE_LIVE_SCRATCH:-${TMPDIR:-/tmp}}")"
export MUKADE_DATA_DIR="$(cd "$WORK" && pwd -W 2>/dev/null || pwd)/data"
trap '"$M" chat stop > /dev/null; cd /; rm -rf "$WORK"' EXIT
cd "$WORK"
"$M" chat start > /dev/null || exit 4

# Codex: greets, then listens; one wake-up is enough.
env -u HTTPS_PROXY -u NODE_EXTRA_CA_CERTS "$M" chat launch codex --room duo --listen 200 --max-wakes 1 -- \
  -c features.plugins=false --dangerously-bypass-hook-trust \
  exec --ephemeral --skip-git-repo-check -s read-only -c 'approval_policy="never"' \
  "Greet the room in one short message with the chat_send tool, then end your turn. If a member asks you something later, answer it with chat_send, addressed to that member." \
  < /dev/null > codex.out 2>&1 &
CODEX=$!

for _ in $(seq 1 90); do
  "$M" chat rooms --json 2>/dev/null | grep -q '"members":1' && break
  sleep 1
done

# Claude Code: asks Codex, then listens for the answer.
env -u CLAUDECODE "$M" chat launch claude_code --room duo --listen 200 --max-wakes 1 -- \
  -p --model haiku --allowedTools "mcp__plugin_mukade_mukade mcp__mukade" --no-session-persistence \
  "Find the Codex member's name (its name starts with codex) with the chat_members tool. Ask it, with one chat_send message addressed to it, which planet is the largest in the solar system, then end your turn. When its answer reaches you, reply to the user with one line: the answer you received." \
  < /dev/null > claude.out 2>&1
wait "$CODEX"

echo "== Codex hooks";   grep -E "^hook:" codex.out
echo "== Codex tools";   grep -E "mcp: mukade/.*\((completed|failed)\)" codex.out
echo "== Codex last line"; tail -1 codex.out
echo "== Claude Code";   tail -2 claude.out
ROOM=$("$M" chat rooms --json 2>/dev/null | python -c "import json,sys;print(json.load(sys.stdin)['items'][0]['room_id'])")
echo "== room events"
"$M" chat ledger --room-id "$ROOM" --json 2>/dev/null | python -c "
import json,sys
for e in json.load(sys.stdin)['items']:
    p=e['payload']; print(e['event_id'], e['kind'], e['actor'].get('name',''), (p.get('text') or p.get('role') or '')[:110])"
