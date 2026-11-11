# Session interchange library: implementation spec (verified)

**What this is for.** The normative contract of mukade's session interchange: the model every
reader produces (§3), what each reader and writer must do (requirements **L1–L14**, §8), the
compaction modes (§4), the resume prompt (§5), the manifest (§6.3) and the strict check (§7).
Check a change against its requirement id; cite sections from code and docs as `spec §N`.

**Where things stand (2026-11-11).** Every requirement L1–L14 is built and covered by a test
(§8). What is still unverified is in §10.

| | |
|---|---|
| Written | 2026-10-27; checked against the code 2026-11-11 |
| Status | draft; every behaviour below was run on this machine unless marked otherwise |
| Origin | mukade began as Shinatama's session-interchange library (`src/lib.rs`). This spec is the companion of Shinatama's technical design (5.3, 5.4, 5.4.1, 5.5, 6.4), which stays with that product's documentation |
| Evidence | the [verification records](README.md#verification-records); `[V: <path>]` names a file of Shinatama's bench, whose fixtures the tests carry under `fixtures/` |
| Background | [`reference/harness-session-persistence-reference.md`](reference/harness-session-persistence-reference.md): secondary sources, superseded here for the four harnesses below |

**Evidence labels.** See the verification records for the scope and limits of the tests they
document.

| label | meaning |
|---|---|
| **[V]** | a reported observation; it does not by itself establish current behaviour |
| **[S]** | read in source code, not exercised |
| **[A]** | a public API rule, not exercised against the real API |
| **[U]** | not established |

## 0. What the library does, and what it does not

It moves one coding-agent conversation from a **source** harness to a **destination** harness, so
that the destination can continue it as one of its own sessions:

1. Read the source session from the source's native store.
2. Apply the hop policy: selection, redaction, **optional compaction**, reasoning policy.
3. Write a new native session into the destination's store.
4. Verify that the destination lists it and can resume it.
5. **Optional:** compact it with the destination's own compaction.
6. **Optional:** resume it with a prompt (the "resume prompt"), non-interactively.

**Harness versions the formats were described against:**

| harness | version | binary on this machine |
|---|---|---|
| Claude Code | 2.1.311 | `C:\Users\alfio\.local\bin\claude.exe` |
| Codex CLI | 0.164.0 | `C:\Users\alfio\AppData\Local\Programs\OpenAI\Codex\bin\codex.exe` |
| OpenCode | 2.1.4 | `%APPDATA%\npm\node_modules\@opencode\cli\bin\opencode.exe` |
| Pi | 1.2.0 (`@earendil-works/pi-coding-agent`) | `node …\pi-coding-agent\dist\bundle\cli.js` |

The installed binaries have moved on since. The live acceptances of 2026-11-04 ran on Claude Code
2.1.318, Codex CLI 0.166.0, OpenCode 2.2.5 and Pi 1.4.0
(`docs/verification/native-compaction-live.md`, `docs/verification/codex-encrypted-reasoning-live.md`).

**One reader and one writer per harness, the library's own.** It reads and writes the four native
formats itself. Where a harness has its **own** import path, the writer uses it:

| destination | how the writer writes | why |
|---|---|---|
| OpenCode | `session import` | lossless round trip (OC2 [V]) |
| Codex | writes the rollout itself (§2.2 says which files Codex needs) | Codex's importer (`externalAgentConfig`, CX4 [V]) turns tool calls into text markers |
| Claude Code, Pi | write the session file | no session import |

**Out of scope:**
- harnesses other than the four above (the reference report lists them);
- writing into an existing session;
- writing while the destination has the same store open for that session;
- replaying one provider's signed or encrypted reasoning to another provider.

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

| stage | what runs | result |
|---|---|---|
| **read** | the source harness's `Reader` (§8.1); technical design 5.2 rules apply | native records → `Session`, every source record kept verbatim in its entry's envelope (§3.1) |
| **write** | the destination's `Writer` (§8.2), through the harness's own import where one exists (OpenCode), else by writing the store files | a new native session: `Written{id, location}` and the native resume command (§2) |
| **verify**, **compaction (destination-native)**, **resume prompt** | the destination's own binary (§2), in the destination's environment | |

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
    Native(NativeBudget),                // whole selected history + the destination's own deterministic record (§4.4)
    DestinationNative,                   // refused: the destination's own compaction is the caller's to run
}
pub enum SummarySource { SourceRecord, Supplied(String),        // SourceRecord: the source's native compaction record (L4)
                         SourceModel(SummarizeOpts), Generated(Summary) }  // §4.5: written by the source's model
pub enum KeepFrom { SourceBoundary, LastTurns(usize), Message(String) }  // LastTurns(0): the handoff alone

pub struct ResumePrompt { pub text: String, pub timeout: Duration }  // ResumePrompt::new: 300 s

pub enum ReasoningPolicy {
    DropForeign,     // default: keep reasoning only when source and dest use the same provider protocol
                     // and its payload has a place in the destination's format; otherwise drop it
    AsText,          // keep foreign reasoning as visible text, labelled, before the answer
    Drop,            // never carry reasoning
}

pub struct Receipt {
    pub dest_id: String, pub location: String,
    pub resume_command: Vec<String>,     // native resume command of the destination (§2, §5)
    pub manifest: serde_json::Value,     // §6.3, as written
    pub resume_output: Option<String>,   // the destination's reply when a resume prompt was given
}

pub trait Reader { fn harness(&self) -> HarnessId;
                   fn list(&self, roots: &StoreRoots) -> anyhow::Result<Vec<SessionInfo>>;      // root sessions
                   fn list_all(&self, roots: &StoreRoots) -> anyhow::Result<Vec<SessionInfo>>;  // forks included
                   fn read(&self, id: &str, roots: &StoreRoots) -> anyhow::Result<Session>; }
pub trait Writer { fn harness(&self) -> HarnessId; fn offered(&self) -> ToolSet;                  // tool tables §2/§6
                   fn write(&self, s: &Session, opts: &WriteOpts) -> anyhow::Result<Written>; }

pub fn hop(req: &HopRequest) -> Result<Receipt, HopError>;
pub fn hop_into(req: &HopRequest, destination: &StoreRoots) -> Result<Receipt, HopError>;  // destination in its own stores
pub fn export(src: &SessionRef, roots: &StoreRoots) -> Result<Session, HopError>;  // read + reader rules
pub fn export_path(harness: HarnessId, path: &Path) -> Result<Session, HopError>;  // a session file
pub fn export_all_branches(src: &SessionRef, roots: &StoreRoots) -> Result<Session, HopError>;   // Codex only
pub fn export_path_all_branches(harness: HarnessId, path: &Path) -> Result<Session, HopError>;  // Codex only
pub fn import(dest: HarnessId, s: &Session, opts: &ImportOpts) -> Result<Receipt, HopError>;
pub fn hop_session(dest: HarnessId, s: &Session, c: &Compaction, opts: &ImportOpts) -> Result<Receipt, HopError>;
pub fn compact(s: &Session, c: &Compaction, roots: &StoreRoots) -> Result<(Cow<Session>, Option<SummaryFacts>), HopError>;  // §4.5
pub fn document(s: &Session, summary: Option<&SummaryFacts>) -> Value;       // the `dump` document
pub fn read_document(doc: &Value) -> Result<Session, HopError>;
pub fn import_document(dest: HarnessId, doc: &Value, s: &Session, opts: &ImportOpts) -> Result<Receipt, HopError>;
```

All of these live in `src/hop.rs`; the traits in `src/reader/mod.rs` and `src/writer/mod.rs`.

**`HopError` → CLI exit code** (technical design 5.5.2: 0 done, 2 refused with a reason, 3 needs
the user, 4 failed and nothing changed, 5 failed and recovery is needed; `HopError::exit_code()`):

| kind | exit | when |
|---|---|---|
| `NotFound` | 2 | the store is not configured, the session is not there or the id is ambiguous (a reader's typed `Lookup`) |
| `Unsupported` | 2 | a pair or option this spec marks blocked |
| `SourceChanged` | 2 | a reviewed source fingerprint no longer matches (`--expect-sha256`) |
| `WriteFailed` | 4 | the destination refused the write, or it could not be written |
| `StrictCheckFailed` | 4 | §7 |
| `SummaryFailed` | 4 | §4.5: the source's model wrote no usable summary; nothing was written |
| `VerifyFailed` | 5 | the source is there but could not be read |
| `WriteUncertain` | 5 | an external import may have written; the error carries its ID and destination |
| `ResumeFailed` | 5 | the session was written; the error carries its receipt and manifest |

OpenCode IDs and metadata supplied by a host are checked in the explicitly selected destination
after import (`src/writer/opencode.rs` `confirm_import()`, read-only). An unconfirmed result must be
reconciled before retrying.

### 1.3 Rules that hold for every hop

| # | rule | detail |
|---|---|---|
| 1 | **New session only** | The destination session always gets a new id; the source is never modified. A second import of the same id is refused by the destination (OpenCode answers 409, OC2) and must not be retried with the same id. |
| 2 | **No real store in tests** | Every test sets the harness's root variables (§2) to a scratch folder and checks that the real store's fingerprint did not change [V: every `*/source/isolation*`, `gen-source.log.txt`, `resume/*.meta.json` `real store unchanged: True`]. |
| 3 | **No foreign process reaches the real home** | OpenCode's CLI hands work to a background service that runs with the user's real environment unless `--standalone` (or `--server`) is passed. Every OpenCode command the library spawns that touches the store (`session export`, `list`, `import`, `run`) carries `--standalone` (L1). Only the version probe, `opencode --version`, runs without it. |
| 4 | **Order is the native order** | Never timestamps: OpenCode `seq`, Codex `ordinal`, Claude and Pi parent links. |
| 5 | **Every loss is written into the manifest** (§6.3) | None is silent. |
| 6 | **No stray files; one executable** | See below. |

**Rule 6 in detail.** mukade is a single executable with no installer. Its own files are:

- the session it writes: a `.mukade-*.tmp` beside it, renamed into place;
- what `--output` / `--manifest` name;
- one data folder: `mukade-data` beside the executable by default, or the absolute path in
  `MUKADE_DATA_DIR` (`src/portable.rs`). It holds:
  - a probe cache in `cache/`;
  - for OpenCode, the document `session import` needs, as a file in `tmp/` (mode 0600, removed
    when the import ends, on failure too). A killed run's leftovers older than an hour are swept
    by the next run.

Where that folder cannot be made there is no cache, and an OpenCode import is refused with a
reason; no other place is used. By default nothing goes to the user profile, the application-data
folders, the registry or the system's temporary folder [V: `tests/portable.rs` runs the real binary
alone in a folder with every such location pointed at an empty sandbox and asserts it stays empty].

## 2. The four harnesses (verified facts the library depends on)

Each subsection gives isolation, commands, the native records the library must read and write, and
what the model receives on resume.

### 2.1 Claude Code 2.1.311

**Isolation [V].**
- Remove inherited `CLAUDE*` / `ANTHROPIC*` variables.
- Set `CLAUDE_CONFIG_DIR=<scratch>\home`, `ANTHROPIC_BASE_URL`, `ANTHROPIC_API_KEY`,
  `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`, `DISABLE_TELEMETRY=1`, `DISABLE_ERROR_REPORTING=1`,
  `DISABLE_AUTOUPDATER=1`.
- Scratch `home\.claude.json` with `hasCompletedOnboarding`, `customApiKeyResponses.approved`,
  `projects[<cwd>].hasTrustDialogAccepted`.
- stdin from an empty file (otherwise a 3 s wait).

**Commands [V]** (from the project dir, `--output-format json --dangerously-skip-permissions`):

| action | command | notes |
|---|---|---|
| new | `claude -p "<prompt>"` | JSON result carries `session_id` |
| resume + prompt | `claude -p --resume <id> "<prompt>"` | same id, appends to the same file; works from any cwd |
| compaction | `claude -p --resume <id> "/compact"` | trigger `manual`; result text `Compacted …` |
| fork | add `--fork-session` | new id, **no lineage field** |

**Store [V].** `<CLAUDE_CONFIG_DIR>\projects\<cwd with every non-alphanumeric char → ->\<uuid>.jsonl`
(a folder name over 200 characters is cut and suffixed with `-` and a base-36 hash of the path:
`src/roots.rs` `claude_project_dir_name()`). No index file is needed: a session file written outside
Claude Code resumed by id [V: `claude_code/resume/*-from-*.meta.json`].

**Records the library depends on [V: `claude_code/source/c45db7bc-….jsonl`].**
- One API response = **one row per content block**, all sharing `message.id`, chained by
  `parentUuid`. A reader merges rows by `message.id`.
- Thinking: `{"type":"thinking","thinking":…,"signature":…}`: **signature stored**.
- **Parallel calls:** the tool_use rows are chained; **each tool_result row's `parentUuid` is its
  own tool_use row**, results written in completion order. Walking parents from the leaf
  therefore loses a call or a result; pair by `tool_use_id`.
- **Compaction**, in file order:
  1. `system/compact_boundary` `{parentUuid:null, logicalParentUuid:<old leaf>,
     compactMetadata{trigger, preTokens, postTokens, preservedSegment{headUuid, anchorUuid, tailUuid},
     preservedMessages}}`;
  2. a `user` row `isCompactSummary:true, isVisibleInTranscriptOnly:true` holding
     `This session is being continued from a previous conversation … Summary: …`;
  3. the `/compact` command rows (`<local-command-caveat>` with `isMeta:true`, `<command-name>`,
     `<local-command-stdout>`);
  4. `attachment type:"file"` rows re-injecting files read before.

  The last assistant reply stays in place and is hung under the summary by `preservedSegment`.
- **Interrupted turn** (process killed): only the user row and an `api-request` row. On the next
  load Claude Code writes a synthetic assistant `"No response requested."` (`model:"<synthetic>"`,
  `stop_reason:"stop_sequence"`) and sends it.
- Each user turn is followed in the request by a `role:"system"` message (environment block).

**What the model receives on resume [V: `claude_code/source/capture-gen.jsonl`, `resume/source-resume.capture.jsonl`].**
- Before a compaction: everything, thinking with its signature.
- After: summary + preserved tail + later turns only. T1's tools and thinking are not sent; files
  read earlier come back as a system message.
- The resume prompt is the last user message.

**Implemented history reconstruction (2026-10-29; local tests, not a new live harness
verification).** The Claude Code reader rebuilds the selected branch from UUID parent links,
compaction metadata, response IDs and tool call/result IDs. File position is not conversation
order: it only breaks ties between otherwise independent recovered records. Timestamps are not used
to sort the history.

| topic | rule |
|---|---|
| **Branch selection** | `last-prompt.leafUuid` selects a checkpoint. A continuation descending from that checkpoint can supersede it. A new user branch after the checkpoint can also supersede it, provided the checkpoint's leaf was recorded before the checkpoint itself; a checkpoint flushed before its own leaf sets no rewind ordering barrier. Text beside tool results can start a new user turn. Without a checkpoint, the last non-sidechain user or assistant row with a UUID selects the branch. |
| **Recovery and pairing** | Split assistant blocks sharing `message.id` become one step, including blocks interleaved with results. Parallel results are paired by `tool_use_id`. UUID-less blocks, results, expanded slash-command prompts and compaction summaries are recovered only when their dependencies identify a safe position. An explicit conversational parent on an abandoned branch takes precedence over `sourceToolAssistantUUID`; such a result is not recovered into the active conversation. |
| **Compaction and preservation** | `logicalParentUuid` connects the earlier history to a compaction boundary. Preserved-message lists and segments are checked; a retained UUID inside a merged response resolves to that response's entry for portable compaction. Original source records remain in `Entry::raw`; excluded branch records become `Meta(OffBranch)` and are counted in the destination manifest, not replayed. |
| **Refusals** | Missing parents, cycles, duplicate UUIDs, contradictory preserved metadata and unplaceable or ambiguous conversation records are refused, not guessed. |
| **Verification limits** | Regression tests cover the review's branch-selection and abandoned-result cases, shuffled fixture rows, a 20,000-row reverse-written chain and native-format round trips through all four destinations. These are local parser/writer checks, not proof of live auto-compaction or rewind compatibility: those remain **[U]** against Claude Code itself. |

User-facing behaviour: [`usage.md`](usage.md#a-session-that-was-rewound-or-branched). Regressions:
`tests/claude_code_order.rs`. The observed native records above keep their original **[V]** scope.

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
  The reader also reads the rollouts `codex archive` moved to `archived_sessions\` (`list --archived`
  includes or hides them).
- A rollout alone resumes by id (Codex rebuilds its SQLite stores); a wrong date folder still works.
- **`state_5.sqlite` `threads.rollout_path` wins over the file on disk.** A row pointing at a
  missing file fails hard: `no rollout found for thread id … (code -32600)`. A writer must not
  leave a stale row (L7).
- The model context comes from the rollout, not from `thread_history_1.sqlite`.
- `thread/list` with default filters hides threads whose source is `exec`.
- No `session_index.jsonl` was written in the scratch home (the reader takes thread names from it
  when it is there).

**Records [V: `codex/source/rollout-…01a17e07….jsonl`].**
- Reasoning `{"type":"reasoning","summary":[…],"content":null,"encrypted_content":…}`: **encrypted content stored**.
- Parallel calls: consecutive `function_call` lines, then `function_call_output` lines, paired by `call_id`.
- Turns: `event_msg/task_started` … `task_complete`.
- **Compaction:** `compacted.payload = {message:"Another language model started … <summary>",
  replacement_history:[user T1, developer skills+permissions, user <environment_context>, user T2,
  user summary bridge], replacement_history_metadata, retained_context, window_number, window ids,
  compaction_response_id, latest_token_usage_record, resume_metadata}`.
  - Compaction is **local**: an ordinary `/v1/responses` call with `request_kind:"compaction"` and
    the prompt `You are performing a CONTEXT CHECKPOINT COMPACTION…`.
  - A `compacted` line reduced to `{message}` still resumes (P6).
- Graceful interrupt: user message `<turn_aborted>…</turn_aborted>` + `event_msg turn_aborted
  {reason:"interrupted"}`, sent to the model on resume. Hard kill: the file just ends.

**On resume [V].** Before compaction: full history with `encrypted_content`. After: base
developer message + `replacement_history` + later items + prompt; no `encrypted_content`.

**Codex Desktop 0.165.0-alpha.1.2 [V: two real threads, 2026-10-28].**
- **Paginated threads.** A thread whose later turns were redone is kept over two files
  (`history_mode: "paginated"`). A page `rollout-<ts>-<thread>_<page>.jsonl` starts with a
  `session_meta` carrying `history_base: {thread_id, end_ordinal_exclusive, end_byte_offset}` and
  continues the thread from that prefix of the earlier file; `state_5.sqlite` `threads.rollout_path`
  names the page.
- **How the reader reads one.** The latest version: the prefix, then the page, following an earlier
  page's own base the same way. A page's earlier files are looked for only in its own home's
  `sessions/` and `archived_sessions/` (`src/reader/codex.rs` `page_folders()`). Each earlier
  file's later records are off-branch (§1.3, counted), or carried with `Branches::All`
  (`--all-branches`, Codex only).
- **Encrypted compaction summaries** (`compacted.message` empty; an item
  `{type:"compaction", encrypted_content}` in `replacement_history`): none is written as an empty
  message (`dropped_unreadable_summaries`), and `Portable` from such a record is refused.
- **Subagent messages to the root agent** (`agent_message`) carry their payload as
  `encrypted_content`, readable only by OpenAI, and nothing but a header in plain text: they are
  **excluded by decision** (2026-10-28), read as `Meta`, neither carried nor counted.
- **Subagent rollouts** name the thread that spawned them in `session_meta.session_id`; the reader
  puts it in `meta.parent` (never carried into a hop).

### 2.3 OpenCode 2.1.4

**Isolation [V].**
- `OPENCODE_DB`, `XDG_DATA_HOME` / `XDG_CONFIG_HOME` / `XDG_STATE_HOME` / `XDG_CACHE_HOME`,
  `OPENCODE_TEST_HOME` → scratch.
- `OPENCODE_DISABLE_MODELS_FETCH=1`, `OPENCODE_DISABLE_PROJECT_CONFIG=1`,
  `OPENCODE_DISABLE_CLAUDE_CODE=1`; remove `PWD`.
- Config (2.x schema): `{"providers":{"anthropic":{"settings":{"baseURL":…,"apiKey":…},"models":{…}}}}`.
- **Every command with `--standalone`** (§1.3 rule 3).

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
- Reasoning `{"type":"reasoning","text":…,"native":{"signature":…}}`: **signature stored**; sent
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

**Isolation [V].**
- `PI_CODING_AGENT_DIR=<scratch>\home`; `models.json` provider with `api:"anthropic-messages"`,
  `baseUrl`, `apiKey`; `settings.json` `compaction.keepRecentTokens`.
- Flags on every run: `--provider <p> --model <m> --offline --no-skills`. **`--no-skills` is
  required**: Pi reads `~\.agents\skills` whatever `PI_CODING_AGENT_DIR` says.
- Remove provider API-key variables from the environment.
- RPC stdin must be UTF-8 **without BOM**.

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
- **Tree.** Every entry has `id` and `parentId`.
  - Navigating back with `/tree` starts a new branch from an earlier entry, optionally through a
    `branch_summary` entry `{summary, fromId}`, and leaves the abandoned branch in the file.
  - Pi builds the context from the last entry back through `parentId`.
  - It shows a `branch_summary` on that path to the model as a user message
    `The following is a summary of a branch that this conversation came back from:\n\n<summary>\n…</summary>`
    [pi-coding-agent 1.2.0 `buildSessionPath`, `BRANCH_SUMMARY_PREFIX`].

**On resume [V].** After a compaction: system + user `The conversation history before this point
was compacted into the following summary:\n\n<summary>…</summary>` + entries from
`firstKeptEntryId` on + the prompt.

## 3. Interchange model

### 3.1 The library's own model (`Session`)

Three layers:

1. a **semantic core** every writer understands;
2. the **source record kept verbatim** in each entry: nothing is lost by reading;
3. the **target-specific** choices, made by the writer.

```rust
pub struct Session {
    pub meta: Meta,                    // source harness + version, id, cwd, title, created/updated,
                                       // model(s), git branch, parent (fork or Codex subagent; never
                                       // carried into a hop), source path and SHA-256
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
                                                             // re-injected file attachments, internal records,
                                                             // off-branch records, a cut-off last line: never conversation
}
pub struct ToolCall   { pub id: String, pub source_name: String, pub canonical: Option<CanonicalTool>,
                        pub input: serde_json::Value }      // canonical per tool-conversion-tables.md
pub struct ToolResult { pub call_id: String, pub content: ResultContent, pub is_error: bool,
                        pub truncated_by_source: bool }
pub struct Reasoning  { pub text: String, pub opaque: Option<Opaque> }
pub struct Opaque     { pub protocol: Protocol,              // anthropic-messages | openai-responses | …
                        pub model: Option<String>, pub signature: Option<String>, pub encrypted: Option<String>,
                        pub chat: Option<ChatReasoning> }
pub struct ChatReasoning { pub provider: Option<String>, pub details: Vec<serde_json::Value> }
```

Code: `src/model/` (`mod.rs`, `kind.rs`, `tool.rs`, `reasoning.rs`, `harness.rs`).

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

Tool calls follow [`tool-conversion-tables.md`](tool-conversion-tables.md) (last updated 2026-11-11: code mode L10, `apply_patch` reading, checked against `src/tools.rs`):

- **translated** into the destination's equivalent tool where the destination offers one and every
  argument used maps without changing its meaning;
- otherwise carried in **text form**;
- results verbatim; ids kept.

### 3.3 Reasoning

**What counts as proof.** Provider proof is conversation state.
- HTTP 200, a correct short answer or a high prompt-cache hit rate does not establish its preservation.
- A visible summary is not the signed/encrypted state it describes.
- Native store support is version- and provider-specific. Claude `signature`, Codex
  `encrypted_content`, OpenCode `native.signature` / `native.encrypted` / `native.reasoningDetails`,
  and Pi `thinkingSignature` are distinct contracts, not interchangeable spellings.

**Policy** (`src/policy.rs` `ReasoningPolicy`; CLI `--reasoning drop_foreign|as_text|drop`):

| policy | rule |
|---|---|
| `DropForeign` (default) | Carry reasoning only with a known compatible destination protocol, a present payload and a representable destination envelope. Otherwise record `reasoning_dropped` and its exact reason in the manifest. |
| `AsText` | Write only the available visible text, labelled `[earlier reasoning, not replayable]`, and increment `reasoning_as_text`. An encrypted-only block has no readable text and is counted as lost. |
| `Drop` | An explicit omission. |

Neither `AsText` nor `Drop` may be reported as native proof replay.

**OpenCode's Chat-compatible providers.** The additional `Opaque.chat` stores the source provider
ID and **the complete `reasoningDetails` array**, through both `Session` and the public document.
The scalar `signature` / `encrypted` fields are compatibility projections, not a substitute for the
envelope:

1. **Keep everything.** Every detail and its order, original assistant/tool owner, `type`,
   `format`, `id`, `index`, opaque payload and extension fields. Summaries may precede several
   encrypted details. Never take only the first detail, concatenate opaque entries, invent an ID or
   regenerate a stored envelope.
2. **Two OpenAI routes.** `openai-native-responses-v1` identifies the native OpenAI route;
   `openai-responses-v1` the aggregator route. Both use the OpenAI Responses family, but their
   envelopes must not be renamed or mixed. Mixed/unknown declared formats do not inherit the first
   detail's protocol.
3. **Same provider only.** Same-provider OpenCode conversion keeps the original array verbatim.
   A missing or different custom provider identity does not prove that an envelope is replayable.
   A known single ordinary Anthropic detail keeps its existing scalar conversion; a multi-item or
   OpenAI Chat envelope is not silently reduced to one Codex/Claude/Pi payload. An unrepresentable
   conversion is counted as a loss, not written in an unsupported native field.
4. **Destination protocol.** OpenCode/Pi default to an unknown destination protocol unless the
   caller states it. `--dest-protocol openai-responses` names the reasoning protocol, even when the
   Gateway-facing HTTP envelope is Chat Completions. This flag does not override incompatible
   provider formats. The JanuraCode Session Bridge deliberately requests `as_text` on import and
   `drop` on Codex export: those are separate handoff policies, not ordinary tool-loop replay.
5. **Permanent regression:** `tests/reasoning_replay.rs`. It covers complete arrays,
   encrypted-only payload ownership after summaries, repeated public-document round trips,
   extension fields, provider mismatch, mixed formats and explicit loss policy. A native
   integration check must also pass converted sessions through the real destination importer and
   inspect persisted proof at the same owner. For a claim of model continuation, observe at least
   two sequential tool steps (three model requests) and the outgoing proof: import success alone
   does not establish provider replay or acceptance.

**Rules for changes.** These requirements apply to reader, model, document, planning, writer,
persistence and resume changes.
- Keep a failing-before/passing-after regression when fixing a loss.
- Compare IDs, formats and payload lengths/hashes; do not persist raw production conversations or
  encrypted payloads merely for diagnostics.
- The verification record must separate source fixtures, native import/storage, outgoing client
  requests, and paid provider acceptance.
- Long-loop answer quality and cost are not inferred from a short successful transfer.

## 4. Compaction between hops (optional)

Selected through `HopRequest.compaction` (CLI `--compaction`).

| mode | CLI | what is written | model called | § |
|---|---|---|---|---|
| `None` | `none` (default) | the whole selected history | none | 4.1 |
| `Portable { SourceRecord \| Supplied, … }` | `portable` (`SourceRecord`, from the source's boundary; `Supplied` is library-only) | handoff message + summary + tail | none | 4.2 |
| `DestinationNative` | none | refused by the library (`Unsupported`): the caller runs the destination's compaction | the destination's, run by the caller | 4.3 |
| `Native(NativeBudget)` | `native` (`document`, `hop`) | whole history + the destination's own deterministic record | none | 4.4 |
| `Portable { SourceModel, … }` | `summary` (`dump`, `hop`), or `--intent NAME` | handoff message + summary from the source's model + last `N` user turns | the source's | 4.5 |

### 4.1 `None`

The whole selected history is written. The destination sends the history back on resume, so the
context it starts with is as large as the source's **pre**-compaction history: the source's own
compaction is lost unless the reader keeps it (L4).

### 4.2 `Portable` (the library writes the compacted form)

One user message, then the tail from the boundary on:

```text
[mukade handoff v1] Summary of the earlier conversation, written by the source agent's own compaction:
<summary>
```

(`src/hop.rs` `PORTABLE_HANDOFF`.) The summary comes from:

| `SummarySource` | where the summary and boundary come from |
|---|---|
| `SourceRecord` (needs L4) | the source's native compaction record. Boundary: Claude `preservedSegment` / next after the summary; Codex `retained_context` / `replacement_history`; OpenCode `metadata.keepID`; Pi `firstKeptEntryId` |
| `Supplied` | caller-supplied text, e.g. produced by a low-cost model per technical design 6.4 |

### 4.3 `DestinationNative` (the destination compacts the imported session)

Write the whole history, run the destination's own compaction (§2 commands), then resume.

**Costs:** one extra model call on the destination's model, and the destination must accept the
full history once (the context window must hold it).

| prefer | when |
|---|---|
| `Portable` | the source already has a compaction (its summary is free and already written), or the full history does not fit the destination's window |
| `DestinationNative` | the summary should be written by the destination's model in the destination's own format |

### 4.4 Writing a native compaction record

The best form of `Portable` is a **native** compaction record in the destination: the destination
then treats the boundary as its own.

**Feasibility found first:** Codex accepts a minimal `compacted{message}` line (P6 [V]); OpenCode's
import round-trips a `compaction` message losslessly (OC2 [V]); Claude Code (`compact_boundary` +
`isCompactSummary` row) and Pi (`compaction` entry with `firstKeptEntryId`) were **[U]** for
hand-written records until the live acceptance below.

**Built for all four destinations.** `Compaction::Native(NativeBudget)` keeps the whole generated
history and appends the destination's own record after it:

| destination | record written |
|---|---|
| OpenCode | one completed `compaction` message, `{"type":"compaction","status":"completed","reason":"manual","summary":"…","metadata":{"keepID":"…"},"time":{"created":…}}`. `keepID` points backwards to the first generated message of the retained tail. |
| Pi | one `compaction` entry with `summary`, `firstKeptEntryId` and `tokensBefore`, chained after the retained rows. |
| Codex | one `compacted` record with `message` and `replacement_history`. The replacement history carries the summary as its first user input item, then the retained response items with their Mukade-generated top-level `id` fields removed: live Codex 0.166.0 refused such IDs and did not replay the record's `message` as context. |
| Claude Code | the native pair `system/compact_boundary` + a user row with `isCompactSummary:true,isVisibleInTranscriptOnly:true`, inserted before the picker rows. The boundary's `preservedSegment` and `preservedMessages` name the retained UUIDs and the summary row. |

**The summary** is deterministic and free (`src/native_compaction.rs`):
- the user requests in the superseded prefix, in order, each truncated to 1,000 characters;
- a 24,000-character body cap;
- an explicit mukade header: `[mukade native compaction] Deterministic summary of earlier user
  requests; the full history remains in this session.`

Every older row stays in the written session and readable there; the destination projects
summary + tail to the model. No model or destination process is called to make the boundary.

**The budget.** The caller supplies the destination facts: `--context-window`, optional
`--max-output-tokens` (default 16,000), optional `--text-token-factor`.
- Text is estimated as chars/3, multiplied by the Gateway factor when one is supplied and valid (≥ 1).
- One conservative budget: `min(window/2, openCodeCeiling(window) - roomForNextTurn)`, where
  `openCodeCeiling(window)` = window − max(15%, 16,000 for windows ≥ 32,000) and
  `roomForNextTurn` = max output tokens + 8,000 (`import_budget()`).
- OpenCode's trigger is verified; the other harnesses' thresholds are not all verified, so the
  common half-window bound is deliberately lower.

**Where it is accepted.** `document` and `hop` accept this mode. `document` reports it in
`native_compaction`; `hop` records `options.compaction = "native"`. `dump` refuses it (its
interchange document is harness-neutral), and `write` does not reapply a boundary to a document
that already exists.

**Live acceptance (2026-11-04)** with an oversized synthetic session (~7.5 MB source, far above
every model window): Claude Code 2.1.318, Codex 0.166.0, OpenCode 2.2.5 and Pi 1.4.0 each resumed
the written session, answered from the deterministic summary, and added no second native compaction
(`docs/verification/native-compaction-live.md`).

### 4.5 `Portable` with a summary from the source's model

`SummarySource::SourceModel(SummarizeOpts)` asks the source harness's own model for the summary
when the session moves.

**Flags:** `--compaction summary` or `--intent NAME`, with `--keep-turns N` (default 0),
`--summary-model` (default: the session's own), `--summary-timeout` (seconds, default 300).

**The prompt.** One prompt, the same for the four harnesses, chosen from the editable
`intent-prompts/<name>.md` files beside the executable (`generic` by default; embedded fallback
versioned `v1`). It is not the harness's native `/compact`: that writes into the source's store,
and Codex's native summary is encrypted.

**The run** goes through the source's binary, credentials and configuration, and saves nothing in
its store [V: `docs/verification/source-model-summary-live.md`; Codex against a mock]:

| source | how nothing is saved |
|---|---|
| Claude Code | `--no-session-persistence` |
| Codex | `exec fork --ephemeral` |
| Pi | `--fork` into a temporary folder |
| OpenCode | a fork deleted afterwards whatever came of the run, with every tool denied through `OPENCODE_CONFIG_CONTENT` |

**The result** is the shape of §4.2 with its own handoff line (`src/summary.rs`
`MODEL_SUMMARY_HANDOFF`), then the last `N` user turns (none by default):

```text
[mukade handoff v1] Summary of the earlier conversation, written by the source agent's model when the session was moved:
```

- `hop::compact` resolves the summary once, before the selection: `dump --compaction summary`
  exports the same document `hop` would write, and `write` imports it later.
- The document's top-level `summary` says who wrote it and which intent prompt was used, so the
  manifest `write` makes says so too.
- The manifest carries `options.summary` (`source: "source_model"`, harness, version, model,
  `prompt` naming the intent or `v1`, `chars`, `usage`, `left_out`) and one loss line.

**Failures and refusals.**
- A failed summary (an error, an empty reply, a deadline) is `HopError::SummaryFailed`, exit 4:
  nothing was written.
- Refused (exit 2): `--all-branches`, and `--from-file` for a source other than Pi.

## 5. Resume prompt (optional)

**How it runs** (`src/hop.rs` `run_resume_with()`, `src/exe.rs` `run_within()`):
- the destination's resume command, from the session's working directory;
- the caller's environment, with the destination's store variable set to the root the write used;
- stdin closed;
- a timeout (`ResumePrompt::new`: 300 s) that stops the harness **and everything it started**:
  - Windows: `taskkill /PID <pid> /T /F`, which also reaches a harness behind a `.cmd` launcher;
  - elsewhere: the tree is read from parent ids (`/proc`, or `ps`), stopped with `SIGSTOP`, read
    again, then killed. The child stays in mukade's process group, so Ctrl-C still reaches the
    harness. A process that detached itself is not found (`kill_tree()`).

**The resume command each writer returns** (`catalog/harnesses.json` `resume`; `{exe}` is the
resolved executable, the prompt is appended):

| destination | command |
|---|---|
| Claude Code | `claude -p --resume <id>` |
| Codex | `codex exec resume <id>` |
| OpenCode | `opencode run --standalone --session <id>` |
| Pi | `pi -p --session <file>` |

**Results.**
- Success: the receipt keeps stdout as `resume_output`.
- Failure: `ResumeFailed` carries the written receipt; its reason names the command, the exit status
  and stderr; the manifest records the failed resume as a loss.
- Refused before anything is written (`Unsupported`): a prompt that starts with `-` (every harness
  CLI reads it as an option), and a prompt with line breaks toward a Windows `.cmd` launcher.

**Cautions [V].**

| harness | caution |
|---|---|
| OpenCode | `run --session <unknown id>` creates a new session: the library checks the id with `session export` (or the DB, read-only) first. |
| Codex | threads written with source `exec` are hidden from `thread/list` defaults; resume by id works. |
| Pi | `--session <id>` searches the current project first; the library passes the file path. |
| Claude Code | resume works from any cwd but appends rows with the new cwd; run from the session's cwd. |

**Interrupted turns** (`--interrupted keep|drop`; `hop`, `write`, `document`).
- An **interrupted turn** is one the source never finished. It holds a step that was killed or
  aborted, or the source's own abort marker after its work (Codex `<turn_aborted>`, Claude Code
  `[Request interrupted by user]`; both are `Meta`, never user text). The step the marker cut is
  `aborted` and keeps what it holds.
- `keep` (default): the turn's work travels, so a session with no finished turn is still carried whole.
- `drop`: every step and result of such a turn is left out and counted
  (`dropped_interrupted_steps`); its prompt stays.
- An **interrupted last turn** in the source is carried as a user message without a reply. On
  resume Codex, OpenCode and Pi send two consecutive user messages; Claude Code inserts its
  synthetic `No response requested.`. Both are accepted. The manifest records `interrupted_last_turn`.

## 6. Destination constraints and manifest

### 6.2 Tool and reasoning constraints

| destination | constraint | req |
|---|---|---|
| Claude Code | each tool result is associated with its own tool call | L2 |
| Pi | parallel calls grouped in one assistant message, followed by their results | L3 |
| any | signed reasoning needs its original compatible payload; otherwise the configured reasoning policy decides whether it is dropped or carried as text | L6 |
| OpenCode | an assistant step's content and tool calls grouped in one message | L8 |

### 6.3 Manifest (written with every hop)

```json
{
  "source": {"harness": "codex", "version": "0.164.0", "session_id": "…", "content_sha256": "…"},
  "dest":   {"harness": "claude_code", "version": "2.1.311", "session_id": "…", "location": "…"},
  "library": {"name": "mukade", "version": "…", "reader": "codex@0.164.0", "writer": "claude_code@2.1.311"},
  "options": {"compaction": "portable", "summary": null, "reasoning": "drop_foreign", "resume_prompt": true,
              "without_tools": false, "interrupted": "keep"},
  "counts":  {"messages_in": 18, "messages_out": 9, "tool_calls": 0, "reasoning_dropped": 1},
  "losses":  ["reasoning dropped: made by another provider protocol (1)", "interrupted_last_turn"],
  "strict":  {"checked": true, "violations": []}
}
```

Built by `src/manifest.rs` `Manifest::to_json()` from `src/writer/plan.rs` `Counts`. **Every count
above zero also has its line in `losses`.**

**Counts: what travelled**

| count | meaning |
|---|---|
| `messages_in` | conversation entries in the source (`Meta` excluded) |
| `messages_out` | blocks the destination receives |
| `tool_calls` | calls translated into a destination tool |
| `tools_text_form` | calls carried in text form |
| `reasoning_as_text` | reasoning blocks written as labelled text |

**Counts: what was left out on purpose**

| count | meaning |
|---|---|
| `tools_omitted` | calls omitted with their results (`--without-tools`) |
| `reasoning_dropped` | reasoning blocks not carried (§3.3) |
| `dropped_off_branch` | records of a branch the source left: a rewind, an edited prompt, Pi tree navigation |
| `dropped_interrupted_steps` | steps of unfinished turns, with `interrupted: drop` (§5) |

**Counts: what was repaired or lost**

| count | meaning |
|---|---|
| `dropped_orphan_results` | a result whose call is nowhere in the source |
| `dropped_duplicate_results` | a second answer to a call (A3) |
| `rewritten_call_ids` | ids outside Anthropic's `^[a-zA-Z0-9_-]+$`, rewritten for the call and its result |
| `unanswered_calls` | a call the source never answered |
| `dropped_empty_steps` | a step that lost everything and cannot be an empty message |

**Counts: what a destination could not hold**

| count | meaning |
|---|---|
| `dropped_attachments` | user attachments the destination cannot carry |
| `dropped_result_blocks` | structured result blocks flattened for a text-only destination |
| `dropped_unreadable_summaries` | a compaction summary with no readable text |
| `dropped_unfinished_lines` | a last source line cut off mid-write, kept as text and not read |
| `aborted_steps_as_replies` | aborted or failed steps written as ordinary replies by a destination with no way to mark them (Pi keeps the step's end) |

## 7. Strict check (what a real API would refuse)

**Provider rules [A]:**

| id | protocol | rule |
|---|---|---|
| A1 | Anthropic | every `tool_use` answered in the next message |
| A2 | Anthropic | every `tool_result` answers the previous assistant message |
| A3 | Anthropic | no double answer |
| A4 | Anthropic | `thinking` carries `signature` |
| R1 / R2 | Responses | every `function_call` has its output, and vice versa |
| R3 | Responses | (warning) reasoning without `encrypted_content` under `store:false` |

**What the library checks** on the write plan, **before** writing (`src/strict.rs` `check_plan()`
over `src/writer/plan.rs` `plan()`; the request itself is built later by the destination):

- parallel calls grouped in one message;
- each grouped call answered in the next entry;
- no `ToolResult` without its `ToolCall`, and no call answered twice (A3);
- native results before the text that answers a call carried as text (Pi ends the tool flow at the
  first user text, §6.2);
- call ids in Anthropic's pattern toward an Anthropic-protocol destination;
- no `Reasoning` carried natively unless its payload has a place in the destination's format: a
  Claude signature as `thinking`, an encrypted payload as `redacted_thinking`; Pi a
  `thinkingSignature`; Codex `encrypted_content`.

**On a violation:** the plan fixes it by policy (drop the orphan, carry an unanswered call as text,
drop the reasoning) and writes it to the manifest; what policy cannot fix fails the hop with
`StrictCheckFailed`, before anything is written.

## 8. Reader and writer requirements

Ids **L1–L14**. Status is as of 2026-11-11; the test named is the one that checks the acceptance.
For live evidence, see the verification records.

### 8.1 Readers

| id | requirement | evidence | acceptance | status |
|---|---|---|---|---|
| L4 | every reader turns the native compaction record into a `Compaction` entry (Claude `compact_boundary` + `isCompactSummary` row + `preservedSegment`; Codex `compacted` + `replacement_history`; OpenCode `compaction` + `metadata.keepID`; Pi `compaction` + `firstKeptEntryId`) | §2 | `Portable{SourceRecord}` works for all 12 pairs without a supplied summary | **built**: one test per reader (`tests/claude_code_reader.rs` `native_compaction_becomes_one_compaction_entry`, `tests/codex_reader.rs` `compaction_keeps_the_summary_and_folds_the_summary_turn`, OpenCode and Pi alike); a portable hop is tested Claude Code → Pi (`tests/hop.rs` `portable_carries_the_source_summary_and_the_tail_from_its_boundary`), not all 12 pairs. Codex 0.165 encrypted summaries: refused (§2.2) |
| L5 | Claude reader: `isMeta`, `<local-command-*>`/`<command-name>` rows, `isCompactSummary`, `isVisibleInTranscriptOnly`, attachment rows → `Meta`; rows merged by `message.id`; results paired by `tool_use_id`, not by the parent chain | §2.1 | Claude → Pi tail strict clean; no `function_call "{}"` into Codex | **built**: `tests/claude_code_reader.rs` `results_are_paired_by_tool_use_id_not_by_parent_chain`, `transcript_only_and_internal_rows_are_meta` |
| L6 | readers keep `signature`/`encrypted` with protocol + model (`Opaque`) | §3.3 | OpenCode/Pi → Claude strict clean with `DropForeign` | **built**: `reasoning_keeps_its_opaque_payload` and the L6 tests of all four reader test files; `tests/reasoning_replay.rs` |
| L9 | Codex `<turn_aborted>`/`turn_aborted` and a hard-killed last turn → `StepEnd::aborted`/`killed`; Claude `<synthetic>` reply and `[Request interrupted by user]` → `Meta`, the step it cut → `aborted`; OpenCode empty assistant without `finish` → `killed` | §2 | manifest `interrupted_last_turn`; no marker text in the destination; `interrupted: drop` leaves the turn's work out | **built**: `tests/codex_reader.rs` `a_reply_cut_by_an_abort_is_an_aborted_step`, `tests/opencode_reader.rs` `a_row_without_finish_is_a_killed_step_and_the_export_omits_it`, `killed_last_turn_is_a_user_entry_with_no_reply` (Claude Code, Pi) |
| L10 | Codex Desktop code-mode `exec`: a script made only of literal `text(await tools.X(...));` statements, whose output holds the header and one text item per statement → those calls, each with its own result (first id the `exec` call's, then `<id>_<n>`); any other script → one text-form call. Codex `apply_patch` input → `Write` / `Edit` / multi-edit when it is one plain file section | census: 4844 of 4908 real Codex calls are `exec`; 2026-11-11: 725 of 981 scripts split (`docs/verification/2026-11-11-codex-code-mode.md`) | a Codex Desktop rollout converts with shell and patch calls translated | **built** 2026-11-11 (`src/reader/codex/exec.rs`, `src/tools/patch.rs`): `tests/codex_reader.rs` `a_sequence_reads_as_calls_with_their_own_results`, `tests/hop.rs` `a_code_mode_rollout_reaches_claude_and_pi_with_native_calls`, census `tests/codex_code_mode_census.rs` (ignored; reads a real `CODEX_HOME`). Live hop of a real Desktop thread: open (§10) |

### 8.2 Writers

| id | requirement | evidence | acceptance | status |
|---|---|---|---|---|
| L1 | every `opencode` command spawned with `--standalone` | §1.3 rule 3 | intended `opencode.db` is used | **built**: `tests/writer_opencode.rs` `the_import_command_always_carries_standalone` |
| L2 | Claude writer: one row per content block sharing `message.id`; each result row parented on its own tool_use row | §6.2 | Codex → Claude full: both results present | **built**: `tests/writer_claude_code.rs` `each_step_is_one_row_per_block_sharing_a_message_id`, `every_result_row_hangs_on_its_own_tool_use_row` |
| L3 | Pi writer: all calls of one `Step` in one assistant `message`, then one `toolResult` entry per result | §6.2 | any → Pi full: strict clean, no `No result provided` | **built**: `tests/writer_pi.rs` `both_parallel_calls_are_in_one_assistant_message_with_one_result_each` |
| L7 | Codex writer: `session_meta` with Codex's own fields (`originator`, `cli_version` of the destination, `model_provider` of the destination config); no stale `threads` row in `state_5.sqlite` | P5 | `codex resume` lists and resumes the thread | **built**: `tests/writer_codex.rs` `session_meta_states_the_destination_and_never_the_source`, `no_database_is_touched` |
| L8 | OpenCode writer: one assistant row per `Step` (`[reasoning, text, tool, tool]`), through `session import` | §6.2; OC2 | any → OpenCode full: calls grouped | **built**: `tests/writer_opencode.rs` `one_assistant_message_per_step_holds_both_calls_and_their_results` |
| L11 | translate tools only into tools the destination offers, else text form | [`tool-conversion-tables.md`](tool-conversion-tables.md) §1, §2, §6 | no destination request carries a tool name outside its offered set | **built**: `tests/tools_translation.rs` `every_translated_name_is_offered_by_its_destination`. Destination default shell: open (§10) |
| L12 | Pi `bash.timeout` seconds ↔ canonical milliseconds | tool schemas [V captures] | Claude → Pi → Claude keeps the timeout | **built**: `tests/tools_translation.rs` `shell_timeout_is_converted_for_pi_and_kept_in_milliseconds_elsewhere` |
| L13 | argument-level equivalence decides translate vs text form (`replace_all`, `output_mode`, context lines, `pages`, background, shell dialect) | conversion tables §4 | a `replace_all: true` edit reaches Pi as text form | **built**: `tests/tools_translation.rs` `replace_all_blocks_pi_and_codex_and_travels_to_opencode` |
| L14 | subagent, skill, question, MCP and harness-internal tools → text form in every direction | conversion tables §3 | no such call is translated | **built**: `tests/tools_translation.rs` `capability_tools_are_text_form_in_every_direction` |

**Also on writers.**
- They apply `ReasoningPolicy` (§3.3) and the compaction mode (§4).
- The native compaction records of §4.4 are built for all four destinations and read back by
  Mukade; each real binary live-accepted its record on 2026-11-04
  (`docs/verification/native-compaction-live.md`).
- Codex 0.166.0 resumed a written copy with real encrypted reasoning byte-identical, and refused a
  one-character tamper of it (`docs/verification/codex-encrypted-reasoning-live.md`).

## 9. Verification

1. Run `cargo test` for the automated checks (the whole workspace: `cargo test --workspace
   --no-fail-fast`).
2. The conformance bench is the test: each reader is checked against its harness's native fixtures
   (`fixtures/<harness>/`, from Shinatama's bench) in `tests/<harness>_reader.rs`, each writer in
   `tests/writer_<harness>.rs`, round trips in `tests/writer_roundtrip.rs`, whole hops in
   `tests/hop.rs`.
3. The [verification records](README.md#verification-records) describe the harness runs beyond the
   tests, their environments and their coverage limits.

## 10. Open

Each item is **[U]** unless it says otherwise. The step-by-step checks are in `next.md`.

**Compaction and summaries**

| item | state |
|---|---|
| Destination-specific native-compaction budget thresholds other than OpenCode's verified ceiling | [U]. The live acceptance of the hand-written records themselves is [V] for Claude Code 2.1.318, Codex 0.166.0, OpenCode 2.2.5 and Pi 1.4.0 (2026-11-04). |
| §4.5 against Codex's real model | [U]: the run was made against a mock |
| §4.5 with a session larger than the source model's context window | [U]: it must end `summary failed`, never half a summary |

**Reasoning (§3.3)**

| item | state |
|---|---|
| Codex reasoning without `encrypted_content` against the real Responses API (R3) | [U] |
| Claude Code signed thinking validated by the real API (a tampered copy refused) | [U] (`next.md`) |
| OpenCode's own `anthropic` provider receiving `native.signature` / `native.encrypted` | [U]: only `openrouter` is verified live (`next.md`) |
| OpenAI encrypted reasoning through OpenRouter, written as `reasoning.encrypted` | [U]: never exercised (`next.md`) |

**Codex**

| item | state |
|---|---|
| Codex 0.165: a chain of pages (a page whose base is itself a page) | [U]: read by following each `history_base` back to the first file, the latest version winning; tested on hand-made files only, since none has been seen on disk (one page in 982 rollouts, its base a plain file) |
| L10 limits, by design | Code mode is read without evaluating it: a script with variables, `Promise.allSettled`, `ALL_TOOLS`, `image(...)`, or an output whose items do not match its statements, stays one opaque call in text form. |
| L10 shell dialect | A split `exec_command` keeps the source's shell dialect: the calls of a PowerShell-declared Desktop rollout translate only toward a PowerShell destination (`--dest-shell powershell`). The destination's default shell is open (L11, `next.md`). |
| L10 on a real Desktop thread | The read-only census passed; a live hop into a destination is not done yet (`next.md`). |

**Process control (§5)**

| item | state |
|---|---|
| Windows: the timeout still stops the whole tree | unchanged code, no automated test (`next.md`) |
| macOS: the tree kill through `ps -A` | [U] (Linux uses `/proc`; same logic) |

**Open verification of real-life shapes**
- Auto-compaction by threshold; a second compaction.
- A rewind or tree navigation in the source: the readers follow the resumed branch, tested on
  hand-made records. Claude `--fork-session` and Pi fork shapes are recorded, not converted.
- Subagent sessions.
- Images and attachments: converted between the four shapes; only OpenCode's verified by a real import.
- Sessions larger than a few turns.

**Bench note.** The source sessions were generated before two mock fixes: their T3 reply is the
mock's summary text, not a harness behaviour (FINDINGS of each leg).
