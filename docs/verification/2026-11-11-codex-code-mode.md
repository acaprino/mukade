# Codex code mode (L10) on real rollouts — 2026-11-11

Read-only census of every Codex rollout on the development machine (Windows 11), with
`tests/codex_code_mode_census.rs`:

```text
CODEX_HOME=%USERPROFILE%\.codex cargo test --test codex_code_mode_census -- --ignored --nocapture
```

Nothing was written; the test only reads `sessions/` and `archived_sessions/`.

## Result

| | count |
|---|---|
| rollouts read (Codex 0.167.0, 0.167.0-alpha.3.2, 0.167.1) | 34, none unreadable |
| `exec` calls | 981 |
| split into the calls they made | **725** (74%) |
| kept whole (text form) | 256 |
| calls made by the split scripts | 1,139 |

Calls made by split scripts, by tool: `exec_command` 696, `apply_patch` 184, `write_stdin` 152,
`web__run` 56, MCP tools 50, `clock__curr_time` 1.

Every call a split script made has exactly one result with its id, and no id carries a character
outside the providers' id alphabet (the test asserts both).

## How many reach a destination as a native call

| destination | without `--dest-shell` | with `--dest-shell powershell` |
|---|---|---|
| Claude Code | 59 | 59 |
| Pi | 88 | 645 |
| Codex | 0 | 696 |

- The 59 toward Claude and 88 toward Pi without the flag are `apply_patch` calls read as writes
  and edits (Pi also takes the multi-hunk ones as a multi-edit). The other patches hold several
  files, a delete, a hunk of context only (in Codex it places the next hunk), or several hunks
  toward Claude, and stay text form.
- Every rollout declares `<shell>powershell</shell>`, so a split `exec_command` is canonical
  `PowerShell`. The dialect rule (L13) keeps it text form unless the destination's shell is
  PowerShell; Claude's `PowerShell` tool is not in its offered set. Both are the next open point
  (`next.md`, *Needs code*, destination shell).
- `write_stdin`, `web__run` and MCP tools are text form by rule (L14).

## Before

Before this change the same 981 calls were one opaque `exec` each, text form toward every
destination, Codex included.
