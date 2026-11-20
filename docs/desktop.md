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
`--no-install` (stop at `release\`), `--target <folder>` (another install folder), `--ci` (`npm ci`
even when `node_modules` is there), `--jobs N`. Shinatama, Xiox and Muro have the same script.
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
| TypeScript: tree and collapse, search, folders, marks, keys, transcript rows into entries, chat ledger, receipt commands | `npm test` |
| Smoke: every view in each look (Smooth, Oni), dark and light, on the mocked commands, Enter on Cancel never hops, the chat's three views, join and send, a new room, search everything, the chat filter survives a poll; screenshots in `apps/desktop/tests/out/` | `npm run smoke` |

## The window

| view | what it shows |
|---|---|
| Installations | the six standard stores, then the ones a scan found (`scanned`); `error` when a store cannot be read (the reason is in the Folders pane and the tooltip) |
| Folders | All, every folder with its count, Unknown |
| Sessions | the tree: subagents (`↳ agent Explore`) and forks under their parent, newest first; `live`, `archived`, `unreadable`; a parent folds with space or its caret; the search box filters by title, id or folder and keeps a matching parent's children |
| Session | harness, id, kind, parent (`not listed`), children `3 (2 subagents, 1 fork)`, folder, updated, archived, live, path, store and engine for a scanned installation; the lineage; Open, Hop, Export, Save as…, Resume |
| Reader | the conversation in native order, opening at its end: user, assistant, reasoning, tool calls (input past 240 characters behind Show all), results, compactions; the last N turns or all; the Raw tab shows one entry's native records. Only the entries in sight are drawn. A live session is read again when its file changes (looked at every 3 s); the reader follows the end only when it was there |
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
| F5, ctrl+R | read the stores again (as `r`) | a page reload would drop the window's state mid-hop |
| ctrl+P, ctrl+S, ctrl+O, ctrl+U, ctrl+J, and devtools in a release build | do nothing | page commands a desktop window does not have; zoom (ctrl +/−) stays |
| Context menu | the WebView's only where text is edited or selected | the page menu (Back, Reload, Print) has no meaning here |
| Overscroll | no rubber-band on the window; each pane keeps its own scroll | `overscroll-behavior` |
| Selection | the chrome is not selectable; the reader, the details, code and fields are | `user-select` |
| A `.json` dropped on the window | opens Import with it | the WebView's drop event |
| Startup | no white flash | Imago builds the transparent frame with the saved theme before showing it |

Still to try on Windows and macOS: the drop, the close confirmation and the terminal opened by
Resume.

## Keys

| key | does |
|---|---|
| `↑` `↓`, `j` `k` | move in the focused list, scroll the reader |
| `←` `→` | previous / next column |
| `⏎` | next column, or open the transcript |
| `esc` | close a dialog or the reader, or back one column |
| `h` · `e` · `i` · `R` | hop · export · import · resume |
| `t` | last N turns ↔ all |
| `g` `G`, PgUp PgDn | reader top / bottom, a page |
| `o` · `r` | scan a folder · reload |
| `/`, `ctrl+f` | filter the list (in the chat view, the focused column's) |
| `ctrl+k` | search rooms, members, messages and sessions |
| `ctrl+shift+k` | command palette |
| `F6` / `shift+F6` | next / previous region |
| `c` | chat ↔ sessions |
| `m` | harnesses ↔ sessions |
| `space` | fold the selected parent; in the chat, play the room back |
| `1` `2` `3` | chat: graph · lanes · feed |
| `n` · `l` | chat: new room · launch an agent |
| `ctrl+⏎` | chat: send |
| `?` | the keys |

Keys do nothing while a text field has the focus, except `esc` and `⏎`.

## Where it differs from the specification

- Installations and Folders share the first column (the spec draws four); the reader hides it to
  take the room. Widths are dragged and kept per machine; a double-click resets one.
- `tab` keeps moving the focus between controls, as every window does, instead of between columns
  (`←` `→` do that): taking it away leaves the buttons out of reach without a mouse.
- The plan in the hop dialog is drawn from the chosen options; the library has no dry run that
  states what a write would lose, so the losses are the receipt's.
- `folders` is computed in the window from the listing rather than by a command of its own.
- Installations are keyed `<harness>` (standard) or `<harness>@<store>` (scanned). The window keeps
  the folders scanned so far and passes them on every reload; each listing runs one at a time.
