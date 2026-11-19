# Hermes and DeepSeek Harness support

Status: design draft, awaiting approval. Date: 2026-10-31.
Decision record: [ADR-0002](../../adr/ADR-0002-hermes-and-deepseek-harness-read-first.md) (Proposed).

Items marked **[U]** are unverified: read in upstream documentation or source, not exercised
against a live install. Items marked **(my call)** are the author's choice, not the user's.

## 1. Intent and why

mukade moves a coding-agent session between harnesses. The user wants two more harnesses it can
work with: **Hermes Agent** (Nous Research) and **DeepSeek Harness** (`dsh`, the official
`deepseek-ai/deepseek-harness`). The motivation is the same as for the four existing harnesses:
continue a session in another harness as one of its own sessions, with no proxy and no loss that
is not reported.

## 2. Goals, non-goals, anti-goals

Goals

1. Read Hermes and DeepSeek Harness sessions into mukade's model, keeping every original record
   (`Entry::raw`), so either can be a hop source, `list`ed and `dump`ed.
2. Show both in `mukade harnesses`, `detect` and `scan`, and in the TUI.
3. Later: write DeepSeek Harness sessions so it can be a hop destination.

Non-goals (this design)

- Writing Hermes sessions. Not now, and not until upstream supports it (ADR-0002, decision 1).
- Writing DeepSeek Harness sessions in phase 1 (ADR-0002, decision 2).
- Deep Code (`lessweb/deepcode-cli`), DeepSeek-TUI, Reasonix and other DeepSeek-oriented agents.
  They are separate harnesses; none is covered here.
- Hermes gateway, cron, kanban and other non-CLI session sources beyond what a plain `sessions`
  row already carries.

Anti-goals (a failure even if it technically works)

- A Hermes session that Hermes itself never produced.
- A guess at the meaning of a store format version mukade was not verified against.
- A dependency that stops `mukade` being one self-contained executable.
- A source store changed by reading it (beyond the SQLite sidecars a read-only WAL open may
  create, see §6).

## 3. Decided by the user

| Decision | Source |
|---|---|
| "DeepSeek Code" means `https://github.com/deepseek-ai/deepseek-harness` | user's answer |
| Hermes: read-only first | user's answer ("Solo lettura prima") |
| DeepSeek Harness: reader first, writer later | user's answer ("Lettura prima, scrittura dopo") |

## 4. What the two harnesses are

### 4.1 Hermes Agent

Python, release `v0.22.5` (2026-10-22). Sources: Hermes
[session storage](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/developer-guide/session-storage.md),
[sessions guide](https://hermes-agent.nousresearch.com/docs/user-guide/sessions), CLI reference.

- **Store.** `state.db`, SQLite in WAL mode, at `get_hermes_home() / "state.db"`. Home resolves
  `HERMES_HOME`, else `~/.hermes` (macOS/Linux) or `%LOCALAPPDATA%/hermes` (Windows, native
  install supported). Named profiles live at `<root>/profiles/<name>/` with their own `state.db`.
- **Schema.** Version 31 (`schema_version` table). Tables that matter: `sessions` (id, source,
  model, model_config, system_prompt, `parent_session_id`, started_at, ended_at, title, cwd,
  token/cost counters, ...) and `messages` (autoincrement `id`, `session_id`, `role`, `content`,
  `tool_call_id`, `tool_calls` as a JSON string, `tool_name`, `timestamp` as epoch float,
  `reasoning`, `reasoning_content`, `reasoning_details`, `codex_reasoning_items`,
  `codex_message_items`, `active`, `compacted`, `message_uid`, `tool_call_uids`, `api_content`).
  The doc's schema is abridged; the full column list is in upstream `SCHEMA_SQL`.
- **Compaction.** In place: old rows get `active=0`, the retained context is inserted as `active=1`.
  A protected message can appear in both generations. Compression can also split a session into a
  child linked by `parent_session_id`.
- **Sources.** `sessions.source` is `cli`, `telegram`, ... and `oneshot`/`tool`/`kanban` runs that
  pickers hide.
- **Write path.** The profile's gateway is the only writer of canonical sessions. No command
  imports a session; `hermes import <zip>` restores a backup over the home. `hermes import-agent`
  imports setups from other agents (config, skills), not transcripts.
- **Export.** `hermes sessions export --format jsonl|md|html|trace` (`trace` is Claude Code JSONL).
- **Resume.** `hermes --resume <id|title|latest>`, `hermes -c [name]`; non-interactive
  `hermes chat --resume <id> --oneshot -q "…"`.
- **Tools.** Names not collected **[U]**.

### 4.2 DeepSeek Harness (`dsh`)

TypeScript monorepo, MIT, "Everything is a Plugin", published tag `dsh-v0.2.0-rc.2`
(pre-release). Sources: the repository's
[persistence note](https://github.com/deepseek-ai/deepseek-harness/blob/master/.agents/notes/archived/architecture/2026-06-14-session-persistence.md),
[directory layout](https://github.com/deepseek-ai/deepseek-harness/blob/master/.agents/notes/archived/architecture/2026-07-24-project-session-directories.md),
[zstd log](https://github.com/deepseek-ai/deepseek-harness/blob/master/.agents/notes/archived/architecture/2026-07-19-zstandard-jsonl-session-logs.md),
[format status](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/session-format-status.md),
[CLI help](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/cli-help.md).

- **Store.** `<root>/--<normalized-cwd>--/<encoded-session-id>/session.v4.jsonl.zstd` (raw root:
  `session.v4.jsonl`; sessions without a cwd use `_no-cwd`). The persistence `root` is required and
  has **no default**: it is set by the booted profile **[U]** (`dsh --dump-config` prints the
  composed tree, which should show it). A root has one encoding; the opposite suffix is refused.
- **Encoding.** Standard concatenation of independent, checksummed Zstandard frames: one frame for
  the header line, then one frame per durable append batch. A torn final frame is a crash tail.
- **Log.** Line 1 is the header `{"type":"session","version":4,"id","createdAt","cwd","isSeeded",
  "delegationDepth"}`. Every other line is an event `{type, seq, time, data, surfaceOp?,
  sourceEventSeqs?}`. Surface events (`system/message`, `developer/message`, `user/message`,
  `assistant/message`, `tool/result`) project to the LLM history; `surfaceOp` is `"append"` or
  `{op:"replace", startSeq, endSeq}` (compaction). Other families: `turn/*`, `step/*`, `tool/call`,
  `compaction/*`, `request/*`, `permission/*`, `subagent/*`, `session/*`.
- **Messages.** Content is an array of parts. `assistant/message` carries `reasoning` (plain text),
  `text` and `tool-call` parts (`arguments` is a JSON string), `usage`, the source
  (`provider: "deepseek-official"`, `model: "deepseek-v4-flash"`) and a timed provider `stream`.
- **Format churn.** Version 4 is finalized and released at rc.2, yet 19 persistence changes
  landed between 2026-09-11 and 2026-10-25, each with a published JSON Schema in
  `docs/persistence-changes/`. Upstream's compatibility rules let a change keep version 4 when it
  adds an optional event property, adds an ordinary event type, or adds a higher `data.version` to
  an ordinary event. The header `version` therefore catches only a bump, not drift inside v4. An
  event the reader does not know is either `ignorable: true` (may be skipped) or required (a
  reader MUST refuse to reconstruct the session).
- **Resume.** `dsh --profile headless --session-id <id> "task"` (unknown id is an error);
  interactive `dsh tui --resume <id>`.
- **Fixtures.** `snapshots/**/session.v4.jsonl` are real v4 logs with placeholders for ids and cwd.
- **Tools.** `bash` seen; the rest **[U]**.

## 5. Design

### 5.1 Capability on the harness definition

`HarnessId` gains two variants and the list of all harnesses becomes six. A harness definition
gains the answer to "can this be a destination": Hermes no, DeepSeek Harness not yet. A hop or
`write` toward one that cannot is refused with exit code 2 before any plan or write
(ADR-0002, INV-002). The check belongs in one place the CLI, the TUI and the library share, so
none of them can disagree. **(my call)** ids `hermes` and `deepseek_harness`.

### 5.2 Catalog and discovery

Two entries in `catalog/harnesses.json`. The schema learns two things the four existing harnesses
did not need: a store with no environment variable (DeepSeek Harness's root comes from its profile
or from `--root`) and a store the catalog cannot give a default for. Detection recognizes:

- Hermes: a `state.db` with `sessions`, `messages` and `schema_version`.
- DeepSeek Harness: `--*--/<id>/session.v*.jsonl[.zstd]`.

Binary names `hermes` and `dsh`, with the usual `--<name>-exe` flag and `MUKADE_*_EXE` variable.

DeepSeek Harness's root is not discovered by running `dsh` **(my call)**: `mukade harnesses` shows
it with its executable and version and a store of "not configured"; its sessions are reached with
`--root <dir>` or found by `scan`. A `dsh --dump-config` probe is a later option, not part of this
design.

### 5.3 Readers

Both readers keep every original record in `Entry::raw`. A record that is not the live
conversation is kept as `Kind::Meta` (`MetaKind::OffBranch` for a generation a compaction
replaced, `MetaKind::Internal` for the rest, `src/model/kind.rs:39`) and reported in the manifest:
never dropped, never written as conversation **(my call)**.

**Hermes (my call):** open `state.db` read-only through the bundled `rusqlite`, check
`schema_version == 31`, and read messages in native order (`messages.id` within a session). Rows
with `active=1` are the conversation; rows with `active=0` are the generation an in-place
compaction archived and become `Meta`. Upstream documents no stored summary row **[U]**, so a
Hermes session yields no `Kind::Compaction` and `--compaction portable` from it is refused with
that reason. `reasoning` and `reasoning_content` become `Reasoning` text. `reasoning_details` and
`codex_reasoning_items` may hold signed or encrypted payloads on some routes **[U]**: phase 2 keeps
them in `raw`, reports them as a loss and never maps them to an opaque payload until a real
session shows what they hold. The export command is not used at runtime (it needs the binary and
is a second representation); it is the cross-check in verification (§7).

*Which sessions `list` shows (my call):* sessions with source `cli`; `oneshot`, `tool`, `kanban`
and gateway sources sit behind a flag, as Hermes's own pickers hide them. A compression child
(`parent_session_id`) is the live continuation, so the **tip** is the listed session and the parent
chain is reachable through its lineage, as a Pi fork is; a hop from a mid-chain session says in the
manifest that later segments exist. Each `state.db` is one installation: a named profile
(`<root>/profiles/<name>/state.db`) is a separate installation that `scan` finds, so
`locate_installation`'s "exactly one" rule needs no change.

**DeepSeek Harness:** read the header, check `version == 4`, decode frames, parse events in `seq`
order, build the conversation from surface events honoring `surfaceOp` and `sourceEventSeqs`, map
`compaction/*` events to `Kind::Compaction`, and keep each original line as `Entry::raw`. Events
cut by `surfaceOp: replace` become `Meta` (`OffBranch`). Drift inside v4 follows upstream's own
rules **(my call, derived from them)**: an unknown optional property is kept in `raw`; an unknown
event type with `ignorable: true` is kept in `raw` and reported; an unknown event type without it,
or a conversation-bearing event with a `data.version` higher than the reader knows, is refused
naming the type and version, as upstream requires of its own readers. The JSON Schemas published
upstream build fixtures and check the phase-3 writer; they are not applied at read time. Reading
needs a Zstandard decoder: **(my call)** the `ruzstd` crate (pure Rust, keeps the executable
self-contained). A torn tail is read up to the last complete frame and reported, never repaired in
place. Sessions with `isSeeded` or `delegationDepth > 0` are listed with those facts in their
metadata; no parent link is invented, because the header names none **(my call)**.

### 5.4 Reasoning and protocol

DeepSeek Harness records reasoning as plain text, with no signature or encrypted payload.
`Protocol::from_provider` maps an unknown provider such as `deepseek-official` to
`Protocol::Other(..)`. Under the default `ReasoningPolicy::DropForeign` (`src/policy.rs:16`) a
reasoning block with no payload is dropped as `NoPayload`, so reasoning from these harnesses is
**dropped by default** and written as labelled text only with `--reasoning as_text`; the manifest
says which happened. Hermes may carry payloads in `reasoning_details` / `codex_reasoning_items`;
phase 2 does not map them (§5.3). Whether `deepseek-official` and Hermes's DeepSeek routes count as
one protocol is left to the builder.

### 5.5 Tools

Both harnesses' tool names go into `docs/tool-conversion-tables.md` once collected from fixtures;
a tool with no counterpart travels as text with a manifest entry, as today. Hermes and DeepSeek
Harness tool calls are read-only inputs in phase 1: only reading needs the table.

### 5.6 CLI, TUI, docs

- `list`, `dump`, `detect`, `scan`, `harnesses` and a `hop` source work for both.
- `--compaction summary` asks the source's own model for a summary and needs a non-interactive run
  that saves nothing. For both new harnesses **(my call)** it is refused with a reason in phase 1.
- The TUI lists both and offers neither Hermes nor (until phase 3) DeepSeek Harness as a hop
  destination.
- New pages `docs/harnesses/hermes.md` and `docs/harnesses/deepseek-harness.md`; the harness
  catalog reference, spec, usage guide and README replace "four" with six and mark Hermes read-only.

## 6. Constraints

- Opening a WAL database read-only can still create `-wal`/`-shm` sidecars or touch shared-memory
  read marks (Hermes documents this). `state.db` itself must stay byte-identical (ADR-0002,
  INV-001). If the Hermes directory is not writable, how to read is open (§8).
- Hermes may be running while mukade reads; the gateway is a live writer.
- DeepSeek Harness's format moves between releases and also inside v4 (§4.2). The header pin makes
  a version bump a refusal; the event rules in §5.3 cover additive drift inside v4.
- Windows first: both run natively there, and this machine has neither installed.

## 7. Phasing and verification

1. **Foundation.** Capability on the harness definition, six `HarnessId`s, catalog schema, detect.
2. **Readers.** Hermes (read-only SQLite), then DeepSeek Harness (zstd). Each with a page, tool
   table rows and tests.
3. **DeepSeek Harness writer.** Only after a session written by mukade is resumed by a live `dsh`
   and recorded under `docs/verification/` (ADR-0002, INV-004). The published v4 JSON Schemas
   validate what it writes.

The implementation plan that follows this design covers phases 1 and 2 **(my call)**. Phase 3 gets
its own design and plan after phase 2 ships. Whether a harness can be a destination is data on its
definition (§5.1); the phase-3 change that flips DeepSeek Harness to writable is the one the live
verification gates (ADR-0002, INV-004), and until then the writer is not reachable from the CLI or
the TUI.

Fixtures follow the repo's rule: recorded real sessions with synthetic content. The DeepSeek Harness
`snapshots/**/session.v4.jsonl` serve as input, taken at an upstream commit recorded beside them;
the released baseline is tag `dsh-v0.2.0-rc.2` **(my call)**. Cross-checks: a Hermes session's mukade
`dump` against `hermes sessions export --format jsonl` of the same session.

Rough effort (author's, excluding installs and live runs): foundation about 2 hours, Hermes reader
2–3 hours, DeepSeek Harness reader about 3 hours, DeepSeek Harness writer 4–5 hours.

## 8. Open points

- **Hermes real session: waiting on the user.** Whether the Hermes reader is built against a
  hand-made schema-31 database (labelled unverified) or waits for a real session; asked at
  hand-over.
- Hermes tool names and the full column list (upstream `SCHEMA_SQL`) **[U]**; what `hermes -c` does
  with a compression chain **[U]**.
- Hermes read when the directory is read-only or the gateway holds the WAL: `mode=ro` versus
  `immutable=1` and what each risks **[U]**.
- DeepSeek Harness: the default root of the shipped `tui` and `headless` profiles, and the tools
  each exposes **[U]**.
- Whether `ruzstd`'s encoder emits frame checksums, which phase 3 needs **[U]**.

## 9. Left to the builder

File layout of the new modules, the exact shape of the capability type, SQL details of reading
Hermes, how the Zstandard decoder is wired, test names, and the order inside each phase.
