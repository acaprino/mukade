<!--
Provenance: pasted by the user on 2026-10-27 (session 8288ab04) as the starting reference for the
session interchange library; saved verbatim, extracted byte-for-byte from the session log.
Status: secondary research report. Its `cite…`/`file…` markers are opaque references of the tool
that produced it and do not resolve outside that tool. Claims are labelled [D]/[C]/[I]/[Gap] by
its author; none was re-run here. Facts re-run on this machine live in
docs/session-interchange-spec.md (labels [V]) and supersede this file where they disagree.
-->

# Verified Session-Persistence Reference for Coding-Agent Harnesses

## Scope, evidence model, and version baseline

This report is aimed at **loss-minimizing session converters**, not at reproducing UI transcripts. That distinction matters: in several harnesses the durable object is an append-only event log, database, parent-linked tree, or cloud thread rather than the visually rendered chat. A converter that copies only visible user/assistant text will silently lose tool-call identity, compaction state, branch ancestry, subagent state, attachments, signed reasoning, or incomplete-turn semantics.

Evidence labels used throughout:

- **[D] Documented** — stated in first-party documentation, release notes, or an official issue/PR.
- **[C] Code** — read directly from first-party source code at the cited commit/tag.
- **[I] Inference** — conclusion from first-party behavior/code, not an explicit compatibility guarantee.
- **[Gap]** — the requested fact is not publicly specified strongly enough to use as a converter contract.

For several proprietary harnesses the important result is negative: **the official product exposes resume/history, but no first-party canonical local transcript schema**. In those cases, scraping application-internal SQLite, IndexedDB, Electron state, VS Code storage, or browser caches would not meet the requested “verified converter” standard.

The version snapshot below uses the latest **stable release I could verify on 27 October 2026**, except that OpenCode is intentionally pinned separately to the requested 2.0.x and 2.1.x lines. Where a vendor uses a rolling release and exposes no authoritative public version/tag, I do not invent one.

| Harness | Verified baseline | Date / commit status |
|---|---|---|
| Claude Code | `v2.1.311` | 2026-10-26; commit `08c6a4f` in the official release record. citeturn1view0 |
| OpenAI Codex | `rust-v0.164.0` / `0.164.0` | Published 2026-10-23; stable GitHub release. fileciteturn36file0L1-L2 |
| OpenCode 2.0.x | `v2.0.32` | 2026-10-16, latest 2.0.x visible in the V2 changelog. fileciteturn3file0L1-L2 |
| OpenCode 2.1.x | `v2.1.4` | 2026-10-23, latest 2.1.x. fileciteturn3file0L1-L2 |
| OpenCode current context | `v2.2.0` | 2026-10-27; relevant because it immediately evolves the same DB/session format. fileciteturn3file0L1-L2 |
| GitHub Copilot CLI | `1.0.104` was reported by the current release feed | There was a same-day discrepancy between cached GitHub release surfaces showing `1.0.103`/`1.0.104-*`; treat exact latest patch as a release-feed race rather than a format guarantee. citeturn14search1turn19view2 |
| Cursor | rolling CLI/Desktop | No stable public transcript-format tag was exposed in the official material examined; persistent CLI sessions were introduced in the 2026-08-26 CLI changelog. citeturn3search4 |
| Gemini CLI | `v0.65.0` | Published 2026-10-20. fileciteturn40file0L1-L2 |
| Qwen Code | CLI `0.26.1` | Official SDK release on 2026-10-26 states that it bundles CLI `0.26.1`; source snapshot examined at `57f4bd142467…`. fileciteturn0file0L1-L2 |
| Kimi Code | `@moonshot-ai/kimi-code@2.3.3` | Published 2026-10-26. fileciteturn41file0L1-L2 |
| Mistral Vibe | `v3.0.3` | 2026-10-26, source examined around commit `7bb48f213a…`. citeturn16search1 |
| Kilo Code | `v7.8.18` | 2026-10-26 in the official release stream. citeturn17search0 |
| Cline | `v4.1.26` | Published 2026-10-26. fileciteturn25file0L1-L2 |
| Roo Code | `v3.54.0` | Final verified release, 2026-05-15, commit `27001b2b5aa47b65e8a6ba1914e0f4216be0ebb0`; repository is archived. fileciteturn29file0L1-L2 |
| Amp | rolling service/CLI | No public semver transcript contract verified; Threads are the documented persistence abstraction. citeturn7search4 |
| pi | rolling/open-source line | Session format is publicly documented, but I did not obtain an authoritative latest-release tag during this pass; version is therefore intentionally left unclaimed. citeturn7search5turn7search17 |
| Oh My Pi | rolling | Current source/docs verified; exact latest tag not sufficiently verified to claim. |
| Grok CLI / Grok Build | rolling | Official repository did not expose a normal GitHub stable-release series in the material examined. citeturn10search0 |
| Antigravity | app `2.22.0`; CLI `1.2.12` latest verified in official changelogs | These are the latest first-party versions located, not a claim that no later private rollout existed on 2026-10-27. citeturn11search23 |
| Hermes Agent | `v0.22.5` | Release date stated as 2026-10-22; GitHub publication 2026-10-23; commit `bf0203a794a…`. fileciteturn30file0L1-L2 |
| OpenHands | `v1.33.0` | Published 2026-10-23, target commit `b968abef4779…`. fileciteturn34file0L1-L2 |
| Devin | rolling service/CLI | No public stable transcript-schema tag. |
| MastraCode | rolling package/harness | Exact latest package version was not verified strongly enough to pin here. |
| Factory Droid | rolling service/CLI | No public durable-transcript schema version located. |
| Windsurf / Cascade | rolling Devin Desktop/Windsurf product | No first-party transcript-file version contract. citeturn14search14 |
| Continue | rolling OSS client line | Source-level session format verified; exact latest package tag was not pinned in this pass. citeturn19view3 |

A notable source contradiction concerns **OpenCode**: GitHub’s conventional `/releases/latest` endpoint returned the legacy `v1.19.4` channel, while the repository’s explicit “OpenCode V2” changelog contains `v2.0.x`, `v2.1.x`, and `v2.2.0`. For the requested V2 analysis, the V2 changelog and V2 source are the relevant authority. fileciteturn1file0L1-L2 fileciteturn3file0L1-L2

## Local and open-format harness dossiers

**Claude Code — `v2.1.311`**

**1 — Storage. [D]** Local Claude Code conversations live under the user’s Claude data tree, with project-scoped transcript files under `~/.claude/projects/<project-key>/`; official issue material identifies the durable transcripts as `.jsonl`. Subagent metadata is stored beneath the session directory, including `~/.claude/projects/{project}/{sessionId}/subagents/agent-{agentId}.meta.json`. Home-relative Windows expansion is therefore normally `%USERPROFILE%\.claude\...`; that Windows spelling is an OS-path inference, not a separately documented Anthropic contract. citeturn13search30turn0search15

**2 — Format. [D/Gap]** The durable transcript is JSONL, but Anthropic does **not** publish a stable, versioned Claude Code transcript schema comparable to an SDK wire schema. Official material confirms transcript JSONL but does not make every row type/field an interoperability contract. Consequently a faithful converter should parse known rows permissively and preserve unrecognized objects verbatim rather than reject them. A first-party “minimal real transcript fixture” with prompt → tool call → result → answer was not located, so none is fabricated here. citeturn13search30

**3 — Conversation model. [D/I]** Claude Code supports user/assistant content, tool interactions, checkpoints and subagents; however, the exact on-disk representation of every content block, image/file attachment, tool input and reasoning block remains undocumented as a public compatibility surface. Tool names should therefore be preserved as opaque identifiers with their original input object rather than mapped solely from UI labels. citeturn0search15turn0search12

**4 — Structure. [D]** Checkpointing is first-class: Claude Code saves a checkpoint before user prompts, keeps roughly the most recent 100 checkpoints, and `/rewind` can return to them; checkpoints survive session resume. `/clear` has different semantics and resets the pointer used for previous-session continuation. Subagents have their own session-associated metadata. citeturn0search12turn0search15

**5 — Turns. [D/Gap]** The UI/runtime distinguishes ordinary completion, cancellation/interruption and rewound history, but no public transcript-schema document defines a universal row-level `turn_complete` predicate for third-party writers. `v2.1.311` specifically includes fixes around transcript ordering and “taken back”/rewound messages, which is a warning not to infer complete turns merely by alternating `user`/`assistant`. citeturn1view0

**6 — Compaction. [D]** Current Claude Code compacts long histories, including contexts above 200K, and release notes expose `DISABLE_COMPACT` as a control. Because checkpoints and compaction coexist, the visible transcript and the model-facing resumed prefix need not be equivalent. A converter should retain both original historical rows and any summary/compaction artifact it recognizes. citeturn1view0

**7 — Reasoning. [Gap]** Claude Code’s provider can use signed/opaque Anthropic thinking state, but Anthropic has not documented the local Claude Code transcript as a third-party reasoning-replay interchange format. Do not synthesize, modify, concatenate or strip opaque reasoning/signature fields encountered in a transcript unless the target harness explicitly supports their semantics.

**8 — Metadata. [D]** Session identity and project association are required to support resume/checkpoint/subagent behavior. Exact durable field names for all of `cwd`, git branch, model, client version, title, token use and cost are not guaranteed by a public transcript schema, so these should be treated as version-observed rather than contractually stable. citeturn0search15turn13search30

**9 — Resume/import/export. [D]** `claude --resume` resumes a selected/specified session and `claude --continue` resumes the most recent relevant session; Desktop integration can open a local session via `claude --desktop --resume <id>` and supports continuation. There is no documented official mechanism that accepts an arbitrary third-party-authored Claude Code JSONL as a validated import format. citeturn0search0turn0search21

**10 — Stability. [D/I]** Treat the JSONL as an **internal persistence format**, not a versioned external protocol. `v2.1.311` changed transcript/rewind/compaction behavior, making raw field-copying safer than aggressive normalization. citeturn1view0


**OpenAI Codex — CLI, IDE extension and desktop/app clients; stable `0.164.0`**

**1 — Storage. [C/D]** The local Codex state root is `CODEX_HOME`, normally `~/.codex`. Local rollouts are kept beneath the session store, while `session_index.jsonl` is an append-only name catalog. Current installations also use a SQLite state database (`state_*.sqlite` in observed official code/issues); the rollout JSONL remains significant for thread replay. The session-index implementation explicitly defines `SESSION_INDEX_FILE = "session_index.jsonl"` and an adjacent lock file. citeturn0search2 fileciteturn39file0L1-L6

**2 — Format. [C]** The name catalog contains:
`{ id: ThreadId, thread_name: String, updated_at: String }`.
Updates are append-only; reverse scans make the newest usable name win. Rollout files themselves are JSONL protocol records, while the newer thread/state layer also projects metadata into SQLite. fileciteturn39file0L1-L6

**3 — Conversation model. [C/D]** Codex rollouts contain protocol items rather than only rendered text: user input, assistant/model output, function/tool calls and outputs, lifecycle events and thread/session metadata. The app-server exposes thread-level APIs, which is why CLI, IDE and desktop/app clients can share thread semantics even when their UI differs. citeturn0search14

**4 — Structure. [C/D]** A thread has a durable `ThreadId`; the thread store resolves persisted rollout paths and can resume them. Name resolution is deliberately many-to-one over historical index entries and checks that a candidate rollout actually exists and is loadable. App-server resume behavior has had bugs where a resume could accidentally create/fork a new thread, so `id` equality—not title—is the canonical identity check. fileciteturn39file0L1-L6 citeturn0search8

**5 — Turns. [D]** Current Codex distinguishes running/recoverable turns from completed turns; recent releases added running-turn recovery and fixes for resumed compressed histories. Converters must therefore retain unfinished output/events rather than treating EOF as “completed”. citeturn2view0

**6 — Compaction. [D]** Codex persists compressed/compacted conversation state and can resume such histories. Recent releases changed partial-summary handling and saved thread settings around compaction; a summary is not simply a visual assistant message. citeturn2view0

**7 — Reasoning. [I/Gap]** The Responses-style model event stream can contain reasoning-related items, but local Codex persistence should be regarded as the source of truth for opaque model items. A converter must not assume that plain text extracted from reasoning is sufficient to replay a signed/encrypted provider item. There is no documented guarantee that arbitrary third-party-generated reasoning blobs are accepted on resume.

**8 — Metadata. [C/D]** At minimum thread ID, session source, rollout path and optional model provider are persisted/resolved; current code filters session candidates by `SessionSource` and optional `model_provider`. Names are external to the rollout in `session_index.jsonl`. fileciteturn39file0L1-L6

**9 — Resume/import/export. [D/C]** CLI resume accepts thread/session identity, and current versions support name-based resume as well as IDs. App-server exposes thread resume/list operations for GUI clients. There is no public guarantee that writing a hand-authored rollout JSONL plus an index row is a supported “import”; multiple consistency layers—rollout metadata, index, SQLite/thread store—can be involved. citeturn0search5turn0search14

**10 — Stability. [D]** `0.164.0` is stable as of 23 October 2026, but the source tree had already advanced beyond it by 27 October. Recent releases modified compaction, running-turn recovery, saved thread settings and subagent resume, so converter code should be version-dispatched. fileciteturn36file0L1-L2


**OpenCode — requested V2 lines `2.0.32` and `2.1.4`**

**1 — Storage. [D/C]** V2 uses SQLite. The usual release DB is `~/.local/share/opencode/opencode.db`; `XDG_DATA_HOME`, release-channel naming and `OPENCODE_DB` can move/change it. `opencode db path` reports the active DB. Local development uses a channel-specific/local database rather than blindly sharing production state. fileciteturn7file3L35-L44 fileciteturn7file6L73-L82 fileciteturn7file9L106-L115

Current schema tables relevant to converters include:

`session_v2`, `session_message`, `session_inbox`, `instruction_entry`, `instruction_blob`, `instruction_state`, `blob`, and `blob_ref`. `session_v2` has indexes on project, workspace, parent and suspended/running-state columns; `session_message` has a unique `(session_id, seq)` ordering index plus type/time indexes. fileciteturn10file0L1-L6

**2 — Format. [C]** `session_v2` fields include `id`, `project_id`, `workspace_id?`, `parent_id?`, `fork_session_id?`, `fork_boundary?` JSON, `slug`, `directory`, `path?`, `title?`, `version`, `share_url?`, summary counters/diffs, `metadata`, `cost`, token counters, `revert`, `permission`, `agent`, `model`, timestamps, idle/compaction/archive/suspension state and `resume_attempts`. `session_message` stores `id`, `session_id`, `type`, `seq`, timestamps and JSON `data`; `session_inbox` stores typed JSON `payload` plus delivery/enqueue sequence. fileciteturn10file0L1-L6

This is substantially safer for converters than reverse-engineering display JSON: preserve `seq`, `type`, `data`, and session-level state exactly.

**3 — Conversation model. [C]** Messages are a typed union serialized into `session_message.data`; tool calls, model content, user content and provider-specific parts are represented inside that union. Newer V2 also has durable media/blob tables, so a content reference can be external to the JSON column. `v2.2.0` added experimental content-addressed session-media blob storage, directly demonstrating that attachment assumptions from 2.0.x are unsafe for future conversion. fileciteturn10file0L1-L6 fileciteturn3file0L1-L2

**4 — Structure. [C]** Threading is explicit: `parent_id`, `fork_session_id`, and JSON `fork_boundary`. Reversion is separate JSON state (`revert`). Message order is `seq`, not timestamp. The inbox has its own `enqueued_seq`. A converter that sorts by timestamp can therefore corrupt canonical ordering. fileciteturn10file0L1-L6

**5 — Turns. [C/I]** Session-level `time_idle`, `idle_outcome`, `time_compacting`, `time_suspended` and `resume_attempts` distinguish lifecycle states; message `type`/`seq` provides event order. There is no safe rule “last assistant row = complete turn”; current V2 explicitly supports running sessions and restoration while SDK clients reconnect. fileciteturn10file0L1-L6 fileciteturn3file0L1-L2

**6 — Compaction. [D/C]** `2.1.4` introduced optional `experimental.background_compaction`; `2.2.0` starts model history from the latest completed compaction. Thus compaction is a durable history boundary, not only a UI summary. Session instructions can survive compaction/revert independently of ordinary messages. fileciteturn3file0L1-L2

**7 — Reasoning. [C/D]** The session aggregates `tokens_reasoning`, while provider-specific message content carries the actual model data. OpenCode has had provider-specific fixes involving reasoning/signature behavior; this is exactly the case where preserving opaque content is superior to converting reasoning into a text message. fileciteturn10file0L1-L6

**8 — Metadata. [C]** This is one of the richest verified schemas in the survey: cwd/project association (`directory`, `project_id`, optional workspace), model `{ id, providerID, variant? }`, version, title, share URL, cost, five token counters, file-diff summary, timestamps, permissions and agent identity are all first-class persisted fields. fileciteturn10file0L1-L6

**9 — Resume/import/export. [D]** V2 exposes official session import/export rather than requiring raw DB mutation; official CLI docs support export/import of OpenCode session data. Third-party DB writing is still unsafe because foreign keys, sequence uniqueness, migration state and auxiliary tables must agree. citeturn5search1

**10 — Stability. [D/C]** `2.0.32` is the last verified 2.0.x and `2.1.4` the last 2.1.x; V2 source contains a dedicated V1 migration path (`packages/core/src/database/v1-migration.bun.ts`). Immediate changes in `2.2.0`—media blobs, instruction persistence, compaction-start logic and cost reporting—show that the V2 schema is actively evolving. fileciteturn9file8L143-L154 fileciteturn3file0L1-L2


**GitHub Copilot CLI**

**1 — Storage. [D]** New sessions are stored under `~/.copilot/session-state`; legacy sessions use `~/.copilot/history-session-state` and are migrated when resumed. Within a session, `events.jsonl` is the durable event log, alongside workspace artifacts such as plans/checkpoints/tracked-file state. fileciteturn11file0L1-L18 citeturn19view0

**2 — Format. [D/Gap]** `events.jsonl` is JSONL and the new format was deliberately introduced to decouple persistence from timeline rendering. Session-usage metrics—requests, tokens and code-change metrics—are appended after a session ends. GitHub does not publish a stable complete event-union schema for third-party writers, so the log should be treated as an internal event stream. fileciteturn11file0L1-L18

**3 — Conversation model. [D]** Tool/event storage is richer than rendered chat, and workspace artifacts live outside the chat event stream. Preserve event payloads and file/checkpoint artifacts as separate namespaces.

**4 — Structure. [D]** The migration from old to new session format happens on resume. No robust public branch/fork file contract was verified; do not derive branches from duplicated message prefixes.

**5 — Turns. [D/I]** A session-end event/closure is semantically different from a partial event log—usage metrics are persisted after session termination. EOF alone is not sufficient to classify a complete session. fileciteturn11file0L7-L18

**6 — Compaction. [D]** Copilot CLI supports effectively long/infinite sessions using intelligent compaction/summarization. Therefore resumed context can differ from the entire visible event history. citeturn14search20

**7 — Reasoning. [Gap]** No public stable on-disk reasoning-signature contract was located. Preserve unknown model events losslessly.

**8 — Metadata. [D]** Local session state is supplemented by usage metrics and can also be synchronized to the user’s GitHub account by default; the canonical state can therefore span local persistence and account-backed synchronization. citeturn19view1

**9 — Resume/import/export. [D]** `--resume` accepts a session identifier/prefix/name and has a picker form; `--continue` is a separate mode and conflicts with resume selection. Official session-management surfaces include naming/sharing/history-related operations. There is no documented arbitrary `events.jsonl` import contract. citeturn14search12turn14search0

**10 — Stability. [D]** The recent “new session logging format” is itself an explicit compatibility break. Converters need at least legacy `history-session-state` and current `session-state` readers. fileciteturn11file0L7-L18


**Gemini CLI — `v0.65.0`**

**1 — Storage. [C/D]** Session recording is file-based, under the Gemini CLI home/project storage; `GEMINI_CLI_HOME` is honored by official code/tests. Session files use a `session-` prefix and are held in project `chats` storage. fileciteturn16file3L49-L65 fileciteturn17file1L16-L34

**2 — Format. [C]** Current recording is **append-oriented JSONL**. Core types are unusually well specified in source:

`BaseMessageRecord = { id, timestamp, content, displayContent? }`

`ToolCallRecord = { id, name, args, result?, status, timestamp, agentId?, displayName?, description?, resultDisplay?, renderOutputAsMarkdown? }`

Gemini messages add `toolCalls?`, `thoughts?`, `tokens?`, `model?`. Conversation metadata has `sessionId`, `projectHash`, `startTime`, `lastUpdated`, `messages`, optional `summary`, `memoryScratchpad`, `directories`, and `kind: "main"|"subagent"`. Token data includes `input`, `output`, `cached`, `thoughts?`, `tool?`, `total`. fileciteturn15file0L1-L13

Record-level deltas are also explicit:

`{ "$rewindTo": "<message-id>" }`

`{ "$patch": { id?, content?, toolCalls?, updates?, removeIds?, orderIds? } }`

and `{ "$set": ... }` metadata updates. fileciteturn15file0L1-L13

No first-party captured user session fixture was located; the types above are the verified source of the minimal shape rather than a fabricated “real” transcript.

**3 — Conversation model. [C]** Content uses Google GenAI `PartListUnion`, so text and multimodal/provider content remain structured rather than flattened. A Gemini message may combine text, tool calls, thought summaries and model/token metadata. Tool result is paired directly by `ToolCallRecord.id`. Multiple tool calls can be stored in one Gemini message, so parallel calls must not be serialized into fake alternating assistant/tool messages. fileciteturn15file0L1-L13

**4 — Structure. [C]** Physical JSONL is append-oriented but logical history is reconstructed by applying rewinds, message patches, deletions and ordering patches. `ChatRecordingService` maintains the effective message map and on `$rewindTo` removes the selected message and subsequent logical history. Physical file order is therefore **not equal to current conversation history**. fileciteturn14file0L1-L2

**5 — Turns. [C/D]** Tool records carry scheduler `status`; user/Gemini records are separately typed. Release `0.65.0` fixed duplicate tool-response turns on resume, prevented deletion of resumed history on quick exit, enforced a terminal user-turn invariant, and fixed poisoning after rapid Ctrl-C—strong evidence that interrupted/incomplete history requires explicit normalization during resume. fileciteturn40file0L1-L2

**6 — Compaction. [C/D]** Conversation metadata supports `summary`; recording has been changed to append-only delta patching with bounded history windows for long sessions. A converter must preserve summary metadata independently of ordinary assistant text. fileciteturn15file0L1-L13 fileciteturn13file7L91-L100

**7 — Reasoning. [C]** There are two distinct things not to conflate: `thoughts` contains timestamped `ThoughtSummary` objects for the session/UI, while `content: PartListUnion` can carry provider-native parts, including thought-signature-bearing parts. The converter should preserve both. `MAX_TOOL_OUTPUT_SIZE` is 50 KiB in the recording types, another reason not to assume display/result data is lossless raw process output. fileciteturn15file0L1-L13

**8 — Metadata. [C]** `sessionId`, project hash, timestamps, model per Gemini message, token summary, workspace directories and main/subagent kind are durable. Cwd is represented indirectly by project hashing/storage and persisted workspace directory additions. fileciteturn15file0L1-L13

**9 — Resume/import/export. [D/C]** The current UI has session browsing/resume support and explicit rewind support; `ResumedSessionData` pairs a parsed `ConversationRecord` with its source file path. A malformed or unreadable session can return `null`; hand-written third-party JSONL therefore has to satisfy the reconstruction logic, not only JSON syntax. fileciteturn15file0L1-L13 fileciteturn13file4L47-L56

**10 — Stability. [D]** The recording format is actively moving: full-history checkpoints are explicitly marked deprecated in favor of append-only `MessageRecord`/patch records, and `0.65.0` contains several resume fixes. A converter should detect both legacy `$set.messages` snapshots and current delta logs. fileciteturn15file0L1-L13


**Qwen Code — CLI `0.26.1`**

**1 — Storage. [C/D]** Qwen Code uses durable active/archive **JSONL transcripts**, with daemon lifecycle code able to locate, resume, load, archive and hard-delete both active and archived forms. Exact cross-platform default root was not extracted with sufficient confidence in this pass; use Qwen’s own path resolver rather than hard-coding an inherited Gemini path. fileciteturn18file0L1-L16

**2 — Format. [C]** `ChatRecord` is a parent-linked JSONL record with:
`uuid`, `parentUuid`, `sessionId`, `timestamp`, `type`, optional `subtype`, optional `provenance`, plus type-specific content. Types are `user`, `assistant`, `tool_result`, `system`. The source comment explicitly describes the format as append-only and tree-reconstructable. fileciteturn20file0L1-L2

System subtypes include, among others, `chat_compression`, `custom_title`, `parent_session`, `session_source`, `session_model`, `session_approval_mode`, `rewind`, `file_history_snapshot`, `branch_checkpoint`, `goal_state`, `goal_runtime`, `goal_turn_end`, `turn_result`, `turn_attempt`, managed-session records, `agent_mention` and `agent_message`. These are **not all conversational messages**. fileciteturn20file0L1-L2

**3 — Conversation model. [C]** Tool-result rows have their own record type and provenance; model-facing content uses GenAI-compatible structured parts. The recorder sanitizes oversized file-diff/display payloads for persistence: large diffs/content can be replaced with preview/truncation metadata. Therefore recorded `resultDisplay` is not always the raw tool output. fileciteturn20file0L1-L2

**4 — Structure. [C/D]** `uuid/parentUuid` forms a tree and was explicitly designed to support branching. Current UI branch handling flushes/snapshots the outgoing recorder, forks the JSONL and assigns a branch title. Rewinds and branch checkpoints are explicit system records. fileciteturn20file0L1-L2 fileciteturn18file1L17-L32

**5 — Turns. [C]** `turn_attempt` and `turn_result` subtypes exist in addition to ordinary user/assistant/tool rows, so they are the safer place to infer lifecycle than message alternation. Goal-runtime records and external-agent records must not be mistaken for a new human turn. fileciteturn20file0L1-L2

**6 — Compaction. [C]** `chat_compression` is an explicit system record. The source documentation says system records can change how model history is reconstructed while retaining original UI history. This is one of the clearest examples of why converters should separately model **visible history** and **resume context**. fileciteturn20file0L1-L2

**7 — Reasoning. [C/I]** Provider content is structured and can carry provider-specific reasoning state. Do not rewrite opaque part metadata. The recorder’s own preview truncation rules apply to display/tool data, not permission to mutate provider reasoning.

**8 — Metadata. [C]** Session ID, timestamp, git context helpers, auto-title state, model/session-model events, approval mode, source snapshots, provenance, file-history snapshots and attribution snapshots are represented in the session system. fileciteturn20file0L1-L2

**9 — Resume/import/export. [D]** ACP supports `newSession`, `unstable_resumeSession` and `loadSession`; current release history includes resume/continue SDK support. An official export command was added for session history in Markdown and HTML. fileciteturn18file0L1-L16 fileciteturn0file0L1-L2

**10 — Stability. [C/D]** This format has evolved far beyond the Gemini recorder it historically resembles: branching, goal protocol, managed-session events, multi-agent rows and execution-engine metadata are Qwen-specific. Treat unknown `subtype` values as forward-compatible opaque records. fileciteturn20file0L1-L2


**Kimi Code — `2.3.3`**

**1 — Storage. [D]** Canonical layout is:

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

`KIMI_CODE_HOME` moves the root. This home-relative design applies across OSes; use the environment variable/path API rather than spelling a platform directory manually. fileciteturn22file0L1-L13

**2 — Format. [D]** `state.json` is session metadata; each `wire.jsonl` is the durable agent event/request stream. Crucially, the wire also records request traces including tool schemas, model request parameters and MCP tool listings. That makes Kimi particularly suitable as a source format for high-fidelity converters—but only if the wire is preserved, not rendered Markdown. fileciteturn22file0L1-L13

**3 — Conversation model. [D]** Main agent and each subagent have separate wire logs. The trace can preserve the exact tool declarations sent to the model, solving the otherwise difficult “what schema did this tool name mean at that turn?” problem. Recent Kimi versions also externalize large base64 media payloads into blob files rather than leaving them inline in `wire.jsonl`. fileciteturn22file0L1-L13 fileciteturn21file3L71-L89

**4 — Structure. [D]** `/fork` creates an independent session copy while leaving the user in the original. Subagent histories remain separately addressable. A saved `/goal` is deliberately not copied into the fork, so session cloning is not a byte-for-byte duplication of all higher-level runtime state. fileciteturn22file0L1-L13

**5 — Turns. [D/I]** `wire.jsonl` is an event stream, so completion must be reconstructed from event semantics rather than assuming every assistant record closes a turn. The session restore mechanism consumes the wire itself.

**6 — Compaction. [D]** Kimi automatically compresses history near the context limit; `/compact [hint]` can force it. If issued during a running agent, compaction is queued; duplicate queued compactions are rejected, and a queued manual compaction can be skipped if auto-compaction already occurred. These distinctions should survive conversion where possible. fileciteturn22file0L1-L13

**7 — Reasoning. [I/Gap]** Provider-native reasoning can appear in wire-level model traffic, but there is no blanket guarantee that it can be transferred to a different provider. Preserve the raw event/request content; do not turn it into plain assistant text unless the target cannot accept it.

**8 — Metadata. [D]** `state.json` contains title/creation metadata; `session_index.jsonl` indexes sessions by workspace; the storage key includes working directory, and request traces contain model/tool/request context. fileciteturn22file0L1-L13

**9 — Resume/import/export. [D]** Official commands are comprehensive:

`kimi --continue`  
`kimi --session <id>`  
`kimi --session` picker  
`/sessions` or `/resume`  
`/fork`  
`kimi export <sessionId> [-o path]` → diagnostic ZIP  
`/export-debug-zip` → ZIP  
`/export-md` / TUI `/export` → Markdown  
`kimi import-sessions` / `/import-sessions` → import Claude Code and Codex sessions. fileciteturn22file0L1-L13

The importer is especially instructive for converter authors: imported Claude/Codex material becomes a regular Kimi session containing a **model-facing condensed import message**, while the complete history and copies of original source files are retained separately. This is a deliberate lossy-runtime/lossless-archive split. It operates locally and does not call a model. fileciteturn22file0L1-L13

**10 — Stability. [D]** Kimi explicitly warns not to manually edit files under `sessions/` because doing so can make restoration fail. Large media recently moved from inline wire data to blob storage, so converters must copy the complete session directory, not merely `wire.jsonl`. fileciteturn22file0L1-L13 fileciteturn21file3L71-L89


**Mistral Vibe — `v3.0.3`**

**1 — Storage. [D/C]** Local Vibe sessions default to `~/.vibe/logs/session/`; configuration `session_logging.save_dir` moves them. Stored metadata includes cwd, enabling workspace-scoped resume. citeturn6search24

**2 — Format. [C]** Vibe 3 uses a “Unified Session Store”; source defines `UnifiedSessionStore`, while legacy-import tests show the preceding layout used `meta.json` plus a line-oriented message file. This is a migration boundary converters need to detect. fileciteturn24file0L1-L13 fileciteturn23file2L37-L63

**3 — Conversation model. [C/D]** The current harness is designed around durable session events/messages and tool lifecycle; reasoning is surfaced as “Thought” in Vibe’s UI. Exact low-level current record union was not fully extracted here, so unsupported record kinds should remain opaque.

**4 — Structure. [D]** Vibe supports subagents, and session recovery is designed around durable local state rather than rendered terminal text. Hook events include `session_id`, `parent_session_id`, `transcript_path` and `cwd`, making parent/subsession relationships available at runtime. fileciteturn23file0L1-L17

**5 — Turns. [D]** Vibe 3.0.x release notes specifically fixed sessions saved **during a tool call** that earlier Vibe 3.0.0 could consider invalid. Therefore mid-tool persistence is a real valid state and must not be normalized into a completed turn. citeturn16search1

**6 — Compaction. [D]** Vibe 3 includes context compaction; the active context can be reduced while durable session history remains available. Compression must be represented separately from deletion. citeturn16search1

**7 — Reasoning. [D/Gap]** Reasoning/Thought display is supported, but no stable cross-provider signed-reasoning interchange contract was found. Preserve provider payloads if available.

**8 — Metadata. [D/C]** Session ID, parent session ID, cwd and transcript path are explicit in hook events. fileciteturn23file0L1-L17

**9 — Resume/import/export. [D]** `/resume`, `--continue`, and bare `--resume` are official session-management paths; resume can be scoped by folder. No arbitrary foreign-session file import contract was verified. citeturn6search24

**10 — Stability. [D]** The transition from older metadata/messages persistence to the Unified Session Store, plus the 3.0.x recovery fix, makes Vibe a version-dispatch case. Do not write legacy `meta.json`/messages and assume V3 will treat it as native without its importer. citeturn6search8turn16search1


**Cline — `v4.1.26`**

**1 — Storage. [D]** The current CLI and VS Code extension do **not** simply share one storage tree. Official issue material identifies CLI data beneath `~/.cline/data/` (Windows `%USERPROFILE%\.cline\data`) and VS Code task data under the extension’s global storage, conventionally:

- Windows: `%APPDATA%\Code\User\globalStorage\saoudrizwan.claude-dev\tasks`
- macOS: `~/Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/tasks`
- Linux: `~/.config/Code/User/globalStorage/saoudrizwan.claude-dev/tasks`.

citeturn6search14

**2 — Format. [D/C]** Local task folders include JSON conversation state such as `api_conversation_history.json` plus UI/task state. The history index can be reconstructed from task folders, proving that the per-task folder is more canonical than the sidebar index. citeturn6search18turn6search6turn6search26

**3 — Conversation model. [D]** API conversation history and UI messages are separate concerns. Tool interactions, user-visible presentation and provider API history should not be collapsed into one transcript representation.

**4 — Structure. [D]** Local tasks are separate units; current product also has cloud sessions/sandboxes. Local and cloud tasks are distinct modes and should not be assumed to share a filesystem schema. `v4.1.26` explicitly introduced/expanded Local/Cloud session behavior. fileciteturn25file0L1-L2

**5 — Turns. [D]** `v4.1.26` fixed a case where a connection dropped after streaming only reasoning: such a turn is now retried rather than ending the task. It also fixed cancellation/session-switch races around pending questions. Those are strong reasons to track an explicit incomplete/interrupted state in an intermediate converter model. fileciteturn25file0L1-L2

**6 — Compaction. [D]** Auto-compaction exists; `v4.1.26` fixed repeated compaction and disabled it for models with unknown context limits. fileciteturn25file0L1-L2

**7 — Reasoning. [D/I]** Reasoning can be streamed before ordinary answer text and affects retry semantics. Provider-specific reasoning fields should remain opaque; a “reasoning-only partial turn” must not be rewritten as a completed assistant response. fileciteturn25file0L1-L2

**8 — Metadata. [D]** Task history/index state and per-task conversation files provide task IDs, timestamps/history information and API/provider state; cloud sessions add repository/branch/sandbox progress metadata. fileciteturn25file0L1-L2

**9 — Resume/import/export. [D]** Cline provides task history and a `Reconstruct Task History` command to rebuild its index from persisted task folders. That is not the same as a generic foreign-session importer. Cloud sessions have their own resume path. citeturn6search6turn6search26

**10 — Stability. [D]** Current Cline has diverged substantially from older Cline-family layouts because CLI and cloud sessions now coexist. A converter should identify **extension-local**, **CLI-local**, and **cloud** tasks separately. fileciteturn25file0L1-L2


**Roo Code — final `v3.54.0`**

**1 — Storage. [D]** Roo Code’s VS Code extension uses its own global storage; task data lives under the Roo extension global-storage task directory (`rooveterinaryinc.roo-cline/.../tasks`), not Cline’s current namespace. citeturn6search3

**2 — Format. [C/D]** Like its Cline ancestry, Roo maintains task history plus API conversation material in task-level files. The source’s `ClineProvider` restores task stacks/history rather than deriving state solely from rendered chat. citeturn6search23turn6search7

**3 — Conversation model. [C]** Provider/API messages and UI task state are separate. Tool-call serialization follows Roo’s own historical Cline-family protocol and should not be assumed compatible with current Cline.

**4 — Structure. [C]** Roo supports task-stack/delegation semantics in its provider code, so child/delegated work needs separate representation rather than flattening into the parent transcript. citeturn6search23

**5 — Turns. [Gap]** No public stable row-level turn-finalization contract was located.

**6 — Compaction. [Gap]** Context-management behavior exists historically, but a complete final-version persistence contract for summaries was not verified.

**7 — Reasoning. [Gap]** No stable cross-provider persisted reasoning contract verified.

**8 — Metadata. [D]** History and task-level global state are both used; an index alone is insufficient for conversion. citeturn6search7

**9 — Resume/import/export. [D/Gap]** Native task reopening/history is supported; no generic third-party file import contract verified.

**10 — Stability. [D]** The official repository’s final release is `v3.54.0`, published 15 May 2026 at commit `27001b2…`, and the repository subsequently became read-only/archived. That makes its final schema frozen in practice but not formally standardized. fileciteturn29file0L1-L2


**Continue**

**1 — Storage. [C/D]** Continue uses a global sessions directory, normally `~/.continue/sessions`; `CONTINUE_GLOBAL_DIR` relocates the global root. Session files are `<sessionId>.json` and a `sessions.json` catalog indexes them. The same global Continue data is relevant to IDE/CLI history rather than each client inventing a completely separate transcript format. citeturn15search9turn15search6

**2 — Format. [C]** `core/util/history.ts` reads/writes JSON session objects. The save path serializes fields including:

`sessionId`, `title`, `workspaceDirectory`, `history`, optional `mode`, optional `chatModelTitle`, optional `usage`.

The catalog tracks metadata such as session ID/title/date/workspace and message count. Legacy objects with older `session_id` naming are explicitly detected, giving a concrete migration case. citeturn19view3

**3 — Conversation model. [C/Gap]** `history` is the durable conversation container, but its nested history-item schema varies with Continue features and versions. Preserve each history object in full; do not rebuild it only from `role/content`.

**4 — Structure. [D]** `/fork` exists in the CLI alongside `/resume`, so branching can create a new session-level object. Do not represent the fork only as an in-place edit of the parent. citeturn15search1

**5 — Turns. [I]** The catalog’s message count is derived from history rather than being an authoritative completion marker; a final partial model/tool state can therefore exist independently of that count.

**6 — Compaction. [D]** `/compact` is a first-class CLI operation. Treat the resulting summarized context as resume state, not as evidence that older durable history may be discarded. citeturn15search1

**7 — Reasoning. [Gap]** Exact provider-specific reasoning persistence was not verified as a stable schema. Preserve unknown nested history fields.

**8 — Metadata. [C]** Title, workspace directory, model title, mode and usage are first-class save fields. citeturn19view3

**9 — Resume/import/export. [D]** `/resume`, `/fork`, `/compact` and title/rename operations are documented. No stable generic import/export file command comparable to Kimi’s importer was verified; direct JSON creation is therefore an internal-format operation rather than an official import API. citeturn15search1

**10 — Stability. [C]** The loader already contains compatibility handling for historical field names, so converters should parse old and new session forms rather than assume one immutable JSON schema. citeturn19view3


**pi**

**1 — Storage. [D]** pi automatically saves sessions unless `--no-session` is used; a configurable session directory is supported, and `--session-dir` has explicit precedence in the official implementation/issues. citeturn7search5turn8search13

**2 — Format. [D]** pi’s official session-format documentation describes a JSONL session/tree representation rather than a flattened transcript. citeturn7search17

**3 — Conversation model. [D]** Model messages, tool interactions and control entries are separate durable session entries. The correct converter strategy is to preserve entry `type` and provider payload rather than force everything into OpenAI-style `{role,content}`.

**4 — Structure. [D]** pi sessions are tree-oriented and support navigation/branching semantics; resume is not merely “read every JSONL line in physical order.” citeturn7search17

**5 — Turns. [Gap]** Exact current completion/interruption row predicates were not captured strongly enough here to publish as a converter contract.

**6 — Compaction. [D]** Compaction/branch summaries are represented as session control information in the documented format rather than only visually.

**7 — Reasoning. [I]** Provider-native assistant content should be preserved; no generic guarantee exists that reasoning from one model/provider can be replayed to another.

**8 — Metadata. [D]** Session identity, parent linkage, timestamps and session labels/names are part of the durable model. citeturn7search17

**9 — Resume/import/export. [D]** `pi --continue`, `pi --resume`, `/resume`, `/new`, `/name` and `/session` are official. citeturn7search5

**10 — Stability. [Gap]** I verified the session documentation but not an authoritative 27 October release tag. Pin converters to a tested source commit/package version rather than the unqualified product name.


**Oh My Pi**

**1 — Storage. [D]** OMP’s user state is under `~/.omp`; foreign-session conversions are persisted under `~/.omp/agent/sessions/`. Runtime/session integrations also expose `PI_SESSION_ID` and `PI_SESSION_FILE`. citeturn7search3turn7search15turn7search18

**2 — Format. [D/I]** OMP is session-file based and retains pi-family semantics, but its own extensions/import adapters mean a pi reader should not blindly assume every OMP record is upstream-compatible.

**3 — Conversation model. [D]** Foreign Claude/Codex imports are converted into OMP session structures rather than merely displayed transiently. citeturn7search18

**4 — Structure. [D]** Session identity/file are explicit environment concepts; branching/navigation inherits the session-oriented model.

**5 — Turns. [Gap]** Current exact low-level completion markers were not verified separately from upstream pi.

**6 — Compaction. [I]** Preserve any pi-family compaction/summary entries and OMP-specific extension records.

**7 — Reasoning. [I]** Imported provider reasoning should be treated as opaque source payload unless OMP’s importer explicitly maps it.

**8 — Metadata. [D]** `PI_SESSION_ID` and `PI_SESSION_FILE` make active-session identity/path explicit to integrations. citeturn7search15

**9 — Resume/import/export. [D]** OMP officially supports importing Claude Code and Codex sources via `--from-claude`, `--from-codex`, and resume UI forms such as `/resume @claude` and `/resume @codex`; at the time of the cited official issue, broader Pi/OpenCode import was still being discussed. citeturn7search18

**10 — Stability. [Gap]** Exact latest tag was not pinned. Import support is an actively evolving surface, so detect capabilities by version rather than product name alone.

## Hybrid and cloud-first harness dossiers

**Kilo Code — `v7.8.18`**

**1 — Storage. [D/Gap]** Current Kilo has moved toward CLI-managed session history and hosted/cloud sessions; old assumptions based on the historical Roo/Cline VS Code global-state tree are no longer a safe contract. citeturn6search21turn17search0

**2 — Format. [Gap]** No stable public local-session DB/file schema was verified for current 7.8.x. Do not use the legacy Cline-family folder layout as a current Kilo interchange format without version detection.

**3 — Conversation model. [D]** Current releases expose sessions through Kilo’s local API and can expose hosted sessions. Tool/model state belongs to session objects rather than merely UI history. citeturn17search0

**4 — Structure. [D]** `7.8.12` introduced session creation/forking from a message using `cloneUpToMessageId`, with edit restoration. That is an explicit fork boundary and should map to a parent-session plus cutoff, not duplicated independent history with no ancestry. citeturn17search0

**5 — Turns. [Gap]** No stable public row-level turn-end schema verified.

**6 — Compaction. [D/Gap]** Context compaction exists in the agent stack, but exact durable summary representation was not verified.

**7 — Reasoning. [D]** `7.8.15` explicitly fixed persistence/replay of late OpenRouter/Gemini reasoning details. A converter that drops “late” post-finish reasoning chunks will therefore lose native Kilo semantics. citeturn17search0

**8 — Metadata. [D]** Session-hosted/cloud state includes provider/session identity and message ancestry; exact backing schema is not public.

**9 — Resume/import/export. [D/Gap]** Local API and current CLI can operate on sessions, including hosted ones. No generic documented foreign-file import contract was verified.

**10 — Stability. [D]** Kilo is a particularly dangerous target for Cline/Roo-era assumptions: the architecture and session ownership have changed. Version-check before selecting a storage adapter. citeturn6search21turn17search0


**Cursor — CLI agent and desktop**

**1 — Storage. [D/Gap]** Cursor CLI now has persistent sessions, but the official user-facing documentation examined does not specify a supported on-disk transcript path/schema for third-party writers. Desktop’s internal application state likewise is not published as a session interchange contract. citeturn3search4

**2 — Format. [D/Gap]** CLI can emit machine-readable `json`/`stream-json` output, but those output streams are not documented as the canonical persistent-session storage format. citeturn3search24

**3 — Conversation model. [D]** Cursor’s public SDK/agent bridge exposes a protobuf contract, providing a better integration surface than reverse-engineering desktop storage. citeturn3search25

**4 — Structure. [D]** Checkpoints are stored locally; persistent sessions can be listed/resumed. Public material does not give a stable local branch-tree file contract. citeturn3search8turn3search4

**5 — Turns. [Gap]** Use SDK/protocol completion events if integrating live; no verified stable file-level final-turn marker.

**6 — Compaction. [Gap]** Not sufficiently specified in public persistence docs for faithful file conversion.

**7 — Reasoning. [Gap]** No local reasoning/signature persistence contract verified.

**8 — Metadata. [D]** Session IDs/chat IDs are exposed through list/resume APIs; SDK local Agent-store methods support listing/getting/resuming session objects. citeturn3search28

**9 — Resume/import/export. [D]** `agent ls`, `agent resume`, `agent --continue`, and `agent --resume=<chat-id>` are official. A generic “import this third-party transcript file” command was not verified. citeturn3search12turn3search0

**10 — Stability. [D]** Persistent CLI sessions are relatively recent, introduced in the August 2026 line. Prefer the public SDK/protobuf abstraction over desktop DB manipulation. citeturn3search4turn3search25


**Amp**

**1 — Storage. [D/I]** Amp’s documented durable object is a **Thread**, addressable by thread ID and service APIs/CLI. No first-party canonical local transcript directory was documented in the material examined; treat the service thread as authoritative, not a local cache. citeturn7search4

**2 — Format. [D]** Official export can produce Markdown and, for the thread creator, full JSON. The export format is the supported extraction surface; local internal caches are not. citeturn7search4

**3 — Conversation model. [D]** JSON export is the right place to preserve structured thread data; Markdown is presentation-only and unsuitable for round-trip converters.

**4 — Structure. [Gap]** No local parent/branch file schema verified.

**5 — Turns. [Gap]** Use the exported/thread API representation, not inferred Markdown boundaries.

**6 — Compaction. [Gap]** Internal context management is not exposed as a documented local persistence contract.

**7 — Reasoning. [Gap]** No signed-reasoning replay contract exposed for imports.

**8 — Metadata. [D]** Thread IDs are stable user-facing identities.

**9 — Resume/import/export. [D]** Official extraction includes thread Markdown by URL/CLI and full JSON export for the creator. No general official JSON re-import into a new Amp thread was verified. citeturn7search4

**10 — Stability. [I]** Build against the official Thread API/export, not an undocumented local cache.


**Grok CLI / Grok Build**

**1 — Storage. [D]** Conversations are saved below `~/.grok/sessions/`, keyed by working directory; this applies to normal TUI/headless/ACP session handling. citeturn10search21turn10search18

**2 — Format. [D]** The core persisted stream is `updates.jsonl`: append-only NDJSON where each line is a self-contained ACP session-update event. citeturn10search1

**3 — Conversation model. [D]** ACP updates are richer than chat text and can represent tool/session/model events. Headless JSON output separately exposes final response fields such as `text`, `stopReason`, `sessionId`, `requestId`, optional `thought`, usage/model-usage and cost; do not confuse that terminal result object with `updates.jsonl`. citeturn10search4

**4 — Structure. [D]** Session ID is canonical; sandbox profile is fixed/persisted with the session. No supported arbitrary file-clone branching contract was identified. citeturn10search14

**5 — Turns. [D]** Headless `stopReason` includes values such as `end_turn` and `max_tokens`, giving an explicit completion signal for emitted results. Session-update events remain necessary for aborted/in-flight reconstruction. citeturn10search4

**6 — Compaction. [Gap]** No sufficiently detailed persistence-level compaction schema verified.

**7 — Reasoning. [D/Gap]** Headless output can contain `thought`, but this is not a guarantee that arbitrary thought text can be replayed as provider-native reasoning.

**8 — Metadata. [D]** Session/request IDs, usage, per-model usage and cost are exposed in machine-readable execution output. citeturn10search4

**9 — Resume/import/export. [D]** `/resume`, `grok --resume <id>`, `--continue`/`-r`, `grok sessions`, and `grok export` are official. An official issue states that session management did not provide a general import/create-from-file restoration path; `--session-id` creates/addresses a session but is not a JSONL importer. citeturn10search9turn10search13turn10search25

**10 — Stability. [D]** The official repo did not expose conventional stable GitHub releases in the material examined, so pin a source revision and ACP schema version when building a converter. citeturn10search0


**Antigravity**

**1 — Storage. [D/I]** Canonical conversations are backend/service objects rather than a documented local transcript database. The CLI keeps a local cache such as `~/.gemini/antigravity-cli/cache/last_conversations.json`, but that file maps workspaces to conversation IDs and the client queries the backend to validate/load them; it is an index/cache, not the transcript. citeturn11search16

**2 — Format. [Gap]** No supported local canonical conversation-file schema is published.

**3 — Conversation model. [D]** Antigravity conversations retain context and tool trajectories; migration from Antigravity Desktop into the CLI can clone conversation history/context/tool trajectories. citeturn11search16

**4 — Structure. [D]** `/rewind`, `/undo`, `/fork`, `/clear` and `/resume` are separate operations, so the service model understands branching/history editing explicitly. citeturn11search7

**5 — Turns. [Gap]** Backend protocol, not a public local file, is authoritative for running/completed state.

**6 — Compaction. [Gap]** No local public summary schema verified.

**7 — Reasoning. [Gap]** No reusable signed-reasoning file contract exposed.

**8 — Metadata. [D]** Workspace→conversation mapping is locally cached, while conversation existence/content is backend-validated. citeturn11search16

**9 — Resume/import/export. [D]** `/resume` has picker/rename/delete behavior; CLI options include continuation/conversation selection. Antigravity can import/clone Antigravity 2.0 desktop history into its newer environment; this is vendor-controlled service migration, not a general arbitrary transcript importer. citeturn11search16

**10 — Stability. [D]** App and CLI have separate version streams; latest verified official changelog entries were app `2.22.0` and CLI `1.2.12`. citeturn11search23


**Hermes Agent — `v0.22.5`**

**1 — Storage. [C/D]** Hermes has both local agent/session persistence and gateway/session mappings. Current code references JSON catalogs such as `sessions.json` and multiple SQLite databases (`state.db`, `kanban.db`, `response_store.db`, etc.) for broader durable application state. The gateway session manager loads and atomically rewrites `sessions.json`. fileciteturn31file2L57-L68 fileciteturn31file4L97-L107

**2 — Format. [C/D]** Hermes exposes session export as JSONL or Markdown, including a filtered user-prompts view. That export surface is safer for interchange than attempting to combine all internal DBs yourself. fileciteturn31file0L1-L12

**3 — Conversation model. [D]** Sessions include tool-call history; resume UI can summarize tool-call counts/names. Current Hermes additionally supports pluggable delegation/subagent backends, so subagent execution must not be assumed to be inlined text. fileciteturn30file0L1-L2

**4 — Structure. [D]** Gateway/channel sessions can map topics/chats to independent Hermes sessions. Subagent/delegation behavior is configurable per session in the current release family. fileciteturn31file9L217-L226 fileciteturn30file0L1-L2

**5 — Turns. [D]** Session operations can remove recent user turns, and current UI distinguishes restored/compacted session state. Treat turn deletion as history mutation, not a new assistant message. fileciteturn31file8L198-L208

**6 — Compaction. [D]** Hermes documentation explicitly warns that compression reduces active context but is **not a privacy deletion**. Preserve underlying durable history if the conversion target supports it. fileciteturn31file1L33-L44

**7 — Reasoning. [Gap]** Provider-specific reasoning persistence is not exposed as a stable cross-model replay contract.

**8 — Metadata. [D/C]** Sessions can have titles/names; gateway catalogs map source/channel identity to sessions; broader state is distributed among JSON and SQLite stores. fileciteturn31file1L33-L44

**9 — Resume/import/export. [D]** Hermes supports named resume/search/history, session export (`jsonl`/`md`), backup/restore tooling and `hermes import <backup.zip>`; current release also includes `hermes import-agent`. fileciteturn31file0L1-L12 fileciteturn30file0L1-L2

**10 — Stability. [D]** `v0.22.5` is a large roll-up patch containing roughly 81 merged PRs since `0.22.4`, with a fuller curated description deferred to `0.23.0`; session/delegation/import surfaces are actively moving. fileciteturn30file0L1-L2


**OpenHands — `v1.33.0`**

**1 — Storage. [D/C]** OpenHands supports pluggable persistence rather than one universal desktop transcript path. Local deployments use configured persistence/file stores, while cloud/self-hosted deployments can put session/conversation artifacts in object stores such as S3/GCS. citeturn12search1

**2 — Format. [D/Gap]** OpenHands’ durable unit is conversation/event state, not a documented universal JSONL file that every deployment shares. SDK persistence saves state changes so a conversation can be recovered. citeturn12search1

**3 — Conversation model. [D]** Conversation IDs are UUID-like durable identifiers, and persistence includes event/plugin/runtime state rather than only text. Plugin resolution itself has to survive persistence/resume. citeturn12search9

**4 — Structure. [D]** Conversation and agent runtime state are separate from UI rendering; exact branch/fork persistence varies with product layer.

**5 — Turns. [D]** App metadata includes execution/sandbox status, so running versus finished state is not inferred solely from the last message. citeturn12search8

**6 — Compaction. [D]** OpenHands has an explicit context-condenser mechanism. Converters should preserve the condensed context separately from the raw event history when the backing store retains both. citeturn12search21

**7 — Reasoning. [Gap]** No generic cross-provider signed-reasoning persistence contract verified.

**8 — Metadata. [D]** Conversation adapters expose ID, title, creation/update times, execution status, sandbox status, runtime information, cost and token usage. citeturn12search8

**9 — Resume/import/export. [D/Gap]** SDK/application persistence supports recovering conversations; no universal portable file format accepted by all OpenHands deployments was verified.

**10 — Stability. [D]** Current stable is `v1.33.0`, published 23 October 2026; because the persistence layer is pluggable, “OpenHands format” should be represented in a converter as an API/event model plus backend adapter, not one guessed disk layout. fileciteturn34file0L1-L2


**Devin**

**1 — Storage. [D]** Devin sessions are service-backed; there is no first-party documented local canonical transcript database suitable for file conversion. Cloud sessions can sleep and later resume; history survives beyond the shorter live-environment window. citeturn12search10

**2 — Format. [Gap]** No public canonical on-disk schema.

**3 — Conversation model. [D]** Sessions contain steps, tool activity and environment state; UI/CLI exposes them as service sessions rather than local JSONL.

**4 — Structure. [D]** CLI commands include `/fork [step]`, `/steps` and `/revert`, making step identity and fork point semantically important. `/btw` is a sidechain-like interaction that should not automatically be converted into the main conversation branch. citeturn12search2

**5 — Turns. [D]** Service/step state is the canonical completion model; do not infer from exported prose.

**6 — Compaction. [Gap]** Internal context management is not exposed as a portable persistence schema.

**7 — Reasoning. [Gap]** No portable reasoning-signature import contract.

**8 — Metadata. [D]** Session name/identity and cloud environment lifecycle are service metadata; session picker/history spans sessions beyond the current cwd. citeturn12search22

**9 — Resume/import/export. [D]** CLI includes continuation/resume, `/resume`, `/ls`, `/rename`, `/rm-session`, fork/revert and `/export` when export capability is enabled. Desktop/plugin UI also has history export. No arbitrary local transcript import contract was verified. citeturn12search2turn12search25

**10 — Stability. [I]** Treat Devin as an API/export target. Building a converter around browser/Desktop caches would be unsupported and fragile.


**MastraCode**

**1 — Storage. [D/Gap]** MastraCode is built on the Mastra Harness and can be instantiated with a storage implementation; session storage is therefore part of the Harness/storage abstraction rather than a single universally documented `~/.mastracode/...` transcript file. citeturn13search1turn13search5

**2 — Format. [Gap]** No single portable canonical file format verified.

**3 — Conversation model. [D]** Harness primitives include messages, streaming, tools, approvals/policies, MCP and subagents. The desktop integration constructs a Harness with agent, tools, storage, MCP and auth. citeturn13search5turn13search13

**4 — Structure. [D]** Worktrees can have isolated session/harness/state/grants, so filesystem worktree identity and conversation identity are related but not interchangeable. citeturn13search5

**5 — Turns. [D]** The Harness event/stream API is the better source for run completion than persisted rendered text.

**6 — Compaction. [D]** Observational/context memory is part of the Harness feature set; persistence may therefore include derived memory distinct from literal history. citeturn13search1

**7 — Reasoning. [Gap]** No vendor-neutral signed-reasoning file contract verified.

**8 — Metadata. [D]** Session/harness/worktree identity, grants and storage configuration are runtime metadata worth preserving.

**9 — Resume/import/export. [Gap]** Programmatic Harness APIs are the supported integration surface; no general third-party transcript file importer/exporter was verified.

**10 — Stability. [Gap]** Exact latest package version was not pinned strongly enough here; pin converters to a specific `mastracode`/Harness package pair.


**Factory Droid**

**1 — Storage. [D/Gap]** Droid exposes persistent sessions, but Factory documentation does not publish a canonical local transcript file/database schema. Some execution environments (“Droid Computers”) are cloud-persistent across sessions. citeturn13search7turn13search11

**2 — Format. [Gap]** No supported local session interchange format verified.

**3 — Conversation model. [D]** Sessions include agent execution/tool state rather than plain chat.

**4 — Structure. [Gap]** No first-party portable branch tree schema verified.

**5 — Turns. [D/I]** Use Droid session/run APIs and execution state, not terminal-output boundaries.

**6 — Compaction. [Gap]** Not publicly specified at storage level.

**7 — Reasoning. [Gap]** No portable signed-reasoning contract.

**8 — Metadata. [D]** Desktop exposes session context, progress and history; cloud computers add persistent environment identity. citeturn13search19turn13search11

**9 — Resume/import/export. [D/Gap]** `droid`/`droid exec` and persistent session UX are official; a generic file import/export contract was not verified. citeturn13search3

**10 — Stability. [I]** Treat Factory’s service/API as canonical. Local-cache conversion is unsupported unless Factory documents it.


**Windsurf / Cascade**

**1 — Storage. [D/Gap]** Current official documentation positions Cascade within the Devin Desktop/Windsurf product line. No supported local canonical Cascade transcript file/database schema is published. citeturn14search14

**2 — Format. [Gap]** Exportable history exists in product UI, but the internal persistence format is not a documented interchange format.

**3 — Conversation model. [D]** Cascade is an agentic session, not just a chat text stream; tool/context state matters.

**4 — Structure. [Gap]** No public durable branch/rewind storage schema verified.

**5 — Turns. [Gap]** Use product/session state, not UI transcript alternation.

**6 — Compaction. [Gap]** No portable summary record contract verified.

**7 — Reasoning. [Gap]** No portable signed-reasoning record contract.

**8 — Metadata. [D]** A separate local “memories” feature is stored under `~/.codeium/windsurf/memories/`, but **memories are not the conversation transcript** and must not be used as a substitute session source. citeturn14search6

**9 — Resume/import/export. [D]** Recent sessions/projects can be revisited in the desktop product; conversation-history export is documented for Devin Desktop Plugins. No general raw Cascade-session import format was verified. citeturn14search22turn14search2

**10 — Stability. [D/I]** Product ownership/naming has shifted toward Devin Desktop. A converter should identify the exact client/version rather than assume historical Windsurf internals remain valid. citeturn14search14

## Comparative matrix

“Opaque” below means “preserve the provider/session payload, but there is no documented cross-harness replay guarantee.”

| Harness | Canonical storage | Format | Threading / branching | Strong turn-end signal | Compaction | Reasoning | Official interchange |
|---|---|---|---|---|---|---|---|
| Claude Code | `~/.claude/projects/...` | JSONL, internal schema | checkpoints, rewind; subagents | not publicly standardized at file level | yes; durable resume implications | opaque; transcript contract unpublished | resume/continue; no arbitrary JSONL import citeturn0search0turn0search12 |
| Codex | `CODEX_HOME`, rollouts + SQLite + `session_index.jsonl` | JSONL + SQLite | thread ID; resume; subagent/thread state | run/turn lifecycle events | persisted summaries/compressed histories | opaque protocol items | resume/app-server APIs; no arbitrary rollout import fileciteturn39file0L1-L6 |
| OpenCode 2.x | `opencode.db` | SQLite + JSON columns/blobs | `parent_id`, `fork_session_id`, `fork_boundary`, `revert` | lifecycle/session status + message sequence | explicit/background compaction | provider-specific parts + token counter | official import/export fileciteturn10file0L1-L6 |
| Copilot CLI | `~/.copilot/session-state/...` | `events.jsonl` + artifacts | session identity; no stable public branch schema | session-end lifecycle, not EOF | intelligent compaction | opaque | resume/share; no raw event import fileciteturn11file0L1-L18 |
| Cursor | undocumented canonical local path | SDK/protobuf + internal persistence | session IDs/checkpoints | SDK/runtime events | not file-documented | opaque | `agent resume`, continue; no generic import citeturn3search12turn3search25 |
| Gemini CLI | project `chats/session-*` | append JSONL with `$patch/$set/$rewindTo` | logical history mutations; main/subagent | tool `status` + runtime invariants | summary + bounded/delta history | native Parts + separate thought summaries | resume/rewind; no generic import contract fileciteturn15file0L1-L13 |
| Qwen Code | active/archive transcripts | parent-linked JSONL | `uuid/parentUuid`, fork, rewind, branch checkpoints | `turn_attempt` / `turn_result` | `chat_compression` system record | provider-native structured content | resume/load ACP; Markdown/HTML export fileciteturn20file0L1-L2 |
| Kimi Code | `$KIMI_CODE_HOME/sessions/...` | `state.json` + per-agent `wire.jsonl` + blobs | fork; per-subagent logs | event-stream semantics | explicit `/compact` + auto | opaque wire/provider data | resume, ZIP/MD export, Claude/Codex import fileciteturn22file0L1-L13 |
| Mistral Vibe | `~/.vibe/logs/session` configurable | Unified Session Store; legacy JSON/JSONL | parent session/subagents | must support mid-tool saved state | yes | Thought/provider state; no portable signature contract | resume/continue citeturn6search24turn16search1 |
| Kilo Code | CLI/hosted session services | current internal/API form | fork at `cloneUpToMessageId` | not publicly file-standardized | yes | persisted provider reasoning fixes | session API; no generic import verified citeturn17search0 |
| Cline | CLI `~/.cline/data`; VS Code globalStorage; cloud separate | task JSON files + index | task-level; cloud/local split | runtime state; reasoning-only failure is incomplete | yes | provider-specific | history reconstruction; no generic import citeturn6search14 |
| Roo Code | VS Code globalStorage | task/API-history files | task/delegation stack | not standardized | historical context mgmt | opaque | native history only |
| Amp | service Thread | JSON export / Markdown projection | service thread | service state | service-internal | opaque | JSON/Markdown export; no generic import citeturn7search4 |
| pi | configurable session files | JSONL tree | parent-linked tree/branching | format-dependent | compaction/branch summaries | opaque provider content | resume/continue citeturn7search5turn7search17 |
| Oh My Pi | `~/.omp/agent/sessions` | pi-family session form + extensions | session/tree semantics | format-dependent | pi-family | opaque | Claude/Codex import citeturn7search18 |
| Grok CLI | `~/.grok/sessions` | ACP `updates.jsonl` | session ID | `stopReason` for headless; ACP lifecycle for full replay | not verified | optional `thought`; not portable | resume/export, no general import citeturn10search4turn10search25 |
| Antigravity | backend; local ID cache only | service objects | rewind/undo/fork | backend state | backend | opaque | resume and vendor migration citeturn11search7turn11search16 |
| Hermes | JSON catalogs + SQLite + session export | internal DB/JSON; export JSONL/MD | session/channel/delegation | session runtime | explicit compression | opaque | resume/search/export/import backup fileciteturn31file0L1-L12 |
| OpenHands | pluggable persistence/object store | conversation/event state | conversation/runtime | execution/sandbox status | condenser | opaque | API recovery; no universal file import citeturn12search1turn12search8 |
| Devin | cloud service | service session/steps | `/fork [step]`, `/revert`, `/btw` | service step state | internal | opaque | resume/export; no arbitrary import citeturn12search2 |
| MastraCode | configurable Harness storage | storage-dependent event/state | session/worktree/subagents | Harness events | observational/context memory | opaque | programmatic Harness API |
| Factory Droid | service/cloud session | unpublished | service session | runtime state | unpublished | opaque | resume/service APIs |
| Windsurf Cascade | app/service, unpublished transcript store | unpublished | session-level | service/app state | unpublished | opaque | GUI history/export only |
| Continue | `~/.continue/sessions` | JSON + `sessions.json` catalog | `/fork` | history/runtime, no single catalog marker | `/compact` | unknown nested provider state | resume/fork; no generic import citeturn19view3turn15search1 |

Several rows deliberately say “unpublished” rather than filling them with commonly circulated reverse-engineered paths. That is necessary to keep this reference within the requested **primary-source-only** standard.

## Conversion architecture and cross-harness failure modes

The safest converter architecture is **not** “Harness A schema → Harness B schema” for every pair. The evidence above supports a three-layer intermediate representation:

1. a normalized semantic core;
2. lossless source envelopes;
3. target-specific synthesized resume state.

A useful internal representation would keep at least:

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

The **raw source record** is essential. Gemini’s `$patch/$rewindTo`, Qwen’s many `system.subtype`s, OpenCode’s JSON `data`, Kimi’s wire records and Copilot’s event log all contain data that a generic schema will initially fail to understand. fileciteturn15file0L1-L13 fileciteturn20file0L1-L2 fileciteturn10file0L1-L6 fileciteturn22file0L1-L13

**Tool-name and input-schema correspondence is the first major lossy boundary.** A `read`, `read_file`, shell/Bash tool, edit/replace tool, browser tool or MCP method with a similar human meaning can have different parameter names, path rules, permission behavior and result shape. Do not normalize calls only by name. Store at least source tool name, source schema if available, exact `arguments`, source call ID, result ID/reference, status and provider namespace. Kimi’s request trace is particularly valuable because it records the tool schemas and MCP tool listing that were actually sent on that request. fileciteturn22file0L1-L13

**Parallel tool calls must remain a set/order relation, not be turned into synthetic turns.** Gemini stores multiple `ToolCallRecord`s on one Gemini message; Qwen has independent tool-result records; event-stream harnesses can interleave progress and results. Rewriting parallel calls as `assistant → tool → assistant → tool` can alter the next model request and create a history that never existed. fileciteturn15file0L1-L13 fileciteturn20file0L1-L2

**Tool call/result pairing must use IDs when available.** Never pair by “nearest preceding tool with same name”. Gemini provides `ToolCallRecord.id`; Qwen provides record/tool identities; many provider protocols have explicit call IDs. A failed/missing result is itself state and should not be repaired with an empty success value. fileciteturn15file0L1-L13

**Reasoning must be treated as a typed opaque artifact.** There are at least three different classes that UIs casually call “reasoning”: human-readable thought summaries; provider-native reasoning text/parts; and provider-authenticated/signed opaque state needed for valid continuation. Gemini demonstrates the first two simultaneously: `thoughts` is a separate summary structure while `content` remains native `PartListUnion`. Cline additionally demonstrates that “only reasoning streamed before disconnect” is an incomplete model turn, not a complete assistant answer. fileciteturn15file0L1-L13 fileciteturn25file0L1-L2

Accordingly, a converter should use a policy such as:

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

Never modify the byte representation of an opaque signed block and then resend it as though its signature were valid. If the target cannot represent it, retain it in the archival envelope and omit it from model context rather than fabricating validity.

**Compaction is not deletion.** Claude, Codex, OpenCode, Gemini, Qwen, Kimi, Copilot CLI, Vibe, Hermes, Continue and other harnesses all have some notion of summary/compaction, but they persist it differently. Qwen explicitly says compression system records can change reconstructed model history while original UI history remains intact; Hermes explicitly warns compression is not privacy deletion; Kimi imports foreign histories by keeping complete originals while exposing a bounded model-facing import message. These three patterns strongly support representing `visibleHistory`, `resumeContext`, and `archivalHistory` separately. fileciteturn20file0L1-L2 fileciteturn31file1L33-L44 fileciteturn22file0L1-L13

**Branches, rewind and forks have different semantics.** At least four patterns exist:

- physical append + logical rewind, as in Gemini’s `$rewindTo`; fileciteturn15file0L1-L13
- parent-linked record trees, as in Qwen and pi; fileciteturn20file0L1-L2
- session-level parent/fork boundary fields, as in OpenCode; fileciteturn10file0L1-L6
- service-level fork-at-step/message, as in Devin and Kilo. citeturn12search2turn17search0

A converter should therefore never implement “fork” by simply copying all records up to timestamp T. Use an explicit source boundary: message ID, node ID, sequence, checkpoint or step.

**Subagents are not ordinary assistant messages.** Kimi gives each agent its own `wire.jsonl`; Qwen has `agent_mention`/`agent_message` and other multi-agent records; Gemini has `kind: "main"|"subagent"`; Claude stores subagent-specific metadata. Flattening these into the parent chat loses attribution and can accidentally feed internal delegation chatter back to the target model. fileciteturn22file0L1-L13 fileciteturn20file0L1-L2 fileciteturn15file0L1-L13 citeturn0search15

**Attachments must be content-addressed or copied with referential integrity.** Modern session stores increasingly move data out of the transcript: OpenCode 2.2 introduces content-addressed blob storage, while Kimi moved large base64 media out of `wire.jsonl`. A converter that copies only the primary chat file can create syntactically valid but semantically broken sessions. fileciteturn10file0L1-L6 fileciteturn21file3L71-L89

**Truncation must be recorded as truncation.** Qwen deliberately sanitizes very large diff/file displays and records that a preview was truncated. Gemini caps recorded tool-output size at 50 KiB. A converter must not present such persisted previews as the complete original process output. fileciteturn20file0L1-L2 fileciteturn15file0L1-L13

**Incomplete turns need first-class states.** Recommended target-neutral states are:

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

This is not overengineering. Gemini `0.65.0` had specific resume/rapid-Ctrl-C fixes; Vibe had to repair sessions saved during tool calls; Cline distinguishes a transport loss after only reasoning; Grok exposes `stopReason`; OpenCode persists idle/suspended/resume-attempt state. fileciteturn40file0L1-L2 citeturn16search1 fileciteturn25file0L1-L2 citeturn10search4 fileciteturn10file0L1-L6

**Service/meta records must remain outside the normal conversation by default.** Examples include Gemini `$set` and patches, Qwen session-model/approval/title/attribution/branch/goal records, OpenCode inbox/instruction state, and Copilot workspace artifacts. Unless a target explicitly expects those records in model context, they belong to converter metadata, not synthetic `system` messages. fileciteturn15file0L1-L13 fileciteturn20file0L1-L2 fileciteturn10file0L1-L6

For **writing native target sessions**, the safest priority order is:

1. use an official import API/command;
2. use a public session SDK/API;
3. generate the target’s documented export/import representation;
4. only then write internal storage, and only against a pinned version with migration/validation tests.

Kimi’s `import-sessions`, OpenCode import/export, OMP’s foreign importers and Hermes backup/import are therefore better targets than manually constructing their internal stores. fileciteturn22file0L1-L13 citeturn5search1turn7search18 fileciteturn31file0L1-L12

A production converter should also retain a manifest such as:

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

That makes unavoidable loss **auditable** instead of silent.

## Verification gaps, contradictions, and unsupported assumptions to avoid

The largest gap is not lack of reverse-engineering information; it is lack of **first-party compatibility guarantees**. Claude Code, Copilot CLI, Cline, Cursor Desktop, Kilo, Amp, Antigravity, Devin, Factory Droid and Windsurf all expose some form of durable history, but not all expose a stable external schema that a third party is entitled to write. For these, converters should favor official resume/import/API surfaces and preserve source archives rather than imitate an internal cache.

**Claude Code:** JSONL location and resume/checkpoint behavior are verified, but Anthropic does not publish a complete versioned transcript union or a validation specification for foreign-created JSONL. Exact reasoning/signature replay rules at the Claude Code local-file level therefore remain a gap. citeturn13search30turn0search12

**Codex:** rollout JSONL, index and SQLite/thread-store pieces are verified, but a complete “these files are sufficient for a hand-created thread to be accepted by every CLI/IDE/Desktop client” contract is not published. Name index alone is demonstrably insufficient because code resolves it back to an actual loadable rollout. fileciteturn39file0L1-L6

**OpenCode:** this is one of the best-verified formats, but the requested `2.0.x`/`2.1.x` lines immediately precede `2.2.0`, which already alters media, compaction and instruction persistence. The GitHub “latest release” endpoint also disagrees with the V2 changelog because legacy V1 and V2 release channels coexist. fileciteturn1file0L1-L2 fileciteturn3file0L1-L2

**Copilot CLI:** new-vs-legacy storage is verified, but the complete `events.jsonl` type union is not documented as a public write contract. There was also a current-release-page race around `1.0.103` versus `1.0.104` surfaces, so pin the actual installed `copilot --version` in fixtures. fileciteturn11file0L1-L18 citeturn14search1turn19view2

**Cursor:** persistent session commands and SDK/protobuf interfaces are verified, but exact Desktop and CLI backing-store schemas/paths are intentionally not claimed here because no sufficiently authoritative primary contract was found. citeturn3search12turn3search25

**Gemini CLI:** the source schema is unusually clear, but it is actively changing from full-history snapshots toward append-only patches. A third-party writer must handle both and should expect malformed/partial JSONL recovery semantics. fileciteturn15file0L1-L13

**Qwen Code:** the JSONL record model is richly visible in code, but the current schema has many specialized system records. A converter based on early Gemini/Qwen formats will silently discard branch, goal, multi-agent and managed-session state. fileciteturn20file0L1-L2

**Kimi Code:** the top-level layout and commands are exceptionally well documented, but the complete wire event union was not reproduced in this report. The official warning against manual editing means the importer/API should be preferred over hand-written wire logs. fileciteturn22file0L1-L13

**Mistral Vibe:** the Unified Session Store is verified at source level, and legacy metadata/messages files are verified, but this report does not claim a complete field-by-field V3 store specification. The recovery fix for mid-tool V3.0.0 sessions proves that edge states matter. fileciteturn24file0L1-L13 citeturn16search1

**Kilo Code:** current session ownership has changed enough that legacy Roo/Cline paths should not be treated as Kilo 7.8.x. A complete current backing-store schema remains a gap. citeturn6search21turn17search0

**Cline:** local CLI and VS Code storage paths are verified, but cloud tasks are a separate persistence domain. A converter needs an explicit `cline-local-cli`, `cline-vscode`, or `cline-cloud` source kind rather than one generic “Cline” adapter. citeturn6search14 fileciteturn25file0L1-L2

**Roo Code:** final version is firmly pinned, but it is archived. Do not infer current Cline or Kilo behavior from Roo’s final schema. fileciteturn29file0L1-L2

**pi and Oh My Pi:** session behavior and foreign-import support are verified, but I did not obtain a sufficiently authoritative latest release tag during the research window. Their exact version rows should therefore be filled from the package/repository revision used by the converter’s test corpus rather than guessed.

**Grok Build:** local ACP JSONL and official resume/export behavior are verified; no conventional stable GitHub release tag and no general import API were found. citeturn10search1turn10search25

**Antigravity:** local `last_conversations.json` is only an index/cache; treating it as a transcript would be a fundamental converter error. The conversation is backend-backed. citeturn11search16

**Hermes:** local durability spans multiple stores; its official export/import operations are a safer portability boundary than copying arbitrary `.db` files while processes may hold them open. Hermes’ own documentation warns about database-file replacement semantics. fileciteturn31file4L97-L107

**OpenHands:** persistence is backend-pluggable, so there is no single universally correct Windows/macOS/Linux transcript path. The relevant converter boundary is the conversation/event API plus the deployment’s configured store. citeturn12search1

**Devin, Factory Droid, Amp and Windsurf/Cascade:** first-party session/history/export behavior is available, but no primary-source canonical local transcript schema suitable for faithful arbitrary writes was verified. Any converter claiming a stable local SQLite/IndexedDB path for these products would exceed the evidence in this report.

**MastraCode:** the Harness/storage abstraction is verified, but a single standard disk serialization and current package tag were not sufficiently pinned.

**Continue:** the JSON save/catalog code is verified, but nested `history` records can evolve. Preserve unknown nested fields and support old `session_id` forms already recognized by Continue itself. citeturn19view3

Finally, the request asked for **file-and-line source links**. For several GitHub fetches, the connected primary-source interface returns the complete source file inside a single JSON `content` field, so the citation line range points to that source response rather than to each original GitHub source line. I have therefore included the exact repository path/symbol names where available—for example `packages/core/src/session/sql.ts`, `chatRecordingTypes.ts`, `chatRecordingService.ts`, and `codex-rs/rollout/src/session_index.rs`—and have not invented original-file line numbers that were not reliably exposed. The code citations still resolve to the first-party file/commit from which the schema was read.
