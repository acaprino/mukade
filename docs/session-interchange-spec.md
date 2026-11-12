# Session interchange library: implementation spec (verified)

**What this is for.** The normative contract of mukade's session interchange: the model every
reader produces (§3), what each reader and writer must do (requirements **L1–L14**, §8), compaction
modes (§4), the resume prompt (§5), the manifest (§6.3), the strict check (§7). Check a change
against its requirement id; cite sections as `spec §N`.

| | |
|---|---|
| Written | 2026-10-27; checked against the code 2026-11-11 |
| Status | 2026-11-11: draft; L1–L14 built and tested (§8), open items in §10; every behaviour below was run on this machine unless marked |
| Origin | mukade began as Shinatama's session-interchange library (`src/lib.rs`); this spec companions Shinatama's technical design (5.3, 5.4, 5.4.1, 5.5, 6.4), kept with that product's documentation |
| Evidence | the [verification records](README.md#verification-records); `[V: <path>]` names a file of Shinatama's bench (fixtures under `fixtures/`) |
| Background | [`reference/harness-session-persistence-reference.md`](reference/harness-session-persistence-reference.md): secondary sources, superseded here for the six harnesses |

**Evidence labels** (scope and limits of the tests: the verification records).

| label | meaning |
|---|---|
| **[V]** | a reported observation; it does not by itself establish current behaviour |
| **[S]** | read in source code, not exercised |
| **[A]** | a public API rule, not exercised against the real API |
| **[U]** | not established |

## 0. What the library does, and what it does not

It moves one coding-agent conversation from a **source** harness to a **destination** harness, which
continues it as one of its own sessions:

1. Read the source session from the source's native store.
2. Apply the hop policy: selection, redaction, **optional compaction**, reasoning policy.
3. Write a new native session into the destination's store.
4. Verify that the destination lists and can resume it.
5. **Optional:** compact it with the destination's own compaction.
6. **Optional:** resume it non-interactively with a prompt (the "resume prompt").

**Versions the formats were described against:**

| harness | version | binary on this machine (catalog locations: [harness catalog](reference/harness-catalog.md#platform-locations)) |
|---|---|---|
| Claude Code | 2.1.311 | `{home}\.local\bin\claude.exe` |
| Codex CLI | 0.164.0 | `{local_app_data}\Programs\OpenAI\Codex\bin\codex.exe` |
| OpenCode | 2.1.4 | `{app_data}\npm\node_modules\@opencode\cli\bin\opencode.exe` |
| Pi | 1.2.0 (`@earendil-works/pi-coding-agent`) | `node …\pi-coding-agent\dist\bundle\cli.js` |

The binaries have since moved on: the live acceptances of 2026-11-04 ran on Claude Code 2.1.318,
Codex CLI 0.166.0, OpenCode 2.2.5 and Pi 1.4.0 (`docs/verification/native-compaction-live.md`,
`docs/verification/codex-encrypted-reasoning-live.md`).

**One reader and one writer per harness, the library's own**; where a harness has its **own**
import path, the writer uses it:

| destination | how the writer writes | why |
|---|---|---|
| OpenCode | `session import` | lossless round trip (OC2 [V]) |
| Codex | writes the rollout itself (§2.2 says which files Codex needs) | Codex's importer (`externalAgentConfig`, CX4 [V]) turns tool calls into text markers |
| Claude Code, Pi | write the session file | no session import |

**Out of scope:** harnesses other than the six (the reference report lists them); writing into an
existing session; writing while the destination has the same store open for that session; replaying
one provider's signed or encrypted reasoning to another provider.

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
| **read** | the source's `Reader` (§8.1); technical design 5.2 rules apply | native records → `Session`, every source record kept verbatim in its entry's envelope (§3.1) |
| **write** | the destination's `Writer` (§8.2): the harness's own import where one exists (OpenCode), else the store files | a new native session: `Written{id, location}` and the native resume command (§2) |
| **verify**, **compaction (destination-native)**, **resume prompt** | the destination's own binary (§2), in the destination's environment | |

### 1.2 Rust surface

```rust
pub struct HopRequest {
    pub source: SessionRef,              // harness + native id (an unambiguous prefix is accepted)
    pub dest: HarnessId,                 // claude_code | codex | opencode | pi | hermes | deepseek_harness
    pub cwd: Option<String>,             // project directory the destination files the session under
    pub compaction: Compaction,          // §4
    pub resume: Option<ResumePrompt>,    // §5
    pub reasoning: ReasoningPolicy,      // §3.3
    pub roots: StoreRoots,               // the stores; None is refused for Claude Code, Codex, Pi, Hermes, DeepSeek Harness;
                                         // "its configured store" for OpenCode
    pub write: WriteDetails,             // destination facts: version, protocol, shell, offered tools,
                                         // provider, model, project id, clock, executable
    pub children: bool,                  // also write the source's subagent sessions (§6.4)
    pub all_branches: bool,              // read the source with every branch it keeps (CLI --all-branches);
                                         // a source that keeps one branch is refused before anything is read
}
// HopRequest::new(source, dest, roots): every option at its default, so a caller names only what it changes

pub enum Compaction {
    None,                                // whole selected history
    Portable { summary: SummarySource, keep_from: KeepFrom },  // one user message + tail, written by us
    Native(NativeBudget),                // whole history + the destination's own deterministic record (§4.4)
    DestinationNative,                   // refused: the caller runs the destination's compaction
}
pub enum SummarySource { SourceRecord, Supplied(String),        // SourceRecord: the source's native compaction record (L4)
                         SourceModel(SummarizeOpts), Generated(Summary) }  // §4.5: written by the source's model
pub enum KeepFrom { SourceBoundary, LastTurns(usize), Message(String) }  // LastTurns(0): the handoff alone

pub struct ResumePrompt { pub text: String, pub timeout: Duration }  // ResumePrompt::new: 300 s

pub enum ReasoningPolicy { DropForeign /* default */, AsText, Drop }   // §3.3

pub struct Receipt {
    pub dest_id: String, pub location: String,
    pub resume_command: Vec<String>,     // the destination's native resume command (§5)
    pub manifest: serde_json::Value,     // §6.3, as written
    pub resume_output: Option<String>,   // the destination's reply to a resume prompt
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
pub fn export_all_branches(src: &SessionRef, roots: &StoreRoots) -> Result<Session, HopError>;   // where reads_all_branches
pub fn export_path_all_branches(harness: HarnessId, path: &Path) -> Result<Session, HopError>;  // where reads_all_branches
pub fn reads_all_branches(harness: HarnessId) -> bool;  // Claude Code, Codex, Pi today; asked without reading
pub fn import(dest: HarnessId, s: &Session, opts: &ImportOpts) -> Result<Receipt, HopError>;
pub fn hop_session(dest: HarnessId, s: &Session, c: &Compaction, opts: &ImportOpts) -> Result<Receipt, HopError>;
pub fn compact(s: &Session, c: &Compaction, roots: &StoreRoots) -> Result<(Cow<Session>, Option<SummaryFacts>), HopError>;  // §4.5
pub fn document(s: &Session, summary: Option<&SummaryFacts>) -> Value;       // the `dump` document
pub fn read_document(doc: &Value) -> Result<Session, HopError>;
pub fn import_document(dest: HarnessId, doc: &Value, s: &Session, opts: &ImportOpts) -> Result<Receipt, HopError>;
// src/children.rs: subagent sessions (§6.4)
pub fn children_of(parent: &SessionRef, roots: &StoreRoots) -> Result<Vec<ChildSession>, HopError>;
pub fn document(s: &Session, summary: Option<&SummaryFacts>, roots: &StoreRoots) -> Result<Value, HopError>;  // `document` + `children`
pub fn import_with_children(dest: HarnessId, selected: &Session, source: &Session, source_roots: &StoreRoots, opts: &ImportOpts) -> Result<Receipt, HopError>;
```

All of these live in `src/hop.rs`; the traits in `src/reader/mod.rs` and `src/writer/mod.rs`.

**`HopError` → CLI exit code** (technical design 5.5.2: 0 done, 2 refused with a reason, 3 needs the
user, 4 failed and nothing changed, 5 failed and recovery needed; `HopError::exit_code()`; codes:
[usage §11](usage.md#11-exit-codes)):

| kind | exit | when |
|---|---|---|
| `NotFound` | 2 | store not configured, session absent or id ambiguous (a reader's typed `Lookup`) |
| `Unsupported` | 2 | a pair or option this spec marks blocked |
| `SourceChanged` | 2 | a reviewed source fingerprint no longer matches (`--expect-sha256`) |
| `WriteFailed` | 4 | the destination refused the write, or it failed |
| `StrictCheckFailed` | 4 | §7 |
| `SummaryFailed` | 4 | §4.5: no usable summary from the source's model; nothing written |
| `VerifyFailed` | 5 | the source is there but could not be read |
| `WriteUncertain` | 5 | an external import may have written; the error carries its ID and destination |
| `ResumeFailed` | 5 | written; the error carries its receipt and manifest |
| `ChildWriteFailed` | 5 | `children`: the session is written, a subagent session was not; the error carries its receipt and manifest (§6.4) |

Host-supplied OpenCode IDs and metadata are checked in the explicitly selected destination after
import (`src/writer/opencode.rs` `confirm_import()`, read-only); reconcile an unconfirmed result
before retrying ([usage §6](usage.md#6-host-and-automation-options)).

### 1.3 Rules that hold for every hop

| # | rule | detail |
|---|---|---|
| 1 | **New session only** | The destination always gets a new id; the source is never modified. A second import of the same id is refused (OpenCode answers 409, OC2), not retried with it. |
| 2 | **No real store in tests** | Every test points the harness's root variables (§2) at a scratch folder and checks the real store's fingerprint is unchanged [V: every `*/source/isolation*`, `gen-source.log.txt`, `resume/*.meta.json` `real store unchanged: True`] |
| 3 | **No foreign process reaches the real home** | OpenCode's CLI hands work to a background service with the user's real environment unless `--standalone` (or `--server`) is passed. Every OpenCode command the library spawns that touches the store (`session export`, `list`, `import`, `run`) carries `--standalone` (L1); only `opencode --version` runs without it. |
| 4 | **Order is the native order** | Never timestamps: OpenCode `seq`, Codex `ordinal`, Claude/Pi parent links. |
| 5 | **Every loss is written into the manifest** (§6.3) | None is silent. |
| 6 | **No stray files; one executable** | See below. |

**Rule 6 in detail.** mukade is one executable with no installer; its files (the session as a
`.mukade-*.tmp` renamed into place, `--output` / `--manifest`, the `mukade-data` folder beside the
executable or `MUKADE_DATA_DIR`, `src/portable.rs`) and what happens when that folder cannot be made
are in [usage §10](usage.md#10-where-its-files-go). For OpenCode the document `session import` needs
is a file in `tmp/` (mode 0600, removed when the import ends, on failure too; a killed run's
leftovers older than an hour are swept by the next run). Nothing goes to the user profile,
application-data folders, registry or system temporary folder [V: `tests/portable.rs` runs the real
binary alone in a folder with every such location pointed at an empty sandbox and asserts it stays
empty].

## 2. The harnesses (verified facts the library depends on)

Sections 2.1 to 2.4 are the first four. Hermes Agent and DeepSeek Harness have their facts on their pages
(session store, records, what the model receives), which this spec does not repeat:
[Hermes Agent](harnesses/hermes-agent.md#session-store), [DeepSeek Harness](harnesses/deepseek-harness.md#session-store).
Where a rule below says "all four", the two later harnesses follow it as their pages state: both read
a stored compaction as a `Compaction` entry, neither takes a native compaction record, both are
destinations.

Each subsection gives isolation, commands, the native records to read and write, and what the model
receives on resume.

### 2.1 Claude Code 2.1.311

**Isolation [V].**
- Remove inherited `CLAUDE*` / `ANTHROPIC*` variables; set `CLAUDE_CONFIG_DIR=<scratch>\home`,
  `ANTHROPIC_BASE_URL`, `ANTHROPIC_API_KEY`, `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`,
  `DISABLE_TELEMETRY=1`, `DISABLE_ERROR_REPORTING=1`, `DISABLE_AUTOUPDATER=1`.
- Scratch `home\.claude.json` with `hasCompletedOnboarding`, `customApiKeyResponses.approved`,
  `projects[<cwd>].hasTrustDialogAccepted`.
- stdin from an empty file (otherwise a 3 s wait).

**Commands [V]** (from the project dir, `--output-format json --dangerously-skip-permissions`):

| action | command | notes |
|---|---|---|
| new | `claude -p "<prompt>"` | JSON result carries `session_id` |
| resume + prompt | `claude -p --resume <id> "<prompt>"` | same id, appends to the same file (cautions: §5) |
| compaction | `claude -p --resume <id> "/compact"` | trigger `manual`; result text `Compacted …` |
| fork | add `--fork-session` | new id; `/branch` writes `forkedFrom` on each copied row, `--fork-session` writes none (2.1.322) |

**Store [V].** `<CLAUDE_CONFIG_DIR>\projects\<cwd with every non-alphanumeric char → ->\<uuid>.jsonl`
(a folder name over 200 characters is cut and suffixed with `-` and a base-36 hash of the path:
`src/roots.rs` `claude_project_dir_name()`). No index file is needed: a session file written outside
Claude Code resumed by id [V: `claude_code/resume/*-from-*.meta.json`].

**Records the library depends on [V: `claude_code/source/c45db7bc-….jsonl`]** (all record kinds:
[claude-code.md](harnesses/claude-code.md#session-store)).
- One API response = **one row per content block**, sharing `message.id`, chained by `parentUuid`;
  a reader merges rows by `message.id`.
- Thinking `{"type":"thinking","thinking":…,"signature":…}`: **signature stored**.
- **Parallel calls:** the tool_use rows are chained; **each tool_result row's `parentUuid` is its
  own tool_use row**, in completion order. Walking parents from the leaf loses a call or a result:
  pair by `tool_use_id`.
- **Compaction**, in file order: `system/compact_boundary`; a `user` row `isCompactSummary:true,
  isVisibleInTranscriptOnly:true` holding `This session is being continued from a previous
  conversation … Summary: …`; the `/compact` command rows; `attachment type:"file"` rows
  re-injecting files read before. The last assistant reply stays in place, hung under the summary
  by `preservedSegment` (fields: [Compaction](harnesses/claude-code.md#compaction)).
- **Interrupted turn** (process killed): only the user row and an `api-request` row; the next load
  writes and sends a synthetic assistant `"No response requested."` (`model:"<synthetic>"`,
  `stop_reason:"stop_sequence"`). Each user turn is followed in the request by a `role:"system"`
  message (environment block).

**What the model receives on resume [V: `claude_code/source/capture-gen.jsonl`, `resume/source-resume.capture.jsonl`].**
Before a compaction: everything, thinking with its signature. After: summary + preserved tail +
later turns only (T1's tools and thinking are not sent; files read earlier come back as a system
message). The resume prompt is the last user message.

**Implemented history reconstruction (2026-10-29; local tests, not a new live harness
verification).** The reader rebuilds the selected branch from UUID parent links, compaction
metadata, response IDs and tool call/result IDs (rules and refusals:
[claude-code.md](harnesses/claude-code.md#how-mukade-reads-it)); file position only breaks ties
between independent recovered records, and timestamps are not used. Also: expanded slash-command
prompts and compaction summaries are recovered only when their dependencies identify a safe
position; an explicit conversational parent on an abandoned branch takes precedence over
`sourceToolAssistantUUID`, and such a result is not recovered into the active conversation; source
records remain in `Entry::raw`; excluded branch records become `Meta(OffBranch)`, counted in the
destination manifest, not replayed. A row of the resumed chain whose parent no row of the file holds
is read after the nearest earlier row of the conversation not below it, and recorded as
`Meta(Relinked)` (`relinked_parents`); a recovered parallel block with a missing parent is still
refused.

Verification limits: regression tests cover the review's branch-selection and abandoned-result
cases, shuffled fixture rows, a 20,000-row reverse-written chain and native-format round trips
through all four destinations. They are local parser/writer checks, not proof of live
auto-compaction or rewind compatibility, which remain **[U]** against Claude Code itself.
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
  The reader also reads rollouts `codex archive` moved to `archived_sessions\` (`list --archived`
  includes or hides them).
- A rollout alone resumes by id (Codex rebuilds its SQLite stores); a wrong date folder still works.
- **`state_5.sqlite` `threads.rollout_path` wins over the file on disk.** A row pointing at a
  missing file fails hard: `no rollout found for thread id … (code -32600)`; a writer must not
  leave a stale row (L7).
- Other files and `thread/list` hiding `exec` threads: [codex.md](harnesses/codex.md#session-store)
  (also §5 cautions).

**Records [V: `codex/source/rollout-…01a17e07….jsonl`].**
- Reasoning `{"type":"reasoning","summary":[…],"content":null,"encrypted_content":…}`: **encrypted content stored**.
- Parallel calls: consecutive `function_call` lines, then `function_call_output` lines, paired by `call_id`.
- Turns: `event_msg/task_started` … `task_complete`.
- **Compaction:** `compacted.payload = {message:"Another language model started … <summary>",
  replacement_history:[user T1, developer skills+permissions, user <environment_context>, user T2,
  user summary bridge], replacement_history_metadata, retained_context, …}` (all fields:
  [codex.md](harnesses/codex.md#compaction)).
  It is **local**: an ordinary `/v1/responses` call with `request_kind:"compaction"` and the prompt
  `You are performing a CONTEXT CHECKPOINT COMPACTION…`. A line reduced to `{message}` still
  resumes (P6).
- Graceful interrupt: user message `<turn_aborted>…</turn_aborted>` + `event_msg turn_aborted
  {reason:"interrupted"}`, sent to the model on resume; hard kill: the file just ends.

**On resume [V].** Before compaction: full history with `encrypted_content`. After: base
developer message + `replacement_history` + later items + prompt; no `encrypted_content`.

**Codex Desktop 0.165.0-alpha.1.2 [V: two real threads, 2026-10-28]** (records and reader rules:
[codex.md](harnesses/codex.md#how-mukade-reads-it)):
- **Paginated threads** (`history_mode: "paginated"`): a page's `session_meta` `history_base` names
  the earlier file and the cut; `state_5.sqlite` `threads.rollout_path` names the page. The reader
  takes the latest version and looks for earlier files only in the page's own home
  (`src/reader/codex.rs` `page_folders()`); their later records are off-branch (§1.3, counted), or
  carried with `Branches::All` (`--all-branches`) as `Session.branches` (§3.4).
- **Encrypted compaction summaries** (`compacted.message` empty; an item
  `{type:"compaction", encrypted_content}` in `replacement_history`): none is written as an empty
  message (`dropped_unreadable_summaries`); `Portable` from one is refused.
- **Messages between agents** (`agent_message`): one with readable text is a `User` turn where the
  receiving agent got it; one with an `encrypted_content` block stays `Meta(SubagentMessage)`,
  counted (`dropped_subagent_messages`); 265 of 320 on the author's store were encrypted (2026-11-12).
- **Subagent rollouts** name their spawning thread in `session_meta.parent_thread_id` (older ones
  only in `session_id`): the reader's `meta.parent` (§3.1 Lineage).

### 2.3 OpenCode 2.1.4

**Isolation [V].**
- `OPENCODE_DB`, `XDG_DATA_HOME` / `XDG_CONFIG_HOME` / `XDG_STATE_HOME` / `XDG_CACHE_HOME`,
  `OPENCODE_TEST_HOME` → scratch; `OPENCODE_DISABLE_MODELS_FETCH=1`,
  `OPENCODE_DISABLE_PROJECT_CONFIG=1`, `OPENCODE_DISABLE_CLAUDE_CODE=1`; remove `PWD`.
- Config (2.x schema): `{"providers":{"anthropic":{"settings":{"baseURL":…,"apiKey":…},"models":{…}}}}`.
- **Every command with `--standalone`** (§1.3 rule 3).

**Commands [V].**

| action | command | notes |
|---|---|---|
| new | `opencode run --standalone --model <p/m> --format json --auto --title <t>` | prompt on stdin |
| resume + prompt | `opencode run --standalone --session <id> --format json --auto` | **an unknown id silently creates a new session** (§5) |
| compaction | on `opencode serve`: `POST /api/session/<id>/compact {}` then `POST /api/experimental/session/<id>/wait` (204) | HTTP only; `run` has no compaction |
| import / export | `opencode session import <file> --directory <D> --standalone` / `session export <id> --standalone` | interchange `{info, messages[]}` (OC2) |

**Records [V: `opencode/source/rows.json`]** (more: [opencode.md](harnesses/opencode.md#how-mukade-reads-it)).
- `session_message {id, session_id, type, seq, …, data}`; order by `seq`.
- Assistant = one row per model step; **parallel calls are one row** `content:[reasoning, text, tool, tool]`;
  each `tool` part holds the call and its result (`state{status,input,content}`).
- A failed call's part is `state{status:"error", input, error, content}`, `error` a
  `Session.StructuredError` `{type, message}`; `session import` refuses it without (`Bad Request`)
  [V 2026-10-28]. Images: user `files:[{data, mime, source:{type:"inline"}}]`
  (`Prompt.FileAttachment`), tool result `{type:"file", uri:"data:…", mime}` (`Tool.FileContent`)
  [V 2026-10-28].
- Reasoning `{"type":"reasoning","text":…,"native":{"signature":…}}`: **signature stored**; sent
  back only when the stored provider and model equal the current ones [S: `to-llm-message.ts:155`].
- Turn end: `{"type":"idle","outcome":"succeeded"}`. Tool input keys: [tool conversion §4](tool-conversion-tables.md#4-argument-tables).
- **Compaction:** `{type:"compaction", metadata{keepID:<first kept msg>}, status:"completed"|"failed",
  reason:"manual", summary:<reply verbatim>, …}`; a summary without a template heading
  (`## Objective` …) fails: `Compaction summary did not match the required template`
  [V: `opencode/run1-shared-mock-compaction-failed/`].
- Interrupted turn: an assistant row with empty text and no `finish`, `time_suspended` set; the
  export omits it; on resume it is dropped from the request.

**On resume [V].** After a compaction the first message is one user
`<conversation-checkpoint>…<summary>…</summary><recent-context>[User]: … [Assistant]: …</recent-context></conversation-checkpoint>`;
the kept tail is flattened into it as text.

### 2.4 Pi 1.2.0

**Isolation [V].**
- `PI_CODING_AGENT_DIR=<scratch>\home`; `models.json` provider with `api:"anthropic-messages"`,
  `baseUrl`, `apiKey`; `settings.json` `compaction.keepRecentTokens`.
- Flags on every run: `--provider <p> --model <m> --offline --no-skills` (**`--no-skills` is
  required**: Pi reads `~\.agents\skills` whatever `PI_CODING_AGENT_DIR` says).
- Remove provider API-key variables; RPC stdin must be UTF-8 **without BOM**.

**Commands [V].**

| action | command |
|---|---|
| new | `pi -p "<prompt>"` |
| resume + prompt | `pi -p --session <file path or id> "<prompt>"` (§5) |
| compaction | `pi --mode rpc --session <id>`, stdin `{"id":"c1","type":"compact"}`, wait for the response (print-mode `/compact` is sent to the model as text) |
| abort | RPC `{"type":"abort"}` |

**Records [V: `pi/source/2026-10-27T16-00-39-689Z_01a17e17-….jsonl`]** (more: [pi.md](harnesses/pi.md#how-mukade-reads-it)).
- Header `{"type":"session","version":3,"id","timestamp","cwd"[, "parentSession" on fork]}`; then
  `model_change`, `thinking_level_change`, and a stored **system message** entry (sections +
  `toolsAdded`), replayed on resume (a different cwd is not reflected).
- Assistant: thinking with `thinkingSignature` (**stored**), text, and **both parallel
  `toolCall`s in one message**; one `toolResult` entry per result.
- **Compaction:** `{type:"compaction", summary (+ a Pi-added <read-files> block), firstKeptEntryId,
  tokensBefore, details{readFiles, modifiedFiles}, usage, fromHook, systemMessage}`.
- Killed turn: only the user entry. Aborted turn: assistant `stopReason:"aborted"`, dropped from
  the next request. A hand-written 3-line session (header, user, assistant) resumes
  [V: `pi/resume/synthetic.*`].
- **Tree.** Every entry has `id` and `parentId`. `/tree` back-navigation starts a new branch from an
  earlier entry (optionally through a `branch_summary` entry `{summary, fromId}`) and leaves the
  abandoned branch in the file. Pi builds the context from the last entry back through `parentId`;
  a `branch_summary` on that path reaches the model as a user message
  `The following is a summary of a branch that this conversation came back from:\n\n<summary>\n…</summary>`
  [pi-coding-agent 1.2.0 `buildSessionPath`, `BRANCH_SUMMARY_PREFIX`].

**On resume [V].** After a compaction: system + user `The conversation history before this point
was compacted into the following summary:\n\n<summary>…</summary>` + entries from
`firstKeptEntryId` on + the prompt.

## 3. Interchange model

### 3.1 The library's own model (`Session`)

Three layers: a **semantic core** every writer understands; the **source record kept verbatim** in
each entry (nothing is lost by reading); the **target-specific** choices, made by the writer.

```rust
pub struct Session {
    pub meta: Meta,                    // source harness + version, id, cwd, title, created/updated,
                                       // model(s), git branch, parent and parent_kind (never carried
                                       // into a hop), source path and SHA-256
    pub entries: Vec<Entry>,           // in native order (seq / ordinal / parent chain), never by time:
                                       // the path the harness resumes
    pub branches: Vec<Branch>,         // the branches it left, only when asked for (§3.4); else empty
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
    Meta      { kind: MetaKind },                            // command rows, synthetic replies, <turn_aborted>, idle
                                                             // markers, model/thinking changes, system prompts, re-injected
                                                             // files, internal and off-branch records, a cut-off last line:
                                                             // never conversation
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

**Lineage.** `Meta.parent` (and `SessionInfo.parent` in a listing) is the session this one came
from, an id in every harness; `parent_kind` says how, and is set whenever `parent` is: `Fork { at }` (a copy
continued on its own; `at` is the origin's last entry (or ordinal) the copy holds, where the source
says, in every harness)
or `Spawn { call_id, agent }` (a subagent; the parent's spawning call and the agent type, where the
source records them). The dump document carries both as `meta.parent` and
`meta.parent_kind` (`{"kind":"fork"}` or `{"kind":"fork","at":…}`, `{"kind":"spawn","call_id":…,"agent":…}`).
The manifest states them as `source.parent` and `source.parent_kind` (absent when the source has none): a hop of a
fork is a self-contained copy, not linked to its origin.

| harness | `parent` | `parent_kind` |
|---|---|---|
| Claude Code | a subagent session (`<parent-id>.<agentId>`): its meta.json `parentAgentId` as `<parent-id>.<parentAgentId>`, else the rows' `sessionId` | `Spawn { call_id: toolUseId, agent: agentType }` (meta.json); a fork: the `forkedFrom.sessionId` of its last copied row (a fork of a fork may keep older rows' origin), `Fork { at: the origin uuid of the last row copied }`; else none |
| Codex | `parent_thread_id` (top level or `source.subagent.thread_spawn`), else `session_id` ≠ `id`; listing: `thread_spawn_edges`; else `forked_from_id` | `Spawn { call_id: None, agent: agent_role, inherits_context: forked_from_id is set }`; `agent: "guardian_review"` for a guardian review; a fork no spawn started: `Fork { at: one less than forked_from_ordinal_exclusive or history_base.end_ordinal_exclusive }` (none when that is 0) |
| OpenCode | `parentID` (export) or `parent_id` (row); else a fork's `fork.sessionID` / `fork_session_id` with its boundary | `Spawn { None, None }`; a fork: `Fork { at: fork_boundary.messageID }` for a `through` boundary; a `before` boundary names the message after the last one held, so `at` is the parent's message just before it in `seq` order, from the store (none without it) |
| Pi | the `id` in the header of the file `parentSession` names (the path as written when that file is not here) | `Fork` |
| Hermes Agent | `model_config._delegate_from` (a `delegate_task` subagent, `source = subagent`), else `_branched_from` or a `parent_session_id` whose `end_reason` is `branched` | `Spawn { None, None }`; a `/branch` copy: `Fork`; a rotated compaction is the same session, no parent |
| DeepSeek Harness | the header's `parentSession` | `Spawn { call_id: None, agent: agentPreset, inherits_context: isSeeded }` when `origin` is `subagent`, else `Fork` |

### 3.2 Why each part is there (evidence)

| part | why [V] |
|---|---|
| `Compaction` entry | every harness stores a compaction record and resumes from it (§2, and the Hermes and DeepSeek Harness pages) |
| `Step.calls: Vec` and `Results` | Claude Code and Pi reject or repair split parallel calls (§6.2); Codex and OpenCode store them as one step |
| `Meta` entries | Claude `/compact` command rows read as conversation became an orphan `tool_result` in Pi and `function_call "{}"` in Codex; Codex `<turn_aborted>` and Claude's synthetic `No response requested.` became ordinary text |
| `Opaque` with protocol and model | decides replay vs drop (§3.3); every native store keeps it |
| `ToolCall.canonical` + `source_name` | tool translation where possible, text form otherwise ([`tool-conversion-tables.md`](tool-conversion-tables.md)) |
| `raw` on every entry | unknown records are kept, not dropped; a later version can read what this one did not understand |

### 3.2.1 Tools

Calls follow [`tool-conversion-tables.md`](tool-conversion-tables.md) (updated 2026-11-11: code mode
L10, `apply_patch` reading, checked against `src/tools.rs`): **translated** where the destination
offers an equivalent and every argument maps without changing its meaning, else **text form**;
results verbatim; ids kept.

### 3.3 Reasoning

**What counts as proof.** Provider proof is conversation state.
- HTTP 200, a correct short answer or a high prompt-cache hit rate does not establish preservation.
- A visible summary is not the signed/encrypted state it describes.
- Native store support is version- and provider-specific: Claude `signature`, Codex
  `encrypted_content`, OpenCode `native.signature` / `native.encrypted` / `native.reasoningDetails`
  and Pi `thinkingSignature` are distinct contracts, not interchangeable spellings.

**Policy** (`src/policy.rs` `ReasoningPolicy`; CLI `--reasoning drop_foreign|as_text|drop`):

| policy | rule |
|---|---|
| `DropForeign` (default) | Carry reasoning only with a known compatible destination protocol, a present payload and a representable destination envelope; otherwise record `reasoning_dropped` and its exact reason in the manifest |
| `AsText` | Write only the visible text, labelled `[earlier reasoning, not replayable]`, and count `reasoning_as_text`; an encrypted-only block has no readable text and is counted as lost |
| `Drop` | An explicit omission |

Neither `AsText` nor `Drop` may be reported as native proof replay.

**OpenCode's Chat-compatible providers.** `Opaque.chat` stores the source provider ID and **the
complete `reasoningDetails` array**, through `Session` and the public document; the scalar
`signature` / `encrypted` fields are compatibility projections, not a substitute for the envelope:

1. **Keep everything.** Every detail and its order, original assistant/tool owner, `type`,
   `format`, `id`, `index`, opaque payload and extension fields (summaries may precede several
   encrypted details). Never take only the first detail, concatenate opaque entries, invent an ID or
   regenerate a stored envelope.
2. **Two OpenAI routes.** `openai-native-responses-v1` is the native route, `openai-responses-v1`
   the aggregator route: both Responses family, envelopes never renamed or mixed. Mixed/unknown
   declared formats do not inherit the first detail's protocol.
3. **Same provider only.** Same-provider OpenCode conversion keeps the array verbatim. A missing or
   different custom provider identity does not prove an envelope replayable. A single ordinary
   Anthropic detail keeps its scalar conversion; a multi-item or OpenAI Chat envelope is not
   silently reduced to one Codex/Claude/Pi payload. An unrepresentable conversion is counted as a
   loss, never written in an unsupported native field.
4. **Destination protocol.** OpenCode/Pi default to an unknown destination protocol unless the
   caller states it. `--dest-protocol openai-responses` names the reasoning protocol even when the
   Gateway-facing HTTP envelope is Chat Completions; it does not override incompatible provider
   formats. The JanuraCode Session Bridge deliberately requests `as_text` on import and `drop` on
   Codex export: separate handoff policies, not ordinary tool-loop replay.
5. **Permanent regression:** `tests/reasoning_replay.rs` (complete arrays, encrypted-only payload
   ownership after summaries, repeated public-document round trips, extension fields, provider
   mismatch, mixed formats, explicit loss policy). A native check also passes converted sessions
   through the real destination importer and inspects persisted proof at the same owner. A claim of
   model continuation needs two sequential tool steps (three model requests) and the outgoing proof:
   import success alone proves neither replay nor acceptance.

**Rules for changes** (reader, model, document, planning, writer, persistence and resume):
- Keep a failing-before/passing-after regression when fixing a loss.
- Compare IDs, formats and payload lengths/hashes; do not persist raw production conversations or
  encrypted payloads merely for diagnostics.
- The verification record separates source fixtures, native import/storage, outgoing client
  requests and paid provider acceptance.
- Long-loop answer quality and cost are not inferred from a short successful transfer.

### 3.4 Branches

`Session.branches` is empty unless the reader was asked for the branches (`--all-branches`,
`Reader::read_all_branches`: Claude Code, Codex and Pi). Then the records of an abandoned path are
not `Meta(OffBranch)` entries of `entries`: they are parsed with the reader's own conversation parser
(steps merged, results paired) into entries of a `Branch`.

```rust
pub struct Branch {
    pub from: Option<String>,   // the entry it leaves from: an id in `entries` or an earlier branch;
                                // None: before the first entry
    pub entries: Vec<Entry>,    // its own entries, after `from`
}
```

- **A branch is a run.** One path of the tree is cut into runs: a run follows the first child in file
  order, every other child starts a run of its own, `from` the entry that holds its parent record.
  Runs come parents first. The decomposition is not canonical: a writer and a reader may cut the same
  tree differently; the set of paths from the first entry to a leaf is.
- **Claude Code and Pi** group the off-path user and assistant records by `parentUuid` / `parentId`
  (`src/reader/branches.rs`). A record that continues its parent (another block of the same Claude
  response, the result of a call its parent row made) stays in its parent's run, so parallel calls
  keep their results; a bookkeeping row between two records is passed through, and a branch leaves
  from the nearest conversation entry above it. **Codex** has no tree: a rollback (`thread_rolled_back { num_turns }`) drops
  the last `num_turns` user turns of what Codex had so far, and an earlier file of a redone thread holds
  records past the cut its page continues from; each such run is a branch from the conversation record
  before it, found by position (a rollout without `ordinal`s places its branches too).
- **The dump document** gains `branches: [{from, messages}]` (the messages as the top level, `from`
  the id of a message, null for the start); absent when empty. `from_document` reads it.
- **Planning** (`writer::plan::plan`): each branch is planned with the path it shares and judged as
  that whole conversation: its first blocks must equal the plan of the path alone, and the strict gate
  (§7) must pass. A `from` that names a bookkeeping entry leaves after the conversation entry before
  it. A branch that fails, whose `from` is not in the session, or that hangs off a branch that failed,
  is dropped. What a branch shares is planned once per point it leaves from. `Plan.branches` holds the kept ones with the block they leave from.
- **Writers**, by `Writer::keeps_branches`:

| destination | `BranchKeeping` | written as |
|---|---|---|
| Claude Code | `Tree` | one file; each row names its `parentUuid`; a branch follows the row it leaves from, the active path ends the file; `last-prompt.leafUuid` is the active leaf |
| Pi | `TreeLeafLast` | `id` / `parentId` entries, the same order; the last entry is the leaf, so a branch that would leave after the active path's last block (and so be written last) is dropped |
| Codex | `Rollbacks` | the branch, then `event_msg thread_rolled_back { num_turns }` for the user-role messages it wrote; only a branch whose first block is a user turn |
| OpenCode, Hermes, DeepSeek Harness | `None` | all dropped |

- **Manifest.** `counts.branches_kept`, `counts.branches_dropped`; one loss line when any is dropped.
  `--compaction native` writes no branches (all counted dropped).
- **Open [U].** Whether Claude Code 2.1.322 resumes the intended leaf of a mukade-written file with
  several leaves (needs a load without a prompt); whether Codex counts the user-role messages mukade
  writes inside a turn (a tool result written as text) as user turns when it rolls back.

## 4. Compaction between hops (optional)

Selected by `HopRequest.compaction` (CLI `--compaction`).

| mode | CLI | what is written | model called | § |
|---|---|---|---|---|
| `None` | `none` (default) | the whole selected history | none | 4.1 |
| `Portable { SourceRecord \| Supplied, … }` | `portable` (`SourceRecord`, from the source's boundary; `Supplied` is library-only) | handoff message + summary + tail | none | 4.2 |
| `DestinationNative` | none | refused by the library (`Unsupported`): the caller runs the destination's compaction | the destination's, run by the caller | 4.3 |
| `Native(NativeBudget)` | `native` (`document`, `hop`) | whole history + the destination's own deterministic record | none | 4.4 |
| `Portable { SourceModel, … }` | `summary` (`dump`, `hop`), or `--intent NAME` | handoff message + summary from the source's model + last `N` user turns | the source's | 4.5 |

### 4.1 `None`

The whole selected history is written. The destination sends it back on resume, so the context
starts as large as the source's **pre**-compaction history: the source's own compaction is lost
unless the reader keeps it (L4).

### 4.2 `Portable` (the library writes the compacted form)

One user message, then the tail from the boundary:

```text
[mukade handoff v1] Summary of the earlier conversation, written by the source agent's own compaction:
<summary>
```

(`src/hop.rs` `PORTABLE_HANDOFF`.) The summary comes from:

| `SummarySource` | where the summary and boundary come from |
|---|---|
| `SourceRecord` (needs L4) | the source's native compaction record. Boundary: Claude `preservedSegment` / next after the summary; Codex `retained_context` / `replacement_history`; OpenCode `metadata.keepID`; Pi `firstKeptEntryId` |
| `Supplied` | caller's text, e.g. from a low-cost model (technical design 6.4) |

### 4.3 `DestinationNative` (the destination compacts the imported session)

Write the whole history, run the destination's own compaction (§2 commands), resume.

**Costs:** one extra call on the destination's model; its window must hold the full history once.

| prefer | when |
|---|---|
| `Portable` | the source already has a compaction (its summary is free and written), or the full history does not fit the destination's window |
| `DestinationNative` | the destination's model should write the summary in its own format |

### 4.4 Writing a native compaction record

The best form of `Portable` is a **native** compaction record, which the destination treats as its
own boundary.

**Feasibility found first:** Codex accepts a minimal `compacted{message}` line (P6 [V]); OpenCode's
import round-trips a `compaction` message losslessly (OC2 [V]); Claude Code (`compact_boundary` +
`isCompactSummary` row) and Pi (`compaction` entry with `firstKeptEntryId`) were **[U]** for
hand-written records until the live acceptance below.

**Built for all four destinations.** `Compaction::Native(NativeBudget)` keeps the whole generated
history and appends the destination's own record after it:

| destination | record written |
|---|---|
| OpenCode | one completed `compaction` message, `{"type":"compaction","status":"completed","reason":"manual","summary":"…","metadata":{"keepID":"…"},"time":{"created":…}}`; `keepID` points back to the first generated message of the retained tail |
| Pi | one `compaction` entry with `summary`, `firstKeptEntryId` and `tokensBefore`, chained after the retained rows |
| Codex | one `compacted` record with `message` and `replacement_history`: the summary as its first user input item, then the retained response items with their Mukade-generated top-level `id` fields removed (live Codex 0.166.0 refused such IDs and did not replay `message` as context) |
| Claude Code | the native pair `system/compact_boundary` + a user row `isCompactSummary:true,isVisibleInTranscriptOnly:true`, inserted before the picker rows; the boundary's `preservedSegment` and `preservedMessages` name the retained UUIDs and the summary row |

**The summary** is deterministic and free (`src/native_compaction.rs`): the user requests in the
superseded prefix, in order, each truncated to 1,000 characters; a 24,000-character body cap; the
header `[mukade native compaction] Deterministic summary of earlier user requests; the full history
remains in this session.` Every older row stays in the written session; the destination projects
summary + tail to the model. No model or destination process is called to make the boundary.

**The budget** from the caller's `--context-window`, `--max-output-tokens` (default 16,000) and
`--text-token-factor`:
- Text is estimated as chars/3, times the Gateway factor when supplied and valid (≥ 1).
- One conservative budget: `min(window/2, openCodeCeiling(window) - roomForNextTurn)`, where
  `openCodeCeiling(window)` = window − max(15%, 16,000 for windows ≥ 32,000) and
  `roomForNextTurn` = max output tokens + 8,000 (`import_budget()`).
- Only OpenCode's trigger is verified, so the common half-window bound is deliberately lower.

**Where it is accepted:** `document` and `hop` ([usage §5](usage.md#make-a-long-session-smaller)).
`dump` refuses it (its document is harness-neutral); `write` does not reapply a boundary to an
existing document.

**Live acceptance (2026-11-04)** with an oversized synthetic session (~7.5 MB, far above every
model window): Claude Code 2.1.318, Codex 0.166.0, OpenCode 2.2.5 and Pi 1.4.0 each resumed the
written session, answered from the deterministic summary and added no second native compaction
(`docs/verification/native-compaction-live.md`).

### 4.5 `Portable` with a summary from the source's model

`SummarySource::SourceModel(SummarizeOpts)` asks the source harness's own model for the summary
when the session moves.

**Flags** (`--compaction summary` or `--intent NAME`, `--keep-turns`, `--summary-model`,
`--summary-timeout`): [usage §5](usage.md#summary-the-sources-own-model-writes-one).

**The prompt.** One prompt for all four harnesses, from the editable `intent-prompts/<name>.md`
files beside the executable (`generic` by default; embedded fallback versioned `v1`). It is not the
harness's native `/compact`: that writes into the source's store, and Codex's native summary is
encrypted.

**The run** uses the source's binary, credentials and configuration and saves nothing in its store
[V: `docs/verification/source-model-summary-live.md`; Codex against a mock]:

| source | how nothing is saved |
|---|---|
| Claude Code | `--no-session-persistence` |
| Codex | `exec fork --ephemeral` |
| Pi | `--fork` into a temporary folder |
| OpenCode | a fork deleted afterwards whatever the outcome, every tool denied through `OPENCODE_CONFIG_CONTENT` |

**The result** is §4.2's shape with its own handoff line (`src/summary.rs`
`MODEL_SUMMARY_HANDOFF`), then the last `N` user turns (none by default):

```text
[mukade handoff v1] Summary of the earlier conversation, written by the source agent's model when the session was moved:
```

- `hop::compact` resolves the summary once, before the selection: `dump --compaction summary`
  exports the document `hop` would write, and `write` imports it later.
- The document's top-level `summary` names the writer and the intent prompt, so the manifest
  `write` makes says so too.
- The manifest carries `options.summary` (`source: "source_model"`, harness, version, model,
  `prompt` naming the intent or `v1`, `chars`, `usage`, `left_out`) and one loss line.

**Failures and refusals** ([usage](usage.md#summary-the-sources-own-model-writes-one)): a failed
summary (an error, an empty reply, a deadline) is `HopError::SummaryFailed`, exit 4, nothing
written; refused (exit 2): `--all-branches`, and `--from-file` for a source other than Pi.

## 5. Resume prompt (optional)

**How it runs** (`src/hop.rs` `run_resume_with()`, `src/exe.rs` `run_within()`): the destination's
resume command, from the session's working directory, in the caller's environment with the
destination's store variable set to the root the write used, stdin closed, and a timeout
(`ResumePrompt::new`: 300 s) that stops the harness **and everything it started**
([usage §7](usage.md#7-stores-and-executables)): on Windows `taskkill /PID <pid> /T /F`, which also
reaches a harness behind a `.cmd` launcher; elsewhere the tree is read from parent ids (`/proc`, or
`ps`), stopped with `SIGSTOP`, read again, then killed. The child stays in mukade's process group,
so Ctrl-C still reaches the harness. A process that detached itself is not found (`kill_tree()`).

**The resume command each writer returns** is the catalog's `resume` argv with the prompt appended
([the six commands](reference/harness-catalog.md#the-six-entries)): `claude -p --resume <id>`,
`codex exec resume <id>`, `opencode run --standalone --session <id>`, `pi -p --session <file>`,
`hermes chat -Q --resume <id> -q`, `dsh --profile headless --session-id <id>`.

**Results.** Success: the receipt keeps stdout as `resume_output`. Failure: `ResumeFailed` carries the
written receipt; its reason names the command, exit status and stderr; the manifest records the
failed resume as a loss. Refused before anything is written (`Unsupported`): a prompt starting with
`-` (every harness CLI reads it as an option), and one with line breaks toward a Windows `.cmd`
launcher.

**Cautions [V].**

| harness | caution |
|---|---|
| OpenCode | `run --session <unknown id>` creates a new session: the library checks the id with `session export` (or the DB, read-only) first. |
| Codex | threads written with source `exec` are hidden from `thread/list` defaults; resume by id works. |
| Pi | `--session <id>` searches the current project first; the library passes the file path. |
| Claude Code | resume works from any cwd but appends rows with the new cwd: run from the session's cwd. |

**Interrupted turns** (`--interrupted keep|drop`; `hop`, `write`, `document`;
[usage §5](usage.md#a-session-with-interrupted-turns)).
- An **interrupted turn** is one the source never finished: a killed or aborted step, or the
  source's own abort marker after its work (Codex `<turn_aborted>`, Claude Code `[Request
  interrupted by user]`; both `Meta`, never user text). The step the marker cut is `aborted` and
  keeps what it holds.
- `keep` (default): the turn's work travels, so a session with no finished turn is carried whole.
  `drop`: every step and result of such a turn is left out and counted
  (`dropped_interrupted_steps`); its prompt stays.
- An **interrupted last turn** is carried as a user message without a reply. On resume Codex,
  OpenCode and Pi send two consecutive user messages; Claude Code inserts its synthetic
  `No response requested.`; both are accepted. The manifest records `interrupted_last_turn`.

## 6. Destination constraints and manifest

### 6.2 Tool and reasoning constraints

| destination | constraint | req |
|---|---|---|
| Claude Code | each tool result is associated with its own tool call | L2 |
| Pi | parallel calls grouped in one assistant message, followed by their results | L3 |
| any | signed reasoning needs its original compatible payload; otherwise the reasoning policy drops it or carries it as text | L6 |
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
above zero also has a `losses` line.**

**Counts: what travelled**

| count | meaning |
|---|---|
| `messages_in` | source conversation entries (`Meta` excluded) |
| `messages_out` | blocks the destination receives |
| `tool_calls` | calls translated into a destination tool |
| `tools_text_form` | calls carried in text form |
| `reasoning_as_text` | reasoning blocks written as labelled text |

**Counts: what was left out on purpose**

| count | meaning |
|---|---|
| `tools_omitted` | calls omitted with their results (`--without-tools`) |
| `reasoning_dropped` | reasoning blocks not carried (§3.3) |
| `dropped_off_branch` | records of a branch the source left (rewind, edited prompt, Pi tree navigation) that were not asked for (`--all-branches`) |
| `branches_kept`, `branches_dropped` | with `--all-branches`: the branches written to the destination, and those left out (one conversation only, not placeable, or refused with the path they share); a loss line when any is dropped (§3.4) |
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
| `dropped_subagent_messages` | a message between a subagent and its parent with no readable text (Codex's encrypted `agent_message`) |
| `dropped_unfinished_lines` | a last source line cut off mid-write, kept as text and not read |
| `relinked_parents` | rows whose parent the source file does not hold, read after the row before them (`Meta(Relinked)`: Claude Code) |
| `aborted_steps_as_replies` | aborted or failed steps written as ordinary replies where the destination cannot mark them (Pi keeps the step's end) |

### 6.4 Subagent sessions (`children`)

Opt-in: `HopRequest.children`, `dump --children`, `hop --children`. Without it nothing changes.
Code: `src/children.rs`; writers: `src/writer/claude_code.rs` `write_child()`,
`src/writer/opencode.rs` `document_from_plan_report()`; the pipeline: `src/hop.rs` `import_inner()`.

- **Which.** Every readable session whose `parent` is the session, at any depth (`children_of()`:
  the store is listed once, the session read once). Order: the calls that started them, in the
  parent's order; a child with no known call comes after, by update time. A child that cannot be
  read is a `losses` line, `child <id>: <reason>`; the hop goes on.
- **Dump document.** `children`: one full dump document per child, nested as they hang; `children_lost`:
  `<id>: <reason>` for each one not read (absent when none). Children are written whole
  (`Compaction::None`); `write` imports the session alone.
- **Write.** The session first, then each child as a session of its own with the parent's options
  (`dest_id`, `dest_path`, metadata, the resume prompt and `--expect-sha256` are the session's).
  Ids are chosen before the session is written (`Writer::new_id`, `child_id`), so its text can
  name them. `Compaction::Native` applies to each child; any other compaction to the session only.

| destination | link | how |
|---|---|---|
| OpenCode | native | the child's import document has `info.parentID` = its parent's destination id; the parent is imported first |
| Claude Code | native | `subagents/agent-<agentId>.jsonl` and `.meta.json` in the folder of the root's id (layout and fields: [Claude Code page](harnesses/claude-code.md)); child id `<root-id>.<agentId>` |
| Codex, Pi | none | a session of its own; neither has a field a writer may set for a spawner (Pi's `parentSession` is a fork) |

- **The spawn call stays text form (L14)**; its text gains one line, `→ <destination child id>`.
- **Manifest.** `children` lists one entry per child written, in write order; its keys are in
  [usage](usage.md#carry-the-subagent-sessions). A child's own `losses` are copied into the
  session's with the prefix `child <id>: `. `inherits_context` is set for a child whose
  `ParentKind::Spawn.inherits_context` is true: its file does not hold the conversation it began from,
  and none is added. The key is absent without `children`.
- **Failure.** A child that cannot be written stops the children there; the session stays written.
  `HopError::ChildWriteFailed { reason, receipt }` (exit 5) carries the receipt, whose manifest lists
  the children written and a loss line for each not written.

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

**What the library checks** on the write plan **before** writing (`src/strict.rs` `check_plan()` over
`src/writer/plan.rs` `plan()`; the destination builds the request later):

- parallel calls grouped in one message;
- each grouped call answered in the next entry;
- no `ToolResult` without its `ToolCall`, and no call answered twice (A3);
- native results before the text answering a call carried as text (Pi ends the tool flow at the
  first user text, §6.2);
- call ids in Anthropic's pattern toward an Anthropic-protocol destination;
- no `Reasoning` carried natively unless its payload has a place in the destination's format (Claude:
  a signature as `thinking`, an encrypted payload as `redacted_thinking`; Pi `thinkingSignature`;
  Codex `encrypted_content`).

**On a violation:** the plan fixes it by policy (drop the orphan, carry an unanswered call as text,
drop the reasoning) and records it in the manifest; what policy cannot fix fails the hop with
`StrictCheckFailed` before anything is written.

## 8. Reader and writer requirements

Ids **L1–L14**. Status as of 2026-11-11; the test named checks the acceptance. Live evidence: the
verification records.

### 8.1 Readers

| id | requirement | evidence | acceptance | status |
|---|---|---|---|---|
| L4 | every reader turns the native compaction record into a `Compaction` entry (Claude `compact_boundary` + `isCompactSummary` row + `preservedSegment`; Codex `compacted` + `replacement_history`; OpenCode `compaction` + `metadata.keepID`; Pi `compaction` + `firstKeptEntryId`) | §2 | `Portable{SourceRecord}` works for all 12 pairs without a supplied summary | **built**: one test per reader (`tests/claude_code_reader.rs` `native_compaction_becomes_one_compaction_entry`, `tests/codex_reader.rs` `compaction_keeps_the_summary_and_folds_the_summary_turn`, OpenCode and Pi alike); a portable hop is tested Claude Code → Pi (`tests/hop.rs` `portable_carries_the_source_summary_and_the_tail_from_its_boundary`), not all 12 pairs. Codex 0.165 encrypted summaries: refused (§2.2) |
| L5 | Claude reader: `isMeta`, `<local-command-*>`/`<command-name>` rows, `isCompactSummary`, `isVisibleInTranscriptOnly`, attachment rows (a `task-notification` one is a user turn) → `Meta`; rows merged by `message.id`; results paired by `tool_use_id`, not by the parent chain | §2.1 | Claude → Pi tail strict clean; no `function_call "{}"` into Codex | **built**: `tests/claude_code_reader.rs` `results_are_paired_by_tool_use_id_not_by_parent_chain`, `transcript_only_and_internal_rows_are_meta` |
| L6 | readers keep `signature`/`encrypted` with protocol + model (`Opaque`) | §3.3 | OpenCode/Pi → Claude strict clean with `DropForeign` | **built**: `reasoning_keeps_its_opaque_payload` and the L6 tests of all four reader test files; `tests/reasoning_replay.rs` |
| L9 | Codex `<turn_aborted>`/`turn_aborted` and a hard-killed last turn → `StepEnd::aborted`/`killed`; Claude `<synthetic>` reply and `[Request interrupted by user]` → `Meta`, the step it cut → `aborted`; OpenCode empty assistant without `finish` → `killed` | §2 | manifest `interrupted_last_turn`; no marker text in the destination; `interrupted: drop` leaves the turn's work out | **built**: `tests/codex_reader.rs` `a_reply_cut_by_an_abort_is_an_aborted_step`, `tests/opencode_reader.rs` `a_row_without_finish_is_a_killed_step_and_the_export_omits_it`, `killed_last_turn_is_a_user_entry_with_no_reply` (Claude Code, Pi) |
| L10 | Codex Desktop code-mode `exec`: a script of only literal `text(await tools.X(...));` statements whose output holds the header and one text item per statement → those calls, each with its own result (first id the `exec` call's, then `<id>_<n>`; [rule](tool-conversion-tables.md#5-reading-codex-desktop-code-mode-exec)); any other script → one text-form call. `apply_patch` input → `Write` / `Edit` / multi-edit when it is one plain file section | census: 4844 of 4908 real Codex calls are `exec`; 2026-11-11: 725 of 981 scripts split (`docs/verification/2026-11-11-codex-code-mode.md`) | a Codex Desktop rollout converts with shell and patch calls translated | **built** 2026-11-11 (`src/reader/codex/exec.rs`, `src/tools/patch.rs`): `tests/codex_reader.rs` `a_sequence_reads_as_calls_with_their_own_results`, `tests/hop.rs` `a_code_mode_rollout_reaches_claude_and_pi_with_native_calls`, census `tests/codex_code_mode_census.rs` (ignored; reads a real `CODEX_HOME`). Live hop of a real Desktop thread: open (§10) |

### 8.2 Writers

| id | requirement | evidence | acceptance | status |
|---|---|---|---|---|
| L1 | every `opencode` command spawned with `--standalone` | §1.3 rule 3 | intended `opencode.db` is used | **built**: `tests/writer_opencode.rs` `the_import_command_always_carries_standalone` |
| L2 | Claude writer: one row per content block sharing `message.id`; each result row parented on its own tool_use row | §6.2 | Codex → Claude full: both results present | **built**: `tests/writer_claude_code.rs` `each_step_is_one_row_per_block_sharing_a_message_id`, `every_result_row_hangs_on_its_own_tool_use_row` |
| L3 | Pi writer: all calls of one `Step` in one assistant `message`, then one `toolResult` entry per result | §6.2 | any → Pi full: strict clean, no `No result provided` | **built**: `tests/writer_pi.rs` `both_parallel_calls_are_in_one_assistant_message_with_one_result_each` |
| L7 | Codex writer: `session_meta` with Codex's own fields (`originator`, `cli_version` of the destination, `model_provider` of the destination config); no stale `threads` row in `state_5.sqlite` | P5 | `codex resume` lists and resumes the thread | **built**: `tests/writer_codex.rs` `session_meta_states_the_destination_and_never_the_source`, `no_database_is_touched` |
| L8 | OpenCode writer: one assistant row per `Step` (`[reasoning, text, tool, tool]`), through `session import` | §6.2; OC2 | any → OpenCode full: calls grouped | **built**: `tests/writer_opencode.rs` `one_assistant_message_per_step_holds_both_calls_and_their_results` |
| L11 | translate tools only into tools the destination offers, else text form | [`tool-conversion-tables.md`](tool-conversion-tables.md) §1, §2, §6 | no destination request carries a tool name outside its offered set | **built**: `tests/tools_translation.rs` `every_translated_name_is_offered_by_its_destination`. The destination's shell and offered set from its own configuration: built, `tests/dest_shell.rs`, `tests/cli.rs` `a_hop_into_*` (tables §4.1) |
| L12 | Pi `bash.timeout` seconds ↔ canonical milliseconds | tool schemas [V captures] | Claude → Pi → Claude keeps the timeout | **built**: `tests/tools_translation.rs` `shell_timeout_is_converted_for_pi_and_kept_in_milliseconds_elsewhere` |
| L13 | argument-level equivalence decides translate vs text form (`replace_all`, `output_mode`, context lines, `pages`, background, shell dialect) | conversion tables §4 | a `replace_all: true` edit reaches Pi as text form | **built**: `tests/tools_translation.rs` `replace_all_blocks_pi_and_codex_and_travels_to_opencode` |
| L14 | subagent, skill, question, MCP and harness-internal tools → text form in every direction | conversion tables §3 | no such call is translated | **built**: `tests/tools_translation.rs` `capability_tools_are_text_form_in_every_direction` |

**Also on writers.**
- They apply `ReasoningPolicy` (§3.3) and the compaction mode (§4).
- The §4.4 native compaction records are built for all four destinations, read back by Mukade, and
  live-accepted by each real binary on 2026-11-04 (`docs/verification/native-compaction-live.md`).
- Codex 0.166.0 resumed a written copy with real encrypted reasoning byte-identical and refused a
  one-character tamper (`docs/verification/codex-encrypted-reasoning-live.md`).

## 9. Verification

1. Run `cargo test --workspace --no-fail-fast`.
2. The conformance bench is the test: readers against the native fixtures
   (`fixtures/<harness>/`, from Shinatama's bench) in `tests/<harness>_reader.rs`, writers in
   `tests/writer_<harness>.rs`, round trips in `tests/writer_roundtrip.rs`, whole hops in
   `tests/hop.rs`.
3. Harness runs beyond the tests, their environments and limits: the
   [verification records](README.md#verification-records).

## 10. Open

Each item is **[U]** unless stated otherwise; step-by-step checks: `next.md`.

**Compaction and summaries**

| item | state |
|---|---|
| Native-compaction budget thresholds other than OpenCode's verified ceiling | [U]; live acceptance of the hand-written records is [V] for Claude Code 2.1.318, Codex 0.166.0, OpenCode 2.2.5, Pi 1.4.0 (2026-11-04) |
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
| Codex 0.165: a chain of pages (a page whose base is itself a page) | [U]: read by following each `history_base` back to the first file, latest version winning; tested on hand-made files only (one page in 982 rollouts, its base a plain file) |
| L10 limits, by design | Code mode is read without evaluating it: a script with variables, `Promise.allSettled`, `ALL_TOOLS`, `image(...)`, or an output not matching its statements stays one opaque text-form call |
| L10 shell dialect | A split `exec_command` keeps the source's dialect and translates only toward a destination that runs it (tables §4.1). A call's own shell is read by name or path; `<shell>cmd</shell>` (or another shell of no translated dialect) leaves the call as `exec_command`, text form |
| L10 on a real Desktop thread | The read-only census passed; a live hop is not done yet (`next.md`) |

**Process control (§5)**

| item | state |
|---|---|
| Windows: the timeout still stops the whole tree | unchanged code, no automated test (`next.md`) |
| macOS: the tree kill through `ps -A` | [U] (Linux uses `/proc`; same logic) |

**Open verification of real-life shapes**
- Auto-compaction by threshold; a second compaction.
- A rewind or tree navigation in the source: readers follow the resumed branch, tested on
  hand-made records; Claude `--fork-session` and Pi fork shapes are recorded, not converted.
- Subagent sessions: read and listed (Claude Code children, Codex child rollouts, OpenCode child
  sessions); a hop of a parent does not carry them yet.
- Images and attachments: converted between the four shapes; only OpenCode's verified by a real import.
- Sessions larger than a few turns.

**Bench note.** The source sessions predate two mock fixes: their T3 reply is the mock's summary
text, not harness behaviour (FINDINGS of each leg).
