# Hermes Agent and DeepSeek Harness — design

Status: decided 2026-11-12 (design delegated; user: "Hermes / DeepSeek installali", then "fai tutto tu").
Supersedes ADR-0002's "source-only, schema 31" for Hermes and "writer later" for DeepSeek; ADR-0002 is
amended (status Accepted, with the changes below). Facts: scratchpad `hermes-deepseek-research.md`
([V] at Hermes v0.23.1 / v0.22.5, dsh 0.2.2-rc.1 / rc.2) and real stores made on this machine on
2026-11-12 with free OpenRouter models.

## Real stores on this machine (made 2026-11-12, for live checks; not fixtures)

| harness | where | what |
|---|---|---|
| Hermes v0.23.1 | `%LOCALAPPDATA%\hermes\state.db` (schema 33) | 6 `local-<64hex>` sessions (5 `source=oneshot`, 1 `cli`), tool calls `read_file`, `patch`, one resumed twice; `20261112_094615_fb3768` imported from a mukade Claude fixture via `hermes sessions import` |
| dsh 0.2.2-rc.1 | `%USERPROFILE%\.dsh\sessions\--<cwd key>--\session-6de20419-…\session.v4.jsonl.zstd` + a subagent child | 2 turns, tools `read`, `edit`, `subagent`, `send_message`; made with `--patch <scratchpad>\dsh-openrouter.patch.yml` (pi-ai route `openrouter`, model `cohere/north-mini-code:free`) |

How to make more (free): Hermes `hermes chat -q "<task>" -Q --source cli --provider openrouter -m cohere/north-mini-code:free --yolo --pass-session-id`;
dsh `dsh --profile headless --patch <dsh-openrouter.patch.yml> --json [--session-id <id>] "<task>"`.

## Decisions

1. **Ids**: `HarnessId::Hermes` = `hermes`, `HarnessId::DeepSeekHarness` = `deepseek_harness` (ADR-0002
   names); binaries `hermes`, `dsh`; catalog entries with store env `HERMES_HOME` / `DSH_HOME`,
   defaults (`%LOCALAPPDATA%\hermes` | `~/.hermes`; `~/.dsh`), exe env `MUKADE_HERMES_EXE`,
   `MUKADE_DSH_EXE`, flags `--hermes-exe`, `--dsh-exe`; detection layouts in `src/detect/`.
2. **Hermes reader** (`state.db`, `file:…?mode=ro`, never `immutable=1`; retry a transient
   `SQLITE_IOERR`/`BUSY` once):
   - gate: `schema_version` ≥ 32 and the needed columns present (`PRAGMA table_info`); a newer
     version with the columns is read; missing columns → refusal naming them.
   - list: root sessions (no `_delegate_from`); `source` in `cli`/`local` (+`created_source='cli'`)
     by default, every source with `--all-sources`? → no new flag: list all except `subagent`, and
     show `source` in the listing; children (`_delegate_from`) and branches (`_branched_from`) come
     through `list_all` with `ParentKind::Spawn` / `Fork`.
   - read: `messages … ORDER BY id`; `active=1` conversation; `active=0` → `Meta(OffBranch)`;
     `_compressed_summary=1` → compaction entry; roles user/assistant/tool; tool pairing by
     `tool_call_id`; `content` via `ValueRef` (`\x00json:` prefix → parts; NULL/BLOB/number legal);
     reasoning text from `reasoning`/`reasoning_content`, opaque payloads kept in raw (provider =
     the session's provider when known, else foreign).
   - legacy rotating chain: list the tip; read follows `parent_session_id` while the parent's
     `end_reason='compression'`.
3. **Hermes writer** (destination) through Hermes's own import, never by writing `state.db`:
   mukade writes the session as a Claude Code JSONL in its scratch folder with **every tool call in
   text form** (Hermes's import condenses calls to `[ran tool: X]` and drops results — verified on
   2026-11-12), runs `hermes sessions import --from claude <file>`, parses
   `Imported Claude Code session as <id>`, deletes the file. Losses stated: tools as text, cwd not
   kept (Hermes import stores none), model not set. Resume: `hermes --resume <id>`. `--dest-id` /
   `--dest-path` refused (Hermes allocates). Acceptance: hop a fixture into Hermes on this machine,
   read it back with the Hermes reader, compare text.
4. **DeepSeek reader** (`~/.dsh` or `DSH_HOME`): walk `sessions/{--*--,_no-cwd}/*/session.v4.jsonl.zstd`
   (or `.jsonl`); decode with `ruzstd` (new dependency, pure Rust) over frames found by a ported
   structural scanner, checksum compared, torn tail dropped and noted; header `version==4` else
   refusal; `cwd`/`id` from the header only; rebuild the surface (`surfaceOp` append/replace —
   replaced nodes → `Meta(OffBranch)`); only `user/message` with `source.kind=="user"` is a prompt,
   other kinds Meta; `tool/call` duplicates the assistant tool-call part → once; unknown
   non-ignorable event type → refusal; children: header `parentSession` + `origin:"subagent"` →
   `ParentKind::Spawn`, `isSeeded` → `Fork` with history before `session/end-seed` inherited.
5. **DeepSeek writer**: write `session.v4.jsonl.zstd` (ruzstd encoder; one frame for the header,
   one for the rest; content checksum if ruzstd supports it, else verified that dsh accepts it) in
   `sessions/--<projectKey(cwd)>--/<encodeSegment(id)>/` with id `session-<uuid>`; tool names mapped
   to dsh's (`read`, `edit`, `write`, `bash`/`powershell`?, …) by a `Destination::offered`/`translate`
   in its adapter, text form otherwise. Acceptance (live, free model): `dsh --profile headless --patch
   <openrouter patch> --session-id <id> "Reply with OK"` resumes it and answers.
   Resume command in the receipt: `dsh --profile headless --session-id <id> "<prompt>"`.
6. **Summaries** (`--compaction summary` from these sources): Hermes `hermes chat -q <prompt> -Q
   --ignore-user-config --ignore-rules -t none`? (verify a no-tool toolset); dsh `dsh headless`
   with the session's model — only if a run leaves no session behind or mukade deletes it; else
   refused with a clear message (as today for unsupported cases).
7. **Architecture**: one adapter file each in `src/harnesses/`, readers/writers/detect in their own
   files; `tests/architecture.rs` stays green; `harnesses::all` and `layouts` grow to 6.

## Tests
Fixtures: a synthetic Hermes `state.db` built by the test from the v0.23.1 `SCHEMA_SQL` (subset with
the needed columns) with a tool pair, a compaction summary, a subagent child, a branch; synthetic dsh
logs (plain and zstd, multi-frame, torn tail, replace op, subagent child). Live `#[ignore]` census
tests read the real stores above (like `tests/codex_code_mode_census.rs`). Round trips: hop a Claude
fixture into dsh and Hermes (Hermes live only, `#[ignore]`), read back.

## Docs
`docs/harnesses/hermes-agent.md`, `deepseek-harness.md` rewritten to the harness-page skeleton with
the verified facts (corrections list in research §4); ADR-0002 amended; usage, catalog reference,
tool tables (dsh tool names), architecture "Adding a harness" checked by actually doing it; next.md.
