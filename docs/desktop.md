# mukade desktop

The Tauri 2 window that browses every installation's sessions, reads them, hops them to another
harness, exports and imports interchange documents, resumes them in a terminal, and reads the chat
rooms. Specification: [`superpowers/specs/2026-11-12-desktop-design.md`](superpowers/specs/2026-11-12-desktop-design.md).

## Run and build

```bash
npm ci
```

```bash
npm run desktop
```

`npm run desktop` opens the window with live reload. `npm run desktop:build` builds a debug binary
(`target/debug/mukade-desktop`, no installer). `npm run dev` serves the same page in a browser with
made-up sessions (`apps/desktop/src/mock.ts`): every view can be seen without stores.

Command line: `--turns N` (the turns a transcript opens with; `0` all; default 50) and
`--scan <dir>` (any number of times: folders scanned for installations at start).

On Linux the build needs WebKitGTK 4.1 and GTK 3 development packages (`libwebkit2gtk-4.1-dev`,
`libgtk-3-dev`).

## Tests

| what | command |
|---|---|
| Rust: commands on fixture stores (list, read, raw, export, a real hop into every writable harness), view models, chat replies, terminal env | `cargo test -p mukade-desktop` |
| No harness branch outside its adapter (the desktop crate included) | `cargo test --test architecture` |
| TypeScript: tree and collapse, search, folders, marks, keys, chat ledger, receipt commands | `npm test` |
| Smoke: every view in dark and light on the mocked commands, screenshots in `apps/desktop/tests/out/` | `npm run smoke` |

## The window

| view | what it shows |
|---|---|
| Installations | the six standard stores, then the ones a scan found (`scanned`); `error` when a store cannot be read (the reason is in the Folders pane and the tooltip) |
| Folders | All, every folder with its count, Unknown |
| Sessions | the tree: subagents (`↳ agent Explore`) and forks under their parent, newest first; `live`, `archived`, `unreadable`; a parent folds with space or its caret; the search box filters by title, id or folder and keeps a matching parent's children |
| Session | harness, id, kind, parent (`not listed`), children `3 (2 subagents, 1 fork)`, folder, updated, archived, live, path, store and engine for a scanned installation; the lineage; Open, Hop, Export, Save as…, Resume |
| Reader | the conversation in native order, opening at its end: user, assistant, reasoning, tool calls (input past 240 characters behind Show all), results, compactions; the last N turns or all; the Raw tab shows one entry's native records. Only the entries in sight are drawn |
| Hop | destination, compaction (none, portable, summary), subagents carry or leave, all branches (where the source keeps them), reasoning, destination shell (default: the destination's own), prompt, and the plan before anything is written |
| Receipt | new id, location, manifest path (saved under `mukade-data/manifests/`), interactive and headless resume with Copy, subagents written and whether each is linked, losses, the destination's reply; Show in sessions, Resume in terminal |
| Import | a document (typed or chosen), the installation, an optional prompt; ends in the receipt |
| Chat | read only, through the running chat service, polled every 3 s: rooms, the messages of one (filter, newest last, follows the end unless scrolled up), its members with Show session. When the service is not running it says so and how to start it (`mukade chat start`); it is never started from here |

Resume opens a new terminal window in the session's folder with the store's variables: Windows
Terminal when installed and no variable is needed, else a new console window; Terminal.app on
macOS; `$TERMINAL` or `x-terminal-emulator` on Linux. A harness with no terminal UI (DeepSeek
Harness) shows its headless command with a copy button instead.

The window refuses to close while a hop or import runs, and asks first.

## Look and desktop behaviour

The skin is Mukade's palette on Shinatama's structure, including its glass: the bars and the
columns are translucent over a still ambient field, dialogs and their scrim blur what is under
them, and a dialog opens by drawing its top edge. As in Shinatama, this is CSS. Shinatama's
acrylic window effect sits under an opaque stage, so it is almost never seen; Mukade keeps the
operating system's window frame instead (snap layouts, the system menu, double-click to
maximize), following `.claude/skills/tauri-native-desktop`.

From that skill's audit:

| surface | state | how |
|---|---|---|
| Open, save, folder | OS dialogs | `tauri-plugin-dialog`, from the backend |
| Hop, import, receipt, export, keys, close confirmation | surfaces inside the main window | they belong to it; long content scrolls and the actions stay in reach at the minimum window size |
| Window close | refused while a hop or import runs, with a confirmation | `CloseRequested` in `main.rs`; covers the X button and Alt+F4 |
| F5, ctrl+R | read the stores again (as `r`) | a page reload would drop the window's state mid-hop |
| ctrl+P, ctrl+S, ctrl+O, ctrl+U, ctrl+J, and devtools in a release build | do nothing | page commands a desktop window does not have; zoom (ctrl +/−) stays |
| Context menu | the WebView's only where text is edited or selected | the page menu (Back, Reload, Print) has no meaning here |
| Overscroll | no rubber-band on the window; each pane keeps its own scroll | `overscroll-behavior` |
| Selection | the chrome is not selectable; the reader, the details, code and fields are | `user-select` |
| A `.json` dropped on the window | opens Import with it | the WebView's drop event |
| Startup | no white flash | the window's `backgroundColor` is the dark stage |

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
| `/`, `ctrl+f` | search (in the chat view, its message filter) |
| `c` | chat ↔ sessions |
| `space` | fold the selected parent |
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
