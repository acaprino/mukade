# mukade desktop (Tauri) — specification

Status: decided 2026-11-12. User: replace the TUI with a Tauri app "come Shinatama", no framework.
The TUI (`mukade-tui`) is removed once the app covers every row of §3 and its tests pass.
Reference app: `D:\Projects\shinatama\apps\desktop` (read only).

## 1. Stack

| part | choice |
|---|---|
| shell | Tauri 2 (`tauri = "2.9"`, `tauri-build = "2.5"`, `tauri-plugin-dialog = "=2.8.4"`), crate `apps/desktop/src-tauri`, package `mukade-desktop`, a workspace member |
| frontend | plain TypeScript, **no framework**; Vite `8.3.5`, TypeScript `7.0.2`, `@tauri-apps/api` `2.12.4`, `@tauri-apps/cli` `2.12.4`; one `<component>.ts` + `<component>.css` per view, as Shinatama |
| files | `apps/desktop/{index.html, vite.config.ts, tsconfig.json, src/, src-tauri/, tests/}`; root `package.json` scripts `dev`, `build`, `desktop`, `desktop:build`, `test` |
| backend | Tauri commands calling the `mukade` library only (never the CLI, never a store by hand); every per-harness answer from `harnesses::get(..)` (the architecture test scans `apps/desktop/src-tauri/src`) |
| look | Shinatama's `styles.css` tokens (colors, spacing, type, radii); light and dark follow the system, with a toggle |

## 2. Window layout

```
┌ toolbar: [Scan…] [Reload] [Import…] [Chat]   search ▢   theme ◐ ┐
├ Installations ┬ Folders ┬ Sessions (tree)          ┬ Detail      ┤
│ Claude Code   │ All (42)│ ▸ Fix login   12:03      │ meta        │
│ Codex       ! │ C:\work │   ↳ agent Explore        │ lineage     │
│ OpenCode·Janu │ Unknown │   ⑂ fork                 │ [Open][Hop] │
│ …             │         │                          │ [Export][R] │
├───────────────┴─────────┴──────────────────────────┴─────────────┤
│ status line: job in progress · last error · counts               │
└──────────────────────────────────────────────────────────────────┘
```

- Four resizable columns (CSS grid, widths kept in `localStorage`). Below 1000 px wide the Detail
  column becomes a drawer; below 700 px one column at a time with a back button.
- The transcript opens in the Detail column as a full-height reader (§4.3); Hop, Import, Scan are
  modal dialogs (Shinatama `modal.ts` pattern); Chat is a separate view that replaces the columns
  (toolbar toggles back).

## 3. Features (parity with the TUI, plus chat)

| # | feature | TUI today | desktop |
|---|---|---|---|
| F1 | installations: six standard + scanned ones, error mark `!` | ✅ | ✅ same rows, error in Detail |
| F2 | folder filter: per-cwd counts, All, Unknown | ✅ | ✅ |
| F3 | session tree: children and forks under their parent, marks `↳ agent <type>`, `⑂ fork`, `↳`, `[live]`, `[archived]`, `[unreadable]` | ✅ | ✅ + collapse/expand per parent |
| F4 | detail: harness, id, parent (`not listed`), kind, children `3 (2 subagents, 1 fork)`, cwd, updated, archived, live, unreadable, path, store/engine for scanned | ✅ | ✅ |
| F5 | transcript: native order, user/assistant/reasoning/tool calls (input cut at 240 chars, expandable)/results/compactions, Meta hidden; last N turns ↔ all | ✅ | ✅ + raw records tab |
| F6 | hop: destination, compaction (none/portable/summary), subagents carry/leave, prompt; result: new id, location, headless + interactive resume, manifest path, losses, child lines, reply | ✅ | ✅ + all-branches (where the source's adapter supports it), reasoning policy, dest shell (default = destination's), plan preview before run |
| F7 | export → `mukade-data/exports/<harness>-<id>.json` | ✅ | ✅ + "Save as…" native dialog |
| F8 | import: document, into, prompt | ✅ | ✅ document picked with the native dialog |
| F9 | resume | runs in the same terminal | opens a new terminal window running the interactive resume (Windows: `wt` if present, else `cmd /c start`; macOS `open -a Terminal`; Linux `$TERMINAL`/`x-terminal-emulator`) in the session's cwd with the store env; `resume_refusal` shown instead when the adapter refuses; DeepSeek shows the headless command with a copy button |
| F10 | scan a folder for installations (≤8 levels, ≤100,000 folders; "stopped early" note) | ✅ `o` | ✅ native folder picker |
| F11 | reload all stores | ✅ `r` | ✅ |
| F12 | search | — | filter the session tree by title/id/cwd (children kept with a matching parent) |
| F13 | chat view | — | §5 |
| F14 | CLI args: `--turns N`, `--scan <dir>` (repeatable) | ✅ | ✅ same; `--check`/`--snapshot` dropped (the CLI `mukade list`/`harnesses` cover scripting) |

## 4. Interactions

### 4.1 Keys (TUI keys kept; mouse everywhere)

| key | does |
|---|---|
| `↑`/`↓`, `j`/`k` | move in the focused list / scroll the reader |
| `←`/`→`, `tab`/`shift+tab` | previous / next column |
| `enter` | next column, or open the transcript |
| `esc` | close reader/dialog, or back one column |
| `h` · `e` · `i` · `R` | hop · export · import · resume the selected session |
| `t` | last N turns ↔ all |
| `g`/`G`, PgUp/PgDn | reader top/bottom, ±10 lines |
| `o` · `r` | scan a folder · reload |
| `/` or `ctrl+f` | focus search |
| `c` | chat view ↔ sessions |
| `space` | expand/collapse the selected parent; toggle a checkbox in a dialog |

Keys do nothing while focus is in a text field (except `esc`, `enter`).

### 4.2 Background work

Every listing, read, scan, export, import and hop runs on a Tauri async command
(`tauri::async_runtime::spawn_blocking` for library calls). The status line shows the running job;
listings cancel and restart on reload; hop and import cannot be cancelled (as in the TUI) and the
window refuses to close while one runs (confirm dialog).

### 4.3 Reader
Virtualized list of entries (render only what is visible; sessions can hold 10,000+ entries). Each
entry: role badge, time, text (markdown not rendered — plain, wrapped, monospace for tool
input/output), collapsible tool input/results. Tabs: Conversation · Raw (the entry's native records,
pretty JSON).

## 5. Chat view (read-only)

Through the running chat service only; **never `Bus::open`** (it creates and migrates the store, and
the service exclusively owns its file).

| pane | content | source |
|---|---|---|
| status bar | running / stopped / unreachable, endpoint; when not running: "start it with `mukade chat start`" (never started from here) | `chat::lifecycle::Paths::from_env()`, `owner_active`, `status` |
| Rooms | name, open/closed, members, messages, last activity | `ChatClient::call("rooms", {"limit":500})` (follow `next_cursor`) |
| Members | name, harness, instance, role, bound session id → "Show session" jumps to it in the session tree (when listed) | `call("members", {"room_id"})` |
| Messages | time, actor (name, role), text, recipients, mentions; `observe` events as a muted line ("A observed B: n items"); newest last, auto-scroll unless the user scrolled up; text filter | `call("ledger", {"room_id","limit":500})`, paged by cursor |

Client: `ChatClient::connect(&lifecycle::client_config(&paths)?)` once per poll cycle (or kept while
the view is open); poll every 3 s while the view is visible; each call times out after 5 s →
"unreachable". A missing call needed here is asked of the chat session, never worked around.

## 6. Backend commands (Rust ↔ TS contract)

All return `Result<T, CmdError>`; `CmdError { kind: "refused"|"not_found"|"failed"|"unreachable", message, exit_code? }`.
Types are `serde` structs mirrored by hand in `src/types.ts` (one home; a test compares the JSON of
each Rust type with a TS fixture).

| command | args | returns |
|---|---|---|
| `installations` | `{ scan: string[] }` | `Installation[]` `{ key, harness, label, store?, engine?, scanned, error? }` |
| `sessions` | `{ installation }` | `SessionRow[]` `{ id, title?, cwd?, updated?, archived, live?, unreadable?, path, parent?, parent_kind? }` |
| `folders` | `{ installation }` | `{ cwd?, label, count }[]` |
| `transcript` | `{ installation, id, turns }` | `Entry[]` (view model: role, time, text, calls, results, compaction) |
| `raw` | `{ installation, id, entry_id }` | `Value[]` |
| `hop_options` | `{ source, id }` | destinations, compaction modes available, `has_children`, `all_branches_supported`, default dest shell per destination |
| `hop` | `{ source, id, dest, compaction, children, all_branches, reasoning?, dest_shell?, prompt? }` | `Receipt` (new id, location, resume commands, manifest path, losses, child lines, reply) |
| `export` | `{ installation, id, to? }` | `{ path, messages }` |
| `import` | `{ document, into, prompt? }` | `Receipt` |
| `resume` | `{ installation, id }` | `{ launched: bool, refusal?, command: string[] }` |
| `scan` | `{ dir }` | `{ added: Installation[], stopped_early: bool }` |
| `chat_status` | — | `{ state, endpoint? }` |
| `chat_rooms` / `chat_members` / `chat_ledger` | `{ cursor? }` / `{ room_id }` / `{ room_id, cursor?, text? }` | the service's JSON, passed through typed |

## 7. Tests

- Rust (`cargo test -p mukade-desktop`): every command's pure part on fixture stores in scratch
  (`tests/support` pattern), tree building, receipt view model, chat reply → rows with canned JSON,
  stopped/unreachable states.
- TS (`node --experimental-strip-types --test apps/desktop/tests/*.test.ts`): tree/flatten with
  collapse, search filter keeps a matching parent's children, key map, receipt rendering.
- Smoke: `npm run build` + `cargo build -p mukade-desktop` + `tauri build --debug --no-bundle`;
  Playwright on `npm run dev` with mocked `invoke` (Shinatama `tests/*-smoke.mjs` pattern) for one
  screenshot per view (sessions, reader, hop dialog, receipt, chat) saved under `apps/desktop/tests/out/`.
- `tests/architecture.rs` scans `apps/desktop/src-tauri/src`.

## 8. Removal of the TUI (last step)
Delete `mukade-tui/`, drop it from the workspace; `docs/desktop.md` replaces `mukade-tui/README.md`
(every row of §3, keys, run/build); README, docs/README.md, usage, architecture, harness pages,
next.md, windows checklist name the desktop app.

## 9. Out of scope (next.md)
Chat writes (send, join, open/close rooms); live tailing of a running session; installers and
signing (`--no-bundle` only); auto-update.
