# Listing coverage: sessions whose transcripts are gone (2026-11-08)

A cleaning job reported that `mukade list` saw fewer projects than a dedicated reader on one
machine: Codex 0 against 60, OpenCode 14 against 16, Claude Code 36 against 38, no `live` flag and a
slower listing. This records how each claim was checked on that machine and what the same commands
print after the change. Read only throughout: the stores were opened read-only and nothing in them
was written. Counts only; no titles, prompts or paths are recorded here.

Machine: Windows 11, Claude Code 2.1.319, Codex 0.166, OpenCode 2.2.5; release build of this
repository, `CODEX_HOME`, `CLAUDE_CONFIG_DIR` and `OPENCODE_DB` set to the real stores.

## Before

| harness | `mukade list` | what the store holds |
|---|---|---|
| Codex | 0 sessions, no error | `sessions/` and `archived_sessions/` do not exist; `state_5.sqlite` has 8598 `threads` rows in 62 working directories, 0 of 8598 `rollout_path` files exist |
| OpenCode | 75 sessions, 14 directories | `opencode.db` (`session_v2`, 126 rows); `opencode-.db` has only the legacy `session` table: 62 rows in 2 directories |
| Claude Code | 213 sessions, 36 directories (after case and separator normalisation) | `history.jsonl` 2555 records, 261 sessions, 88 of them without a transcript, in 19 directories no transcript names; union 38 |
| Claude Code, speed | 11.3 s the first run after idle, 0.51–0.53 s three runs later | 213 files, up to 512 KiB read from each |

Not as reported: the Claude Code slowness is a cold-disk figure (the first run after idle); a warm
listing was already 0.5 s, not 2.7–8.9 s.

## After

| harness | `mukade list` | |
|---|---|---|
| Codex | **1105 sessions, 61 directories**, all `unreadable`, 135 ms | the 7493 subagent and guardian threads are left out of `list` (`source` containing `subagent`, `thread_source` `guardian_review`/`subagent`); including them the database has 62 directories |
| OpenCode | **137 sessions, 16 directories**, 62 `unreadable`, 135 ms | the 75 roots of `opencode.db` and the 62 legacy rows of `opencode-.db` |
| Claude Code | **303 sessions, 39 directories**, 88 `unreadable`, 5 `live`, first run 285 ms, then 65–72 ms | 215 transcripts (5 of them live) and 88 sessions from `history.jsonl` |

The 5 live sessions are the 5 `sessions/<pid>.json` files whose process runs; the machine also ran a
6th `claude` process with no registration, which is not a session.

## Facts found on the way

- OpenCode 2.2.5 **migrates what it opens**. Pointed at a copy of `opencode-.db` it created
  `session_v2` and moved 37 of the 62 legacy rows, and `session export` of the newest legacy row
  still answered `Session not found`. So mukade never starts OpenCode against a database that is
  not the selected one: it lists the file by reading it, and `read` refuses with the file's name.
- `sessions/<pid>.json` holds `sessionId`, `cwd`, `name`, `status`, `procStart` (a FILETIME equal to
  the process's creation time to the second) and `pidDomain` (`win32:<machine>`). The `.key` file
  beside it holds the peer-messaging key and is not read.
- Not verified: the liveness check on Linux and macOS (`/proc`, `ps`; `procStart` is not compared
  there), and a cold disk with an empty mukade cache.

## Re-run

```powershell
$env:CODEX_HOME = "$HOME\.codex"; $env:CLAUDE_CONFIG_DIR = "$HOME\.claude"
$env:OPENCODE_DB = "$HOME\.local\share\opencode\opencode.db"
foreach ($h in 'codex','opencode','claude_code') {
  $rows = mukade list $h | ConvertFrom-Json
  "{0}: {1} sessions, {2} directories, {3} unreadable" -f $h, $rows.Count,
    ($rows.cwd | Where-Object { $_ } | ForEach-Object { $_.TrimEnd('\','/').ToLower() } | Select-Object -Unique).Count,
    ($rows | Where-Object unreadable).Count
}
```
