# Live check: a Claude Code fork hopped after its origin

| | |
|---|---|
| **Date** | 2026-11-12 |
| **Machine** | Windows 11, debug build of `701a26d` (no code change since), the author's real Claude Code store |
| **Client** | Claude Code 2.1.322 |
| **Result** | the hopped `/branch` fork names the origin's copy in `forkedFrom`, Claude Code opens it, and `/resume` lists it as it lists its own `/branch`: a row of its own. Claude Code never puts a fork under its origin. `--fork-session` writes no `forkedFrom`. |

The author's store held no fork (no `forkedFrom` in any transcript), so the sessions were made for
this check, each in a scratch project folder. `/branch` only runs in the interactive interface; the
scripts in [`fork-live/`](fork-live) drive it in a pseudo-console (`node-pty`, here the copy that
comes with dsh): [`drive-branch.cjs`](fork-live/drive-branch.cjs) resumes a session and types
`/branch <name>`, [`drive-native.cjs`](fork-live/drive-native.cjs) types one prompt and then
`/branch`, [`drive-screen.cjs`](fork-live/drive-screen.cjs) saves the screen of `--resume` (the
picker, or one session) without typing a prompt. They drop the `CLAUDE_CODE_*` markers a parent
Claude Code passes down: with `CLAUDE_CODE_CHILD_SESSION` the child saves no transcript.

## What ran

| step | command | session |
|---|---|---|
| 1 | `claude -p --model haiku "Reply with only the word ORIGIN."` in `fork-live` | origin `f2e1d6ff` |
| 2 | `claude -p --resume f2e1d6ff --fork-session "Reply with only the word FORK."` | `3af510f5` |
| 3 | `claude --resume f2e1d6ff`, `/branch mukade-fork-check`, `/exit` (no model turn) | fork `0249c78e` |
| 4 | `mukade hop claude_code f2e1d6ff claude_code --cwd …\fork-live-copy` | origin copy `68484e86` |
| 5 | `mukade hop claude_code 0249c78e claude_code --cwd …\fork-live-copy` | fork copy `b0864162` |
| 6 | in `fork-native`: one typed prompt, then `/branch native-fork-check` | `ae064155`, fork `c3219b50` |

Step 5's manifest: `source.parent_kind` `{kind: fork, at: 1c07bc2e…}` (the last row `/branch`
copied), `dest.parent` `68484e86…`, no losses.

## Seen

| check | result |
|---|---|
| `/branch` rows | every copied row keeps its uuid and carries `forkedFrom: {sessionId: <origin>, messageUuid: <that uuid>}`; the `custom-title` is the name given; its own rows (the `/branch` and `/exit` commands) carry none |
| `--fork-session` rows | the origin's uuids, the new `sessionId`, **no `forkedFrom`** on any row; `mukade list` shows it as a root |
| the written fork | its 3 rows carry `forkedFrom: {sessionId: 68484e86…, messageUuid: <the copy's uuid of the same entry>}`; each uuid is a row of the origin copy |
| `/resume` in `fork-live-copy` | two rows: `mukade-fork-check` (the fork copy) and `Reply with only the word ORIGIN.` (the origin copy) |
| `/resume` in `fork-native` | two rows: `native-fork-check` and `ORIGIN reply`: Claude Code's own fork is a row of its own too |
| Claude Code opens the fork copy | `claude --resume b0864162` shows `❯ Reply with only the word ORIGIN.` `● ORIGIN` and waits for a prompt |

Why no fork sits under its origin: the picker groups the sessions by `_g(e)` = `e.sessionId ??
e.messages[0]?.sessionId`. A header with `(+N other sessions)` gathers several leaves of one
session; a fork has its own id. Where 2.1.322 does follow `forkedFrom.sessionId` (its strings):
moving a session to the cloud reads each origin it names and refuses with `history_gone` when the
origin is not in the store; restoring file history copies the backups from the origin
(`forkedFromSessionId`). Neither was run: the first uploads the session.

## Found

- **`/resume` lists no session started in print mode** (`entrypoint: sdk-cli`), nor a `/branch` of
  one: `fork-live` showed `No conversations found in this project.` though it held three sessions.
  mukade writes no `entrypoint`, and its sessions are listed.
- In the picker a native row shows the git branch and the size of the whole transcript; a written
  one shows neither branch nor attachments (mukade writes no `gitBranch` and no attachment rows).

## Left in the store

Seven sessions in `~\.claude\projects\`: `D--mukade-test-scratch-fork-live` (3),
`D--mukade-test-scratch-fork-live-copy` (2), `D--mukade-test-scratch-fork-native` (2). Remove them
with the old test sessions ([checklist](windows-checklist.md) step 9).
