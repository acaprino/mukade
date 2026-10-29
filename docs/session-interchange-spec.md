# Session interchange library: implementation spec (verified)

Date: 2026-10-27.
Status: draft, every behaviour below was run on this machine unless marked otherwise. Companion of Shinatama's technical design (5.3, 5.4, 5.4.1, 5.5, 6.4), which stays with that product's documentation.
For available verification records, see the [documentation index](README.md#verification-records).
Background reading (secondary sources, superseded here for the four harnesses below):
[`reference/harness-session-persistence-reference.md`](reference/harness-session-persistence-reference.md).

**Evidence labels.** **[V]** indicates a reported observation, which does not by itself establish
current behavior. **[S]** read in source code, not exercised. **[A]** a public API rule, not
exercised against the real API. **[U]** not established. See the verification records for the
scope and limitations of the tests they document.

## 0. What the library does, and what it does not

The library moves one coding-agent conversation from a **source** harness to a **destination**
harness so that the destination can continue it as one of its own sessions:

1. read the source session from the source's native store;
2. apply the hop policy: selection, redaction, **optional compaction**, reasoning policy;
3. write a new native session into the destination's store;
4. verify the destination lists it and can resume it;
5. **optionally** compact it with the destination's own compaction;
6. **optionally** resume it with a prompt (the "resume prompt"), non-interactively.

Harnesses and versions described:

| harness | version | binary on this machine |
|---|---|---|
| Claude Code | 2.1.311 | `C:\Users\alfio\.local\bin\claude.exe` |
| Codex CLI | 0.164.0 | `C:\Users\alfio\AppData\Local\Programs\OpenAI\Codex\bin\codex.exe` |
| OpenCode | 2.1.4 | `%APPDATA%\npm\node_modules\@opencode\cli\bin\opencode.exe` |
| Pi | 1.2.0 (`@earendil-works/pi-coding-agent`) | `node …\pi-coding-agent\dist\bundle\cli.js` |

**The library is Shinatama's own**: it reads and writes the four native formats itself, with one
reader and one writer per harness.

Where a harness has its **own** import path, the writer uses it instead of writing the store:
OpenCode `session import` (lossless round trip, OC2 [V]). Codex's importer (`externalAgentConfig`,
CX4 [V]) turns tool calls into text markers, so the Codex writer writes the rollout itself (§2.2
says which files Codex needs). Claude Code and Pi have no session import: their writers write the
session file.

Out of scope: harnesses other than the four above (the reference report lists them); writing
into an existing session; writing while the destination has the same store open for that
session; replaying one provider's signed or encrypted reasoning to another provider.

## 1. Pipeline and API

### 1.1 Stages

```text
SessionRef ──read──▶ Session (IR, §3) ──policy──▶ Session ──write──▶ Written{id, location}
                                         │                               │
                          compaction: none | portable            verify (list + resume dry run)
                          reasoning policy, tool mapping, redaction      │
                                                       compaction: destination-native (optional)
                                                                         │
                                                       resume prompt (optional) ──▶ Receipt
```

- **read**: `Reader` for the source harness (§8.1): native records → `Session`, keeping every
  source record verbatim in the entry's envelope (§3.1). Technical design 5.2 rules apply.
- **write**: `Writer` for the destination (§8.2): `Session` → a new native session, through the
  harness's own import where one exists (OpenCode), else by writing the store files. Returns
  `Written{id, location}` and the native resume command (§2).
- **verify**, **compaction (destination-native)** and **resume prompt** run the destination's
  own binary (§2), in the destination's environment.

### 1.2 Rust surface

```rust
pub struct HopRequest {
    pub source: SessionRef,              // harness + native id (an unambiguous prefix is accepted)
    pub dest: HarnessId,                 // claude_code | codex | opencode | pi
    pub cwd: Option<String>,             // project directory the destination files the session under
    pub compaction: Compaction,          // §4
    pub resume: Option<ResumePrompt>,    // §5
    pub reasoning: ReasoningPolicy,      // §3.3, default DropForeign
    pub roots: StoreRoots,               // the stores; None is refused for Claude Code, Codex and Pi,
                                         // and means "its configured store" for OpenCode
    pub write: WriteDetails,             // destination facts: version, protocol, shell, offered tools,
                                         // provider, model, project id, clock, executable
}

pub enum Compaction {
    None,                                // whole selected history
    Portable { summary: SummarySource, keep_from: KeepFrom },  // written by us as one user message + tail
    DestinationNative,                   // refused: the destination's own compaction is the caller's to run
}
pub enum SummarySource { SourceRecord, Supplied(String),        // SourceRecord: the source's native compaction record (L4)
                         SourceModel(SummarizeOpts), Generated(Summary) }  // §4.5: written by the source's model
pub enum KeepFrom { SourceBoundary, LastTurns(usize), Message(String) }  // LastTurns(0): the handoff alone

pub struct ResumePrompt { pub text: String, pub timeout: Duration }

pub enum ReasoningPolicy {
    DropForeign,     // default: keep reasoning only when source and dest use the same provider protocol
                     // and its payload has a place in the destination's format; otherwise drop it
    AsText,          // keep foreign reasoning as visible text, labelled, before the answer
    Drop,            // never carry reasoning
}

pub struct Receipt {
    pub dest_id: String, pub location: String,
    pub resume_command: Vec<String>,     // native resume command of the destination (§2)
    pub manifest: serde_json::Value,     // §6.3, as written
    pub resume_output: Option<String>,   // the destination's reply when a resume prompt was given
}

pub trait Reader { fn harness(&self) -> HarnessId;
                   fn list(&self, roots: &StoreRoots) -> anyhow::Result<Vec<SessionInfo>>;
                   fn read(&self, id: &str, roots: &StoreRoots) -> anyhow::Result<Session>; }
pub trait Writer { fn harness(&self) -> HarnessId; fn offered(&self) -> ToolSet;                  // tool tables §2/§6
                   fn write(&self, s: &Session, opts: &WriteOpts) -> anyhow::Result<Written>; }

pub fn hop(req: &HopRequest) -> Result<Receipt, HopError>;
pub fn export(src: &SessionRef, roots: &StoreRoots) -> Result<Session, HopError>;  // read + reader rules
pub fn export_path(harness: HarnessId, path: &Path) -> Result<Session, HopError>;  // a session file
pub fn import(dest: HarnessId, s: &Session, opts: &ImportOpts) -> Result<Receipt, HopError>;
pub fn hop_session(dest: HarnessId, s: &Session, c: &Compaction, opts: &ImportOpts) -> Result<Receipt, HopError>;
pub fn compact(s: &Session, c: &Compaction, roots: &StoreRoots) -> Result<(Cow<Session>, Option<SummaryFacts>), HopError>;  // §4.5
```

`HopError` kinds map to the CLI exit codes of technical design 5.5.2: `NotFound` (2: the store is
not configured, the session is not there or the id is ambiguous — a reader's typed `Lookup`),
`Unsupported` (2: a pair or option this spec marks blocked), `SourceChanged` (2: a reviewed source
fingerprint no longer matches), `WriteFailed` (4), `StrictCheckFailed`
(4, §7), `SummaryFailed` (4, §4.5: the source's model wrote no usable summary; nothing was written),
`VerifyFailed` (5: the source is there but could not be read), `WriteUncertain` (5: an external
import may have written; the error carries its ID and destination), and `ResumeFailed` (5: the
session was written; the error carries its receipt and manifest). OpenCode IDs and metadata supplied
by a host are checked in the explicitly selected destination after import; an unconfirmed result
must be reconciled before retrying.

### 1.3 Rules that hold for every hop

1. **New session only.** The destination session always gets a new id; the source is never
   modified. A second import of the same id is refused by the destination (OpenCode answers
   409, OC2) and must not be retried with the same id.
2. **No real store in tests.** Every test sets the harness's root variables (§2) to a scratch
   folder, and checks that the real store's fingerprint did not change [V: every
   `*/source/isolation*`, `gen-source.log.txt`, `resume/*.meta.json` `real store unchanged: True`].
3. **No foreign process reaches the real home.** OpenCode's CLI hands work to a background
   service that runs with the user's real environment unless `--standalone` (or `--server`) is
   passed. Every OpenCode
   command the library spawns that touches the store (`session export`, `list`, `import`, `run`)
   carries `--standalone` (L1); only the version
   probe, `opencode --version`, runs without it.
4. **Order is the native order**, never timestamps (OpenCode `seq`, Codex `ordinal`, Claude
   and Pi parent links).
5. **Every loss is written into the manifest** (§6.3); none is silent.
6. **No stray files; one executable.** mukade is a single executable with no installer.
   Its own files are the session it writes (a `.mukade-*.tmp` beside it, renamed into place), what
   `--output` / `--manifest` name, and one data folder: by default `mukade-data` beside the
   executable, or the absolute path explicitly selected by `MUKADE_DATA_DIR`. It contains a probe
   cache in `cache/` and, for OpenCode, the document `session import` needs as a file in `tmp/`
   (mode 0600, removed when the import ends, on failure too; a killed run's leftovers older than an
   hour are swept by the next run). Where that folder cannot be made there is no cache and an
   OpenCode import is refused with a reason; no other place is used. By default nothing goes to the user
   profile, the application-data folders, the registry or the system's temporary folder [V:
   `tests/portable.rs` runs the real binary alone in a folder with every such location pointed at an
   empty sandbox and asserts it stays empty].

## 2. The four harnesses (verified facts the library depends on)

Each subsection gives isolation, commands, native records the library must read and write, and
what the model receives on resume.

### 2.1 Claude Code 2.1.311

**Isolation [V].** Remove inherited `CLAUDE*`/`ANTHROPIC*` variables, then
`CLAUDE_CONFIG_DIR=<scratch>\home`, `ANTHROPIC_BASE_URL`, `ANTHROPIC_API_KEY`,
`CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`, `DISABLE_TELEMETRY=1`, `DISABLE_ERROR_REPORTING=1`,
`DISABLE_AUTOUPDATER=1`. Scratch `home\.claude.json` with `hasCompletedOnboarding`,
`customApiKeyResponses.approved`, `projects[<cwd>].hasTrustDialogAccepted`. stdin from an empty
file (otherwise a 3 s wait).

**Commands [V]** (from the project dir, `--output-format json --dangerously-skip-permissions`):

| action | command | notes |
|---|---|---|
| new | `claude -p "<prompt>"` | JSON result carries `session_id` |
| resume + prompt | `claude -p --resume <id> "<prompt>"` | same id, appends to the same file; works from any cwd |
| compaction | `claude -p --resume <id> "/compact"` | trigger `manual`; result text `Compacted …` |
| fork | add `--fork-session` | new id, **no lineage field** |

**Store [V].** `<CLAUDE_CONFIG_DIR>\projects\<cwd with every non-alphanumeric char → ->\<uuid>.jsonl`.
No index file is needed: a session file written outside Claude Code resumed by id [V: `claude_code/resume/*-from-*.meta.json`].

**Records the library depends on [V: `claude_code/source/c45db7bc-….jsonl`].**
- One API response = **one row per content block**, all sharing `message.id`, chained by
  `parentUuid`. A reader merges rows by `message.id`.
- Thinking: `{"type":"thinking","thinking":…,"signature":…}` — **signature stored**.
- **Parallel calls:** the tool_use rows are chained; **each tool_result row's `parentUuid` is its
  own tool_use row**, results written in completion order. Walking parents from the leaf
  therefore loses a call or a result; pair by `tool_use_id`.
- **Compaction:** `system/compact_boundary` `{parentUuid:null, logicalParentUuid:<old leaf>,
  compactMetadata{trigger, preTokens, postTokens, preservedSegment{headUuid, anchorUuid, tailUuid},
  preservedMessages}}`, then a `user` row `isCompactSummary:true, isVisibleInTranscriptOnly:true`
  holding `This session is being continued from a previous conversation … Summary: …`, then the
  `/compact` command rows (`<local-command-caveat>` with `isMeta:true`, `<command-name>`,
  `<local-command-stdout>`), then `attachment type:"file"` rows re-injecting files read before.
  The last assistant reply stays in place and is hung under the summary by `preservedSegment`.
- **Interrupted turn** (process killed): only the user row and an `api-request` row. On the next
  load Claude Code writes a synthetic assistant `"No response requested."` (`model:"<synthetic>"`,
  `stop_reason:"stop_sequence"`) and sends it.
- Each user turn is followed in the request by a `role:"system"` message (environment block).

**What the model receives on resume [V: `claude_code/source/capture-gen.jsonl`, `resume/source-resume.capture.jsonl`].**
Before a compaction: everything, thinking with its signature. After: summary + preserved tail +
later turns only; T1's tools and thinking are not sent; files read earlier come back as a system
message. The resume prompt is the last user message.

**Implemented history reconstruction (2026-10-29; local tests, not a new live harness
verification).** The Claude Code reader reconstructs a selected branch using UUID parent links,
compaction metadata, response IDs and tool call/result IDs. File position is not conversation
order; it is a tie-breaker for otherwise independent recovered records. Timestamps are not used
to sort the history.

- **Branch selection.** `last-prompt.leafUuid` selects a checkpoint. A continuation descending
  from that checkpoint can supersede it. A new user branch after the checkpoint can also
  supersede it, provided the checkpoint's leaf was recorded before the checkpoint itself.
  A checkpoint flushed before its own leaf does not establish a rewind ordering barrier.
  Text beside tool results can start a new user turn. Without a checkpoint, the last
  non-sidechain user or assistant row with a UUID selects the branch.
- **Recovery and pairing.** Split assistant blocks sharing `message.id` become one step,
  including blocks interleaved with results. Parallel results are paired by `tool_use_id`.
  UUID-less blocks, results, expanded slash-command prompts and compaction summaries are
  recovered only when their dependencies identify a safe position. An explicit conversational
  parent on an abandoned branch takes precedence over `sourceToolAssistantUUID`; such a result
  is not recovered into the active conversation.
- **Compaction and preservation.** `logicalParentUuid` connects the earlier history to a
  compaction boundary. Preserved-message lists and segments are checked, and a retained UUID
  inside a merged response resolves to that response's entry for portable compaction.
  Original source records remain in `Entry::raw`; excluded branch records become
  `Meta(OffBranch)` and are counted in the destination manifest rather than replayed.
- **Refusals and verification limits.** Missing parents, cycles, duplicate UUIDs, contradictory
  preserved metadata and unplaceable or ambiguous conversation records are refused rather
  than guessed. Regression tests cover the review's branch-selection and abandoned-result
  cases, shuffled fixture rows, a 20,000-row reverse-written chain and native-format round trips
  through all four destinations. These are local parser/writer checks, not proof of live
  auto-compaction or rewind compatibility; those remain **[U]** against Claude Code itself.

See [`usage.md`](usage.md#a-session-that-was-rewound-or-branched) for user-facing behavior and
`tests/claude_code_order.rs` for the reconstruction regressions. The observed native records
above retain their original **[V]** scope.

### 2.2 Codex CLI 0.164.0

**Isolation [V].** `CODEX_HOME`, `USERPROFILE`, `HOME` → scratch; `config.toml`:

```toml
model = "<model>"
model_provider = "<id>"
check_for_update_on_startup = false
[model_providers.<id>]
base_url = "http://127.0.0.1:<port>/v1"
wire_api = "responses"
env_key = "<VAR>"
[analytics]
enabled = false
[features]
plugins = false   # with plugins on, every process contacts chatgpt.com and github.com
```

**Commands [V]** (`--json --skip-git-repo-check --dangerously-bypass-approvals-and-sandbox`):

| action | command | notes |
|---|---|---|
| new | `codex exec "<prompt>"` | thread id on stdout line 1 (`thread.started`) |
| resume + prompt | `codex exec resume <id> "<prompt>"` | appends to the same rollout; header `history_initialization:"cold_resume"` |
| compaction | `codex app-server` over stdio: `initialize`, `initialized`, `thread/resume {threadId,cwd}`, `thread/compact/start {threadId}`, wait for item `contextCompaction` and `turn/completed` | `exec` has no compaction |
| interrupt | app-server `turn/start` then `turn/interrupt {threadId, turnId}` | |

**Store.**
- `sessions\YYYY\MM\DD\rollout-<local ts>-<uuid>.jsonl`; lines `{timestamp, ordinal, type, payload[, metadata]}`.
- A rollout alone resumes by id (Codex rebuilds its SQLite stores); a wrong date folder still works.
- **`state_5.sqlite` `threads.rollout_path` wins over the file on disk**; a row pointing at a
  missing file fails hard: `no rollout found for thread id … (code -32600)`. A writer must not
  leave a stale row (L7).
- The model context comes from the rollout, not from `thread_history_1.sqlite`.
- `thread/list` with default filters hides threads whose source is `exec`.
- No `session_index.jsonl` was written in the scratch home.

**Records [V: `codex/source/rollout-…01a17e07….jsonl`].**
- Reasoning `{"type":"reasoning","summary":[…],"content":null,"encrypted_content":…}` — **encrypted content stored**.
- Parallel calls: consecutive `function_call` lines, then `function_call_output` lines, paired by `call_id`.
- Turns: `event_msg/task_started` … `task_complete`.
- **Compaction:** `compacted.payload = {message:"Another language model started … <summary>",
  replacement_history:[user T1, developer skills+permissions, user <environment_context>, user T2,
  user summary bridge], replacement_history_metadata, retained_context, window_number, window ids,
  compaction_response_id, latest_token_usage_record, resume_metadata}`. Compaction is **local**:
  an ordinary `/v1/responses` call with `request_kind:"compaction"` and the prompt
  `You are performing a CONTEXT CHECKPOINT COMPACTION…`. A `compacted` line reduced to
  `{message}` still resumes (P6).
- Graceful interrupt: user message `<turn_aborted>…</turn_aborted>` + `event_msg turn_aborted
  {reason:"interrupted"}`, sent to the model on resume. Hard kill: the file just ends.

**On resume [V].** Before compaction: full history with `encrypted_content`. After: base
developer message + `replacement_history` + later items + prompt; no `encrypted_content`.

**Codex Desktop 0.165.0-alpha.1.2 [V: two real threads, 2026-10-28].** A thread whose later turns
were redone is kept over two files (`history_mode: "paginated"`): a page
`rollout-<ts>-<thread>_<page>.jsonl` starts with a `session_meta` carrying
`history_base: {thread_id, end_ordinal_exclusive, end_byte_offset}`, and continues the thread from
that prefix of the earlier file; `state_5.sqlite` `threads.rollout_path` names the page. The reader
reads the latest version: the prefix, then the page, following an earlier page's own base the same
way; each earlier file's later records are off-branch (§1.3, counted), or carried with
`Branches::All` (`--all-branches`). Compaction summaries are encrypted
(`compacted.message` empty; an item `{type:"compaction", encrypted_content}` in
`replacement_history`): none is written as an empty message (`dropped_unreadable_summaries`), and
`Portable` from such a record is refused. Subagent messages to the root agent (`agent_message`)
carry their payload as `encrypted_content`, readable only by OpenAI, and nothing but a header in
plain text: they are **excluded by decision** (2026-10-28), read as `Meta`, neither carried nor
counted.

### 2.3 OpenCode 2.1.4

**Isolation [V].** `OPENCODE_DB`, `XDG_DATA_HOME`/`XDG_CONFIG_HOME`/`XDG_STATE_HOME`/`XDG_CACHE_HOME`,
`OPENCODE_TEST_HOME` → scratch; `OPENCODE_DISABLE_MODELS_FETCH=1`, `OPENCODE_DISABLE_PROJECT_CONFIG=1`,
`OPENCODE_DISABLE_CLAUDE_CODE=1`; remove `PWD`. Config (2.x schema):
`{"providers":{"anthropic":{"settings":{"baseURL":…,"apiKey":…},"models":{…}}}}`.
**Every command with `--standalone`** (§1.3 rule 3).

**Commands [V].**

| action | command | notes |
|---|---|---|
| new | `opencode run --standalone --model <p/m> --format json --auto --title <t>` | prompt on stdin |
| resume + prompt | `opencode run --standalone --session <id> --format json --auto` | **an unknown id silently creates a new session**: check the id exists first |
| compaction | on `opencode serve`: `POST /api/session/<id>/compact {}` then `POST /api/experimental/session/<id>/wait` (204) | HTTP only; `run` has no compaction |
| import / export | `opencode session import <file> --directory <D> --standalone` / `session export <id> --standalone` | interchange `{info, messages[]}` (OC2) |

**Records [V: `opencode/source/rows.json`].**
- `session_message {id, session_id, type, seq, …, data}`; order by `seq`.
- Assistant = one row per model step; **parallel calls are one row** `content:[reasoning, text, tool, tool]`;
  each `tool` part holds the call and its result (`state{status,input,content}`).
- A failed call's part is `state{status:"error", input, error, content}`, where `error` is a
  `Session.StructuredError` `{type, message}` (OpenCode records `tool.execution` for a tool that
  failed); `session import` refuses an error part without it (`Bad Request`) [V 2026-10-28].
- Images: a user turn carries `files:[{data, mime, source:{type:"inline"}}]`
  (`Prompt.FileAttachment`), a tool result `{type:"file", uri:"data:…", mime}`
  (`Tool.FileContent`) [V 2026-10-28, import and export of a scratch session].
- Reasoning `{"type":"reasoning","text":…,"native":{"signature":…}}` — **signature stored**; sent
  back only when the stored provider and model equal the current ones [S: `to-llm-message.ts:155`].
- Turn end: `{"type":"idle","outcome":"succeeded"}`.
- **Compaction:** `{type:"compaction", metadata{keepID:<first kept msg>}, status:"completed"|"failed",
  reason:"manual", summary:<reply verbatim>, …}`. A summary without one of the template headings
  (`## Objective` …) fails: `Compaction summary did not match the required template` [V: `opencode/run1-shared-mock-compaction-failed/`].
- Interrupted turn: an assistant row with empty text and no `finish`; `time_suspended` set; the
  export omits it; on resume it is dropped from the request.
- Tools and input keys: `read{path,offset,limit}`, `edit{path,oldString,newString,replaceAll}`,
  `write{path,content}`, `shell{command,workdir,timeout,background}`, `grep`, `subagent`, `skill`, …

**On resume [V].** After a compaction the first message is one user
`<conversation-checkpoint>…<summary>…</summary><recent-context>[User]: … [Assistant]: …</recent-context></conversation-checkpoint>`;
the kept tail is flattened into it as text.

### 2.4 Pi 1.2.0

**Isolation [V].** `PI_CODING_AGENT_DIR=<scratch>\home`; `models.json` provider with
`api:"anthropic-messages"`, `baseUrl`, `apiKey`; `settings.json` `compaction.keepRecentTokens`.
Flags on every run: `--provider <p> --model <m> --offline --no-skills` (**`--no-skills` is
required**: Pi reads `~\.agents\skills` whatever `PI_CODING_AGENT_DIR` says). Remove provider
API-key variables from the environment. RPC stdin must be UTF-8 **without BOM**.

**Commands [V].**

| action | command |
|---|---|
| new | `pi -p "<prompt>"` |
| resume + prompt | `pi -p --session <file path or id> "<prompt>"` (id searched in the current project first) |
| compaction | `pi --mode rpc --session <id>`, stdin `{"id":"c1","type":"compact"}`, wait for the response (print-mode `/compact` is sent to the model as text) |
| abort | RPC `{"type":"abort"}` |

**Records [V: `pi/source/2026-10-27T16-00-39-689Z_01a17e17-….jsonl`].**
- Header `{"type":"session","version":3,"id","timestamp","cwd"[, "parentSession" on fork]}`; then
  `model_change`, `thinking_level_change`, and a stored **system message** entry (sections +
  `toolsAdded`), replayed on resume (a different cwd is not reflected).
- Assistant: thinking with `thinkingSignature` (**stored**), text, and **both parallel
  `toolCall`s in one message**; `toolResult` entries one per result.
- **Compaction:** `{type:"compaction", summary (+ a Pi-added <read-files> block), firstKeptEntryId,
  tokensBefore, details{readFiles, modifiedFiles}, usage, fromHook, systemMessage}`.
- Killed turn: only the user entry. Aborted turn: assistant `stopReason:"aborted"`, dropped from the next request.
- A hand-written 3-line session (header, user, assistant) resumes [V: `pi/resume/synthetic.*`].
- **Tree.** Every entry has `id` and `parentId`. Navigating back with `/tree` starts a new branch
  from an earlier entry, optionally through a `branch_summary` entry `{summary, fromId}`, and
  leaves the abandoned branch in the file. Pi builds the context from the last entry back through
  `parentId`, and shows a `branch_summary` on that path to the model as a user message
  `The following is a summary of a branch that this conversation came back from:\n\n<summary>\n…</summary>`
  [pi-coding-agent 1.2.0 `buildSessionPath`, `BRANCH_SUMMARY_PREFIX`].

**On resume [V].** After a compaction: system + user `The conversation history before this point
was compacted into the following summary:\n\n<summary>…</summary>` + entries from
`firstKeptEntryId` on + the prompt.

## 3. Interchange model

### 3.1 The library's own model (`Session`)

Three layers: a **semantic core**
that every writer understands, the **source record kept verbatim** in each entry (nothing is
lost by reading), and the **target-specific** choices made by the writer.

```rust
pub struct Session {
    pub meta: Meta,                    // source harness + version, id, cwd, title, created/updated,
                                       // model(s), git branch, parent/fork boundary if any
    pub entries: Vec<Entry>,           // in native order (seq / ordinal / parent chain), never by time
}
pub struct Entry { pub id: String, pub kind: Kind, pub raw: Vec<serde_json::Value> }  // raw = source record(s)

pub enum Kind {
    User      { text: String, attachments: Vec<Attachment> },
    Step      { model: Option<ModelRef>,                    // one model response = one step
                reasoning: Vec<Reasoning>, text: Vec<String>,
                calls: Vec<ToolCall>,                        // parallel calls stay together
                end: StepEnd },                              // completed | tool_use | aborted | error | killed | unknown
    Results   { results: Vec<ToolResult> },                  // answers to the previous Step's calls, by id
    Compaction{ summary: String, kept_from: Option<String>, trigger: Trigger, tokens_before: Option<u64> },
    Meta      { kind: MetaKind },                            // command rows, synthetic replies, <turn_aborted>,
                                                             // idle markers, model/thinking changes, system prompts,
                                                             // re-injected file attachments: never conversation
}
pub struct ToolCall   { pub id: String, pub source_name: String, pub canonical: Option<CanonicalTool>,
                        pub input: serde_json::Value }      // canonical per tool-conversion-tables.md
pub struct ToolResult { pub call_id: String, pub content: ResultContent, pub is_error: bool,
                        pub truncated_by_source: bool }
pub struct Reasoning  { pub text: String, pub opaque: Option<Opaque> }
pub struct Opaque     { pub protocol: Protocol,              // anthropic-messages | openai-responses | …
                        pub model: Option<String>, pub signature: Option<String>, pub encrypted: Option<String> }
```

### 3.2 Why each part is there (evidence)

| part | why [V] |
|---|---|
| `Compaction` entry | all four harnesses store a compaction record and resume from it (§2) |
| `Step.calls: Vec` and `Results` | Claude Code and Pi reject or repair split parallel calls (§6.2); Codex and OpenCode store them as one step |
| `Meta` entries | Claude `/compact` command rows read as conversation became an orphan `tool_result` in Pi and `function_call "{}"` in Codex; Codex `<turn_aborted>` and Claude's synthetic `No response requested.` became ordinary text |
| `Opaque` with protocol and model | decides replay vs drop (§3.3); every native store keeps it |
| `ToolCall.canonical` + `source_name` | tool translation where possible, text form otherwise ([`tool-conversion-tables.md`](tool-conversion-tables.md)) |
| `raw` on every entry | unknown records are kept, not dropped; a later version can read what this one did not understand |

### 3.2.1 Tools

Tool calls follow [`tool-conversion-tables.md`](tool-conversion-tables.md) (last updated 2026-10-27): translated into the destination's equivalent tool where the destination offers one and every argument used maps without changing its meaning, otherwise carried in text form; results verbatim; ids kept.

### 3.3 Reasoning

[V] Every native store keeps the opaque part: Claude `signature`, Codex `encrypted_content`,
OpenCode `native.signature` (through OpenRouter `native.reasoningDetails`), Pi `thinkingSignature`; and every harness sends it back on its own
resume (all four `capture-gen.jsonl`).

Policy (default `DropForeign`): carry a reasoning block only when the destination speaks the
same provider protocol as the block's provenance **and** the opaque payload is present
unmodified; otherwise drop it and record `reasoning_dropped: n` in the manifest, with a loss
line per reason (no payload, the destination's protocol not known, another protocol, no place for
the payload in the destination's format, dropped on request). `AsText` keeps it as a labelled text
block (`[earlier reasoning, not replayable]`) and counts it in `reasoning_as_text`. Never
synthesize, edit or concatenate a signed or encrypted payload.

## 4. Compaction between hops (optional)

Compaction options are selected through `HopRequest.compaction`.

### 4.1 `None`

The whole selected history is written. The destination sends the history back on resume, so the context the destination
starts with is as large as the source's **pre**-compaction history: the source's own compaction
is lost unless the reader keeps it (L4).

### 4.2 `Portable` (the library writes the compacted form)

The library writes one user message
`[shinatama handoff v1] Summary of the earlier conversation, written by the source agent's own compaction:\n<summary>`
followed by the tail from the boundary on. Summary source: the source's native compaction record
(`SourceRecord`, needs L4; boundary = Claude `preservedSegment`/next after summary, Codex
`retained_context`/`replacement_history`, OpenCode `metadata.keepID`, Pi `firstKeptEntryId`) or a
caller-supplied text (`Supplied`, e.g. produced by a low-cost model per technical design 6.4).

### 4.3 `DestinationNative` (the destination compacts the imported session)

Write the whole history, then run the destination's own compaction (§2 commands), then resume.
Costs:
one extra model call on the destination's model, and the destination must accept the full
history once (the context window must hold it). Prefer `Portable` when the source already has a
compaction (its summary is free and already written) or when the full history does not fit the
destination's window; prefer `DestinationNative` when the summary should be written by the
destination's model in the destination's own format.

### 4.4 Writing a native compaction record (next step, not yet built)

The best form of `Portable` is a **native** compaction record in the destination, so that the
destination treats the boundary as its own. Feasibility per destination: Codex accepts a
minimal `compacted{message}` line (P6 [V]); OpenCode's import round-trips a `compaction` message
losslessly (OC2 [V]); Claude Code (`compact_boundary` + `isCompactSummary` row) and Pi (`compaction`
entry with `firstKeptEntryId`) are **[U]** for hand-written records. Until a fixture proves one,
`Portable` writes the user-message form of 4.2.

### 4.5 `Portable` with a summary from the source's model

`SummarySource::SourceModel(SummarizeOpts)` asks the source harness's own model for the summary when
the session moves (`--compaction summary`, with `--keep-turns N`, `--summary-model`,
`--summary-timeout`). mukade sends one prompt of its own, the same for the four harnesses and
versioned (`v1`), not the harness's native `/compact`: that writes into the source's store, and
Codex's native summary is encrypted. The run goes through the source's binary, credentials and
configuration, and saves nothing in its store: Claude Code `--no-session-persistence`, Codex
`exec fork --ephemeral`, Pi `--fork` into a temporary folder, OpenCode a fork deleted afterwards
whatever came of the run (with every tool denied through `OPENCODE_CONFIG_CONTENT`)
[V: `docs/verification/source-model-summary-live.md`; Codex against a mock].

The result is the shape of §4.2 with its own handoff line,
`[mukade handoff v1] Summary of the earlier conversation, written by the source agent's model when the session was moved:`,
then the last `N` user turns (none by default). `hop::compact` resolves the summary once, before
the selection, so `dump --compaction summary` exports the same document `hop` would write, and
`write` imports it later; the document's top-level `summary` says who wrote it, so the manifest
`write` makes says so too. The manifest carries `options.summary` (`source: "source_model"`,
harness, version, model, `prompt: "v1"`, `chars`, `usage`, `left_out`) and one loss line. A failed
summary (an error, an empty reply, a deadline) is `HopError::SummaryFailed`, exit 4: nothing was
written. Refused (exit 2): `--all-branches`, and `--from-file` for a source other than Pi.

## 5. Resume prompt (optional)

The library runs the destination's resume command from the session's working directory, with
the caller's environment (the destination's store variables must be set there), stdin closed and a
timeout that stops the harness, including one behind a Windows `.cmd` launcher. On success the receipt keeps stdout as
`resume_output`; on failure `ResumeFailed` carries the written receipt, its reason names the
command, the exit status and stderr, and the manifest records the failed resume as a loss.

Cautions [V]:
- OpenCode `run --session <unknown id>` creates a new session: the library checks the id with
  `session export` (or the DB, read-only) first.
- Codex: threads written with source `exec` are hidden from `thread/list` defaults; resume by id works.
- Pi: `--session <id>` searches the current project first; the library passes the file path.
- Claude Code: resume works from any cwd but appends rows with the new cwd; run from the session's cwd.
- An **interrupted last turn** in the source is carried as a user message without a reply. On
  resume Codex, OpenCode and Pi send two consecutive user messages; Claude Code inserts its
  synthetic `No response requested.`. Both are accepted. The manifest records `interrupted_last_turn`.

## 6. Destination constraints and manifest

### 6.2 Tool and reasoning constraints

- Claude Code associates each tool result with its own tool call (L2).
- Pi groups parallel calls in one assistant message followed by their results (L3).
- Signed reasoning needs its original compatible payload; otherwise the configured reasoning
  policy determines whether it is dropped or carried as text (L6).
- OpenCode groups an assistant step's content and tool calls in one message (L8).

### 6.3 Manifest (written with every hop)

```json
{
  "source": {"harness": "codex", "version": "0.164.0", "session_id": "…", "content_sha256": "…"},
  "dest":   {"harness": "claude_code", "version": "2.1.311", "session_id": "…", "location": "…"},
  "library": {"name": "shinatama-interchange", "version": "…", "reader": "codex@0.164.0", "writer": "claude_code@2.1.311"},
  "options": {"compaction": "portable", "summary": null, "reasoning": "drop_foreign", "resume_prompt": true},
  "counts":  {"messages_in": 18, "messages_out": 9, "tool_calls": 0, "reasoning_dropped": 1},
  "losses":  ["reasoning dropped: made by another provider protocol (1)", "interrupted_last_turn"],
  "strict":  {"checked": true, "violations": []}
}
```

The counts are `messages_in`, `messages_out`, `tool_calls`, `tools_text_form`,
`reasoning_dropped`, `reasoning_as_text`, `dropped_orphan_results`, `dropped_duplicate_results` (a second answer to a
call, A3), `rewritten_call_ids` (ids outside Anthropic's `^[a-zA-Z0-9_-]+$`, rewritten for the
call and its result), `dropped_off_branch` (records of a branch the source left: a rewind, an
edited prompt, Pi tree navigation), `unanswered_calls`, `dropped_empty_steps`,
`dropped_attachments`, `dropped_result_blocks` and `dropped_unreadable_summaries` (a compaction
summary with no readable text). Every count above zero also has its line in
`losses`.

## 7. Strict check (what a real API would refuse)

The provider rules [A] include: Anthropic **A1** every `tool_use` answered in the next message, **A2**
every `tool_result` answers the previous assistant message, **A3** no double answer, **A4**
`thinking` carries `signature`; Responses **R1/R2** every `function_call` has its output and vice
versa, **R3** (warning) reasoning without `encrypted_content` under `store:false`.

The library runs the same invariants on the `Session` **before** writing (the request
itself is built later by the destination): parallel calls grouped in one message, each grouped
call answered in the next entry, no `ToolResult` without its `ToolCall`, no call answered twice
(A3), native results before the text that answers a call carried as text (Pi ends the tool flow at
the first user text, §6.2), call ids in Anthropic's pattern toward an Anthropic-protocol
destination, and no `Reasoning` carried natively unless its payload has a place in the
destination's format (a Claude signature as `thinking`, an encrypted payload as
`redacted_thinking`; Pi a `thinkingSignature`; Codex `encrypted_content`). A violation is fixed by policy
(drop the orphan, drop the reasoning) and written to the manifest, or the hop fails with
`StrictCheckFailed`.

## 8. Reader and writer requirements

Ids **L1–L14** describe reader and writer requirements; consult the tests and verification records
for evidence about individual behaviors.

### 8.1 Readers

| id | requirement | evidence | acceptance |
|---|---|---|---|
| L4 | every reader turns the native compaction record into a `Compaction` entry (Claude `compact_boundary` + `isCompactSummary` row + `preservedSegment`; Codex `compacted` + `replacement_history`; OpenCode `compaction` + `metadata.keepID`; Pi `compaction` + `firstKeptEntryId`) | §2 | `Portable{SourceRecord}` works for all 12 pairs without a supplied summary |
| L5 | Claude reader: `isMeta`, `<local-command-*>`/`<command-name>` rows, `isCompactSummary`, `isVisibleInTranscriptOnly`, attachment rows → `Meta`; rows merged by `message.id`; results paired by `tool_use_id`, not by the parent chain | §2.1 | Claude → Pi tail strict clean; no `function_call "{}"` into Codex |
| L6 | readers keep `signature`/`encrypted` with protocol + model (`Opaque`) | §3.3 | OpenCode/Pi → Claude strict clean with `DropForeign` |
| L9 | Codex `<turn_aborted>`/`turn_aborted` and a hard-killed last turn → `StepEnd::aborted`/`killed`; Claude `<synthetic>` reply → `Meta`; OpenCode empty assistant without `finish` → `killed` | §2 | manifest `interrupted_last_turn`; no marker text in the destination |
| L10 | Codex Desktop code-mode `exec`: a script that is exactly one `tools.exec_command` / `tools.apply_patch` call → that call; any other script → one text-form call | census: 4844 of 4908 real Codex calls are `exec` | a Codex Desktop rollout converts with shell and patch calls translated |

### 8.2 Writers

| id | requirement | evidence | acceptance |
|---|---|---|---|
| L1 | every `opencode` command spawned with `--standalone` | §1.3 rule 3 | intended `opencode.db` is used |
| L2 | Claude writer: one row per content block sharing `message.id`; each result row parented on its own tool_use row | §6.2 | Codex → Claude full: both results present |
| L3 | Pi writer: all calls of one `Step` in one assistant `message`, then one `toolResult` entry per result | §6.2 | any → Pi full: strict clean, no `No result provided` |
| L7 | Codex writer: `session_meta` with Codex's own fields (`originator`, `cli_version` of the destination, `model_provider` of the destination config); no stale `threads` row in `state_5.sqlite` | P5 | `codex resume` lists and resumes the thread |
| L8 | OpenCode writer: one assistant row per `Step` (`[reasoning, text, tool, tool]`), through `session import` | §6.2; OC2 | any → OpenCode full: calls grouped |
| L11 | translate tools only into tools the destination offers, else text form | [`tool-conversion-tables.md`](tool-conversion-tables.md) §1, §2, §6 | no destination request carries a tool name outside its offered set |
| L12 | Pi `bash.timeout` seconds ↔ canonical milliseconds | tool schemas [V captures] | Claude → Pi → Claude keeps the timeout |
| L13 | argument-level equivalence decides translate vs text form (`replace_all`, `output_mode`, context lines, `pages`, background, shell dialect) | conversion tables §4 | a `replace_all: true` edit reaches Pi as text form |
| L14 | subagent, skill, question, MCP and harness-internal tools → text form in every direction | conversion tables §3 | no such call is translated |

Writers also apply `ReasoningPolicy` (§3.3) and the compaction mode (§4); the native compaction
record forms of §4.4 are written only after a fixture proves the destination accepts them.

## 9. Verification

Run `cargo test` for automated checks. The [verification records](README.md#verification-records)
describe additional harness runs, their environments and their coverage limitations.

## 10. Open

- [U] Hand-written native compaction records for Claude Code and Pi (§4.4).
- [U] §4.5 against Codex's real model (the run was made against a mock), and a session larger than
  the source model's context window.
- [U] Codex reasoning without `encrypted_content` against the real Responses API (R3).
- [U] Codex 0.165: a chain of pages (a page whose base is itself a page) is read by following
  each `history_base` back to the first file, the latest version winning; tested on hand-made files
  only, since none has been seen on disk (one page in 982 rollouts, its base a plain file).
- Not implemented: L10, reading a Codex Desktop code-mode `exec` script as the single
  `tools.exec_command` / `tools.apply_patch` call it wraps. An `exec` call is read as one opaque
  call and travels in text form in every direction; an `apply_patch` call reads as an edit but
  carries no path, so it is text form too. Most calls of a Codex Desktop rollout are of this kind
  (tables §2).
- Open verification: auto-compaction by threshold, a second compaction, a rewind or
  tree navigation in the source (the readers follow the resumed branch, tested on hand-made
  records; Claude `--fork-session` and Pi fork shapes are recorded, not converted), subagent
  sessions, images and attachments (converted between the four shapes; only OpenCode's verified by
  a real import), sessions larger than a few turns.
- The source sessions were generated before two mock fixes; their T3 reply is the mock's summary
  text, not a harness behaviour (FINDINGS of each leg).
