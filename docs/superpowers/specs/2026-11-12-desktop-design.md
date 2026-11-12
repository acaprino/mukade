# mukade desktop (Tauri) — design

Status: decided 2026-11-12. User: replace the TUI with a Tauri app, "come Shinatama" (built the way
`D:\Projects\shinatama\apps\desktop` is). The TUI (`mukade-tui`) is removed once the app covers it.

## Stack (Shinatama's, same versions)

| part | choice |
|---|---|
| shell | Tauri 2 (`tauri = "2.9"`, `tauri-build = "2.5"`, `tauri-plugin-dialog = "=2.8.4"`), crate `apps/desktop/src-tauri` (`mukade-desktop`), a workspace member |
| frontend | plain TypeScript (no framework), Vite `8.3.5`, TypeScript `7.0.2`, `@tauri-apps/api` `2.12.4`, `@tauri-apps/cli` `2.12.4`; one `.ts` + one `.css` per component, as Shinatama's `apps/desktop/src` |
| layout | `apps/desktop/{index.html, vite.config.ts, src/, src-tauri/}`, root `package.json` scripts `dev`, `build`, `desktop`, `desktop:build`, `test` |
| backend | Tauri commands that call the `mukade` library directly (never the CLI, never a harness store by hand) |
| tests | Rust: unit tests of every command's pure part; frontend: `node --test` on pure TS (row models, tree building); one smoke run that builds the app (`tauri build --debug --no-bundle`) |

Visual language: take Shinatama's `styles.css` tokens (colors, spacing, type) as the base so the two
apps look alike; light and dark.

## What it covers (everything the TUI does, plus)

| area | content | library calls |
|---|---|---|
| Installations | standard locations + scanned ones, per harness (six) | `catalog`, `detect::scan_installations`, `roots` |
| Sessions | list per installation, filter/search, children and forks nested under their parent (spawn `↳ agent`, fork `⑂`), archived toggle | `reader::for_harness(..).list_all`, `SessionInfo.parent`/`parent_kind` |
| Session detail | meta, parent, kind, children count, the conversation (user/assistant/tool text), raw view | `hop::export` / readers |
| Hop | pick destination installation, options (`--children`, `--all-branches` when the source supports it, compaction, reasoning, dest shell), plan preview, run, receipt (child lines, losses), resume command | `hop::HopRequest`, `hop::hop_into`, `Destination` answers |
| Export / import | export to a file (dump document), import a document into an installation | `hop::export`, `hop::import` / `import_document` |
| Resume | run the destination's resume command in a new terminal window | `catalog::interactive_resume`, `Harness::resume_refusal` |
| Chat | rooms (open/closed, members, messages), members (harness, instance, role, bound session, jump to it), messages (time, sender, text, observations), service status; read-only | `chat::lifecycle::{Paths, owner_active, status, client_config}`, `chat::client::ChatClient::call("rooms"|"members"|"ledger")` — never `Bus::open` (the service owns its file) |

Long work (listing a big store, a hop) runs off the UI thread; the UI shows progress and errors in
place. Nothing is written to a harness store except by a hop/import the user starts.

## Out of scope (next.md)
Chat writes (send, join, open/close rooms); live tailing of a running session; installers/bundles
(`--no-bundle` only for now).

## Removal of the TUI
When every row of the table works in the app and its tests pass: delete `mukade-tui/`, remove it
from the workspace, replace TUI docs with the desktop doc (`docs/desktop.md`), update README,
usage, architecture (the test scans `apps/desktop/src-tauri/src` instead of `mukade-tui/src`).
