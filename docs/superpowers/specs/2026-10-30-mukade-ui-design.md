# Mukade UI — sessions explorer and manager

Status: design draft, awaiting approval. Date: 2026-10-30.

Implementation note (2026-10-30): a native TUI now exists in
[`mukade-tui/`](../../../mukade-tui/): harnesses, their sessions, transcript, interactive resume
and a first hop dialog (destination, compaction, resume prompt). It shares this design's
information model but not yet all of its manager actions or the library changes in §7. This
document remains the target for the full GUI explorer/manager.

Interaction reference: [`docs/mockups/tui.html`](../../mockups/tui.html). That mockup is a
terminal-shaped HTML prototype, not an implementation choice; this design keeps its information
model, screens and shortcuts in a native window.

## 1. Intent and why

Mukade today is a library and a command line. A user who wants to find a session, read it, resume
it or move it to another harness has to know the harness, the id and the flags. `mukade-ui` is a
native desktop sessions explorer and manager for the same four harnesses, built on the same UI
stack as Shinatama (GPUI + GPUI Kit), while the `mukade` executable stays a standalone CLI.

The UI is a second consumer of the library, not a second implementation: it never parses a
harness store itself and never invents a session.

## 2. Goals

1. One list of every session of Claude Code, Codex CLI, OpenCode and Pi, with per-harness state,
   store and executable.
2. Open a session and read its transcript faithfully, including tool calls, results, reasoning
   and compaction records.
3. Resume a session in its own harness, interactively, from the session's working directory and
   against the store it belongs to.
4. Hop a session to another harness, with optional compaction/summary, resume prompt, reasoning
   and tool options; show the receipt and every loss.
5. Quick export of a session to the interchange document and quick import of a document into a
   harness.
6. Keep the `mukade` CLI autonomous: building or testing it must not build, or depend on, GPUI.

## 3. Non-goals for v1

- Embedding a PTY or a terminal emulator. Resume opens the harness in the system terminal.
- Editing, deleting or archiving sessions in a harness store.
- Full-text search across every transcript. v1 filters session metadata and searches inside the
  opened transcript.
- Provider login, account management, model pickers for a live harness.
- Daodan tasks, the global overview, roundtable, browser panels and other Shinatama surfaces.
- User-authored summaries. Mukade generates a summary with the source model or reuses the
  source's own record; a free-text summary is not a library capability.
- Installers and auto-update.

## 4. Constraints

- Rust 2024 edition, current toolchain 1.99.1; GPUI Kit `0.8.1` (`gpui-kit`), no webview, no
  Tauri, no Electron.
- All session reads and writes go through `mukade`: `reader::for_harness`, `Reader::list`,
  `export`, `wire::to_document` / `wire::from_document`, `hop`, `import`.
- Store roots are explicit. The UI reads the harness variables first (`StoreRoots::from_env`),
  may resolve a catalog default for a missing root, labels that choice as a default, and lets
  the user override it. Nothing is written without an explicit action.
- The existing dependency list of the `mukade` package does not change. The UI is a separate
  crate with its own dependencies.
- No silent loss: a hop or import always ends with the manifest, including a zero-loss one.
- Data directory follows the CLI rule: `MUKADE_DATA_DIR` when absolute, else `mukade-data` beside
  the executable. The UI writes `ui.json`, `exports/` and `manifests/` there. Nothing goes to the
  user profile by default.

## 5. Approaches considered

| Approach | Trade-off | Verdict |
|---|---|---|
| GPUI + GPUI Kit, separate crate | Same stack as Shinatama; native components; cross-platform; heavy first build; young API | Chosen |
| egui + eframe | Smaller and more stable, but a second visual language beside Shinatama | Rejected |
| ratatui TUI | Matches the existing mockup literally, but the request is a GUI and it cannot show the same layouts | Rejected |
| Web UI served by the binary | Contradicts the single-binary, no-web-shell direction and adds a browser surface | Rejected |

## 6. Architecture

Workspace layout:

```text
Cargo.toml                 # [workspace] members = ["mukade-ui"], default-members = ["."]
src/                       # unchanged mukade library + CLI
mukade-ui/
  Cargo.toml               # gpui-kit, rfd; mukade = { path = ".." }
  src/main.rs
  src/app.rs               # application, window, routing
  src/state.rs             # AppState, scan state, selection, filters, dialogs
  src/sources.rs           # StoreRoots assembly, defaults, overrides
  src/sessions.rs          # list/read/export, background work
  src/resume.rs            # interactive argv per harness, terminal launch
  src/actions.rs           # hop/import option validation and execution
  src/persist.rs           # ui.json (roots, exe overrides, last filter)
  src/views/               # harnesses, sessions, transcript, lineage, dialogs, receipt, settings
```

`cargo build` and `cargo test` at the repository root keep building only the `mukade` package
(`default-members = ["."]`); `cargo build -p mukade-ui` builds the desktop app.

Concurrency: GPUI's background executor runs one scan per harness. Results are applied on the
foreground only if the request generation still matches, so a selection change discards stale
previews. No subprocess, file read or SQLite open runs on the UI thread.

## 7. Library contract and the changes it needs

Used as-is:

- `export(&SessionRef, &roots)` returns `Session` with `Meta` (including `parent`, `models`,
  `git_branch`, counts derivable from entries) and ordered `Entry`s.
- `wire::to_document` / `wire::from_document` implement quick export/import.
- `hop(&HopRequest)` and `import(dest, &Session, &ImportOpts)` write a new native session and
  return `Receipt { id, location, resume, manifest, resume_output }`.
- `StoreRoots::apply_to` points a harness process at the selected store.

The explorer cannot be built on `Reader::list` alone. The builder check found three gaps; this
design fixes them in the library, where the harness knowledge belongs, rather than duplicating it
in the UI.

### 7.1 `SessionInfo` enrichment

`SessionInfo` currently returns `id` and `path` only for Codex and Pi, and no parent for anyone.
The explorer needs title, cwd, updated and parent. Change:

```rust
pub struct SessionInfo {
    pub id: String,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub updated: Option<String>,
    pub archived: bool,
    pub path: String,
    pub parent: Option<String>,   // native link only; None when the harness records none
}
```

- Codex: read `session_index.jsonl` once per `list` for `thread_name` / `updated_at`, and the
  first `session_meta` line for `cwd`. No full rollout read. `parent` stays `None` (Codex records
  no reliable cross-thread parent).
- Pi: read the session header for `cwd`, `parentSession` and timestamp. A Pi display name is not
  cheaply available, so `title` stays `None`; the UI shows the id and the first user message once
  the session is opened. `updated` stays `None` until read.
- Claude Code and OpenCode already fill the available fields; Claude's `parent` stays `None`
  (`--fork-session` writes no parent), OpenCode fills `parent_id`.

The CLI's `list` output gains these values for Codex and Pi. That is additive: the JSON keys are
already present; only `null` becomes a value.

### 7.2 `Reader::list_all`

OpenCode's `list` deliberately lists root sessions only (`WHERE parent_id IS NULL`), and the CLI
depends on that. Add:

```rust
trait Reader {
    fn list(&self, roots: &StoreRoots) -> Result<Vec<SessionInfo>>;
    /// Every session the store holds, forks included. Defaults to `list`.
    fn list_all(&self, roots: &StoreRoots) -> Result<Vec<SessionInfo>> { self.list(roots) }
}
```

OpenCode overrides `list_all` with the same query without the `parent_id IS NULL` filter. The
migration keeps `mukade list opencode` unchanged. The UI uses `list_all`, so forks appear
in the explorer and lineage without a second "scan forks" action. For Claude and Codex the result
is identical to `list`; the UI says lineage is not recorded.

### 7.3 Destination preparation

`hop` and `import` need a destination version (Codex refuses a rollout without one) and, for
OpenCode, a project id. The CLI computes both in private functions. Extract the OpenCode project
probe into a public helper used by both call sites:

```rust
// reader/opencode.rs
pub fn project_id_for(
    exe: &str,
    cwd: &str,
    roots: &StoreRoots,
    cache: Option<&Path>,
) -> Option<String>;
```

The UI resolves `dest_version` with the already-public `exe::version_of`, and `project_id` with
this helper. Model and provider are optional metadata: the UI exposes them as advanced fields and
never guesses them. This keeps one source of truth for the fragile probe and does not change the
CLI's behavior.

### 7.4 Interactive resume catalog entry

Added to `catalog/harnesses.json` and `catalog.rs` (ignored by the CLI):

```json
"interactive_resume": {
  "claude_code": ["{exe}", "--resume", "{id}"],
  "codex": ["{exe}", "resume", "{id}"],
  "opencode": ["{exe}", "mini", "--standalone", "--session", "{id}"],
  "pi": ["{exe}", "--session", "{file}"]
}
```

`{file}` is the native file recorded in `SessionInfo.path` (Pi's id lookup is project-scoped).
OpenCode gets `--standalone`; `StoreRoots::apply_to` supplies `OPENCODE_DB` and the XDG variables,
so the command cannot fall into another store or the user's background service. The catalog
carries the harness knowledge; `resume.rs` only expands placeholders and launches.

Known limits the UI states rather than hides:

- `SessionInfo` still has no model or message count. Those appear in the detail pane after the
  session is read; the list does not read every session.
- Codex and Claude Code do not record a cross-session parent; their lineage screen says so
  instead of drawing a guessed edge.
- Pi titles are absent until the session is read; the UI shows the id and, after opening, the
  first user message as a display title (not written back to the store).

## 8. Screens

1. **Explorer (default screen).** Harnesses and their sessions are visible together, in one
   window, with no intermediate screen. Two panes:
   - **Tree** (left): one root per harness with state dot, version, session count and fork count
     where recorded; its sessions are child rows under it. A harness expands and collapses; the
     children load in the background on first expand and stay cached until rescan. Child rows show
     archived state, title or id, updated when known, cwd and a fork marker. A filter box matches
     title, id, cwd and harness across the tree. Selecting a harness shows its diagnostics; selecting
     a session shows its detail.
   - **Detail** (right): for a harness, state, resolved binary, store, evidence, executable
     override and lineage support, with `r` rescan; for a session, the overview (harness, id, path,
     cwd, model(s) after load, parent, counts) with tabs `Overview | Transcript | Lineage` and the
     actions resume, hop, export.
2. **Harness diagnostics.** Not a separate screen: it is the right-hand detail of the selected
   harness node. A `Stores` toolbar action can focus the harness roots and executable overrides
   when setup or troubleshooting is needed.
3. **Transcript.** Conversation entries in native order: user, assistant, reasoning (labelled),
   tool call (name + arguments), tool result, compaction boundary and summary. Collapsible tool
   payloads, raw JSON toggle per entry, in-session search, virtualized list. Read-only.
4. **Lineage.** Tree built from `SessionInfo.parent` for the whole harness (available from
   `list_all`), drawn with the opened session focused. Only native links are drawn; a harness that
   records none says so instead of inventing an edge.
5. **Hop dialog.** Destination harness (source excluded), then options grouped:
   compaction `none | portable | summary`; for `summary`, keep-turns, model and timeout; resume
   prompt; reasoning `drop_foreign | as_text | drop`; keep or omit historical tools; cwd override;
   `all-branches` for Codex; advanced destination id/path. Validation mirrors the CLI before any
   write starts.
6. **Export.** Quick export writes `mukade-data/exports/<harness>-<id>.json`; "Save as…" uses the
   native file dialog. Success shows the path and offers "Open folder".
7. **Import.** Native file picker, destination harness, cwd, resume prompt, reasoning, tools.
   Reads the document with `wire::from_document`, writes with `import`, shows the receipt.
8. **Receipt and manifest.** Destination id and location; two distinct commands, the **headless**
   command returned by the receipt (what Mukade itself ran or would run with a prompt) and the
   **interactive** command from the catalog, each with its own copy action; every loss grouped by
   kind; "Resume now" (interactive) and "Open folder". A failed resume still shows the written
   session's receipt (`HopError::ResumeFailed`).
9. **Settings.** Per-harness root (environment, catalog default, custom path) and executable
   override; data directory shown read-only. Persisted in `mukade-data/ui.json`; a malformed file
   falls back to environment defaults and reports it.
10. **Empty and error states.** No store, no sessions, unreadable session, ambiguous id, refused
    action and recovery-needed action each have their own message and next action.

## 9. Behaviour

- Filtering is client-side and case-insensitive over metadata already loaded.
- Opening a session never writes and never resumes. Resume and hop are separate explicit actions.
- Resume runs from the session's cwd with the harness's store variable set; if no terminal
  launcher is available, the command is shown for copying.
- Long actions show progress. A hop or import is one library call that cannot be interrupted
  once started (a summary may run the source harness for minutes), so the dialog's Cancel is
  available only before start; after start the UI shows the phase and, once a write happens, the
  receipt is always shown. The headless resume command is not run by the UI: it is displayed for
  copying, while "Resume now" runs the interactive command.
- Shortcuts follow the mockup: `j/k` move in the tree, `left/right` collapse/expand a harness,
  `enter` open the selected session or harness, `/` filter, `h` hop, `d` transcript, `e` export,
  `i` import, `r` resume/rescan, `esc` back. Every action is also reachable by mouse and has a
  visible label.
- Theme: dark, compact, monospace metadata, green accent, consistent with the mockup and Mukade's
  brand. No custom title bar requirements in v1.

## 10. Packaging

- `cargo run -p mukade-ui` for development; `target/release/mukade-ui.exe` for release.
- First GPUI build is heavy; this is documented in the new crate's README.
- The workspace `.cargo/config.toml` forces `+crt-static` on Windows MSVC. The UI inherits it;
  if GPUI Kit or `rfd` fails to link statically, the documented UI build uses
  `RUSTFLAGS="-C target-feature=-crt-static"` and the UI is not claimed as a single-file binary.
  The CLI keeps its static CRT either way.
- GPUI Kit is Apache-2.0; its notice is added to the UI crate. No fonts are bundled in v1 beyond
  what GPUI Kit provides.

## 11. Verification

Automated:

- Unit: interactive resume argv per harness, filter predicate, hop/import option validation,
  export file naming, `ui.json` round-trip and malformed fallback.
- Library tests for the new behavior: Codex/Pi `SessionInfo` enrichment; `list_all` includes
  OpenCode children while `list` does not; `parent` values per harness; `project_id_for` unchanged
  against the CLI's behavior.
- Integration: list and open the fixture stores read-only; export a fixture session and re-read
  the document with `wire::from_document`; import/hop into a scratch store; assert the manifest
  is shown and the source fingerprint is unchanged.
- `cargo test` at the root stays green and does not build the UI.
- `cargo build -p mukade-ui` on Windows is the build gate.

Manual:

- Windows 11 smoke: scan, filter, open, transcript, export, import, hop with portable and summary
  options, receipt, resume into each installed harness's terminal, scaling and keyboard only.
- macOS/Linux are claimed by GPUI Kit but not verified in this milestone.

Not verified automatically: a real summary model call (credentials/quota) and a real interactive
resume in a terminal.

## 12. Risks

| Risk | Mitigation |
|---|---|
| GPUI Kit 0.8.1 is young; API churn | Pin the version; isolate components behind `views/` |
| GPU/RDP environments may fail to create a window | Report the GPUI error; CLI remains the fallback |
| First build time and binary size | Separate crate and default-members; document the cost |
| Terminal launch differs per OS | One `resume.rs` per-platform table; show the command when launch fails |
| Reading a large store blocks or floods memory | Background per-harness scan, lazy preview, bounded lists |
| Catalog defaults point at real user stores | Read-only, labelled as defaults, explicit override, no writes without action |
| `SessionInfo` enrichment changes `mukade list` output for Codex and Pi | Additive keys only; tests pin root-only OpenCode list and CLI JSON shape |
| `+crt-static` conflicts with GPUI Kit or `rfd` | Verify link early; documented UI-only `RUSTFLAGS` fallback, CLI unchanged |

## 13. My calls to check

- Separate `mukade-ui` crate in a workspace, `default-members = ["."]`.
- Resume opens the system terminal in v1; no PTY embedding.
- `interactive_resume` goes into the harness catalog, not a UI-only table.
- The library changes in §7 are part of this milestone: they are the minimum the explorer needs.
- The session list is metadata-only; model and counts load lazily, lineage comes from `SessionInfo`.
- Quick export writes to `mukade-data/exports/`; "Save as…" uses a native dialog.
- Settings live in `mukade-data/ui.json`.
- No delete/archive, no user-authored summary, no full-text global search in v1.

## 14. Approval questions

1. Is an external terminal acceptable for resume in v1, or must the UI host a PTY from the start?
2. Does "import" mean writing a document as a new session (Mukade's rule), or also merging into an
   existing one? The latter is not a library capability and would be a separate design.
3. Confirm the library changes in §7 (Codex/Pi metadata, `list_all`, project-id helper) are in
   scope, since they also touch `mukade list` output for two harnesses.
4. UI copy in English (like the repository) or Italian?
