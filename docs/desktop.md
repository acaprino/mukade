# mukade desktop

The Tauri 2 window that browses every installation's sessions, reads them, hops them to another
harness, exports and imports interchange documents, resumes them in a terminal, and runs the chat
rooms. Specification: [`superpowers/specs/2026-11-12-desktop-design.md`](superpowers/specs/2026-11-12-desktop-design.md).

## Run and build

**Download (Windows x64):** `Mukade-<version>-windows-x64.zip` from the
[releases](https://github.com/acaprino/mukade/releases) holds `mukade-ui.exe` and `mukade.exe`. Extract it
to a plain local folder outside any git checkout and OneDrive, then run `mukade-ui.exe`; its data
goes to `mukade-data` beside it. The programs are not signed: SmartScreen may ask once.

**From source:**

```bash
npm ci
```

```bash
npm run desktop
```

`npm run desktop` opens the window with live reload. `npm run desktop:build` builds a debug binary
(`target/debug/mukade-ui`, no installer). `npm run dev` serves the same page in a browser with
made-up sessions (`apps/desktop/src/mock.ts`): every view can be seen without stores.

The portable package: `build-release.bat` (Windows) builds the release command line and the app
with the UI embedded into `release\` (made again on every build), installs it
into `D:\Software\Mukade` and removes what the build staged (cargo's `target\` stays as the build
cache). There `mukade-ui.exe` (the app) and `mukade.exe` sit side by side, sharing
`mukade-data` beside them (exports, manifests, the chat service). An install copies over the
programs and leaves `mukade-data` alone; it waits while a program runs from that folder. Options:
`--no-install` (stop at `release\`), `--target <folder>` (another install folder), `--jobs N`. Both release scripts run `npm ci` before building. Windows also accepts
`--ci` for compatibility. Shinatama, Xiox and Muro have the same script.
`build-release.sh` builds `release\` only. Chat refuses a data folder inside the clone: use the
installed copy, or set `MUKADE_DATA_DIR` for every new process
([how](usage.md#local-chat-between-harness-instances)). The app needs the WebView2 runtime (part of
Windows 11).

Command line: `--turns N` (the turns a transcript opens with; `0` all; default 50) and
`--scan <dir>` (any number of times: folders scanned for installations at start).

On Linux the build needs WebKitGTK 4.1 and GTK 3 development packages (`libwebkit2gtk-4.1-dev`,
`libgtk-3-dev`).

## Tests

| what | command |
|---|---|
| Rust: listing, transcript and raw pages on fixture stores, export, a real hop into every writable harness, the receipt, chat replies, terminal commands | `cargo test -p mukade-desktop` |
| No harness branch outside its adapter (the desktop crate included) | `cargo test --test architecture` |
| TypeScript: tree and collapse, search, folders, marks, keys, transcript rows into entries, chat ledger, receipt commands, the lines of Work on disk | `npm test` |
| Smoke: every view in each look (Smooth, Oni), dark and light, on the mocked commands, Enter on Cancel never hops, the chat's three views, join and send, a new room, search everything, the chat filter survives a poll; screenshots in `apps/desktop/tests/out/` | `npm run smoke` |

## The window

| view | what it shows |
|---|---|
| Installations | the six standard stores, then the ones a scan found (`scanned`); `error` when a store cannot be read (the reason is in the Folders pane and the tooltip) |
| Folders | All, every folder with its count, Unknown |
| Sessions | the tree: subagents (`↳ agent Explore`) and forks under their parent, newest first; `live`, `archived`, `unreadable`, and `hosted` for a chat ChatGPT hosts (below); a parent folds with space or its caret; the search box filters by title, id or folder and keeps a matching parent's children |
| Session | harness, id, kind, parent (`not listed`), children `3 (2 subagents, 1 fork)`, folder, updated, archived, live, path, store and engine for a scanned installation; the lineage; Open, Hop, Export, Save as…, Resume. A session with a folder also has [Work on disk](#work-on-disk-and-start-in), and Start in… when it cannot be read |
| Reader | beside the sessions list, the conversation in native order, opening at its end: user, assistant, reasoning, tool calls (input past 240 characters behind Show all), results, compactions; the last N turns or all; the Raw tab shows one entry's native records. Only the entries in sight are drawn. A live session is read again when its file changes (looked at every 3 s); the reader follows the end only when it was there |
| Hop | destination, compaction (none, portable, summary with its intent, model and turns kept whole, native with the destination model's context window), subagents carry or leave, all branches (where the source keeps them), reasoning, destination shell (default: the destination's own), prompt, and under More options the destination's model and provider, unfinished turns keep or drop, and without tools; the plan before anything is written |
| Receipt | new id, location, manifest path (saved under `mukade-data/manifests/`), interactive and headless resume with Copy, subagents written and whether each is linked, losses, the destination's reply; Show in sessions, Resume in terminal |
| Import | a document (typed or chosen), the installation, an optional prompt; ends in the receipt |
| Chat | through the running chat service, polled every 3 s ([design](superpowers/specs/2026-11-16-desktop-chat-redesign-design.md), layer A). Rooms with a filter, New room, Close, Reopen, Delete (the service's summary first). The room in three views of the same events with one selected event: Graph (a plate per member in join order around everyone, a line per pair and kind, wider with each event, the selected one with an arrow; a strip to pick, step and play the room back), Lanes (a column per member, a wire per event), Feed (recipients, the state of each instruction, a signal's instruction). One filter for the three: text, kinds, members. Members with role and state (`left`, or the last command: nothing is invented), their details, Mention, Show session; the instructions leads gave workers, waiting, blocked or done. To write, the window joins as a member of its own, peer or lead, marked as the host's: what it sends reaches the agents as the user's. Its handle is kept per room across restarts, recovered with Rejoin when the service calls it stale; the draft is kept per room. Start and Stop the service; Launch agent runs `mukade chat launch` in a new terminal, the command shown first |
| Search | `ctrl+K` or Search: rooms and members by name, messages by text (the 20 most recent open rooms first), sessions of every installation by title, id or folder; a result opens its room, member, event or session |
| Harnesses | the third tab (`m`): each harness with the binary the window runs and where it was found (a variable, `PATH`, the catalog's location), its version, its standard store and session count, and its other installations. Scan this computer reads your home, then every top-level folder of every drive (the system's own left out), one at a time through `mukade::detect` (8 levels, 100,000 folders each at most), with progress and Stop; what it finds outside the standard stores is kept. Scan folder is kept too; Forget drops a folder. Install runs the harness's own installer in a terminal (Hermes's script is not verified, so it has none), or installs it with npm into a sandbox: `mukade-data/sandboxes/<harness>-<name>` holds the package (`app`), a store of its own (`home`) and a launcher that sets the store's variables (OpenCode: the four XDG folders). A sandbox is recorded in `mukade-data/sandboxes.json`, listed as an installation (hops can land in it), opened in a terminal with Run, and Forget unlists it without deleting its folder |
| In rooms | under a session's details, the rooms where a member is bound to it; each opens the room at that member |

Resume opens a new terminal window in the session's folder with the store's variables: Windows
Terminal when installed and no variable is needed, else a new console window; Terminal.app on
macOS; `$TERMINAL` or `x-terminal-emulator` on Linux. A harness with no terminal UI (DeepSeek
Harness) shows its headless command with a copy button instead.

The window refuses to close while a hop or import runs, and asks first.

## Work on disk and Start in…

A session can have a folder and no readable conversation: a thread Codex still has a row for whose rollout
is gone, or a chat the Codex desktop app hosts on ChatGPT whose project has a local folder. The window
shows the work such a session left and starts another harness on it.
Decision: [ADR-0005](adr/ADR-0005-hosted-chats-read-locally.md) (Proposed). Codex side:
[Chats hosted by ChatGPT](harnesses/codex.md#chats-hosted-by-chatgpt).

**The chip `hosted`.** A session whose reason starts with `hosted by ChatGPT` shows a neutral `hosted`
chip in the list, with the reason as its tooltip, in place of the red `unreadable`. The detail panel shows a
neutral note, `Hosted by ChatGPT: the conversation is not on this machine; the work it left is in its
folder`, where another unreadable session shows the red `Cannot be read: …`. Open transcript, Hop, Export, Save as… and Resume stay disabled, as for any
session that cannot be read. Its conversation is not read, and the text is not in this version at all.

**Work on disk.** Any session with a folder has a "Work on disk" section in its detail panel: the folder's
path and one row for each repository in it (the folder itself, the repository that holds it when it is a
subfolder, or else each direct child that is a git repository, the first 20 with a note when there are
more). It is the same read as [`mukade work`](usage.md#mukade-work) and runs the same git commands,
none of which changes a repository or fetches. A row is `<branch> · <upstream> · <state>`, with `no
upstream` when there is none and a state of `clean`, `N commits on no remote`, `N ahead` or
`N uncommitted`. A repository is marked `at risk` when it has uncommitted paths or commits on no remote:
that is the work that exists only in this folder. Other worktrees and branches that hold such work are
listed under it, a repository git refuses shows what git said, and a worktree git cannot read shows what
git said as a note row. What it says about a remote is as of
the last fetch.

- The read starts 200 ms after a row is selected and is dropped when the selection has moved on, so
  moving down the list does not run git on every row.
- The result is kept for 60 seconds for each folder, for at most 20 folders (the oldest goes first).
  Sessions that share a folder read it once, and a folder kept is drawn at once with no `Reading…` and no
  call. A commit made meanwhile shows after 60 seconds, when you select another row and come back.

**Start in….** For a session that cannot be read and has a folder, `Start in…` (key `s`) is the primary
button. It opens a dialog with the destination (every installation of Claude Code and Codex CLI the
window knows, the session's own included) and the note, composed from the same template as
[`mukade handoff`](usage.md#mukade-handoff) (`handoff-prompts/continue.md` beside the executable) and
editable before anything happens. A session with no folder is not offered `Start in…` at all.

`Start` (ctrl+⏎) saves the note you edited as `mukade-data/handoffs/<harness>-<id>.md`, never in the
project folder, and opens the destination's command in a new terminal in the session's folder, with the
store variables of the chosen installation, the same way Resume opens one. The command carries one first
prompt, `Read <path> and continue from it.`, that points at the note. The status line then reads
`Started <label> in <folder>; note saved to <path>`. A refusal or a failure (the folder is gone, no data
folder, the terminal did not start) stays in the dialog and Start can be tried again.

Not verified: nothing of this was run on a Windows build, and that the one-line prompt reaches the harness
whole through Windows Terminal is not seen yet. The owner's live check settles both.

## Look and desktop behaviour

Mukade uses the same Imago release for its web UI and native Tauri shell. Settings keeps the
existing Smooth and ONI preferences. Smooth follows the system by default, and ONI starts dark.
The title bar contains the views, command palette and window controls. Sessions has a collapsible
installations sidebar and a session inspector. Below 1000 px the inspector opens over the list,
below 850 px the sidebar becomes a rail, and below 620 px the title bar opens the sidebar and Views menu.
Windows uses the saved theme for the first frame, restores the last window position, and supports
Snap Layouts on the maximize button. The browser preview uses opaque surfaces.

| surface | state | how |
|---|---|---|
| Open, save, folder | OS dialogs | `tauri-plugin-dialog`, from the backend |
| Hop, import, receipt, export, keys, close confirmation | surfaces inside the main window | they belong to it; long content scrolls and the actions stay in reach at the minimum window size |
| Window close | refused while a hop or import runs, with a confirmation | `CloseRequested` in `main.rs`; covers the title bar's close button and Alt+F4 |
| F5, ctrl+R | do nothing (the guard refuses the browser's reload) | a page reload would drop the window's state mid-hop; `r` reads the stores again |
| ctrl+P, ctrl+S, ctrl+O, ctrl+U, ctrl+J, and devtools in a release build | do nothing | page commands a desktop window does not have; zoom (ctrl +/−) stays |
| Context menu | the WebView's only where text is edited or selected | the page menu (Back, Reload, Print) has no meaning here |
| Overscroll | no rubber-band on the window; each pane keeps its own scroll | `overscroll-behavior` |
| Selection | the chrome is not selectable; the reader, the details, code and fields are | `user-select` |
| A `.json` dropped on the window | opens Import with it | the WebView's drop event |
| Startup | no white flash | Imago builds the transparent frame with the saved theme before showing it |

Still to try on Windows and macOS: the drop, the close confirmation and the terminal opened by
Resume.

The desktop verification workflow exercises the embedded frontend on macOS and Linux using the
production custom protocol. A test-only probe waits for the first view and real startup IPC before
exiting. The frontend job runs the bundle with simulated IPC for failure and keyboard scenarios.
Run `npm ci` and `npx playwright install chromium` before running the browser tests locally.

Hop and Import retain the active operation and its receipt in the native process until the result is
dismissed. If the WebView reloads, the replacement page reconnects to that operation. Closing the
whole application ends the lifetime of this in-memory record.

## Keys

Every key of the window is a command of one registry (`src/keys.ts` is the table, `main.ts` registers it, `mountApp` of Imago
attaches it). The key caps on the buttons, the footer's key list, the Keys dialog (`?`) and the palette all come from it.

| key | command |
|---|---|
| `ctrl+k` | the palette (every command by name, and the look and the theme: Smooth, ONI, follow Windows, light, dark) |
| `ctrl+f` | filter the view: the sessions filter; in the chat the rooms filter (from the sidebar) or the messages filter; on Harnesses it opens Search everything |
| `ctrl+shift+f` | search rooms, members, messages and sessions |
| `c` · `m` | chat ↔ sessions · harnesses ↔ sessions |
| `o` · `i` · `r` | scan a folder · import · reload the stores |
| `h` · `e` · `shift+R` · `s` | with a session selected: hop · export · resume · start another harness in its folder (Start in…) |
| `t` · `g` · `shift+G` | with the reader open: last N turns ↔ all · top · bottom |
| `1` `2` `3` · `n` · `l` | in the chat: graph, lanes, feed · new room · launch an agent (in an open room) |
| `esc` | the top layer; else leave a field, back in the chat, close the reader (Raw, then Conversation, then the list), one column |
| `F6` / `shift+F6` | next / previous region (sidebar, content, inspector) |
| `?` | the keys |

Commands with no key, in the palette and on their buttons: Sessions, Open transcript, Export as…, Play the room back, Start the
chat service, Stop the chat service.

The focused list or the reader owns these, not the registry:

| key | does |
|---|---|
| `↑` `↓`, `j` `k` | move in the focused list; scroll the reader; in the chat, the event or member |
| `Home` `End`, `PgUp` `PgDn` | first, last, ten rows, in a list; a page in the reader |
| `⏎` | open the transcript of the selected session; on a sidebar row, on to the sessions |
| `space` | fold the selected parent; in the chat, play the room back |
| `ctrl+⏎` | chat: send; in a dialog: the dialog's own button |

Single letters do nothing while a text field has the focus, and no command runs while a dialog or the palette is open.

Dropped with this move, and what replaces them: `/` (use `ctrl+f`); `F5` and `ctrl+R` (the browser's reload, which the desktop
guard refuses: `r` reads the stores again); `←` `→` between columns (`F6`, `shift+F6` and `Tab`); `ctrl+shift+k` for the palette
(it is `ctrl+k`, which was Search everything: that is now `ctrl+shift+f`); the Theme and Settings buttons and the Settings dialog
(look and theme are five commands of the palette, the same in every Imago app).

## Where it differs from the specification

- The window is Imago's one frame (`src/frame.ts` writes it): title bar with the mark and the view tabs, status row (last result, summary,
  the version at its end), sidebar (Installations and Folders share it), content, inspector, action bar, footer. The Sessions view holds the
  list and, while a transcript is open, the reader beside it (`splitView`); the inspector docked from 1000 up holds the selected session's
  detail. Under 1000 the inspector is a drawer (its header has Close details, and Escape closes it) and the reader takes the list's place
  until Escape. Widths are dragged and kept per machine (`mukade.width.installations`, `mukade.width.sessions`); a double-click on a
  separator forgets its width. In the docked inspector Escape moves the focus back to the list or the reader and does not close it.
- `tab` keeps moving the focus between controls, as every window does; `F6` and `shift+F6` move between the regions.
- The plan in the hop dialog is drawn from the chosen options; the library has no dry run that
  states what a write would lose, so the losses are the receipt's.
- `folders` is computed in the window from the listing rather than by a command of its own.
- Installations are keyed `<harness>` (standard) or `<harness>@<store>` (scanned). The window keeps
  the folders scanned so far and passes them on every reload; each listing runs one at a time.
