# ChatGPT-hosted chats of the Codex desktop app — design

Status: approved by the owner on 2026-11-21 (approach "2": local catalogue first, then the official
export). Changed while planning, the same day: phase 1 lives in the Codex adapter, not in a seventh
harness (see "Where it lives"); to be confirmed by the owner with the plan.

## Intent and why

The owner had a chat in the Codex desktop app, "Capire lo stack di Daodan", that stopped at the
context limit. The agent in it could no longer push to `develop`, and the owner had to cherry-pick
the work by hand so as not to lose it. With mukade the owner wanted to **find** that chat,
**explore** it, and **port** it to Codex CLI or Claude Code "with all its files, worktree, local
checkout".

mukade did not show the chat at all: it is not a Codex rollout. This design makes mukade show such
chats, show the work they left on disk, start a new session on that work in another harness, and,
when the owner supplies ChatGPT's own export, carry the conversation too.

## What was found on the owner's machine (2026-11-21, read-only)

Codex desktop `26.1117.41037` (MSIX `OpenAI.Codex_2p2nqsd0c76g0`), `codex-cli 0.167.0-alpha.3.2`,
Windows 11.

| fact | where | checked how |
|---|---|---|
| The desktop app keeps a catalogue of chats that is not the `threads` table | `<CODEX_HOME>/sqlite/codex.db`, table `local_thread_catalog` (137 rows), hosts in `local_thread_catalog_hosts` | opened `mode=ro` |
| Every row there is hosted by ChatGPT | `host_kind = 'chatgpt'`, `source_kind = 'chatgpt'`, `cwd` NULL on all 137; `threads` in `state_5.sqlite` has 0 rows | query |
| A row has | `thread_id`, `display_title`, `source_created_at`, `source_updated_at` (epoch seconds, fractional), `project_id` (`g-p-<hex>` or NULL), `conversation_origin` (`tpp` or NULL), `missing_candidate` | query |
| 15 of the 137 belong to a ChatGPT project; 122 are plain chats | `project_id` not NULL | query |
| A ChatGPT project maps to a local folder | `state_<n>.sqlite`: `projects.metadata` is JSON whose `codex.chatGptBacking` value is itself a JSON string holding `projectId` (the `g-p-…` id); `project_roots.path` is the folder (`C:\Users\…\Documents\ChatGPT\Daodan`) | query |
| The project folder is not a repository; it holds clones one level down | `daodan/` (`develop`, in sync), `imago/` (`develop`, clean), `stack-analyser/` (`feature/stack-detection`, no upstream, 133 uncommitted paths) | `git status` |
| The text of a hosted chat is not on disk | no rollout, nothing in the app's web cache; the app's log shows it refetched by the web layer (`chatgpt_conversation_refetch_completed … mappingSize=56`) | search, log |
| Codex's own programs do not return it | `codex app-server`: `thread/list` empty, `thread/read` → `thread not loaded`, `thread/items/list` → `not supported yet`; `codex cloud list` → 0 tasks | live probe |
| The schema of `codex.db` is versioned | table `codex_schema_migrations` (40 rows: `id`, `app_version`, …) | query |

Not checked: the layout of ChatGPT's export as it is today (the owner has to request one), and
whether the catalogue's `thread_id` is the export's conversation id (the app's route
`/g/<project>/c/<thread_id>` says it is a ChatGPT conversation id).

## Goals

1. `mukade list codex` and the desktop app list the hosted chats that have a local folder: title,
   date, and the project's folder as the working directory. (All hosted chats, in phase 2.)
2. A hosted chat whose text mukade does not have is listed with the reason, not hidden and not
   shown as empty.
3. For a chat with a local folder, mukade shows the work on disk: each repository in the folder,
   its branch, upstream, commits not pushed, and paths not committed.
4. From such a chat the owner starts a **new** session of Codex CLI, Claude Code or another harness
   in that folder, with a handoff note that names the chat and the state of the work.
5. Given ChatGPT's export, mukade reads the conversation of a listed chat, shows it in the reader,
   and hops it to any destination like any other source.

## Non-goals

- Signing in to ChatGPT, or any network request for a chat's text.
- Reading the app's web cache or its logs for text.
- Writing a ChatGPT conversation (ChatGPT is a source only).
- Claude Code cloud sessions and the Claude desktop app (separate work; `--teleport` is untested).
- Carrying files attached to a ChatGPT conversation in the first version.
- Marking which Codex rollouts the desktop app made (`threads.originator`): separate, small.

## Anti-goals

- A chat shown as readable when its text is not there, or a hop that writes an empty or partial
  session without saying so.
- mukade holding or using the owner's ChatGPT credentials (`auth.json` is never opened).
- Any write to `codex.db`, `state_<n>.sqlite`, the export, or the repositories in the project
  folder. The work-on-disk view runs no git command that changes anything.
- Reading an export or a catalogue layout mukade was not verified against as if it were known.

## Constraints

- One adapter per harness; nothing outside it branches on `HarnessId` (`tests/architecture.rs`).
- A source-only harness keeps `destination()` = `None` (ADR-0002, INV-002).
- A reader accepts only a store version it was verified against and names what it found otherwise
  (ADR-0002, INV-003).
- No session is too large to read (ADR-0004): an export can be hundreds of MiB in one file.
- Dependencies stay pure Rust; the single-executable build is kept.
- Rust is compiled and tested on GitHub Actions only; nothing is built on the owner's machine.
- Desktop UI uses Imago's controls and tokens.

## Approaches

| | what | trade-off |
|---|---|---|
| 1 | Local only: catalogue, work on disk, handoff into a new session | everything is on disk and nothing can break by a server change; the conversation itself does not travel |
| **2** | **1, plus reading ChatGPT's export and joining it to the catalogue by id** | **the port is complete and uses only a file ChatGPT gives the owner; the export is requested by hand and arrives by e-mail** |
| 3 | Fetch the text with the owner's login | immediate; uses private endpoints that change without notice, is outside the service's terms, and would make mukade handle credentials |

Dropped earlier: asking the installed Codex (`codex app-server`) for the chat, the clean way, because
the live probe showed it does not serve hosted chats.

**Chosen: 2**, by the owner, built in two phases so that phase 1 is useful alone: it is the part
that would have avoided the manual cherry-pick.

## Design

### Where it lives

**Phase 1 is inside the Codex adapter; no new harness.** The Codex reader already lists a thread
whose rollout is gone as unreadable, from Codex's own state database (`reader/codex_state.rs`,
`list_in` in `reader/codex.rs`). A hosted chat is the same case from the app's other catalogue: a
chat of a Codex installation whose text is not on disk. So the hosted chats are read by a new
module beside `codex_state` and merged into the Codex listing. `HarnessId`, the catalog's
required fields, the CLI's harness names and the desktop's locations stay as they are.

A seventh harness (`chatgpt`) was the first design and is put off to phase 2, where a ChatGPT
conversation format has to be read. It was dropped from phase 1 while planning: it touches the
closed list in some forty places (21 uses of the catalog's binary and resume accessors, every
loop over `HarnessId::ALL`, `tests/harnesses.rs`), none of which phase 1 needs, and Rust cannot be
compiled on the owner's machine to find them one by one.

### Phase 1 — find, see the work, hand off

**Listing.** `mukade list codex` and the desktop's Codex location also list each catalogue row
whose host has `host_kind = 'chatgpt'` **and whose project has a local folder** (15 of the 137 on
the owner's machine). A hosted chat without a folder has nothing on disk and nothing mukade can do
with it before phase 2, so it is not listed yet.

| field | from |
|---|---|
| `id` | `thread_id` |
| `title` | `display_title` |
| `updated` | `source_updated_at` |
| `cwd` | the `project_roots.path` of the project whose `codex.chatGptBacking.projectId` equals the row's `project_id` |
| `path` | the catalogue file |
| `unreadable` | "hosted by ChatGPT: the conversation is not on this machine; the work it left is in its folder" |

A thread id that Codex also has a rollout or a `threads` row for keeps that listing; the hosted
row is not added twice.

The join is `projects.id = project_roots.project_id`; a project with several roots gives the one
at `position` 0 as `cwd`, and the work-on-disk view covers them all (every project has one root on
the owner's machine).

`missing_candidate` is 1 on 37 of the 137 rows; what the app means by it is not known (the name
suggests a chat it did not find at its last sync). Such a row is listed like the others and keeps
every action: the work on disk is local whatever happened to the chat. The listing does not show
the flag until its meaning is known.

**Version gate.** The module checks that the tables and columns it uses exist
(`PRAGMA table_info`). Like `codex_state`, it is best effort: a database that is missing, locked,
damaged or laid out otherwise gives no hosted chats, and the rest of the Codex listing is what it
was. The verified state is 40 migrations at app `26.1117`; a newer one with the columns is read.

**Reading.** `mukade dump codex <hosted id>` and every read are refused with the listing's reason,
as a thread whose rollout is gone is.

**Work on disk.** A library function, harness-neutral, that takes a folder and returns its
repositories: the folder itself when it is one, else each direct child that is. For each
repository it reports:

- every linked worktree (`git worktree list`), with its path, its branch or detached commit, and
  its count of uncommitted paths (tracked changes and untracked files, ignored files left out);
- every local branch, checked out or not, with its upstream or "none", and its **commits not on
  any remote**: the commits reachable from the branch and from no remote-tracking ref. This is
  the number that matters and it is defined without an upstream, which the owner's own case
  (`stack-analyser`, `feature/stack-detection`) does not have. With an upstream, the count ahead
  of it is shown too.

It runs the installed `git` read-only and never fetches, so what it says about a remote is as of
the last fetch, and it says so. Without `git` it says so. It is offered for every session that has
a `cwd`, not only ChatGPT's: the same view is useful for a Codex or Claude Code session.

- CLI: `mukade work <harness> <id>` prints the table; `--json` for a host.
- Desktop: a "Work on disk" section in the session's detail panel, with the folder path and one
  row per repository. A repository with uncommitted paths or a branch on no remote is marked, since
  that is the work that can be lost.

**Handoff.** A new action, "Start in…", on any session that has a folder and cannot be read: a
hosted chat, and equally a Codex thread whose rollout is gone. It is a harness-neutral library
function with a neutral template. The desktop refuses every action on an unreadable session today
(`readable` in `apps/desktop/src-tauri/src/actions.rs`), so this action and the work-on-disk view
get their own path, which does not need the transcript.

The owner picks a destination; mukade writes the handoff note to a file in its own data folder
(`mukade-data/handoffs/<harness>-<id>.md`, never in the project folder) and opens the destination's
interactive command in a terminal, in the session's folder, with a one-line first prompt that
names the file ("Read <path> and continue from it."). One line, because a multi-line argument does
not survive `wt new-tab -- …` and the `.cmd` launchers. No native session is written: there is no
history to write.

Destinations in phase 1 are the two the owner named, whose start-with-a-prompt form is in their
own help text: Claude Code (`claude [prompt]`, 2.1.330) and Codex CLI (`codex [PROMPT]`, 0.167).
Another harness is added when its form has been seen to work; until then it is not offered, with
the reason. The argv is a new catalog field, `interactive_start`.

The note is text mukade composes, shown and editable before the terminal opens:

```text
This continues a ChatGPT chat that can no longer be used: "Capire lo stack di Daodan"
(ChatGPT project Daodan, last active 2026-11-20). Its conversation is not available here.
Work it left in C:\Users\alfio\Documents\ChatGPT\Daodan:
- daodan: branch develop, in sync with origin/develop, 1 untracked path
- imago: branch develop, clean
- stack-analyser: branch feature/stack-detection, no upstream, 133 uncommitted paths
Start by reading the state of these repositories. Do not push or discard anything before asking.
```

The template is an editable file in a folder of its own beside the executable
(`handoff-prompts/`), not in `intent-prompts/`, where every file is taken for a summary intent
(`intent::list`). A session without a folder has no "Start in…": there is nothing local to
continue from, and the reason is shown.

- CLI: `mukade handoff <harness> <id> <dest>` prints the note and the command; `--run` starts it.

### Phase 2 — the conversation, from ChatGPT's export

**Planning.** The implementation plan written now covers phase 1 only. Phase 2 is planned when the
export has arrived and its layout is recorded here.

**Gate.** Phase 2 starts from a real export of the owner's account. The reader is written against
that layout and a sanitized fixture cut from it; a verification record goes under
`docs/verification/`. Until then nothing below is promised beyond what the real file shows.

**Input.** The export as ChatGPT delivers it: the `.zip`, or the folder it extracts to. mukade
reads it where it is and never copies or changes it.

**What is expected in it** (to confirm on the real file): one or more JSON files of conversations;
each conversation has an id, a title, times, a `mapping` of nodes (id, parent, children, message)
and a `current_node`.

**Reading.**

- The branch read is the chain from `current_node` up through the parents; other nodes are
  off-branch entries (`Meta(OffBranch)`), as the other readers do for an abandoned branch.
- User and assistant text become conversation entries. Tool calls and results whose shape the
  reader knows become tool entries; every other content type is kept verbatim in `raw`, travels as
  text when it has text, and is named in the manifest. Nothing is dropped silently.
- An attachment is named in the manifest as not carried.
- A layout the reader does not recognise is refused, naming the file and what was found.

**Size.** The listing never decodes a whole export. The first time an export is seen, mukade
builds an index (conversation id → file and byte range, title, times) in its cache, keyed by the
export's path, size and modification time; listing and `overview` use the index; a read decodes
one conversation. `read_clipped` and `read_streamed` follow ADR-0004.

**Joining.** A catalogue row and an export conversation with the same id are one session: the
catalogue gives the project and the folder, the export gives the text, and `unreadable` is cleared.
A conversation only in the export is listed too (no folder). A row only in the catalogue stays
unreadable, with the export's date in the reason when the chat is newer than the export.

**Harness.** Whether the conversations are read by a new `chatgpt` harness or by the Codex adapter
is decided here, with the format known. The export is one optional setting (a path), a newer one
replacing the older.

**Hop.** With text, `mukade hop <harness> <id> <dest>` and the desktop hop work as for any source,
with the chat's folder as the working directory. The existing compaction options apply, which
matters here: a chat that hit its context limit is long.

**Desktop.** "Add a ChatGPT export…" in the place where installations are added, with a file and
folder picker. The chat's reader shows the conversation; "Start in…" stays for a chat still
without text, and the normal hop replaces it once the text is there.

## Decided by the owner

- Approach 2, in two phases.
- No sign-in inside mukade for the first version.

## My calls (correct any of them)

- Phase 1 extends the Codex adapter and lists only the hosted chats with a local folder; a
  separate `chatgpt` harness is decided in phase 2, with the export in hand.
- The work-on-disk view is harness-neutral, reads repositories one level below the folder, and
  covers every local branch and linked worktree, without fetching.
- "Start in…" is harness-neutral: any unreadable session with a folder. It starts a fresh
  interactive session and writes no native session; the note goes in a file in mukade's data and
  the prompt is one line.
- Phase 1 offers Claude Code and Codex CLI as handoff destinations; others when verified.
- The export (phase 2) is one separate setting, a newer one replacing the older.
- A row with `missing_candidate` is treated like any other.
- The export is read in place, zip or folder, never copied into mukade's data.
- Attachments are not carried in the first version.
- An ADR (ADR-0005) records the invariants: no credentials and no network for a chat's text, no
  write to Codex's databases or the export, no git command that changes a repository.

## Left to the builder

- The pure-Rust crate that reads the zip, or reading only the extracted folder if none fits the
  single-executable build.
- The exact shape of the index in the cache.
- How the terminal is opened for "Start in…" (the desktop already opens one for resume).
- The mapping of ChatGPT's tool messages to canonical tools, once the real export shows them.
- Names of the Tauri commands and of the detail-panel components, from Imago's design file.

## Appendix: the tables read (Codex desktop 26.1117, 2026-11-21)

`<CODEX_HOME>/sqlite/codex.db`:

```sql
CREATE TABLE "local_thread_catalog" (
  host_id TEXT NOT NULL,
  thread_id TEXT NOT NULL,
  display_title TEXT NOT NULL,
  source_created_at REAL NOT NULL,
  source_updated_at REAL NOT NULL,
  cwd TEXT,
  source_kind TEXT NOT NULL,
  source_detail TEXT,
  model_provider TEXT,
  git_branch TEXT,
  observation_sequence INTEGER NOT NULL,
  missing_candidate INTEGER NOT NULL DEFAULT 0 CHECK (missing_candidate IN (0, 1)),
  thread_source TEXT,
  source_recency_at REAL NOT NULL DEFAULT 0,
  pending_observed_title INTEGER NOT NULL DEFAULT 0,
  project_id TEXT,
  conversation_origin TEXT, trial_conversation_type TEXT, chatgpt_async_status INTEGER,
  PRIMARY KEY (host_id, thread_id)
);
CREATE TABLE "local_thread_catalog_hosts" (
  host_id TEXT PRIMARY KEY,
  host_kind TEXT NOT NULL CHECK (host_kind IN ('local', 'ssh', 'wsl', 'remote-control', 'chatgpt'))
);
```

`<CODEX_HOME>/state_<n>.sqlite` (n = 5):

```sql
CREATE TABLE projects (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  metadata TEXT NOT NULL DEFAULT '{}',
  position INTEGER NOT NULL,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
);
CREATE TABLE project_roots (
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  position INTEGER NOT NULL,
  path TEXT NOT NULL,
  PRIMARY KEY (project_id, position)
);
```

`projects.metadata` of a project backed by ChatGPT, with the inner value a JSON string:

```json
{"codex.chatGptBacking": "{\"accountId\":\"<uuid>\",\"userId\":\"user-<id>\",\"projectId\":\"g-p-<hex>\"}",
 "codex.localProjectId": "<uuid>"}
```

A hosted chat's `host_id` is `chatgpt:<accountId>:<userId>`. Only rows whose host has
`host_kind = 'chatgpt'` are hosted chats; `local`, `ssh`, `wsl` and `remote-control` hosts are
not read here.

## Tests (run on CI)

- Catalogue reader over a fixture `codex.db` and `state_<n>.sqlite` built by the test: listing,
  project folder join, missing columns or database absent → no hosted chats.
- The fixture databases are byte-identical after a read.
- Work on disk over repositories the test creates: clean, uncommitted, no upstream, ahead,
  detached, folder that is itself a repository, no repository.
- Phase 2: export reader over the sanitized fixture (branch choice, off-branch nodes, unknown
  content type kept and named), index reuse and invalidation, join by id, hop to each destination.
- Every new test is a unit test in the library or the desktop crate: the workflow on `develop`
  (`verify.yml`) runs `cargo test -p mukade --lib --test chat_connect` and `-p mukade-desktop`, not
  the other files under `tests/`.
