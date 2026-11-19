# Directory filtering in the sessions explorer

The user requested a directory column to filter the flat list of sessions. The
explorer will show harness/installation → directory → sessions → details.

## Behavior

- Directories come from each session's recorded working directory, within the
  selected installation. Show counts, an All directories row, and an Unknown
  directory group for sessions without a usable cwd. Keep original paths intact;
  directories need not exist on this machine.
- Selecting a directory filters the session list. Rendering, transcript reads,
  resume and hop all resolve through the same filtered selection. Changing the
  harness or directory clears the previous transcript and session selection.
- Tab, Shift+Tab and left/right move between the three selection columns. Enter
  advances from harness to directory to sessions, then opens the transcript.
  Esc moves back one level. Reload resets the filter to All directories.
- At 120 columns show all four panels. In narrower terminals show the three
  selection columns, and use the full body for an opened transcript. Below 70
  columns show the focused panel. Directory rows show the leaf name and cwd;
  matching leaf names use the shortest distinguishing path suffix before clipping.
  The selected directory expands its cwd over wrapped lines. In a short viewport,
  cap the item height and retain the path tail so the selected row stays visible.
- Codex and Pi readers expose cwd with a bounded header peek. The TUI never
  interprets native stores or exports full histories merely to build groups.

## Approach

A separate list of working directories implements the requested extra filter
column directly. An expandable directory tree would add hierarchy navigation
that the user did not request. Group headers inside the session list would not
provide an independent filter. All directories preserves access to the full list.

Implementation choices above are defaults selected by the implementer. No new
dependencies, filesystem canonicalization, persistent filter state or store
writes are required.

## Verification

Exercise interleaved sessions, missing directories, matching leaf names, selection
and hop identity, harness switches and reload. Render wide, narrow and empty
screens. Verify native listing metadata with fixture stores and malformed files.

Native formats checked against upstream sources:
[Codex's rollout listing](https://github.com/openai/codex/blob/main/codex-rs/rollout/src/list.rs)
reads cwd from SessionMeta; [Pi's session format](https://raw.githubusercontent.com/badlogic/pi-mono/main/packages/coding-agent/docs/session-format.md)
records cwd in its first-line session header. The missing listing values were in
Mukade's readers, rather than in those native formats.
