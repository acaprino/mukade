# Documentation

Every living document, and when to read it. New here: the [project README](../README.md), then the [usage guide](usage.md).

## Getting started

| document | read it when |
|---|---|
| [Project README](../README.md) | you want the support matrix, a quick start, the library API or the command list |
| [Command-line usage](usage.md) | you run `mukade`: setup, commands, compaction, summary intents, examples and troubleshooting |
| [Sessions TUI](../mukade-tui/README.md) | you want the native terminal explorer: its keys, resume, hop, export and import |
| [Open points](../next.md) | you want the status board: done, to verify, known limits |
| [Windows checklist](verification/windows-checklist.md) | you settle an open check: every one as a step-by-step, with what counts as a pass |

## Harness pages

Read one before touching that harness's reader or writer, or to learn how it installs, launches, stores sessions, compacts, and uses tools, plugins and hooks.

| harness | mukade support |
|---|---|
| [Claude Code](harnesses/claude-code.md) | reads and writes |
| [Codex CLI](harnesses/codex.md) | reads and writes |
| [OpenCode](harnesses/opencode.md) | reads and writes |
| [Pi](harnesses/pi.md) | reads and writes |
| [Hermes Agent](harnesses/hermes-agent.md) | none yet; read-first plan in [ADR-0002](adr/ADR-0002-hermes-and-deepseek-harness-read-first.md) (Proposed) |
| [DeepSeek Harness](harnesses/deepseek-harness.md) | none yet; same plan, [ADR-0002](adr/ADR-0002-hermes-and-deepseek-harness-read-first.md) |

## Technical reference

| document | read it when |
|---|---|
| [Session interchange specification](session-interchange-spec.md) | you change a reader, a writer or the manifest: data model, conversion rules, contracts, requirement ids (L1…) |
| [Tool conversion tables](tool-conversion-tables.md) | you need to know how a tool call of one harness becomes a call of another |
| [Harness catalog](reference/harness-catalog.md) | you need the four harnesses' own install and store locations on Windows, macOS and Linux, their store layouts and the lineage each records (`catalog/harnesses.json`) |
| [Harness session persistence reference](reference/harness-session-persistence-reference.md) | you look beyond the four supported harnesses: secondary research (2026-10-27) on 24 harnesses, restructured on 2026-11-11, with its provenance and limitations. For the four supported harnesses, the harness pages and the specification come first (the specification's `[V]` facts supersede it) |

## Decisions

| document | read it when |
|---|---|
| [ADR index](adr/README.md) | before touching the paths, modules or domains an active record governs; it also states how a record moves from Proposed to Accepted |
| [ADR-0001](adr/ADR-0001-installation-scan-in-library-and-cli.md) | you work on `scan`: installation scanning is a library capability the CLI exposes first (Accepted) |
| [ADR-0002](adr/ADR-0002-hermes-and-deepseek-harness-read-first.md) | you plan Hermes Agent or DeepSeek Harness support: read-first, Hermes source-only (Proposed) |

## Verification records

Records of particular runs, not guarantees for later harness versions. Dates, environments and limitations remain in each record. All of them are in [verification/](verification/); supporting scripts are kept next to them in `verification/live-hops/`, `verification/native-compaction-live/`, `verification/pi-live/` and `verification/pi-reasoning-live/`.

**Hops and reasoning:**

- [Live hops and provider reasoning](verification/live-hops-and-provider-reasoning.md): Claude Code, Codex and OpenCode (2026-10-28).
- [Codex encrypted reasoning live](verification/codex-encrypted-reasoning-live.md): the Codex → Codex hop of 2026-11-04, with the discriminating negative control.
- [Pi session and reasoning verification](verification/pi-session-and-reasoning-verification.md): mock-provider import and real-provider signed reasoning, with separate results and limitations.
- [Opaque reasoning preservation](verification/2026-11-02-opaque-reasoning.md): OpenCode keeps the complete provider-owned Chat reasoning array (2026-11-02).

**Compaction and summary:**

- [Native compaction records](verification/native-compaction-live.md): live acceptance of §4.4's hand-written records by Claude Code, Codex, OpenCode and Pi.
- [Source-model summary](verification/source-model-summary-live.md): summary runs and their isolation.

**Platforms, listing and Codex code mode:**

- [Linux verification 2026-11-11](verification/linux-2026-11-11.md): Claude Code `live` on Linux (a bug found and fixed), the process-tree kill on timeout, the whole suite green on Linux.
- [Listing coverage](verification/listing-coverage-2026-11-08.md): sessions whose transcripts are gone (Codex `state_5.sqlite`, Claude Code `history.jsonl`, OpenCode's other databases), `live`, and the listing cache, counted on one machine before and after.
- [Codex code mode](verification/2026-11-11-codex-code-mode.md): the L10 census of real rollouts, 725 of 981 `exec` scripts read as their calls.

## Archive

Designs and plans preserve historical intent; for implemented behavior and evidence, use the technical reference and the verification records.

- [Source-model summary design](archive/design/source-model-summary.md) and its [implementation plan](archive/plans/source-model-summary-implementation.md).
- [Design drafts](superpowers/specs/): the TUI (2026-10-30), Hermes and DeepSeek Harness support (2026-10-31), the directory filter (2026-11-02), Codex code mode (2026-11-11).
- [Implementation plans](superpowers/plans/): the directory filter (2026-11-02), Codex code mode (2026-11-11).
- [TUI mockup](mockups/tui.html) and the [branding assets](assets/branding/) (logo and its generation prompt).
