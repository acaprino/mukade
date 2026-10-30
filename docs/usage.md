# Using Mukade

Mukade moves a coding-agent session from one harness to another — Claude Code, Codex CLI,
OpenCode, Pi — and the destination continues it **as one of its own sessions**. This guide is the
command-line manual: how to get it, the use cases it is made for, how to read what it prints, and
every harness's details.

Paths, session IDs, project IDs, model names and provider names in the examples are
**illustrative**: replace them with values from your installation. Commands are shown for Windows
PowerShell; on macOS or Linux use the corresponding paths and `export NAME=value` instead of
`$env:NAME = 'value'`.

## Contents

1. [Get it](#1-get-it) · 2. [Quick start](#2-quick-start) · 3. [Use cases](#3-use-cases) ·
4. [Reading the receipt and the manifest](#4-reading-the-receipt-and-the-manifest) ·
5. [Speed](#5-speed) · 6. [Where its files go](#6-where-its-files-go) · 7. [Exit codes](#7-exit-codes) ·
8. [Stores and executables](#8-stores-and-executables) · 9. [Per-harness examples](#9-per-harness-examples) ·
10. [Discover, export, import](#10-discover-export-import) · 11. [Troubleshooting](#11-troubleshooting)

## 1. Get it

mukade is one command-line executable. There is nothing to install and nothing to configure:

```powershell
cargo build --release                       # target\release\mukade.exe
Copy-Item target\release\mukade.exe D:\tools\    # anywhere: a folder, a USB stick
D:\tools\mukade.exe --help
```

The Windows executable has the C runtime built in, so it runs on a machine with no Visual C++
redistributable. Run it from a shell, by its path or from a folder on `PATH`; in the examples below
`mukade` stands for whichever you use (`D:\tools\mukade.exe`, say). It has no installer and no
configuration file. On first start it copies the editable intent prompts to an `intent-prompts`
folder beside the executable (an existing file is never replaced), and by default generates its
other files in a `mukade-data` folder beside the executable, made only when something is first
written. A host may set an absolute `MUKADE_DATA_DIR` instead. See
[Where its files go](#6-where-its-files-go).

Install and configure the harnesses you intend to use separately: mukade moves sessions, it does
not install harnesses or transfer their credentials.

## 2. Quick start

### Find supported harnesses and existing installations

List the supported harnesses found on this machine, with their binary, version, store, session
counts and the lineage information each format records:

```powershell
mukade harnesses
```

This command checks the standard store locations and honors `CLAUDE_CONFIG_DIR`, `CODEX_HOME`,
`OPENCODE_DB`, `PI_CODING_AGENT_DIR` and `MUKADE_*_EXE`. It reads store files without modifying them
and asks a discovered executable for its version; unknown counts and lineage are shown as unknown,
not guessed. It reports **one resolved executable per harness**, not every copy on disk, and does
not change the explicit-root behavior of `list`, `dump`, `write` or `hop`.

The store line carries one of these states, so a store that is there but unreadable is never mistaken
for an empty one:

| state | meaning |
| --- | --- |
| `recognized` | the store holds files in that harness's own layout and mukade listed them |
| `recognized, read error: <reason>` | the layout is right but counting failed; the reason follows |
| `present, layout not recognized` | the path exists but no file matches that harness's layout. An explicit store variable skips this check and is read directly (failures then show as a read error) |
| `read error: <reason>` | the harness reader could not be opened at all |
| `not found` | no store at the default or the configured path |
| `non-persistent (:memory:)` | OpenCode's `OPENCODE_DB` points at an in-memory database |

The harness line reports the executable as `configured` (a `--<harness>-exe` flag or `MUKADE_*_EXE`),
`installed` (`PATH` or the catalog location) or `not found`. When a store reads
`present, layout not recognized`, `mukade detect <dir>` prints the evidence it looked for (see
[Discover, export, import](#10-discover-export-import)).

`harnesses` takes no options and prints text, not JSON: it is meant to be read by a person. A program
that needs the same facts should use the library or `mukade list`, whose rows are JSON.

The locations it consults, per harness and per platform, are the [harness catalog](reference/harness-catalog.md)
(`catalog/harnesses.json`), which ships inside the executable. To enumerate `PATH` matches yourself:

```powershell
# Windows PowerShell
Get-Command claude,codex,opencode,pi -All -ErrorAction SilentlyContinue |
  Select-Object Name,Source
```

```bash
# macOS / Linux
for h in claude codex opencode pi; do command -v "$h" || true; done
```

Use `npm prefix -g`, `npm root -g` or `brew --prefix` for package-managed installs, then run each
binary's `--version`. For custom executables and stores, the override order and `--root` are in
[Stores and executables](#8-stores-and-executables); the full per-OS discovery procedure and the
canonical native locations are in [Finding existing installations](reference/harness-catalog.md#finding-existing-installations).

### Move your first session

Continue a Claude Code session in Pi:

```powershell
$env:CLAUDE_CONFIG_DIR   = Join-Path $HOME '.claude'
$env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'

mukade list claude_code                                  # find the session id
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project'
```

The last command prints one JSON receipt. The new Pi session exists; open it with the `resume`
command the receipt gives, from the project folder:

```powershell
Set-Location 'C:\work\my-project'
pi -p --session 'C:\Users\you\.pi\agent\sessions\--C--work-my-project--\2026-10-28T16-00-00-000Z_a7b29d83-….jsonl' 'where were we?'
```

Three things always hold: the destination session is **new** (the source is never touched), the
order is the harness's **native order** (never timestamps), and **no loss is silent**: whatever was
left behind or changed is a line in the receipt's manifest.

Before you start, know the **existing project directory** the source belongs to; `--cwd` names the
destination's project directory (if omitted, mukade uses the source's recorded one, when present).
A `--resume-prompt` *runs* the destination harness and may make a model/API call; without it mukade
only writes the session and gives you the command.

## 3. Use cases

### Switch harness in the middle of a task

*You started in Claude Code and want to continue in Pi, Codex or OpenCode.* This is the quick start
above. Any pair works; the four harnesses are `claude_code`, `codex`, `opencode`, `pi`:

```powershell
mukade hop codex YOUR_CODEX_SESSION_ID claude_code --cwd 'C:\work\my-project'
mukade hop pi    YOUR_PI_SESSION_ID    opencode    --cwd 'C:\work\my-project' --project-id YOUR_PROJECT --model-provider YOUR_PROVIDER --model YOUR_MODEL
```

Tool calls are translated to what the destination really offers; a call it cannot express travels
as text, with its arguments, and is counted (`tools_text_form`). Add `--resume-prompt 'continue'`
to also run the destination once and keep its reply in the receipt (`resume_output`).

### Carry a very long session cheaply

*The history is huge but only the recent turns matter.* `--compaction portable` writes one handoff
message holding the **source's own compaction summary**, then the tail the source's compaction
kept, instead of the whole history:

```powershell
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project' --compaction portable
```

The source must have compacted at least once (a Claude `/compact`, a Pi compaction, …); otherwise
the hop is refused with a reason and nothing is written. The manifest keeps the lines
`the source's compaction summary travels as one user message` and how many entries were kept.

### Move only a summary, written by the source's own model

*The session is long, it never compacted, or its summary is encrypted (Codex).* `--compaction summary`
asks the source harness to summarize the session with its own model, its own login and its own
configuration, in a run that saves nothing in its store; the destination starts from that summary:

```powershell
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project' --compaction summary
```

`--keep-turns 2` also keeps the last two user turns verbatim after it; `--summary-model` names a
cheaper model; `--summary-timeout` (seconds, default 300) bounds the run. To export the summary and
import it later, or elsewhere:

```powershell
mukade dump codex YOUR_CODEX_SESSION_ID --compaction summary --output 'C:\work\summary.json'
mukade write opencode 'C:\work\summary.json' --cwd 'C:\work\my-project'
```

It costs one model call on the source's account; the manifest's `options.summary` says which model
wrote it, which intent prompt was used, how long it is and, for Claude Code, what it cost. If the
summary cannot be written (an error, an empty reply, the deadline) nothing is written and the exit
code is 4.

#### Choose what the summary emphasizes

`--intent <name>` selects a prompt for the source model. It implies `--compaction summary`, so the
destination receives the summary alone unless `--keep-turns` adds turns:

```powershell
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project' --intent review
mukade hop codex YOUR_CODEX_THREAD_ID claude_code --intent execute --keep-turns 2
```

The prompts are ordinary `.md` files in `intent-prompts` beside the executable, copied there on the
first start. The file stem is the intent name, so `security.md` is selected with
`--intent security`. `generic.md` is used when `--intent` is omitted. Files edited or added are
picked up by the next run; mukade never overwrites one. List the folder and every available intent:

```powershell
mukade intents
```

The shipped intents are `generic`, `continue`, `execute`, `review`, `analyze`, `brainstorm` and
`plan`. A prompt is read as plain text and its whitespace is folded to one line before it is passed
to the harness. A Windows `.cmd` launcher can receive only ASCII letters, digits, spaces and
`.,;:()`; a prompt outside that subset is refused before the harness starts. Name the harness's
native executable to use a prompt with accents or richer punctuation. A host may set an absolute
`MUKADE_INTENT_DIR` to keep the prompts elsewhere.

The intent affects only the summary prompt. It does not start the destination, change its
permissions, or add a second model call. `--intent` is accepted by `hop` and `dump`; `write`
imports the summary a document already holds.

### Rehearse before touching the real store

*You want to see what a hop would do first.* Point the **destination's** store variable at an empty
scratch folder: the session is written there, and your real store is not touched.

```powershell
$env:CLAUDE_CONFIG_DIR   = Join-Path $HOME '.claude'      # the real source
$env:PI_CODING_AGENT_DIR = 'C:\work\rehearsal\pi'         # an empty scratch destination
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project' --manifest C:\work\rehearsal\manifest.json
```

Read the manifest ([section 4](#4-reading-the-receipt-and-the-manifest)); when it is what you want,
set the variable to the real store and run it again. A hop is repeatable: each run writes another
new session and changes nothing else.

### Hand a session to someone else, or another machine

*Export a harness-neutral document, move the file, write it on the other side.* Use `--output`, not
a shell pipe (PowerShell re-encodes piped text with the console code page and garbles non-ASCII
text):

```powershell
# machine A
mukade dump claude_code YOUR_CLAUDE_SESSION_ID --output C:\work\session.json

# machine B (any destination harness; its store variable set)
$env:CODEX_HOME = 'D:\codex\home'
mukade write codex C:\work\session.json --cwd 'D:\work\my-project'
```

A document holds conversation data: protect it like the original session. `write` needs the
destination harness named explicitly, and the destination must have its own configuration and
credentials — mukade does not transfer them.

### Audit what a conversion lost

*You need to know exactly what changed.* `--manifest <file>` also saves the manifest, and the
receipt prints it anyway. Its `counts` and `losses` say what was dropped, rewritten, or written as
text; its `source.content_sha256` is the SHA-256 of the **source file's own bytes**, so you can
check it with `Get-FileHash -Algorithm SHA256 <file>` (or `sha256sum`) and know which exact file
was carried. See [section 4](#4-reading-the-receipt-and-the-manifest).

### A session that was rewound or branched

*You rewound in Claude Code, edited a prompt, or navigated back in Pi's `/tree`.* Mukade reads the
selected branch: the abandoned turns do not become history in the destination. They stay in the
source untouched, and the manifest counts them
(`dropped_off_branch`). Pi's `branch_summary`, `bashExecution`, `custom_message` and `context_edit`
entries reach the destination the way Pi shows them to its model.

For Claude Code, `last-prompt.leafUuid` selects a checkpoint when present. A later continuation
descending from it can supersede it. A new user branch can supersede it when the checkpoint's
leaf was already recorded before the checkpoint. A checkpoint flushed before its own leaf is
not treated as a rewind ordering barrier. Text beside tool results can start a new user turn too.
Without a checkpoint, the last non-sidechain user or assistant
row selects the branch. Parent links and compaction metadata reconstruct its order; split responses
and parallel results are grouped by message and call IDs, even if written late. A UUID-less block,
result or compaction summary is recovered only when those identities establish its position.
An explicit parent on an abandoned conversation branch prevents a tool result from being
recovered into the selected branch, even if `sourceToolAssistantUUID` names an active response.
Missing parents, cycles, duplicate UUIDs, contradictory compaction metadata and genuinely ambiguous
records are refused, not silently assigned to a branch. Real automatic compaction and rewind
remain to be verified against Claude Code itself.

Codex Desktop 0.165 keeps such a thread over two files: the earlier one, and a page that continues
from part of it, which is what Codex resumes (redone again, a thread gets a further page that
continues from the previous one). mukade reads the latest version, the way Codex resumes it: each
earlier file up to the point the next one continues from, then the latest page; what the earlier
files hold past those points is left behind and counted. To
carry **everything**, the abandoned turns included (in file order, before the active ones), add
`--all-branches` to `dump` or `hop`:

```powershell
mukade hop codex YOUR_CODEX_THREAD_ID claude_code --cwd 'D:\Projects\my-project' --all-branches
```

`--all-branches` is read for Codex only; for another harness it is refused (exit 2).

To omit historical calls and their results, use `--without-tools` with `hop` or `write`. The
manifest records `options.without_tools`, the `tools_omitted` count and the loss. It is not a
`dump` or `list` option.

`--expect-sha256 <hash>` on `hop` checks the captured source fingerprint before writing or
starting a source-model summary. A mismatch is refused with exit 2. It checks the selected source
in its entirety; an appended file changes the fingerprint rather than automatically freezing the
previous prefix.

A host importing into OpenCode can reserve an ID with `--dest-id ses_<id>`, select the primary
agent with `--dest-agent <name>`, and supply a JSON object with `--metadata-file <path>`. When an
ID or metadata is supplied, `OPENCODE_DB` must identify the destination explicitly: mukade checks
the resulting ID and metadata there after the native import. If the process fails or the check
cannot confirm the write, exit 5 reports an uncertain result and its ID; inspect the destination
before retrying. A metadata file may be at most 1 MiB and should be protected by the host.

`--dest-path <path>` reserves the destination file itself, for the file-based destinations (Claude
Code, Codex, Pi). It must be absolute, inside the destination root, and free of `..`; OpenCode refuses
it (exit 2) because its native import allocates the file. A host that retries a hop states both
`--dest-id`/`--dest-path` and `--expect-sha256`, so the second run lands on the same destination and
only while the source is unchanged.

`--tool-map <path>` states the source-to-destination tool bindings explicitly instead of the
destination catalog's defaults. The file is a JSON array of `[source, destination, original, mapped]`
tuples: the two names are tool names (up to 256 characters) and the two values are the JSON objects
the conversation holds and the destination should receive. At most 4096 entries and 1 MiB; `hop`,
`write` and `document` accept it, and a malformed map is refused before anything is written.

`--dest-shell bash|sh|zsh|powershell|pwsh` states the shell the destination is configured to use; it
matters only where the destination records one.

### Use it from a task runner or another program

*A host that already runs these harnesses wants the pipeline in-process.* mukade is a library first:
`mukade::hop(&HopRequest { … })` does everything the command does, with typed errors
(`HopError::ResumeFailed` carries the written session's receipt, so a caller never has to guess
whether the destination exists). See the [README](../README.md#library).

### Carry mukade itself on a stick

*A machine you do not own, or one you cannot install on.* Copy `mukade.exe` to the stick and run
it from there: it generates what it needs next to itself ([section 6](#6-where-its-files-go)), so
deleting the folder leaves nothing on the machine.

## 4. Reading the receipt and the manifest

`hop` and `write` print one JSON object (this one is a real run: the Claude Code fixture hopped
into Pi with `--compaction portable`; long paths shortened):

```json
{
  "id": "a7b29d83-3e83-4c9a-87fd-b136eba01dfa",
  "location": "<PI_CODING_AGENT_DIR>/sessions/--…--/2026-10-28T16-00-00-000Z_<id>.jsonl",
  "resume": ["pi", "-p", "--session", "<that file>"],
  "manifest": {
    "source": { "harness": "claude_code", "version": "2.1.311",
                "session_id": "c45db7bc-…", "content_sha256": "df8f8f84d7fd3d21…" },
    "dest":   { "harness": "pi", "version": "1.2.0", "session_id": "a7b29d83-…", "location": "…" },
    "options": { "compaction": "portable", "summary": null, "reasoning": "drop_foreign", "resume_prompt": false },
    "counts": { "messages_in": 5, "messages_out": 5, "tool_calls": 0, "tools_text_form": 0,
                "reasoning_dropped": 0, "reasoning_as_text": 0,
                "dropped_orphan_results": 0, "dropped_duplicate_results": 0,
                "rewritten_call_ids": 0, "dropped_off_branch": 0, "dropped_unfinished_lines": 0,
                "aborted_steps_as_replies": 0, "unanswered_calls": 0, "dropped_empty_steps": 0,
                "dropped_attachments": 0, "dropped_result_blocks": 0,
                "dropped_unreadable_summaries": 0 },
    "losses": [
      "interrupted_last_turn",
      "the source's compaction summary travels as one user message",
      "the source's own compaction is in the session (kept 49 entries after the boundary, from 5237bfb4-…)"
    ],
    "strict": { "checked": true, "violations": [] }
  },
  "resume_output": null
}
```

- `id`, `location`, `resume`: the new session's id, where it was written, and the destination's own
  command to continue it. Keep the destination's store variable set when you run it by hand.
- `resume_output`: the destination's reply, when you passed `--resume-prompt`.
- `counts`: every kind of loss as a number; every number above zero also has a line in `losses`.
- `strict`: the plan was held against the rules a provider API enforces (every tool call answered
  once, ids in the allowed pattern, …) **before** anything was written; a plan that would be
  refused is not written (exit code 4).

What the `losses` lines mean, and what to do:

| line | what happened |
|---|---|
| `interrupted_last_turn` | the source ended on a turn with no answer (the harness was killed, or the step was aborted); it travels as a user message with no reply. Normal. |
| `reasoning dropped: <reason> (n)` | thinking can only be replayed to a provider that speaks the same protocol and whose format can hold its signature or encrypted payload; otherwise it is dropped, one line per reason: *the destination's provider protocol is not known; name it with --dest-protocol* (OpenCode, Pi), *made by another provider protocol*, *no signature or encrypted payload to replay it with*, *the destination's format has no place for its payload*, *not carried, as --reasoning drop asks*, *encrypted only, no visible text to write as text*. `--reasoning as_text` keeps it as labelled text instead. |
| `reasoning written as labelled text, no longer replayable by a provider (n)` | `--reasoning as_text`: the destination's model can read it, its provider cannot replay it. |
| `tool calls written as text, not translated (n, Tool, …)` | the destination has no equivalent tool, or an argument had no counterpart; the call and its result travel as text. Nothing is invented. |
| `unanswered tool calls carried as text (n)` | the source never recorded a result for a call (a killed tool); a provider refuses a call without its result, so it travels as text. |
| `orphan tool results dropped (n)` · `duplicate tool results dropped, the first answer kept (n)` | a result with no call, or a second answer to one call; a provider would refuse either. |
| `tool call ids rewritten to the destination's id pattern (n)` | Anthropic accepts only `[a-zA-Z0-9_-]` ids; the call and its result were rewritten together. |
| `records of an abandoned branch … left behind (n)` | a rewind, an edited prompt, or the earlier file of a Codex paginated thread: only the resumed branch travelled. `--all-branches` carries a Codex thread whole. |
| `compaction summaries with no readable text (encrypted by the source's provider), not carried (n)` | Codex 0.165 keeps its compaction summary encrypted: there is no text to write, so it is left out rather than written as an empty message. The history before and after it travels. |
| `an unfinished last line of the source left unread (n)` | a hard kill cut the last line mid-write; the rest of the session was read. |
| `steps the source aborted or that failed, written as ordinary replies (n)` | the destination cannot mark a step as aborted (Pi can, and does). |
| `empty steps dropped (n)` · `user attachments dropped (n)` · `structured tool-result blocks flattened to text (n)` | content the destination cannot hold (a PDF, an image toward a text-only result) is counted, never silently dropped. Images are rewritten into each harness's own shape. |
| `the source's compaction summary travels as one user message` | `--compaction portable`. |
| `the resume prompt did not complete: …` | the session was written but `--resume-prompt` failed (exit 5): see [troubleshooting](#11-troubleshooting). |

### Tool calls and reasoning: what is converted, and when

**`dump` converts nothing.** The document keeps every tool call as the source wrote it: its own
name, its own arguments, and the canonical form mukade read it as. Reasoning blocks keep their text
and their opaque part (below). Conversion happens when a session is **written** into a harness, by
`write` or `hop`, so one document can be written into any of the four with the same result a hop
would have.

**Tool calls** are translated to the destination's own tool, by the rules of
[`tool-conversion-tables.md`](tool-conversion-tables.md); one with no equivalent travels as text,
with its arguments, and is counted (`tools_text_form`).

**Reasoning (chain of thought)** is not converted, because it cannot be. A block has visible text
and an opaque part — a signature over the text, or a provider-encrypted payload — that only the
provider protocol that produced it can read back, and that mukade never edits or makes up.
`--reasoning` decides what happens to each block:

| `--reasoning` | what happens |
|---|---|
| `drop_foreign` (default) | kept **natively** when the destination speaks the same protocol and its format has a place for the opaque part; otherwise dropped and counted (`reasoning_dropped`), with the reason in `losses` |
| `as_text` | written as visible text before the answer it led to, labelled `[earlier reasoning, not replayable]`: the destination's model can read it, but it is no longer reasoning to its provider; counted (`reasoning_as_text`). A block with no visible text (encrypted only) is dropped |
| `drop` | never carried |

Where the opaque part fits: Claude Code takes a signature (`thinking`) or an encrypted payload
(`redacted_thinking`), Codex only an encrypted payload, Pi only a signature, OpenCode either. So
Claude → Claude and Codex → Codex keep reasoning natively; Claude → Codex and Codex → Claude drop
it, or carry it as text with `as_text`.

OpenCode and Pi speak whatever provider they are configured with, so mukade does not know their
protocol, and **with the default every reasoning block toward them is dropped**. Say which protocol
the destination will use and blocks from the same protocol are kept:

```powershell
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project' --dest-protocol anthropic-messages
```

`--dest-protocol` is `anthropic-messages`, `openai-responses` or `openai-chat`. Name the one the
destination's configured provider really uses. mukade takes it on trust: the strict check refuses a
block of one protocol carried into another (exit 4), but it compares against this value, so it
cannot tell that the value itself is wrong.

## 5. Speed

A handover is meant to feel instant. Measured on this repository's release build, hopping a Claude
Code session of that size (mostly tool output, like real ones) into Pi, Claude or Codex:

| source session | time (Pi, Claude or Codex as destination) | peak memory |
|---|---|---|
| 1 MB | 30–35 ms (`dump` alone: 24 ms) | not measured |
| 10 MB | 185–240 ms (`dump` alone: 150 ms) | about 100 MB |
| 30 MB | 440–630 ms (`dump` alone: 420 ms) | about 300 MB |

Before the work on speed the same hops took 83, 556 and 1657 ms. The time is proportional to the
size of the session, because every record is read and decoded once.

`--timing` prints where the time went, phase by phase, on stderr:

```text
read file                          0.5 ms
parse json lines                   2.5 ms
resumed branch                     0.2 ms
entries                            0.4 ms
finish                             0.0 ms
fingerprint (parallel)             3.4 ms
build entries                      1.8 ms
probe destination version          0.6 ms
select                             0.8 ms
plan                               0.0 ms
strict check                       0.0 ms
serialize + write file             5.2 ms
write files                        5.8 ms
total                             16.2 ms
```

(A real run on the 5-message Claude Code fixture, `hop claude_code … pi --compaction portable`.
A phase is printed when it ends, so one that runs inside another comes first: `serialize + write
file` is inside `write files`, and `resumed branch`, `entries`, `finish` are inside `build entries`.
`fingerprint (parallel)` runs on a second thread while the file is parsed.)

What makes it fast, and what you can do:

- **The file formats are not the cost.** Claude Code, Codex and Pi store JSON Lines and OpenCode
  SQLite; those are the harnesses' own and mukade reads and writes them as they are. A binary
  intermediate would not help — a hop has none. Decoding is spread over up to 8 threads for large
  files; the source's fingerprint is hashed on another thread while the rest is read.
- **Starting the destination to ask its version is the real cost for small sessions.** Pi's
  `--version` takes about half a second (it starts Node). mukade therefore reads the version from the
  npm package the launcher starts, without starting anything; for a native binary it asks once and
  remembers the answer by the binary's path, size and modification time, in `mukade-data/cache`
  beside the executable ([section 6](#6-where-its-files-go)). To skip the question entirely,
  state the facts: `--dest-version`, and for OpenCode `--project-id`, `--model-provider`, `--model`.
- A `--resume-prompt` starts the destination harness and a model: that is its time, not mukade's.

## 6. Where its files go

mukade has no installer or configuration file. On its first start it copies the editable intent
prompts to `intent-prompts` beside the executable; an existing file is never replaced. By default it
keeps its other files in `mukade-data` beside the executable. `MUKADE_DATA_DIR` can select a
different **absolute** data folder for a host; an invalid override is refused rather than
redirected. The data folder is made the first time something is written to it (never at start-up):

| file | what for |
|---|---|
| `intent-prompts/*.md` | the editable summary prompts: `generic.md` is the default, every other file stem is an `--intent` name; copied on first start and never overwritten |
| `mukade-data/cache/probes.json` | the versions and OpenCode project ids learned from harness binaries, keyed by the binary's path, size and modification time, so the next hop does not start the harness to ask again |
| `mukade-data/tmp/mukade-import-*.json` | the document `opencode session import` needs as a file; private, and removed when the import ends, even when it fails |
| `mukade-data/tmp/summary-*/` | one folder per `--compaction summary` run: the file Codex writes its summary to, the session Pi forks; removed when the run ends, even when it fails |
| `.mukade-*.tmp` | beside the session being written, in the destination's store: the session as it is written, renamed into place when complete |

Nothing is written to the user profile, the application-data folders, the registry or the system's
temporary folder. A temporary file or summary folder a **killed** run left behind is removed by the
next run once it is an hour old. The files that *are* yours stay yours: the sessions it writes into the harnesses'
stores, and what you ask for with `--output` and `--manifest`. To remove mukade, delete the folder
it is in.

If the selected data folder is in a place it cannot write to, the folder cannot be made.
mukade does not look for another place: the cache is not kept (every hop asks the harness, which
costs time but works), and a hop to OpenCode stops with an error that names the folder, before
anything is written. Set `MUKADE_DATA_DIR` to an absolute writable folder, or run the executable
from a writable location without the override.

**What it looks like.** A real run: the executable copied alone into an empty folder, then two hops
to Claude Code whose launcher answers `--version` (`--timing` shows the cost of asking):

```text
before the first hop          after it
tools\                          tools\
  mukade.exe                        mukade.exe
                                    intent-prompts\
                                    mukade-data\cache\probes.json

1st hop   probe destination version   70.8 ms   total  78.6 ms
2nd hop   probe destination version    0.3 ms   total   9.4 ms     (answered from the cache)

probes.json: {"version|<dir>/claude.cmd|20|1793205746861450700":"2.9.9"}
```

A hop that has nothing to ask adds nothing under `mukade-data`: a hop to Pi, with no `pi` launcher
to start, left `mukade-data` absent.

This is checked by an automated test that runs the real binary alone in a folder, with the
temporary folder, the home folder and the application-data folders pointed at an empty sandbox, and
asserts the sandbox stays empty and the folder holds the executable, `intent-prompts` and
`mukade-data`.

**Harnesses are separate.** A harness installed in its own folder is reached with its store
variable and `--<harness>-exe`: see [section 8](#8-stores-and-executables) and the examples in
[section 9](#9-per-harness-examples).

## 7. Exit codes

| code | meaning |
|---|---|
| 0 | done |
| 1 | a command-line mistake (unknown option, a flag without its value, an unexpected argument) |
| 2 | refused with a reason; nothing was written: the session is not there, the store is not configured, the id is ambiguous, or the request is not supported (a resume prompt starting with `-`, `--compaction portable` on a source that never compacted, …) |
| 4 | failed and nothing changed: the plan would be refused by a provider, the source's model could not write the summary, or the write failed before starting the destination import |
| 5 | recovery is needed: the source could not be read, an external import has an uncertain outcome, or the session was written but the resume failed. A failed resume still prints its receipt and saves `--manifest`; an uncertain import names its destination ID |

## 8. Stores and executables

| Harness | CLI name | Store/config variable | Executable flag | Executable environment variable |
|---|---|---|---|---|
| Claude Code | `claude_code` | `CLAUDE_CONFIG_DIR` | `--claude-exe` | `MUKADE_CLAUDE_EXE` |
| Codex CLI | `codex` | `CODEX_HOME` | `--codex-exe` | `MUKADE_CODEX_EXE` |
| OpenCode | `opencode` | `OPENCODE_DB` (database file), plus its applicable `XDG_*` settings | `--opencode-exe` | `MUKADE_OPENCODE_EXE` |
| Pi | `pi` | `PI_CODING_AGENT_DIR` | `--pi-exe` | `MUKADE_PI_EXE` |

Claude Code, Codex and Pi require an explicit root, even for standard installations (an empty
variable is no root). OpenCode may use its configured store, but supply its database and
environment explicitly when isolation matters.

An executable override points to the **launcher or executable**, not its installation directory.
The CLI resolves the destination executable using its flag, then its `MUKADE_*_EXE` variable,
then `PATH` (`.cmd` launchers included), then the platform locations in
[`catalog/harnesses.json`](../catalog/harnesses.json), and finally the bare command name. Run
`mukade harnesses` to see which one it resolves. An explicitly configured executable is also put
into the returned resume command. OpenCode is special: when it is the **source**, its export reader
uses `MUKADE_OPENCODE_EXE` or default resolution, not the `--opencode-exe` flag. The flag applies to
OpenCode as a destination.

`--root` changes the selected harness's store for `list`, `dump` or `write`. For `hop`, it applies
to the **source**, including when both ends use the same harness; set the destination variable in
the environment. For OpenCode,
an existing directory passed as `--root` means its `opencode.db`; a file path is used as-is.

A `--resume-prompt` runs the destination with its store variable set to the root the session was
written to, whether it came from the environment or from `--root`, and from the destination's
working directory (`--cwd`, or the source's recorded directory, which must exist on this machine).
A resume command you run later by hand does not inherit that: set the destination's variable in
that shell first. For OpenCode, also configure the appropriate `XDG_*` and `OPENCODE_CONFIG`
settings where needed; a database path alone is not a guarantee of complete isolation.

A resume prompt that starts with `-` is refused, because every harness CLI would read it as an
option; so is one with line breaks when the destination's executable is a `.cmd` launcher, which
cannot take them. Both are refused before anything is written. Every harness process mukade
starts runs with a deadline; one that outlives it is stopped together with everything it started.

## 9. Per-harness examples

Most examples use the harnesses' standard locations. Where a harness is kept in a folder of its own
(`D:\portable\…`, a USB stick, say) the example shows its store variable and executable flag; that
is about the **harness**, not about mukade, which needs nothing beyond its own executable.

### Claude Code

Standard location, read Claude and write Pi (select the source ID from the `list` output):

```powershell
$env:CLAUDE_CONFIG_DIR = Join-Path $HOME '.claude'
$env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'
mukade list claude_code
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project'
```

A custom Claude destination, with Pi as the source:

```powershell
$env:PI_CODING_AGENT_DIR = 'D:\portable\pi\agent'
$env:CLAUDE_CONFIG_DIR = 'D:\portable\claude\config'
mukade list pi
mukade hop pi YOUR_PI_SESSION_ID claude_code --cwd 'C:\work\my-project' --claude-exe 'D:\portable\claude\bin\claude.exe'
```

Keep `CLAUDE_CONFIG_DIR` set when executing the returned `resume` arguments. Mukade does not install
Claude or create its authentication configuration. Claude Code writes the folder name of a project
with its own rule (every character outside `[a-zA-Z0-9]` becomes `-`, a name past 200 characters
is cut and suffixed with a hash), and mukade follows it.

### Codex CLI

```powershell
$env:CODEX_HOME = Join-Path $HOME '.codex'
$env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'
mukade list codex
mukade hop codex YOUR_CODEX_SESSION_ID pi --cwd 'C:\work\my-project'
```

A custom Codex destination:

```powershell
$env:CLAUDE_CONFIG_DIR = 'D:\portable\claude\config'
$env:CODEX_HOME = 'D:\portable\codex\home'
mukade hop claude_code YOUR_CLAUDE_SESSION_ID codex --cwd 'C:\work\my-project' --codex-exe 'D:\portable\codex\bin\codex.exe'
```

Codex requires a destination CLI version in its session metadata. Mukade asks the chosen
executable's `--version`; if that cannot be done or parsed, supply it with `--dest-version 0.164.0`
(your version). Mukade reads `model_provider` from `$env:CODEX_HOME\config.toml` when available;
`--model-provider` states it explicitly: only the top-level `model_provider` key counts, not a
profile's or a provider table's. Keep `CODEX_HOME` set for a resume command run later.

**Threads of the Codex Desktop app (0.165).** The id in a `codex://threads/<id>` link is the
thread id mukade takes:

```powershell
$env:CODEX_HOME = Join-Path $HOME '.codex'
$env:CLAUDE_CONFIG_DIR = Join-Path $HOME '.claude'
mukade hop codex 01a181e2-4cd9-7c63-82b8-f75e3d6bb172 claude_code --cwd 'D:\Projects\daodan'
```

What to expect from such a thread:

- A thread kept over two files (a page that redoes its later turns) is read as Codex resumes it;
  `--all-branches` carries it whole (see [use cases](#a-session-that-was-rewound-or-branched)).
- Its compaction summaries are encrypted, so `--compaction portable` is refused (exit 2) and a
  whole-history hop leaves them out, counted. A long thread is therefore carried whole: a 17.8 MB
  thread became a 4.1 MB Claude Code session of several hundred thousand tokens, which only a
  model with a very large context can resume.
- Most of its tool calls are one `exec` script each (Codex's "code mode"); they travel as text,
  with their script, not as translated calls. Its reasoning is OpenAI's and is dropped toward any
  other provider, counted.
- Messages its subagents sent to the main agent are left out on purpose: their content is
  encrypted, readable only by OpenAI. The messages the main agent sent to its subagents travel as
  text with that encrypted content replaced by `<encrypted by the source's provider, N chars>`.
- In Claude Code, resuming such a session prints `Session model gpt-… could not be restored … using
  claude-…`: the rows name the model that really wrote each reply, which Claude Code does not
  know, so it carries on with its own. Expected; nothing to fix.
- **Mind the size before writing in it.** Each message sent in a resumed session reads its whole
  context first: a session of several hundred thousand tokens costs that much on the first message,
  or makes Claude Code compact it before answering.

### OpenCode

Read OpenCode and write Pi. `list opencode` requires the selected database via `OPENCODE_DB` or
`--root`, reads it without starting OpenCode, and includes top-level archived sessions.

```powershell
$env:OPENCODE_DB = 'C:\path\to\opencode.db'
$env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'
Set-Location 'C:\work\my-project'
mukade list opencode
mukade hop opencode YOUR_OPENCODE_SESSION_ID pi --cwd 'C:\work\my-project'
```

Mukade calls `opencode session export … --standalone` to read the source. Set the `XDG_*`
variables used by your OpenCode installation in this shell as well, if applicable.

A custom OpenCode destination. The database, configuration, XDG directories and executable must
belong to the intended installation; the project ID, provider and model must already be valid for it.

```powershell
$env:CLAUDE_CONFIG_DIR = 'D:\portable\claude\config'
$env:OPENCODE_DB = 'D:\portable\opencode\data\opencode.db'
$env:XDG_DATA_HOME = 'D:\portable\opencode\data'
$env:XDG_CONFIG_HOME = 'D:\portable\opencode\config'
$env:XDG_STATE_HOME = 'D:\portable\opencode\state'
$env:XDG_CACHE_HOME = 'D:\portable\opencode\cache'
$env:OPENCODE_CONFIG = 'D:\portable\opencode\config\opencode.json'
mukade hop claude_code YOUR_CLAUDE_SESSION_ID opencode --cwd 'C:\work\my-project' --opencode-exe 'D:\portable\opencode\opencode.exe' --project-id YOUR_OPENCODE_PROJECT_ID --model-provider YOUR_PROVIDER --model YOUR_MODEL
```

OpenCode's import needs a project ID and a complete provider/model reference. Mukade can discover
the project from OpenCode's existing sessions (asked of the store the session will be written to),
and the provider and model from its configuration; explicit values avoid relying on discovery, and
are faster (see [Speed](#5-speed)). OpenCode imports with `--standalone`.

For a **custom OpenCode source**, set its executable through the environment variable before
exporting:

```powershell
$env:MUKADE_OPENCODE_EXE = 'D:\portable\opencode\opencode.exe'
$env:PI_CODING_AGENT_DIR = 'D:\portable\pi\agent'
mukade hop opencode YOUR_OPENCODE_SESSION_ID pi --cwd 'C:\work\my-project'
```

For an OpenCode destination, the receipt's `location` names the database the session was imported
into (`OPENCODE_DB`), or says it went to OpenCode's configured store; the import document is a
temporary file of mukade's own and is removed after the import, on success and on failure. Use the
returned ID with that store, and keep the OpenCode environment configured when using the resume command.

### Pi

Read Pi and write Claude Code:

```powershell
$env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'
$env:CLAUDE_CONFIG_DIR = Join-Path $HOME '.claude'
mukade list pi
mukade hop pi YOUR_PI_SESSION_ID claude_code --cwd 'C:\work\my-project'
```

A Pi destination kept in its own folder:

```powershell
$env:CLAUDE_CONFIG_DIR = 'D:\portable\claude\config'
$env:PI_CODING_AGENT_DIR = 'D:\portable\pi\agent'
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project' --pi-exe 'D:\portable\pi\bin\pi.cmd'
```

Mukade writes the new Pi session below `PI_CODING_AGENT_DIR\sessions`, in the folder Pi itself
would use for the project. The receipt's `resume` contains the chosen launcher and the session
file path. Keep `PI_CODING_AGENT_DIR` set when running it; separately verify that Pi
installation's provider credentials and configuration. A separate agent directory does not by itself
prove that every resource Pi reads is isolated. Sessions written this way were run against Pi 1.2.0
and 1.4.0 (see `docs/verification/pi-session-and-reasoning-verification.md`).

Each harness has its own reference page, with how it is installed and launched, its store,
compaction, tools, plugins and hooks: [`docs/harnesses/`](harnesses/).

## 10. Discover, export, import

`detect` inspects store evidence; it does **not** identify an installation directory or configure an
executable. It can return zero or multiple candidates. An empty or ambiguous root cannot be inferred;
pass the harness explicitly when you know it. For a nonstandard OpenCode database filename, pass
`opencode` and its file path explicitly rather than relying on detection.

`intents` lists the editable summary prompts without reading a session or starting a harness. Its
JSON gives the folder, the default intent, and every prompt with `name`, `path`, `builtin` (the name
ships with mukade) and `custom` (a file in the folder supplies it):

```powershell
mukade intents
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --intent review
mukade dump codex YOUR_CODEX_SESSION_ID --intent analyze --output 'C:\work\summary.json'
```

A trimmed answer (paths are illustrative):

```json
{
  "directory": "D:\\tools\\intent-prompts",
  "default": "generic",
  "prompts": [
    {"name": "generic", "path": "D:\\tools\\intent-prompts\\generic.md", "builtin": true, "custom": true},
    {"name": "review", "path": "D:\\tools\\intent-prompts\\review.md", "builtin": true, "custom": true},
    {"name": "security", "path": "D:\\tools\\intent-prompts\\security.md", "builtin": false, "custom": true}
  ]
}
```

```powershell
mukade detect 'D:\portable\pi\agent'
mukade list --root 'D:\portable\pi\agent'
mukade dump pi YOUR_PI_SESSION_ID --root 'D:\portable\pi\agent' --output 'C:\work\session.json'
$env:CLAUDE_CONFIG_DIR = 'D:\portable\claude\config'
mukade write claude_code 'C:\work\session.json' --cwd 'C:\work\my-project' --claude-exe 'D:\portable\claude\bin\claude.exe'
```

With `--root` the harness may be left out of `list`, `dump` and a hop's source: it is inferred from
the directory's own layout when exactly one harness is recognized. `write` always needs it.

## 11. Troubleshooting

- **No store or session found** (exit 2): check the variable, `--root`, source ID and whether the
  intended project/store contains that session. Use `mukade detect <path>` for diagnostic evidence.
- **Ambiguous or empty detection:** specify the harness explicitly; a newly created empty store has
  no reliable session layout to infer.
- **Cannot run the executable:** verify the launcher path, its runtime and permissions. A custom
  OpenCode source requires `MUKADE_OPENCODE_EXE`; a destination uses its corresponding flag or env var.
- **Codex version or OpenCode project/model missing:** provide the actual destination version,
  project ID, provider or model using the flags described above.
- **"mukade could not create its temporary directory"** (exit 4, from a hop to OpenCode): the
  selected data folder cannot be written. Nothing was imported; set `MUKADE_DATA_DIR` to an
  absolute writable folder or use the default beside a writable executable.
- **"summary failed: …"** (exit 4, `--compaction summary`): the source harness could not write the
  summary, and nothing was written. The message names the command (the prompt shown as
  `<summary prompt v1>`), its exit status and what it printed: a login or quota problem of the
  source harness, a model it cannot reach (name another with `--summary-model`), a run longer than
  `--summary-timeout`, or a model that called a tool instead of summarizing. When OpenCode's fork
  could not be deleted, the message ends with the command that removes it.
- **"the summary prompt cannot be passed through the batch launcher â€¦"** (exit 2): the selected
  `intent-prompts/<name>.md` contains a line break or a character an install's `.cmd` shim cannot
  carry. Name the harness's native executable, or keep the prompt to ASCII letters, digits, spaces
  and `.,;:()`.
- **External OpenCode import is uncertain** (exit 5): the error names the attempted session ID.
  Check that ID and its provenance in the selected database before retrying; a zero exit from the
  underlying command alone is not enough to prove that it imported the session.
- **Resume fails after a successful write** (exit 5): the session already exists. Mukade still
  prints its receipt on stdout and saves `--manifest`: use that `id` and `location` instead of
  retrying, which would create another session. A resume that outlives its timeout is stopped,
  including a harness behind a `.cmd` launcher. Check configuration and credentials in the process
  environment; provider availability must be verified in your own environment.
- **Manifest could not be saved:** the session was still written, and its receipt is on stdout; the
  error names its location. Save the `manifest` field of that receipt instead of running again.
- **It is slower than the table in [Speed](#5-speed):** run with `--timing`. A large `probe …` line
  is a harness being started to ask its version or project: pass `--dest-version` (and
  `--project-id`), so the question is not asked at all. (The answer is remembered after the first
  time, in `mukade-data/cache`, when the executable's folder can be written to.)
- **"The paging file is too small" / "memory allocation failed":** the machine ran out of memory
  (a 30 MB session needs about 300 MB). Close other programs and retry; nothing is half-written,
  because a session is written to a temporary file and renamed into place.
- **A path or file name problem on Windows:** Pi files a session under a folder named after the
  whole project path, so a very deep project path can pass the 260-character limit; use a shorter
  `--cwd`.
