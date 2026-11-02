# Directory Filter Implementation Plan

**Goal:** Filter sessions by their recorded working directory in a separate TUI column.

**Architecture:** Build a directory index from SessionInfo, retaining original
session indices. All consumers resolve selection through that index. Readers
supply cwd from bounded native headers; the TUI remains store independent.

**Tech Stack:** Existing Rust workspace, ratatui and crossterm; no dependencies added.

**Spec:** `docs/superpowers/specs/2026-11-02-directory-filter-design.md`

## Global Constraints

- Preserve original paths, session ordering and installation roots.
- Never read full transcripts for grouping or write to native stores.
- Keep sessions with absent or malformed headers listable.

## Work

1. Extend Codex/Pi listing metadata with bounded header peeks and fixture tests.
2. Add a directory index, filtered selection, keyboard navigation and adaptive
   rendering. Test rendering first, then selection identity, resets and hop source.
3. Update usage, run workspace tests and clippy, inspect fixture snapshots, build
   the Windows release executable, and request an independent review.

## Review Focus

- Interleaved session indices must not select a different session for hop/resume.
- Missing cwd, empty stores and deleted groups after reload stay navigable.
- Same leaf name in different directories remains distinguishable.
- Native metadata reads are bounded and tolerate damaged files.
- Narrow terminal layouts keep the filter and transcript reachable.
