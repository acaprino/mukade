<!--
Provenance: pasted by the user on 2026-10-27 (session 8288ab04) as the starting reference for the
session interchange library, extracted byte-for-byte from the session log. Restructured on
2026-11-11 for reading (tables, short sentences); every claim, evidence label, version, path and
source marker was kept. The verbatim paste is in that session log and in this file's git history.
Status: secondary research report. Its source markers are opaque references of the tool that
produced it and do not resolve outside that tool. Claims are labelled [D]/[C]/[I]/[Gap] by its
author; none was re-run here. Facts re-run on this machine live in docs/session-interchange-spec.md
and docs/harnesses/ (labels [V]) and supersede this file where they disagree.
-->

# Harness session-persistence reference (secondary research, 2026-10-27)

How 24 coding-agent harnesses store, resume and import sessions, as of 27 October 2026. Written for
authors of **loss-minimizing session converters**, not for copying UI transcripts. A secondary
report: none of its claims was re-run here.

**Use it like this**

1. **Claude Code, Codex, OpenCode, Pi, Hermes Agent or DeepSeek Harness** (the six mukade reads and
   writes): read the harness page first ([Claude Code](../harnesses/claude-code.md),
   [Codex](../harnesses/codex.md), [OpenCode](../harnesses/opencode.md), [Pi](../harnesses/pi.md),
   [Hermes Agent](../harnesses/hermes-agent.md), [DeepSeek Harness](../harnesses/deepseek-harness.md)) and the
   [spec](../session-interchange-spec.md). They hold facts run on this machine and win over this
   report. [What mukade has settled since](#what-mukade-has-settled-since) lists where they go further.
2. **Any other harness**: start at the [Comparative matrix](#comparative-matrix), then read its
   dossier.
3. **Designing converter behaviour**: read
   [Conversion architecture](#conversion-architecture-and-cross-harness-failure-modes) and
   [Verification gaps](#verification-gaps-contradictions-and-unsupported-assumptions-to-avoid).

**Why the rendered chat is not enough.** In several harnesses the durable object is an append-only
event log, a database, a parent-linked tree or a cloud thread. A converter that copies only visible
user/assistant text silently loses tool-call identity, compaction state, branch ancestry, subagent
state, attachments, signed reasoning and incomplete-turn semantics.

**The main negative result.** For several proprietary harnesses the official product exposes
resume/history but **no first-party canonical local transcript schema**. Scraping their internal
SQLite, IndexedDB, Electron state, VS Code storage or browser caches would not meet the report's
"verified converter" standard.

## Evidence labels and source markers

| Label | Meaning |
|---|---|
| **[D]** Documented | stated in first-party documentation, release notes, or an official issue/PR |
| **[C]** Code | read directly from first-party source code at the cited commit/tag |
| **[I]** Inference | a conclusion from first-party behaviour/code, not an explicit compatibility guarantee |
| **[Gap]** | not publicly specified strongly enough to use as a converter contract |

**Src column.** `c:` is the report tool's `cite` marker and `f:` its `filecite` marker, with the
`turn` prefix dropped: `c:13search30+0search12` is one marker naming two results,
`f:36file0:L1-L2` is file result `36file0`, lines L1-L2 of the tool's response (see
[Citations](#citations)). They resolve only inside that tool.

## What mukade has settled since

Where the report says [Gap] or "not documented" for one of the supported harnesses, mukade has
since observed more. The harness page is the authority; this table only points to it.

| Harness | Report (item) | Observed by mukade | Where |
|---|---|---|---|
| Claude Code | 1: `~/.claude/projects/<project-key>/`; subagent `.meta.json` | Root is `CLAUDE_CONFIG_DIR` (default `~/.claude`), file `projects/<project>/<session-id>.jsonl`. Subagent transcripts sit in `<session>/subagents/`, same format | [claude-code.md](../harnesses/claude-code.md), Session store |
| Claude Code | 5 [Gap]: no turn predicate | An interrupted turn ends with a user row `[Request interrupted by user]`; on resume Claude Code inserts a synthetic `No response requested.` | claude-code.md, section 8, Known limits |
| Claude Code | 7 [Gap]: reasoning | `thinking` keeps its `signature`, `redacted_thinking` its `data`; mukade carries them only from the same protocol, unmodified | claude-code.md, section 8 |
| Claude Code | 9: no documented import | A session file written outside Claude Code resumes by id; no index file is needed | claude-code.md, Session store |
| Codex | 1: `state_*.sqlite` | The file is `state_5.sqlite`; its `threads.rollout_path` wins over the file on disk | [codex.md](../harnesses/codex.md), Session store |
| Codex | 9: hand-written rollout + index row not guaranteed | A rollout alone resumes by id; Codex refuses one without `cli_version` | codex.md, Session store |
| Codex | 8: metadata names no parent | A subagent's rollout names the spawning thread in `session_meta.session_id`; `state_5.sqlite` `thread_spawn_edges` links child to parent. mukade reads both (`src/reader/codex.rs` `Builder::finish()`, `src/reader/codex_state.rs` `parent_links()`) | codex.md, Session store |
| OpenCode | 1: `opencode db path` | The command run on this machine is `opencode debug paths db`. A build on another channel uses `opencode-<channel>.db` unless `OPENCODE_DISABLE_CHANNEL_DB` | [opencode.md](../harnesses/opencode.md), Session store |
| OpenCode | 9: official import/export | mukade reads through `session export` and writes through `session import`, never the database | opencode.md, sections 1 and 8 |
| pi | 1 and 10: `--session-dir`; no version pinned | `--session-dir` beats `PI_CODING_AGENT_SESSION_DIR`, which beats `sessionDir`; root `PI_CODING_AGENT_DIR` (default `~/.pi/agent`). mukade-written sessions verified on Pi 1.2.0 and 1.4.0 | [pi.md](../harnesses/pi.md), Session store; [Pi verification](../verification/pi-session-and-reasoning-verification.md) |
| Hermes Agent | 1 and 9: `state.db`, `hermes import <zip>` | `state.db` schema 32 (v0.22.5) and 33 (v0.23.1); the way in is `hermes sessions import --from claude <file>`, which keeps user and assistant text only | [hermes-agent.md](../harnesses/hermes-agent.md), Session store; [verification](../verification/2026-11-12-hermes-writer-live.md) |
| DeepSeek Harness | format and root | Format 4, `<DSH_HOME>/sessions/--<project>--/<id>/session.v4.jsonl.zstd`; a log written by mukade resumes in `dsh` | [deepseek-harness.md](../harnesses/deepseek-harness.md), Session store; [verification](../verification/2026-11-12-dsh-writer-live.md) |
| pi | 5 [Gap]: no turn predicate | Assistant `stopReason` is `toolUse`, `aborted`, `error` or another (completed); mukade reads `aborted` as an interrupted turn | pi.md, section 8 |

## Comparative matrix

"Opaque" means: preserve the provider/session payload; there is no documented cross-harness replay
guarantee. Rows say "unpublished" rather than repeat commonly circulated reverse-engineered paths:
the report keeps to a **primary-source-only** standard.

**Where and how each harness stores sessions**

| Harness | Canonical storage | Format | Threading / branching |
|---|---|---|---|
| Claude Code | `~/.claude/projects/...` | JSONL, internal schema | checkpoints, rewind; subagents |
| Codex | `CODEX_HOME`, rollouts + SQLite + `session_index.jsonl` | JSONL + SQLite | thread ID; resume; subagent/thread state |
| OpenCode 2.x | `opencode.db` | SQLite + JSON columns/blobs | `parent_id`, `fork_session_id`, `fork_boundary`, `revert` |
| Copilot CLI | `~/.copilot/session-state/...` | `events.jsonl` + artifacts | session identity; no stable public branch schema |
| Cursor | undocumented canonical local path | SDK/protobuf + internal persistence | session IDs/checkpoints |
| Gemini CLI | project `chats/session-*` | append JSONL with `$patch/$set/$rewindTo` | logical history mutations; main/subagent |
| Qwen Code | active/archive transcripts | parent-linked JSONL | `uuid/parentUuid`, fork, rewind, branch checkpoints |
| Kimi Code | `$KIMI_CODE_HOME/sessions/...` | `state.json` + per-agent `wire.jsonl` + blobs | fork; per-subagent logs |
| Mistral Vibe | `~/.vibe/logs/session`, configurable | Unified Session Store; legacy JSON/JSONL | parent session/subagents |
| Kilo Code | CLI/hosted session services | current internal/API form | fork at `cloneUpToMessageId` |
| Cline | CLI `~/.cline/data`; VS Code globalStorage; cloud separate | task JSON files + index | task-level; cloud/local split |
| Roo Code | VS Code globalStorage | task/API-history files | task/delegation stack |
| Amp | service Thread | JSON export / Markdown projection | service thread |
| pi | configurable session files | JSONL tree | parent-linked tree/branching |
| Oh My Pi | `~/.omp/agent/sessions` | pi-family session form + extensions | session/tree semantics |
| Grok CLI | `~/.grok/sessions` | ACP `updates.jsonl` | session ID |
| Antigravity | backend; local ID cache only | service objects | rewind/undo/fork |
| Hermes | JSON catalogs + SQLite + session export | internal DB/JSON; export JSONL/MD | session/channel/delegation |
| OpenHands | pluggable persistence/object store | conversation/event state | conversation/runtime |
| Devin | cloud service | service session/steps | `/fork [step]`, `/revert`, `/btw` |
| MastraCode | configurable Harness storage | storage-dependent event/state | session/worktree/subagents |
| Factory Droid | service/cloud session | unpublished | service session |
| Windsurf Cascade | app/service, unpublished transcript store | unpublished | session-level |
| Continue | `~/.continue/sessions` | JSON + `sessions.json` catalog | `/fork` |

**Turns, compaction, reasoning and interchange**

| Harness | Strong turn-end signal | Compaction | Reasoning | Official interchange | Src |
|---|---|---|---|---|---|
| Claude Code | not publicly standardized at file level | yes; durable resume implications | opaque; transcript contract unpublished | resume/continue; no arbitrary JSONL import | `c:0search0+0search12` |
| Codex | run/turn lifecycle events | persisted summaries/compressed histories | opaque protocol items | resume/app-server APIs; no arbitrary rollout import | `f:39file0:L1-L6` |
| OpenCode 2.x | lifecycle/session status + message sequence | explicit/background compaction | provider-specific parts + token counter | official import/export | `f:10file0:L1-L6` |
| Copilot CLI | session-end lifecycle, not EOF | intelligent compaction | opaque | resume/share; no raw event import | `f:11file0:L1-L18` |
| Cursor | SDK/runtime events | not file-documented | opaque | `agent resume`, continue; no generic import | `c:3search12+3search25` |
| Gemini CLI | tool `status` + runtime invariants | summary + bounded/delta history | native Parts + separate thought summaries | resume/rewind; no generic import contract | `f:15file0:L1-L13` |
| Qwen Code | `turn_attempt` / `turn_result` | `chat_compression` system record | provider-native structured content | resume/load ACP; Markdown/HTML export | `f:20file0:L1-L2` |
| Kimi Code | event-stream semantics | explicit `/compact` + auto | opaque wire/provider data | resume, ZIP/MD export, Claude/Codex import | `f:22file0:L1-L13` |
| Mistral Vibe | must support mid-tool saved state | yes | Thought/provider state; no portable signature contract | resume/continue | `c:6search24+16search1` |
| Kilo Code | not publicly file-standardized | yes | persisted provider reasoning fixes | session API; no generic import verified | `c:17search0` |
| Cline | runtime state; reasoning-only failure is incomplete | yes | provider-specific | history reconstruction; no generic import | `c:6search14` |
| Roo Code | not standardized | historical context mgmt | opaque | native history only | |
| Amp | service state | service-internal | opaque | JSON/Markdown export; no generic import | `c:7search4` |
| pi | format-dependent | compaction/branch summaries | opaque provider content | resume/continue | `c:7search5+7search17` |
| Oh My Pi | format-dependent | pi-family | opaque | Claude/Codex import | `c:7search18` |
| Grok CLI | `stopReason` for headless; ACP lifecycle for full replay | not verified | optional `thought`; not portable | resume/export, no general import | `c:10search4+10search25` |
| Antigravity | backend state | backend | opaque | resume and vendor migration | `c:11search7+11search16` |
| Hermes | session runtime | explicit compression | opaque | resume/search/export/import backup | `f:31file0:L1-L12` |
| OpenHands | execution/sandbox status | condenser | opaque | API recovery; no universal file import | `c:12search1+12search8` |
| Devin | service step state | internal | opaque | resume/export; no arbitrary import | `c:12search2` |
| MastraCode | Harness events | observational/context memory | opaque | programmatic Harness API | |
| Factory Droid | runtime state | unpublished | opaque | resume/service APIs | |
| Windsurf Cascade | service/app state | unpublished | opaque | GUI history/export only | |
| Continue | history/runtime, no single catalog marker | `/compact` | unknown nested provider state | resume/fork; no generic import | `c:19view3+15search1` |

## Version baseline

The latest **stable** release the report could verify on 27 October 2026, with two exceptions:

- OpenCode is pinned to the requested 2.0.x and 2.1.x lines.
- A vendor with a rolling release and no authoritative public version/tag gets none; none is
  invented.

| Harness | Verified baseline | Date / commit status | Src |
|---|---|---|---|
| Claude Code | `v2.1.311` | 2026-10-26; commit `08c6a4f` in the official release record | `c:1view0` |
| OpenAI Codex | `rust-v0.164.0` / `0.164.0` | published 2026-10-23; stable GitHub release | `f:36file0:L1-L2` |
| OpenCode 2.0.x | `v2.0.32` | 2026-10-16, latest 2.0.x in the V2 changelog | `f:3file0:L1-L2` |
| OpenCode 2.1.x | `v2.1.4` | 2026-10-23, latest 2.1.x | `f:3file0:L1-L2` |
| OpenCode current context | `v2.2.0` | 2026-10-27; relevant because it immediately evolves the same DB/session format | `f:3file0:L1-L2` |
| GitHub Copilot CLI | `1.0.104`, reported by the current release feed | same-day discrepancy: cached GitHub release surfaces showed `1.0.103`/`1.0.104-*`; a release-feed race, not a format guarantee | `c:14search1+19view2` |
| Cursor | rolling CLI/Desktop | no stable public transcript-format tag in the official material; persistent CLI sessions introduced in the 2026-08-26 CLI changelog | `c:3search4` |
| Gemini CLI | `v0.65.0` | published 2026-10-20 | `f:40file0:L1-L2` |
| Qwen Code | CLI `0.26.1` | the official SDK release of 2026-10-26 bundles CLI `0.26.1`; source snapshot `57f4bd142467…` | `f:0file0:L1-L2` |
| Kimi Code | `@moonshot-ai/kimi-code@2.3.3` | published 2026-10-26 | `f:41file0:L1-L2` |
| Mistral Vibe | `v3.0.3` | 2026-10-26; source examined around commit `7bb48f213a…` | `c:16search1` |
| Kilo Code | `v7.8.18` | 2026-10-26 in the official release stream | `c:17search0` |
| Cline | `v4.1.26` | published 2026-10-26 | `f:25file0:L1-L2` |
| Roo Code | `v3.54.0` | final release, 2026-05-15, commit `27001b2b5aa47b65e8a6ba1914e0f4216be0ebb0`; repository archived | `f:29file0:L1-L2` |
| Amp | rolling service/CLI | no public semver transcript contract; Threads are the documented persistence abstraction | `c:7search4` |
| pi | rolling/open-source line | session format publicly documented; no authoritative latest-release tag obtained, so no version claimed | `c:7search5+7search17` |
| Oh My Pi | rolling | current source/docs verified; latest tag not verified enough to claim | |
| Grok CLI / Grok Build | rolling | the official repository showed no normal GitHub stable-release series | `c:10search0` |
| Antigravity | app `2.22.0`; CLI `1.2.12` | latest first-party versions in official changelogs; not a claim that no later private rollout existed on 2026-10-27 | `c:11search23` |
| Hermes Agent | `v0.22.5` | release date 2026-10-22; GitHub publication 2026-10-23; commit `bf0203a794a…` | `f:30file0:L1-L2` |
| OpenHands | `v1.33.0` | published 2026-10-23, target commit `b968abef4779…` | `f:34file0:L1-L2` |
| Devin | rolling service/CLI | no public stable transcript-schema tag | |
| MastraCode | rolling package/harness | latest package version not verified enough to pin | |
| Factory Droid | rolling service/CLI | no public durable-transcript schema version | |
| Windsurf / Cascade | rolling Devin Desktop/Windsurf product | no first-party transcript-file version contract | `c:14search14` |
| Continue | rolling OSS client line | source-level session format verified; latest package tag not pinned | `c:19view3` |

**OpenCode release channels disagree.** GitHub's `/releases/latest` returned the legacy `v1.19.4`
channel; the repository's "OpenCode V2" changelog lists `v2.0.x`, `v2.1.x` and `v2.2.0`. For V2,
the V2 changelog and V2 source are the authority. Src: `f:1file0:L1-L2` `f:3file0:L1-L2`.

## Local and open-format harness dossiers

Each dossier answers the same ten questions: 1 Storage, 2 Format, 3 Conversation model,
4 Structure, 5 Turns, 6 Compaction, 7 Reasoning, 8 Metadata, 9 Resume/import/export, 10 Stability.

### Claude Code — `v2.1.311`

Verified on this machine: [claude-code.md](../harnesses/claude-code.md), sections 5 and 8. They
supersede this dossier.

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[D]** | Transcripts are project-scoped under `~/.claude/projects/<project-key>/`; official issue material identifies them as `.jsonl`. Subagent metadata sits beneath the session directory: `~/.claude/projects/{project}/{sessionId}/subagents/agent-{agentId}.meta.json`. Windows: normally `%USERPROFILE%\.claude\...`, an OS-path inference, not a documented Anthropic contract | `c:13search30+0search15` |
| 2 Format **[D/Gap]** | JSONL, but Anthropic publishes **no** stable, versioned transcript schema comparable to an SDK wire schema; not every row type/field is an interoperability contract. Parse known rows permissively; keep unknown objects verbatim, do not reject them. No first-party minimal real fixture (prompt → tool call → result → answer) was located; none is fabricated | `c:13search30` |
| 3 Conversation model **[D/I]** | User/assistant content, tool interactions, checkpoints and subagents. The on-disk form of each content block, image/file attachment, tool input and reasoning block is not a public compatibility surface. Keep tool names as opaque identifiers with their original input object; do not map them from UI labels | `c:0search15+0search12` |
| 4 Structure **[D]** | Checkpoints are first-class: one saved before each user prompt, roughly the 100 most recent kept, `/rewind` returns to them, and they survive resume. `/clear` differs: it resets the pointer used to continue the previous session. Subagents have their own session-associated metadata | `c:0search12+0search15` |
| 5 Turns **[D/Gap]** | The runtime tells ordinary completion, cancellation/interruption and rewound history apart, but no public schema defines a row-level `turn_complete` predicate for third-party writers. `v2.1.311` fixes transcript ordering and "taken back"/rewound messages: do not infer complete turns from alternating `user`/`assistant` | `c:1view0` |
| 6 Compaction **[D]** | Long histories are compacted, including contexts above 200K; release notes expose `DISABLE_COMPACT`. Checkpoints and compaction coexist, so the visible transcript and the model-facing resumed prefix can differ. Keep the original rows and any summary/compaction artifact recognized | `c:1view0` |
| 7 Reasoning **[Gap]** | The provider can use signed/opaque Anthropic thinking state, but the local transcript is not documented as a reasoning-replay interchange format. Do not synthesize, modify, concatenate or strip opaque reasoning/signature fields unless the target supports their semantics | |
| 8 Metadata **[D]** | Session identity and project association are required for resume/checkpoint/subagents. Field names for `cwd`, git branch, model, client version, title, token use and cost are not guaranteed by a public schema: treat them as version-observed | `c:0search15+13search30` |
| 9 Resume/import/export **[D]** | `claude --resume` resumes a selected/specified session; `claude --continue` the most recent relevant one. Desktop opens a local session with `claude --desktop --resume <id>` and supports continuation. No documented mechanism accepts arbitrary third-party JSONL as a validated import | `c:0search0+0search21` |
| 10 Stability **[D/I]** | An **internal persistence format**, not a versioned external protocol. `v2.1.311` changed transcript/rewind/compaction behaviour: raw field-copying is safer than aggressive normalization | `c:1view0` |

### OpenAI Codex — CLI, IDE extension and desktop/app clients; stable `0.164.0`

Verified on this machine: [codex.md](../harnesses/codex.md), sections 5 and 8. They supersede this
dossier.

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[C/D]** | State root `CODEX_HOME`, normally `~/.codex`. Rollouts live in the session store; `session_index.jsonl` is an append-only name catalog. Current installs also use a SQLite state database (`state_*.sqlite` in observed official code/issues); the rollout JSONL stays significant for thread replay. The session-index code defines `SESSION_INDEX_FILE = "session_index.jsonl"` and an adjacent lock file | `c:0search2` `f:39file0:L1-L6` |
| 2 Format **[C]** | Name catalog rows: `{ id: ThreadId, thread_name: String, updated_at: String }`. Append-only; reverse scans make the newest usable name win. Rollouts are JSONL protocol records; the newer thread/state layer also projects metadata into SQLite | `f:39file0:L1-L6` |
| 3 Conversation model **[C/D]** | Rollouts hold protocol items, not rendered text: user input, assistant/model output, function/tool calls and outputs, lifecycle events, thread/session metadata. The app-server exposes thread-level APIs, so CLI, IDE and desktop/app clients share thread semantics despite different UIs | `c:0search14` |
| 4 Structure **[C/D]** | A thread has a durable `ThreadId`; the thread store resolves persisted rollout paths and resumes them. Name resolution is many-to-one over historical index entries and checks that a candidate rollout exists and loads. App-server resume has had bugs that created/forked a new thread: compare `id`, never title | `f:39file0:L1-L6` `c:0search8` |
| 5 Turns **[D]** | Running/recoverable turns differ from completed ones; recent releases added running-turn recovery and fixes for resumed compressed histories. Keep unfinished output/events; EOF is not "completed" | `c:2view0` |
| 6 Compaction **[D]** | Compressed/compacted state is persisted and resumable. Recent releases changed partial-summary handling and saved thread settings around compaction. A summary is not just a visual assistant message | `c:2view0` |
| 7 Reasoning **[I/Gap]** | The Responses-style event stream can hold reasoning items; local persistence is the source of truth for opaque model items. Plain text extracted from reasoning is not enough to replay a signed/encrypted provider item. No documented guarantee that third-party reasoning blobs are accepted on resume | |
| 8 Metadata **[C/D]** | Persisted/resolved at minimum: thread ID, session source, rollout path, optional model provider. Code filters session candidates by `SessionSource` and optional `model_provider`. Names live outside the rollout, in `session_index.jsonl` | `f:39file0:L1-L6` |
| 9 Resume/import/export **[D/C]** | CLI resume takes thread/session identity; current versions also resume by name. The app-server exposes thread resume/list for GUI clients. No public guarantee that a hand-authored rollout JSONL plus an index row is a supported import: rollout metadata, index and SQLite/thread store may all be involved | `c:0search5+0search14` |
| 10 Stability **[D]** | `0.164.0` was stable as of 23 October 2026; the source tree had moved past it by 27 October. Recent releases changed compaction, running-turn recovery, saved thread settings and subagent resume: dispatch converter code by version | `f:36file0:L1-L2` |

### OpenCode — requested V2 lines `2.0.32` and `2.1.4`

Verified on this machine: [opencode.md](../harnesses/opencode.md), sections 5 and 8. They supersede
this dossier.

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[D/C]** | V2 uses SQLite. Usual release DB: `~/.local/share/opencode/opencode.db`; `XDG_DATA_HOME`, release-channel naming and `OPENCODE_DB` move or rename it. `opencode db path` reports the active DB. Local development uses a channel-specific/local DB instead of sharing production state | `f:7file3:L35-L44` `f:7file6:L73-L82` `f:7file9:L106-L115` |
| 1 Tables **[C]** | Relevant to converters: `session_v2`, `session_message`, `session_inbox`, `instruction_entry`, `instruction_blob`, `instruction_state`, `blob`, `blob_ref`. `session_v2` is indexed on project, workspace, parent and suspended/running-state columns; `session_message` has a unique `(session_id, seq)` ordering index plus type/time indexes | `f:10file0:L1-L6` |
| 2 Format **[C]** | `session_v2`: `id`, `project_id`, `workspace_id?`, `parent_id?`, `fork_session_id?`, `fork_boundary?` (JSON), `slug`, `directory`, `path?`, `title?`, `version`, `share_url?`, summary counters/diffs, `metadata`, `cost`, token counters, `revert`, `permission`, `agent`, `model`, timestamps, idle/compaction/archive/suspension state, `resume_attempts`. `session_message`: `id`, `session_id`, `type`, `seq`, timestamps, JSON `data`. `session_inbox`: typed JSON `payload` plus delivery/enqueue sequence. Far safer than reverse-engineering display JSON: preserve `seq`, `type`, `data` and session-level state exactly | `f:10file0:L1-L6` |
| 3 Conversation model **[C]** | Messages are a typed union serialized into `session_message.data`: tool calls, model content, user content and provider-specific parts. Durable media/blob tables mean a content reference can point outside the JSON column. `v2.2.0` added experimental content-addressed session-media blob storage: attachment assumptions from 2.0.x are unsafe for later conversion | `f:10file0:L1-L6` `f:3file0:L1-L2` |
| 4 Structure **[C]** | Threading is explicit: `parent_id`, `fork_session_id`, JSON `fork_boundary`. Reversion is separate JSON state (`revert`). Message order is `seq`, not timestamp; the inbox has its own `enqueued_seq`. Sorting by timestamp can corrupt canonical order | `f:10file0:L1-L6` |
| 5 Turns **[C/I]** | Session-level `time_idle`, `idle_outcome`, `time_compacting`, `time_suspended` and `resume_attempts` distinguish lifecycle states; message `type`/`seq` gives event order. No safe rule "last assistant row = complete turn": V2 supports running sessions and restoration while SDK clients reconnect | `f:10file0:L1-L6` `f:3file0:L1-L2` |
| 6 Compaction **[D/C]** | `2.1.4` introduced optional `experimental.background_compaction`; `2.2.0` starts model history from the latest completed compaction. Compaction is a durable history boundary, not only a UI summary. Session instructions can survive compaction/revert independently of ordinary messages | `f:3file0:L1-L2` |
| 7 Reasoning **[C/D]** | The session aggregates `tokens_reasoning`; provider-specific message content carries the model data. OpenCode has had provider-specific fixes for reasoning/signature behaviour: preserve opaque content, do not convert reasoning into a text message | `f:10file0:L1-L6` |
| 8 Metadata **[C]** | One of the richest verified schemas in the survey. First-class persisted fields: cwd/project (`directory`, `project_id`, optional workspace), model `{ id, providerID, variant? }`, version, title, share URL, cost, five token counters, file-diff summary, timestamps, permissions, agent identity | `f:10file0:L1-L6` |
| 9 Resume/import/export **[D]** | V2 exposes official session import/export (official CLI docs cover export/import of session data), so raw DB mutation is not required. Third-party DB writing is still unsafe: foreign keys, sequence uniqueness, migration state and auxiliary tables must agree | `c:5search1` |
| 10 Stability **[D/C]** | `2.0.32` is the last verified 2.0.x, `2.1.4` the last 2.1.x. V2 source has a dedicated V1 migration path (`packages/core/src/database/v1-migration.bun.ts`). `2.2.0` changed media blobs, instruction persistence, compaction-start logic and cost reporting: the V2 schema is actively evolving | `f:9file8:L143-L154` `f:3file0:L1-L2` |

### GitHub Copilot CLI

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[D]** | New sessions: `~/.copilot/session-state`. Legacy: `~/.copilot/history-session-state`, migrated when resumed. Per session, `events.jsonl` is the durable event log, beside workspace artifacts (plans, checkpoints, tracked-file state) | `f:11file0:L1-L18` `c:19view0` |
| 2 Format **[D/Gap]** | `events.jsonl` is JSONL; the new format was introduced to decouple persistence from timeline rendering. Session-usage metrics (requests, tokens, code-change metrics) are appended after a session ends. No stable complete event-union schema for third-party writers: treat it as an internal event stream | `f:11file0:L1-L18` |
| 3 Conversation model **[D]** | Tool/event storage is richer than rendered chat; workspace artifacts live outside the chat event stream. Keep event payloads and file/checkpoint artifacts as separate namespaces | |
| 4 Structure **[D]** | The old → new format migration happens on resume. No robust public branch/fork file contract verified: do not derive branches from duplicated message prefixes | |
| 5 Turns **[D/I]** | A session-end event/closure differs from a partial event log: usage metrics are persisted after termination. EOF alone does not make a session complete | `f:11file0:L7-L18` |
| 6 Compaction **[D]** | Long/"infinite" sessions through intelligent compaction/summarization: resumed context can differ from the whole visible event history | `c:14search20` |
| 7 Reasoning **[Gap]** | No public stable on-disk reasoning-signature contract. Preserve unknown model events losslessly | |
| 8 Metadata **[D]** | Local session state is supplemented by usage metrics and can be synchronized to the user's GitHub account by default: canonical state can span local persistence and account-backed sync | `c:19view1` |
| 9 Resume/import/export **[D]** | `--resume` takes a session identifier/prefix/name, or opens a picker; `--continue` is a separate mode and conflicts with resume selection. Official session management includes naming/sharing/history operations. No documented `events.jsonl` import | `c:14search12+14search0` |
| 10 Stability **[D]** | The new session logging format is itself an explicit compatibility break. Readers need both legacy `history-session-state` and current `session-state` | `f:11file0:L7-L18` |

### Gemini CLI — `v0.65.0`

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[C/D]** | File-based recording under the Gemini CLI home/project storage; `GEMINI_CLI_HOME` is honoured by official code/tests. Session files have a `session-` prefix and live in project `chats` storage | `f:16file3:L49-L65` `f:17file1:L16-L34` |
| 2 Format **[C]** | **Append-oriented JSONL.** Core types (below) are well specified in source. Conversation metadata: `sessionId`, `projectHash`, `startTime`, `lastUpdated`, `messages`, optional `summary`, `memoryScratchpad`, `directories`, and `kind: "main"\|"subagent"`. No first-party captured session fixture was located: the types are the verified minimal shape, not a fabricated "real" transcript | `f:15file0:L1-L13` |
| 3 Conversation model **[C]** | Content uses Google GenAI `PartListUnion`: text and multimodal/provider content stay structured. One Gemini message can combine text, tool calls, thought summaries and model/token metadata. A tool result pairs by `ToolCallRecord.id`. Several tool calls can sit in one message: never serialize parallel calls into fake alternating assistant/tool messages | `f:15file0:L1-L13` |
| 4 Structure **[C]** | The physical JSONL is append-oriented; logical history is rebuilt by applying rewinds, message patches, deletions and ordering patches. `ChatRecordingService` keeps the effective message map; on `$rewindTo` it removes the selected message and the logical history after it. Physical file order is **not** the current conversation history | `f:14file0:L1-L2` |
| 5 Turns **[C/D]** | Tool records carry scheduler `status`; user/Gemini records are typed separately. `0.65.0` fixed duplicate tool-response turns on resume, stopped a quick exit from deleting resumed history, enforced a terminal user-turn invariant and fixed poisoning after rapid Ctrl-C: interrupted/incomplete history needs explicit normalization on resume | `f:40file0:L1-L2` |
| 6 Compaction **[C/D]** | Conversation metadata supports `summary`; recording moved to append-only delta patching with bounded history windows for long sessions. Keep summary metadata separate from ordinary assistant text | `f:15file0:L1-L13` `f:13file7:L91-L100` |
| 7 Reasoning **[C]** | Two things not to conflate: `thoughts` holds timestamped `ThoughtSummary` objects for the session/UI; `content: PartListUnion` can carry provider-native parts, including thought-signature-bearing ones. Keep both. `MAX_TOOL_OUTPUT_SIZE` is 50 KiB in the recording types: display/result data is not lossless raw process output | `f:15file0:L1-L13` |
| 8 Metadata **[C]** | Durable: `sessionId`, project hash, timestamps, model per Gemini message, token summary, workspace directories, main/subagent kind. Cwd is indirect: project hashing/storage plus persisted workspace-directory additions | `f:15file0:L1-L13` |
| 9 Resume/import/export **[D/C]** | The UI browses/resumes sessions and supports rewind; `ResumedSessionData` pairs a parsed `ConversationRecord` with its source file path. A malformed or unreadable session can return `null`: hand-written JSONL must satisfy the reconstruction logic, not just JSON syntax | `f:15file0:L1-L13` `f:13file4:L47-L56` |
| 10 Stability **[D]** | Full-history checkpoints are marked deprecated in favour of append-only `MessageRecord`/patch records; `0.65.0` has several resume fixes. Detect both legacy `$set.messages` snapshots and current delta logs | `f:15file0:L1-L13` |

Gemini record types (item 2, `f:15file0:L1-L13`):

```text
BaseMessageRecord = { id, timestamp, content, displayContent? }
ToolCallRecord    = { id, name, args, result?, status, timestamp, agentId?, displayName?,
                      description?, resultDisplay?, renderOutputAsMarkdown? }
Gemini message    = BaseMessageRecord + { toolCalls?, thoughts?, tokens?, model? }
tokens            = { input, output, cached, thoughts?, tool?, total }

record-level deltas:
{ "$rewindTo": "<message-id>" }
{ "$patch": { id?, content?, toolCalls?, updates?, removeIds?, orderIds? } }
{ "$set": ... }        metadata updates
```

### Qwen Code — CLI `0.26.1`

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[C/D]** | Durable active/archive **JSONL transcripts**; daemon lifecycle code locates, resumes, loads, archives and hard-deletes both forms. The cross-platform default root was not extracted with confidence: use Qwen's own path resolver, do not hard-code an inherited Gemini path | `f:18file0:L1-L16` |
| 2 Format **[C]** | `ChatRecord` is a parent-linked JSONL record: `uuid`, `parentUuid`, `sessionId`, `timestamp`, `type`, optional `subtype`, optional `provenance`, plus type-specific content. Types: `user`, `assistant`, `tool_result`, `system`. A source comment calls the format append-only and tree-reconstructable | `f:20file0:L1-L2` |
| 2 System subtypes **[C]** | Among others: `chat_compression`, `custom_title`, `parent_session`, `session_source`, `session_model`, `session_approval_mode`, `rewind`, `file_history_snapshot`, `branch_checkpoint`, `goal_state`, `goal_runtime`, `goal_turn_end`, `turn_result`, `turn_attempt`, managed-session records, `agent_mention`, `agent_message`. These are **not all conversational messages** | `f:20file0:L1-L2` |
| 3 Conversation model **[C]** | Tool-result rows have their own record type and provenance; model-facing content uses GenAI-compatible structured parts. The recorder sanitizes oversized file-diff/display payloads (preview/truncation metadata replaces large diffs/content): recorded `resultDisplay` is not always the raw tool output | `f:20file0:L1-L2` |
| 4 Structure **[C/D]** | `uuid`/`parentUuid` forms a tree, designed for branching. UI branch handling flushes/snapshots the outgoing recorder, forks the JSONL and assigns a branch title. Rewinds and branch checkpoints are explicit system records | `f:20file0:L1-L2` `f:18file1:L17-L32` |
| 5 Turns **[C]** | `turn_attempt` and `turn_result` subtypes exist beside ordinary user/assistant/tool rows: the safer place to infer lifecycle than message alternation. Goal-runtime and external-agent records are not a new human turn | `f:20file0:L1-L2` |
| 6 Compaction **[C]** | `chat_compression` is an explicit system record. Source docs: system records can change how model history is reconstructed while the original UI history stays. A clear case for modelling **visible history** and **resume context** separately | `f:20file0:L1-L2` |
| 7 Reasoning **[C/I]** | Provider content is structured and can carry provider-specific reasoning state. Do not rewrite opaque part metadata: the recorder's truncation rules apply to display/tool data, not to provider reasoning | |
| 8 Metadata **[C]** | Session ID, timestamp, git context helpers, auto-title state, model/session-model events, approval mode, source snapshots, provenance, file-history snapshots, attribution snapshots | `f:20file0:L1-L2` |
| 9 Resume/import/export **[D]** | ACP supports `newSession`, `unstable_resumeSession`, `loadSession`; release history includes resume/continue SDK support. An official export command writes session history as Markdown and HTML | `f:18file0:L1-L16` `f:0file0:L1-L2` |
| 10 Stability **[C/D]** | Evolved far beyond the Gemini recorder it resembles: branching, goal protocol, managed-session events, multi-agent rows and execution-engine metadata are Qwen-specific. Treat unknown `subtype` values as forward-compatible opaque records | `f:20file0:L1-L2` |

### Kimi Code — `2.3.3`

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[D]** | Layout below. `KIMI_CODE_HOME` moves the root. The home-relative design holds on every OS: use the variable/path API, do not spell a platform directory by hand | `f:22file0:L1-L13` |
| 2 Format **[D]** | `state.json` is session metadata; each `wire.jsonl` is the durable agent event/request stream. The wire also records request traces: tool schemas, model request parameters, MCP tool listings. A good source for high-fidelity converters, but only if the wire is kept, not rendered Markdown | `f:22file0:L1-L13` |
| 3 Conversation model **[D]** | The main agent and each subagent have separate wire logs. The trace keeps the exact tool declarations sent to the model, which answers "which schema did this tool name mean at that turn?". Recent versions move large base64 media into blob files instead of `wire.jsonl` | `f:22file0:L1-L13` `f:21file3:L71-L89` |
| 4 Structure **[D]** | `/fork` creates an independent session copy and leaves the user in the original. Subagent histories stay separately addressable. A saved `/goal` is not copied into the fork: cloning is not a byte-for-byte copy of all runtime state | `f:22file0:L1-L13` |
| 5 Turns **[D/I]** | `wire.jsonl` is an event stream: rebuild completion from event semantics, not from each assistant record. Session restore consumes the wire itself | |
| 6 Compaction **[D]** | Automatic near the context limit; `/compact [hint]` forces it. Issued during a running agent, it is queued; duplicate queued compactions are rejected; a queued manual compaction can be skipped if auto-compaction already ran. Keep these distinctions where possible | `f:22file0:L1-L13` |
| 7 Reasoning **[I/Gap]** | Provider-native reasoning can appear in wire-level model traffic; no guarantee it transfers to another provider. Keep the raw event/request content; turn it into plain assistant text only if the target cannot accept it | |
| 8 Metadata **[D]** | `state.json` holds title/creation metadata; `session_index.jsonl` indexes sessions by workspace; the storage key includes the working directory; request traces hold model/tool/request context | `f:22file0:L1-L13` |
| 9 Resume/import/export **[D]** | Commands below. The importer turns Claude/Codex material into a regular Kimi session with a **model-facing condensed import message**, and keeps the complete history and copies of the original source files separately: a deliberate lossy-runtime / lossless-archive split. It runs locally and calls no model | `f:22file0:L1-L13` |
| 10 Stability **[D]** | Kimi warns not to edit files under `sessions/` by hand: restoration can fail. Large media moved from inline wire data to blob storage: copy the whole session directory, not just `wire.jsonl` | `f:22file0:L1-L13` `f:21file3:L71-L89` |

```text
$KIMI_CODE_HOME/                 # default ~/.kimi-code/
├── config.toml
├── session_index.jsonl
└── sessions/
    └── <workDirKey>/
        └── <sessionId>/
            ├── state.json
            └── agents/
                ├── main/wire.jsonl
                └── <subagentId>/wire.jsonl
```

**Resume and fork**
- `kimi --continue`
- `kimi --session <id>`; bare `kimi --session` opens a picker
- `/sessions` or `/resume`
- `/fork`

**Export and import**
- `kimi export <sessionId> [-o path]` → diagnostic ZIP
- `/export-debug-zip` → ZIP
- `/export-md` / TUI `/export` → Markdown
- `kimi import-sessions` / `/import-sessions` → import Claude Code and Codex sessions

### Mistral Vibe — `v3.0.3`

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[D/C]** | Default `~/.vibe/logs/session/`; `session_logging.save_dir` moves it. Stored metadata includes cwd, enabling workspace-scoped resume | `c:6search24` |
| 2 Format **[C]** | Vibe 3 uses a "Unified Session Store" (source defines `UnifiedSessionStore`); legacy-import tests show the earlier layout: `meta.json` plus a line-oriented message file. A migration boundary to detect | `f:24file0:L1-L13` `f:23file2:L37-L63` |
| 3 Conversation model **[C/D]** | Built around durable session events/messages and tool lifecycle; reasoning shows as "Thought" in the UI. The current low-level record union was not fully extracted: keep unsupported record kinds opaque | |
| 4 Structure **[D]** | Subagents supported; recovery relies on durable local state, not rendered terminal text. Hook events carry `session_id`, `parent_session_id`, `transcript_path` and `cwd`: parent/subsession links are available at runtime | `f:23file0:L1-L17` |
| 5 Turns **[D]** | 3.0.x fixed sessions saved **during a tool call** that 3.0.0 could treat as invalid. Mid-tool persistence is a real, valid state: do not normalize it into a completed turn | `c:16search1` |
| 6 Compaction **[D]** | Vibe 3 compacts context; durable session history stays available. Represent compression separately from deletion | `c:16search1` |
| 7 Reasoning **[D/Gap]** | Thought display is supported; no stable cross-provider signed-reasoning interchange contract. Keep provider payloads if available | |
| 8 Metadata **[D/C]** | Session ID, parent session ID, cwd and transcript path are explicit in hook events | `f:23file0:L1-L17` |
| 9 Resume/import/export **[D]** | `/resume`, `--continue` and bare `--resume`; resume can be scoped by folder. No foreign-session file import contract verified | `c:6search24` |
| 10 Stability **[D]** | The move from metadata/messages files to the Unified Session Store, plus the 3.0.x recovery fix, make Vibe a version-dispatch case. Do not write legacy `meta.json`/messages and expect V3 to treat them as native without its importer | `c:6search8+16search1` |

### Cline — `v4.1.26`

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[D]** | CLI and VS Code extension do **not** share one storage tree. CLI data: `~/.cline/data/` (Windows `%USERPROFILE%\.cline\data`). VS Code task data, in the extension's global storage (paths below) | `c:6search14` |
| 2 Format **[D/C]** | Task folders hold JSON conversation state such as `api_conversation_history.json` plus UI/task state. The history index can be rebuilt from task folders: the per-task folder is more canonical than the sidebar index | `c:6search18+6search6+6search26` |
| 3 Conversation model **[D]** | API conversation history and UI messages are separate. Do not collapse tool interactions, user-visible presentation and provider API history into one transcript | |
| 4 Structure **[D]** | Local tasks are separate units; the product also has cloud sessions/sandboxes. Local and cloud are distinct modes: do not assume a shared filesystem schema. `v4.1.26` introduced/expanded Local/Cloud session behaviour | `f:25file0:L1-L2` |
| 5 Turns **[D]** | `v4.1.26`: a connection dropped after streaming only reasoning now retries instead of ending the task; cancellation/session-switch races around pending questions were fixed. Track an explicit incomplete/interrupted state in an intermediate model | `f:25file0:L1-L2` |
| 6 Compaction **[D]** | Auto-compaction exists; `v4.1.26` fixed repeated compaction and disabled it for models with unknown context limits | `f:25file0:L1-L2` |
| 7 Reasoning **[D/I]** | Reasoning can stream before answer text and affects retry. Keep provider reasoning fields opaque; a "reasoning-only partial turn" is not a completed assistant response | `f:25file0:L1-L2` |
| 8 Metadata **[D]** | Task history/index state and per-task files give task IDs, timestamps/history and API/provider state; cloud sessions add repository/branch/sandbox progress metadata | `f:25file0:L1-L2` |
| 9 Resume/import/export **[D]** | Task history plus a `Reconstruct Task History` command that rebuilds the index from task folders; not a generic foreign-session importer. Cloud sessions have their own resume path | `c:6search6+6search26` |
| 10 Stability **[D]** | Diverged from older Cline-family layouts: CLI and cloud sessions coexist. Identify **extension-local**, **CLI-local** and **cloud** tasks separately | `f:25file0:L1-L2` |

VS Code task folders, conventionally (`c:6search14`):

| OS | Path |
|---|---|
| Windows | `%APPDATA%\Code\User\globalStorage\saoudrizwan.claude-dev\tasks` |
| macOS | `~/Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/tasks` |
| Linux | `~/.config/Code/User/globalStorage/saoudrizwan.claude-dev/tasks` |

### Roo Code — final `v3.54.0`

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[D]** | Its own VS Code global storage: tasks live under the Roo extension's global-storage task directory (`rooveterinaryinc.roo-cline/.../tasks`), not Cline's current namespace | `c:6search3` |
| 2 Format **[C/D]** | Like its Cline ancestry: task history plus API conversation material in task-level files. `ClineProvider` restores task stacks/history instead of deriving state from rendered chat | `c:6search23+6search7` |
| 3 Conversation model **[C]** | Provider/API messages and UI task state are separate. Tool-call serialization follows Roo's own historical Cline-family protocol: do not assume compatibility with current Cline | |
| 4 Structure **[C]** | Task-stack/delegation semantics in provider code: represent child/delegated work separately, do not flatten it into the parent transcript | `c:6search23` |
| 5 Turns **[Gap]** | No public stable row-level turn-finalization contract | |
| 6 Compaction **[Gap]** | Context management existed; no complete final-version persistence contract for summaries verified | |
| 7 Reasoning **[Gap]** | No stable cross-provider persisted reasoning contract verified | |
| 8 Metadata **[D]** | History and task-level global state are both used; an index alone is not enough for conversion | `c:6search7` |
| 9 Resume/import/export **[D/Gap]** | Native task reopening/history; no generic third-party file import contract verified | |
| 10 Stability **[D]** | Final release `v3.54.0`, published 15 May 2026 at commit `27001b2…`; the repository then became read-only/archived. The final schema is frozen in practice, not formally standardized | `f:29file0:L1-L2` |

### Continue

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[C/D]** | Global sessions directory, normally `~/.continue/sessions`; `CONTINUE_GLOBAL_DIR` moves the global root. Files are `<sessionId>.json`, indexed by a `sessions.json` catalog. IDE and CLI history use this same global data rather than each client inventing its own format | `c:15search9+15search6` |
| 2 Format **[C]** | `core/util/history.ts` reads/writes JSON session objects. Saved fields: `sessionId`, `title`, `workspaceDirectory`, `history`, optional `mode`, optional `chatModelTitle`, optional `usage`. The catalog tracks session ID/title/date/workspace and message count. Legacy objects named with `session_id` are detected: a concrete migration case | `c:19view3` |
| 3 Conversation model **[C/Gap]** | `history` is the durable container; its nested item schema varies with features and versions. Keep each history object whole; do not rebuild it from `role/content` only | |
| 4 Structure **[D]** | CLI `/fork` exists beside `/resume`: branching can create a new session-level object. Do not represent a fork only as an in-place edit of the parent | `c:15search1` |
| 5 Turns **[I]** | The catalog's message count is derived from history, not a completion marker; a final partial model/tool state can exist regardless of that count | |
| 6 Compaction **[D]** | `/compact` is a first-class CLI operation. The summarized context is resume state, not evidence that older durable history may be discarded | `c:15search1` |
| 7 Reasoning **[Gap]** | Provider-specific reasoning persistence not verified as a stable schema. Keep unknown nested history fields | |
| 8 Metadata **[C]** | Title, workspace directory, model title, mode and usage are first-class save fields | `c:19view3` |
| 9 Resume/import/export **[D]** | `/resume`, `/fork`, `/compact` and title/rename are documented. No stable generic import/export file command like Kimi's importer: writing JSON directly is an internal-format operation, not an official import API | `c:15search1` |
| 10 Stability **[C]** | The loader already handles historical field names: parse old and new session forms, do not assume one immutable schema | `c:19view3` |

### pi

Verified on this machine: [pi.md](../harnesses/pi.md), sections 5 and 8. They supersede this
dossier.

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[D]** | Sessions are saved automatically unless `--no-session` is used; the session directory is configurable, and `--session-dir` has explicit precedence in the official implementation/issues | `c:7search5+8search13` |
| 2 Format **[D]** | The official session-format docs describe a JSONL session/tree, not a flattened transcript | `c:7search17` |
| 3 Conversation model **[D]** | Model messages, tool interactions and control entries are separate durable entries. Keep entry `type` and provider payload; do not force everything into OpenAI-style `{role,content}` | |
| 4 Structure **[D]** | Tree-oriented, with navigation/branching: resume is not "read every JSONL line in physical order" | `c:7search17` |
| 5 Turns **[Gap]** | Current completion/interruption row predicates were not captured strongly enough for a converter contract | |
| 6 Compaction **[D]** | Compaction/branch summaries are session control information in the documented format, not only visual | |
| 7 Reasoning **[I]** | Keep provider-native assistant content; no guarantee that reasoning from one model/provider replays to another | |
| 8 Metadata **[D]** | Session identity, parent linkage, timestamps and session labels/names are durable | `c:7search17` |
| 9 Resume/import/export **[D]** | `pi --continue`, `pi --resume`, `/resume`, `/new`, `/name`, `/session` | `c:7search5` |
| 10 Stability **[Gap]** | Session docs verified, but no authoritative 27 October release tag. Pin converters to a tested source commit/package version, not the bare product name | |

### Oh My Pi

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[D]** | User state under `~/.omp`; foreign-session conversions are persisted under `~/.omp/agent/sessions/`. Runtime/session integrations see `PI_SESSION_ID` and `PI_SESSION_FILE` | `c:7search3+7search15+7search18` |
| 2 Format **[D/I]** | Session-file based with pi-family semantics, but its extensions/import adapters mean a pi reader should not assume every OMP record is upstream-compatible | |
| 3 Conversation model **[D]** | Foreign Claude/Codex imports become OMP session structures, not transient displays | `c:7search18` |
| 4 Structure **[D]** | Session identity/file are explicit environment concepts; branching/navigation follows the session-oriented model | |
| 5 Turns **[Gap]** | Low-level completion markers not verified separately from upstream pi | |
| 6 Compaction **[I]** | Keep pi-family compaction/summary entries and OMP-specific extension records | |
| 7 Reasoning **[I]** | Treat imported provider reasoning as opaque source payload unless OMP's importer maps it explicitly | |
| 8 Metadata **[D]** | `PI_SESSION_ID` and `PI_SESSION_FILE` make the active session's identity/path explicit to integrations | `c:7search15` |
| 9 Resume/import/export **[D]** | Imports Claude Code and Codex with `--from-claude`, `--from-codex`, and resume UI forms such as `/resume @claude` and `/resume @codex`. When the cited issue was written, broader Pi/OpenCode import was still being discussed | `c:7search18` |
| 10 Stability **[Gap]** | Latest tag not pinned. Import support is evolving: detect capabilities by version, not by product name | |

## Hybrid and cloud-first harness dossiers

### Kilo Code — `v7.8.18`

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[D/Gap]** | Moved toward CLI-managed session history and hosted/cloud sessions; assumptions from the historical Roo/Cline VS Code global-state tree are no longer a safe contract | `c:6search21+17search0` |
| 2 Format **[Gap]** | No stable public local-session DB/file schema verified for 7.8.x. Do not use the legacy Cline-family folder layout as a current Kilo interchange format without version detection | |
| 3 Conversation model **[D]** | Sessions are exposed through Kilo's local API, which can also expose hosted sessions. Tool/model state belongs to session objects, not just UI history | `c:17search0` |
| 4 Structure **[D]** | `7.8.12` added creating/forking a session from a message with `cloneUpToMessageId`, with edit restoration: an explicit fork boundary. Map it to parent session plus cutoff, not to duplicated history with no ancestry | `c:17search0` |
| 5 Turns **[Gap]** | No stable public row-level turn-end schema verified | |
| 6 Compaction **[D/Gap]** | Context compaction exists in the agent stack; the durable summary representation was not verified | |
| 7 Reasoning **[D]** | `7.8.15` fixed persistence/replay of late OpenRouter/Gemini reasoning details. A converter that drops "late" post-finish reasoning chunks loses native Kilo semantics | `c:17search0` |
| 8 Metadata **[D]** | Hosted/cloud state includes provider/session identity and message ancestry; the backing schema is not public | |
| 9 Resume/import/export **[D/Gap]** | The local API and current CLI operate on sessions, hosted ones included. No documented foreign-file import contract verified | |
| 10 Stability **[D]** | Particularly dangerous for Cline/Roo-era assumptions: architecture and session ownership changed. Check the version before choosing a storage adapter | `c:6search21+17search0` |

### Cursor — CLI agent and desktop

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[D/Gap]** | The CLI now has persistent sessions, but the official docs give no supported on-disk transcript path/schema for third-party writers. Desktop's internal state is not a published interchange contract either | `c:3search4` |
| 2 Format **[D/Gap]** | The CLI emits machine-readable `json`/`stream-json` output; these streams are not documented as the canonical persistent storage format | `c:3search24` |
| 3 Conversation model **[D]** | The public SDK/agent bridge exposes a protobuf contract: a better integration surface than reverse-engineering desktop storage | `c:3search25` |
| 4 Structure **[D]** | Checkpoints are stored locally; persistent sessions can be listed/resumed. No stable local branch-tree file contract | `c:3search8+3search4` |
| 5 Turns **[Gap]** | Use SDK/protocol completion events when integrating live; no verified file-level final-turn marker | |
| 6 Compaction **[Gap]** | Not specified enough in public persistence docs for faithful file conversion | |
| 7 Reasoning **[Gap]** | No local reasoning/signature persistence contract verified | |
| 8 Metadata **[D]** | Session/chat IDs are exposed through list/resume APIs; SDK local Agent-store methods list, get and resume session objects | `c:3search28` |
| 9 Resume/import/export **[D]** | `agent ls`, `agent resume`, `agent --continue`, `agent --resume=<chat-id>`. No generic third-party transcript import command verified | `c:3search12+3search0` |
| 10 Stability **[D]** | Persistent CLI sessions are recent (introduced in the August 2026 line). Prefer the public SDK/protobuf over desktop DB manipulation | `c:3search4+3search25` |

### Amp

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[D/I]** | The documented durable object is a **Thread**, addressed by thread ID through service APIs/CLI. No first-party canonical local transcript directory documented: the service thread is authoritative, not a local cache | `c:7search4` |
| 2 Format **[D]** | Official export: Markdown, and full JSON for the thread creator. Export is the supported extraction surface; local internal caches are not | `c:7search4` |
| 3 Conversation model **[D]** | Keep structured thread data from the JSON export; Markdown is presentation only, unsuitable for round-trip converters | |
| 4 Structure **[Gap]** | No local parent/branch file schema verified | |
| 5 Turns **[Gap]** | Use the exported/thread API representation, not boundaries inferred from Markdown | |
| 6 Compaction **[Gap]** | Internal context management is not a documented local persistence contract | |
| 7 Reasoning **[Gap]** | No signed-reasoning replay contract exposed for imports | |
| 8 Metadata **[D]** | Thread IDs are stable user-facing identities | |
| 9 Resume/import/export **[D]** | Thread Markdown by URL/CLI; full JSON export for the creator. No official JSON re-import into a new thread verified | `c:7search4` |
| 10 Stability **[I]** | Build on the official Thread API/export, not an undocumented local cache | |

### Grok CLI / Grok Build

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[D]** | Conversations are saved below `~/.grok/sessions/`, keyed by working directory; this covers normal TUI, headless and ACP session handling | `c:10search21+10search18` |
| 2 Format **[D]** | The core persisted stream is `updates.jsonl`: append-only NDJSON, one self-contained ACP session-update event per line | `c:10search1` |
| 3 Conversation model **[D]** | ACP updates carry tool/session/model events, richer than chat text. Headless JSON output separately gives the final result: `text`, `stopReason`, `sessionId`, `requestId`, optional `thought`, usage/model-usage, cost. Do not confuse that result object with `updates.jsonl` | `c:10search4` |
| 4 Structure **[D]** | Session ID is canonical; the sandbox profile is fixed/persisted with the session. No supported file-clone branching contract identified | `c:10search14` |
| 5 Turns **[D]** | Headless `stopReason` includes values such as `end_turn` and `max_tokens`: an explicit completion signal for emitted results. Session-update events are still needed to rebuild aborted/in-flight turns | `c:10search4` |
| 6 Compaction **[Gap]** | No detailed persistence-level compaction schema verified | |
| 7 Reasoning **[D/Gap]** | Headless output can contain `thought`; no guarantee that it replays as provider-native reasoning | |
| 8 Metadata **[D]** | Session/request IDs, usage, per-model usage and cost are in the machine-readable execution output | `c:10search4` |
| 9 Resume/import/export **[D]** | `/resume`, `grok --resume <id>`, `--continue`/`-r`, `grok sessions`, `grok export`. An official issue says there is no general import/create-from-file restore; `--session-id` creates/addresses a session but is not a JSONL importer | `c:10search9+10search13+10search25` |
| 10 Stability **[D]** | The official repository showed no conventional stable GitHub releases: pin a source revision and ACP schema version | `c:10search0` |

### Antigravity

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[D/I]** | Conversations are backend/service objects, not a documented local transcript database. The CLI cache `~/.gemini/antigravity-cli/cache/last_conversations.json` maps workspaces to conversation IDs, which the client validates/loads from the backend: an index/cache, not the transcript | `c:11search16` |
| 2 Format **[Gap]** | No supported local conversation-file schema published | |
| 3 Conversation model **[D]** | Conversations keep context and tool trajectories; migrating Antigravity Desktop into the CLI can clone history, context and tool trajectories | `c:11search16` |
| 4 Structure **[D]** | `/rewind`, `/undo`, `/fork`, `/clear` and `/resume` are separate operations: the service models branching/history editing explicitly | `c:11search7` |
| 5 Turns **[Gap]** | The backend protocol, not a public local file, is authoritative for running/completed state | |
| 6 Compaction **[Gap]** | No local public summary schema verified | |
| 7 Reasoning **[Gap]** | No reusable signed-reasoning file contract | |
| 8 Metadata **[D]** | The workspace→conversation mapping is cached locally; conversation existence/content is backend-validated | `c:11search16` |
| 9 Resume/import/export **[D]** | `/resume` has picker/rename/delete; CLI options cover continuation/conversation selection. Antigravity can import/clone Antigravity 2.0 desktop history into its newer environment: vendor-controlled service migration, not a general transcript importer | `c:11search16` |
| 10 Stability **[D]** | App and CLI have separate version streams; latest verified changelog entries: app `2.22.0`, CLI `1.2.12` | `c:11search23` |

### Hermes Agent — `v0.22.5`

mukade's own notes, now the authority: [hermes-agent.md](../harnesses/hermes-agent.md), Session store.
The report's schema 31, its `hermes import <backup.zip>` route and its "no stored summary row" were
superseded by reading v0.23.1: schema 32 and 33, `hermes sessions import --from claude`, a stored
summary row (`_compressed_summary = 1`).

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[C/D]** | Local agent/session persistence plus gateway/session mappings. Code references JSON catalogs such as `sessions.json` and several SQLite databases (`state.db`, `kanban.db`, `response_store.db`, etc.) for wider durable state. The gateway session manager loads and atomically rewrites `sessions.json` | `f:31file2:L57-L68` `f:31file4:L97-L107` |
| 2 Format **[C/D]** | Session export as JSONL or Markdown, including a filtered user-prompts view: safer for interchange than combining the internal DBs yourself | `f:31file0:L1-L12` |
| 3 Conversation model **[D]** | Sessions include tool-call history; the resume UI can summarize tool-call counts/names. Delegation/subagent backends are pluggable: do not assume subagent work is inlined text | `f:30file0:L1-L2` |
| 4 Structure **[D]** | Gateway/channel sessions can map topics/chats to independent Hermes sessions. Subagent/delegation behaviour is configurable per session in the current release family | `f:31file9:L217-L226` `f:30file0:L1-L2` |
| 5 Turns **[D]** | Session operations can remove recent user turns; the UI distinguishes restored/compacted session state. Turn deletion is history mutation, not a new assistant message | `f:31file8:L198-L208` |
| 6 Compaction **[D]** | The docs warn that compression reduces active context but is **not a privacy deletion**. Keep the durable history if the target supports it | `f:31file1:L33-L44` |
| 7 Reasoning **[Gap]** | Provider-specific reasoning persistence is not a stable cross-model replay contract | |
| 8 Metadata **[D/C]** | Sessions can have titles/names; gateway catalogs map source/channel identity to sessions; wider state is spread over JSON and SQLite stores | `f:31file1:L33-L44` |
| 9 Resume/import/export **[D]** | Named resume/search/history, session export (`jsonl`/`md`), backup/restore tooling and `hermes import <backup.zip>`; the current release also has `hermes import-agent` | `f:31file0:L1-L12` `f:30file0:L1-L2` |
| 10 Stability **[D]** | `v0.22.5` is a large roll-up patch of roughly 81 merged PRs since `0.22.4`; the fuller curated description is deferred to `0.23.0`. Session/delegation/import surfaces are moving | `f:30file0:L1-L2` |

### OpenHands — `v1.33.0`

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[D/C]** | Pluggable persistence, not one desktop transcript path. Local deployments use configured persistence/file stores; cloud/self-hosted deployments can put session/conversation artifacts in object stores such as S3/GCS | `c:12search1` |
| 2 Format **[D/Gap]** | The durable unit is conversation/event state, not a JSONL file shared by every deployment. SDK persistence saves state changes so a conversation can be recovered | `c:12search1` |
| 3 Conversation model **[D]** | Conversation IDs are UUID-like durable identifiers; persistence includes event/plugin/runtime state, not only text. Plugin resolution itself must survive persistence/resume | `c:12search9` |
| 4 Structure **[D]** | Conversation and agent runtime state are separate from UI rendering; branch/fork persistence varies with the product layer | |
| 5 Turns **[D]** | App metadata includes execution/sandbox status: running vs finished is not inferred from the last message alone | `c:12search8` |
| 6 Compaction **[D]** | An explicit context-condenser. Keep the condensed context separate from the raw event history when the store keeps both | `c:12search21` |
| 7 Reasoning **[Gap]** | No generic cross-provider signed-reasoning persistence contract verified | |
| 8 Metadata **[D]** | Conversation adapters expose ID, title, creation/update times, execution status, sandbox status, runtime information, cost and token usage | `c:12search8` |
| 9 Resume/import/export **[D/Gap]** | SDK/application persistence recovers conversations; no portable file format accepted by every deployment verified | |
| 10 Stability **[D]** | `v1.33.0`, published 23 October 2026. Persistence is pluggable: model "OpenHands format" in a converter as an API/event model plus a backend adapter, not one guessed disk layout | `f:34file0:L1-L2` |

### Devin

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[D]** | Service-backed; no documented local canonical transcript database suitable for file conversion. Cloud sessions can sleep and resume later; history outlives the shorter live-environment window | `c:12search10` |
| 2 Format **[Gap]** | No public canonical on-disk schema | |
| 3 Conversation model **[D]** | Sessions hold steps, tool activity and environment state; UI/CLI present them as service sessions, not local JSONL | |
| 4 Structure **[D]** | CLI `/fork [step]`, `/steps` and `/revert`: step identity and fork point matter. `/btw` is a sidechain-like interaction; do not convert it into the main branch automatically | `c:12search2` |
| 5 Turns **[D]** | Service/step state is the canonical completion model; do not infer it from exported prose | |
| 6 Compaction **[Gap]** | Internal context management is not exposed as a portable persistence schema | |
| 7 Reasoning **[Gap]** | No portable reasoning-signature import contract | |
| 8 Metadata **[D]** | Session name/identity and cloud environment lifecycle are service metadata; the session picker/history spans sessions beyond the current cwd | `c:12search22` |
| 9 Resume/import/export **[D]** | CLI continuation/resume, `/resume`, `/ls`, `/rename`, `/rm-session`, fork/revert, and `/export` when export capability is enabled. Desktop/plugin UI also has history export. No local transcript import contract verified | `c:12search2+12search25` |
| 10 Stability **[I]** | Treat Devin as an API/export target; a converter built on browser/Desktop caches is unsupported and fragile | |

### MastraCode

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[D/Gap]** | Built on the Mastra Harness, instantiated with a storage implementation: session storage is part of the Harness/storage abstraction, not one documented `~/.mastracode/...` transcript file | `c:13search1+13search5` |
| 2 Format **[Gap]** | No single portable canonical file format verified | |
| 3 Conversation model **[D]** | Harness primitives: messages, streaming, tools, approvals/policies, MCP, subagents. The desktop integration builds a Harness with agent, tools, storage, MCP and auth | `c:13search5+13search13` |
| 4 Structure **[D]** | Worktrees can have isolated session/harness/state/grants: filesystem worktree identity and conversation identity are related, not interchangeable | `c:13search5` |
| 5 Turns **[D]** | The Harness event/stream API is a better completion source than persisted rendered text | |
| 6 Compaction **[D]** | Observational/context memory is a Harness feature: persistence can include derived memory distinct from literal history | `c:13search1` |
| 7 Reasoning **[Gap]** | No vendor-neutral signed-reasoning file contract verified | |
| 8 Metadata **[D]** | Session/harness/worktree identity, grants and storage configuration are runtime metadata worth keeping | |
| 9 Resume/import/export **[Gap]** | Programmatic Harness APIs are the supported integration surface; no general third-party transcript importer/exporter verified | |
| 10 Stability **[Gap]** | Latest package version not pinned; pin converters to a specific `mastracode`/Harness package pair | |

### Factory Droid

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[D/Gap]** | Persistent sessions, but no published canonical local transcript file/database schema. Some execution environments ("Droid Computers") are cloud-persistent across sessions | `c:13search7+13search11` |
| 2 Format **[Gap]** | No supported local session interchange format verified | |
| 3 Conversation model **[D]** | Sessions include agent execution/tool state, not plain chat | |
| 4 Structure **[Gap]** | No first-party portable branch tree schema verified | |
| 5 Turns **[D/I]** | Use Droid session/run APIs and execution state, not terminal-output boundaries | |
| 6 Compaction **[Gap]** | Not publicly specified at storage level | |
| 7 Reasoning **[Gap]** | No portable signed-reasoning contract | |
| 8 Metadata **[D]** | Desktop exposes session context, progress and history; cloud computers add persistent environment identity | `c:13search19+13search11` |
| 9 Resume/import/export **[D/Gap]** | `droid`/`droid exec` and persistent session UX are official; no generic file import/export contract verified | `c:13search3` |
| 10 Stability **[I]** | Treat Factory's service/API as canonical; local-cache conversion is unsupported unless Factory documents it | |

### Windsurf / Cascade

| Item | Claim | Src |
|---|---|---|
| 1 Storage **[D/Gap]** | Official docs place Cascade in the Devin Desktop/Windsurf product line. No supported local canonical Cascade transcript file/database schema is published | `c:14search14` |
| 2 Format **[Gap]** | History is exportable in the product UI; the internal persistence format is not a documented interchange format | |
| 3 Conversation model **[D]** | Cascade is an agentic session, not a chat text stream; tool/context state matters | |
| 4 Structure **[Gap]** | No public durable branch/rewind storage schema verified | |
| 5 Turns **[Gap]** | Use product/session state, not UI transcript alternation | |
| 6 Compaction **[Gap]** | No portable summary record contract verified | |
| 7 Reasoning **[Gap]** | No portable signed-reasoning record contract | |
| 8 Metadata **[D]** | A separate local "memories" feature is stored under `~/.codeium/windsurf/memories/`; **memories are not the conversation transcript** and never a substitute session source | `c:14search6` |
| 9 Resume/import/export **[D]** | Recent sessions/projects can be revisited in the desktop product; conversation-history export is documented for Devin Desktop Plugins. No raw Cascade-session import format verified | `c:14search22+14search2` |
| 10 Stability **[D/I]** | Ownership/naming shifted toward Devin Desktop: identify the exact client/version; do not assume historical Windsurf internals still hold | `c:14search14` |

## Conversion architecture and cross-harness failure modes

The report's recommendation: **not** one "Harness A schema → Harness B schema" mapping per pair, but
a three-layer intermediate representation:

1. a normalized semantic core;
2. lossless source envelopes;
3. target-specific synthesized resume state.

Minimum the internal representation should keep:

```text
Session
  sourceHarness
  sourceVersion
  sourceSessionId
  cwd / workspace
  title
  createdAt / updatedAt
  model/provider
  git metadata
  parentSessionId?
  forkBoundary?
  lifecycleState
  rawMetadata

Node[]
  stableId
  parentId?
  physicalSeq
  logicalSeq?
  actor        # human / assistant / tool / system / subagent
  subtype
  content[]
  toolCalls[]
  toolResults[]
  reasoning[]
  attachments[]
  lifecycle
  rawSourceRecord

Compaction[]
  boundary
  summary
  supersededPrefix?
  rawRecord

Subsession[]
  relationship
  agentId
  sourceSessionRef

Blob[]
  digest?
  mime
  sourceURI / bytes / file
```

**Keep the raw source record.** Gemini's `$patch`/`$rewindTo`, Qwen's many `system.subtype`s,
OpenCode's JSON `data`, Kimi's wire records and Copilot's event log all hold data a generic schema
will at first fail to understand. Src: `f:15file0:L1-L13` `f:20file0:L1-L2` `f:10file0:L1-L6`
`f:22file0:L1-L13`.

### Failure modes and rules

**Tools**

1. **Tool names and input schemas are the first major lossy boundary.** A `read`, `read_file`,
   shell/Bash, edit/replace or browser tool, or an MCP method, with a similar human meaning can
   differ in parameter names, path rules, permission behaviour and result shape. Never normalize
   calls by name alone. Store at least: source tool name, source schema if available, exact
   `arguments`, source call ID, result ID/reference, status, provider namespace. Kimi's request
   trace is especially valuable: it records the tool schemas and MCP tool listing actually sent on
   that request. Src: `f:22file0:L1-L13`.
2. **Parallel tool calls stay a set/order relation, never synthetic turns.** Gemini stores several
   `ToolCallRecord`s on one Gemini message; Qwen has independent tool-result records; event-stream
   harnesses can interleave progress and results. Rewriting parallel calls as
   `assistant → tool → assistant → tool` can change the next model request and create a history
   that never existed. Src: `f:15file0:L1-L13` `f:20file0:L1-L2`.
3. **Pair call and result by ID when available.** Never by "nearest preceding tool with the same
   name". Gemini gives `ToolCallRecord.id`; Qwen gives record/tool identities; many provider
   protocols have explicit call IDs. A failed/missing result is itself state: never repair it with
   an empty success value. Src: `f:15file0:L1-L13`.

**Reasoning**

4. **Reasoning is a typed opaque artifact.** UIs call three different things "reasoning":
   human-readable thought summaries; provider-native reasoning text/parts; provider-authenticated
   or signed opaque state needed for valid continuation. Gemini shows the first two at once:
   `thoughts` is a separate summary structure while `content` stays native `PartListUnion`. Cline
   shows that "only reasoning streamed before disconnect" is an incomplete model turn, not a
   complete answer. Src: `f:15file0:L1-L13` `f:25file0:L1-L2`.

   Use a policy such as:

   ```text
   reasoning.kind =
     "display-summary"
     | "provider-native"
     | "signed-opaque"
     | "encrypted-opaque"
     | "unknown"

   replayPolicy =
     "safe-same-provider"
     | "safe-same-model-family-only"
     | "display-only"
     | "drop-from-model-context"
   ```

   Never change the bytes of an opaque signed block and resend it as though its signature were
   valid. If the target cannot represent it, keep it in the archival envelope and leave it out of
   model context; never fabricate validity.

**History shape**

5. **Compaction is not deletion.** Claude, Codex, OpenCode, Gemini, Qwen, Kimi, Copilot CLI, Vibe,
   Hermes, Continue and others all have summary/compaction, persisted differently. Three patterns
   argue for separate `visibleHistory`, `resumeContext` and `archivalHistory`:
   - Qwen: compression system records can change reconstructed model history while the original UI
     history stays;
   - Hermes: compression is not privacy deletion;
   - Kimi: imports keep complete originals behind a bounded model-facing import message.

   Src: `f:20file0:L1-L2` `f:31file1:L33-L44` `f:22file0:L1-L13`.
6. **Branches, rewind and forks have different semantics.** Never implement "fork" by copying all
   records up to timestamp T. Use an explicit source boundary: message ID, node ID, sequence,
   checkpoint or step. At least four patterns exist:

   | Pattern | Example | Src |
   |---|---|---|
   | physical append + logical rewind | Gemini `$rewindTo` | `f:15file0:L1-L13` |
   | parent-linked record trees | Qwen, pi | `f:20file0:L1-L2` |
   | session-level parent/fork boundary fields | OpenCode | `f:10file0:L1-L6` |
   | service-level fork at step/message | Devin, Kilo | `c:12search2+17search0` |
7. **Subagents are not ordinary assistant messages.** Kimi gives each agent its own `wire.jsonl`;
   Qwen has `agent_mention`/`agent_message` and other multi-agent records; Gemini has
   `kind: "main"|"subagent"`; Claude stores subagent-specific metadata. Flattening them into the
   parent chat loses attribution and can feed internal delegation chatter back to the target
   model. Src: `f:22file0:L1-L13` `f:20file0:L1-L2` `f:15file0:L1-L13` `c:0search15`.

**Content integrity**

8. **Attachments: content-addressed, or copied with referential integrity.** Session stores
   increasingly move data out of the transcript: OpenCode 2.2 adds content-addressed blob storage;
   Kimi moved large base64 media out of `wire.jsonl`. Copying only the primary chat file can give a
   syntactically valid but semantically broken session. Src: `f:10file0:L1-L6` `f:21file3:L71-L89`.
9. **Truncation is recorded as truncation.** Qwen sanitizes very large diff/file displays and
   records that a preview was truncated; Gemini caps recorded tool output at 50 KiB. Never present
   such a persisted preview as the complete process output. Src: `f:20file0:L1-L2`
   `f:15file0:L1-L13`.

**Lifecycle and metadata**

10. **Incomplete turns need first-class states.** Recommended target-neutral states (below). Not
    overengineering: Gemini `0.65.0` had resume/rapid-Ctrl-C fixes; Vibe had to repair sessions
    saved during tool calls; Cline distinguishes a transport loss after only reasoning; Grok
    exposes `stopReason`; OpenCode persists idle/suspended/resume-attempt state. Src:
    `f:40file0:L1-L2` `c:16search1` `f:25file0:L1-L2` `c:10search4` `f:10file0:L1-L6`.

    ```text
    queued
    running
    waiting_for_tool
    waiting_for_user
    completed
    interrupted_by_user
    cancelled
    tool_failed
    model_failed
    context_overflow
    max_tokens
    transport_lost
    superseded_by_rewind
    unknown_incomplete
    ```
11. **Service/meta records stay outside the conversation by default.** Examples: Gemini `$set` and
    patches; Qwen session-model/approval/title/attribution/branch/goal records; OpenCode
    inbox/instruction state; Copilot workspace artifacts. Unless a target expects them in model
    context, they are converter metadata, not synthetic `system` messages. Src: `f:15file0:L1-L13`
    `f:20file0:L1-L2` `f:10file0:L1-L6`.

### Writing native target sessions

Safest order:

1. use an official import API/command;
2. use a public session SDK/API;
3. generate the target's documented export/import representation;
4. only then write internal storage, and only against a pinned version with migration/validation
   tests.

So Kimi's `import-sessions`, OpenCode import/export, OMP's foreign importers and Hermes
backup/import are better targets than hand-built internal stores. Src: `f:22file0:L1-L13`
`c:5search1+7search18` `f:31file0:L1-L12`.

Keep a manifest per conversion, so unavoidable loss is **auditable** instead of silent:

```json
{
  "source": {
    "harness": "qwen-code",
    "version": "0.26.1",
    "sessionId": "...",
    "contentDigest": "..."
  },
  "conversion": {
    "target": "kimi-code",
    "targetVersion": "2.3.3",
    "losses": [
      "provider-native reasoning not replayed",
      "one tool display payload was source-truncated"
    ],
    "preservedOpaqueRecords": 14
  }
}
```

## Verification gaps, contradictions, and unsupported assumptions to avoid

The largest gap is not missing reverse-engineering information but missing **first-party
compatibility guarantees**. Claude Code, Copilot CLI, Cline, Cursor Desktop, Kilo, Amp, Antigravity,
Devin, Factory Droid and Windsurf all expose some durable history, but not all expose a stable
external schema a third party is entitled to write. For these: favour official resume/import/API
surfaces and keep source archives; do not imitate an internal cache.

| Harness | Verified | Gap, contradiction or trap | Src |
|---|---|---|---|
| Claude Code | JSONL location; resume/checkpoint behaviour | no complete versioned transcript union and no validation spec for foreign-created JSONL; reasoning/signature replay rules at the local-file level remain a gap | `c:13search30+0search12` |
| Codex | rollout JSONL, index, SQLite/thread-store pieces | no published contract that hand-created files are accepted by every CLI/IDE/Desktop client. The name index alone is demonstrably insufficient: code resolves it back to an actual loadable rollout | `f:39file0:L1-L6` |
| OpenCode | one of the best-verified formats | the requested `2.0.x`/`2.1.x` lines immediately precede `2.2.0`, which already changes media, compaction and instruction persistence. GitHub's "latest release" endpoint disagrees with the V2 changelog: legacy V1 and V2 release channels coexist | `f:1file0:L1-L2` `f:3file0:L1-L2` |
| Copilot CLI | new vs legacy storage | the complete `events.jsonl` type union is not a public write contract. Release-page race `1.0.103` vs `1.0.104`: pin the installed `copilot --version` in fixtures | `f:11file0:L1-L18` `c:14search1+19view2` |
| Cursor | persistent session commands; SDK/protobuf interfaces | Desktop and CLI backing-store schemas/paths deliberately not claimed: no authoritative primary contract found | `c:3search12+3search25` |
| Gemini CLI | source schema, unusually clear | moving from full-history snapshots toward append-only patches: a third-party writer must handle both and expect malformed/partial JSONL recovery semantics | `f:15file0:L1-L13` |
| Qwen Code | JSONL record model, richly visible in code | many specialized system records: a converter based on early Gemini/Qwen formats silently discards branch, goal, multi-agent and managed-session state | `f:20file0:L1-L2` |
| Kimi Code | top-level layout and commands, exceptionally well documented | the complete wire event union was not reproduced; the official warning against manual edits means: prefer the importer/API over hand-written wire logs | `f:22file0:L1-L13` |
| Mistral Vibe | Unified Session Store (source level); legacy metadata/messages files | no complete field-by-field V3 store specification claimed; the recovery fix for mid-tool 3.0.0 sessions proves edge states matter | `f:24file0:L1-L13` `c:16search1` |
| Kilo Code | — | session ownership changed enough that legacy Roo/Cline paths are not Kilo 7.8.x; the current backing-store schema remains a gap | `c:6search21+17search0` |
| Cline | local CLI and VS Code storage paths | cloud tasks are a separate persistence domain: use explicit `cline-local-cli`, `cline-vscode` or `cline-cloud` source kinds, not one generic "Cline" adapter | `c:6search14` `f:25file0:L1-L2` |
| Roo Code | final version, firmly pinned | archived: do not infer current Cline or Kilo behaviour from Roo's final schema | `f:29file0:L1-L2` |
| pi and Oh My Pi | session behaviour; foreign-import support | no authoritative latest release tag in the research window: fill version rows from the package/repository revision of the converter's test corpus, never guess | |
| Grok Build | local ACP JSONL; official resume/export | no conventional stable GitHub release tag; no general import API | `c:10search1+10search25` |
| Antigravity | — | local `last_conversations.json` is only an index/cache; treating it as a transcript is a fundamental converter error. The conversation is backend-backed | `c:11search16` |
| Hermes | — | durability spans several stores; its official export/import is a safer boundary than copying `.db` files that processes may hold open. Hermes' own docs warn about database-file replacement semantics | `f:31file4:L97-L107` |
| OpenHands | — | persistence is backend-pluggable: no single correct Windows/macOS/Linux transcript path. The boundary is the conversation/event API plus the deployment's configured store | `c:12search1` |
| Devin, Factory Droid, Amp, Windsurf/Cascade | first-party session/history/export behaviour | no primary-source canonical local transcript schema for faithful arbitrary writes. A converter claiming a stable local SQLite/IndexedDB path for these products exceeds the evidence | |
| MastraCode | Harness/storage abstraction | no single standard disk serialization; current package tag not pinned | |
| Continue | JSON save/catalog code | nested `history` records evolve: keep unknown nested fields and support the old `session_id` forms Continue already recognizes | `c:19view3` |

## Citations

The research request asked for file-and-line source links. For several GitHub fetches, the tool
returned the whole source file inside one JSON `content` field, so an `f:` line range points into
that response, not to the original GitHub source lines. Where available the report names the
repository path/symbol instead, for example `packages/core/src/session/sql.ts`,
`chatRecordingTypes.ts`, `chatRecordingService.ts` and `codex-rs/rollout/src/session_index.rs`, and
invents no original-file line numbers. The code citations still resolve to the first-party
file/commit the schema was read from.
