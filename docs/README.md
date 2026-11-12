# Documentation

New here: the [project README](../README.md), then the [usage guide](usage.md).

## Getting started

| document | read it when |
|---|---|
| [Project README](../README.md) | you want the matrix, quick start, library, commands |
| [Command-line usage](usage.md) | you run `mukade`: setup, commands, compaction, intents, troubleshooting |
| [Sessions TUI](../mukade-tui/README.md) | you want the terminal explorer: keys, resume, hop, export, import |
| [Open points](../next.md) | you want the status board: done, to verify, known limits |
| [Windows checklist](verification/windows-checklist.md) | you settle an open check: steps, pass criterion |

## Harness pages

Install, store, compaction, tools, hooks; read before touching that harness's reader or writer.

| harness | mukade support |
|---|---|
| [Claude Code](harnesses/claude-code.md) | reads and writes |
| [Codex CLI](harnesses/codex.md) | reads and writes |
| [OpenCode](harnesses/opencode.md) | reads and writes |
| [Pi](harnesses/pi.md) | reads and writes |
| [Hermes Agent](harnesses/hermes-agent.md) | none yet; read-first plan in [ADR-0002](adr/ADR-0002-hermes-and-deepseek-harness-read-first.md) (Proposed) |
| [DeepSeek Harness](harnesses/deepseek-harness.md) | none yet; same plan |

## Technical reference

| document | read it when |
|---|---|
| [Session interchange specification](session-interchange-spec.md) | you change a reader, writer or the manifest: data model, conversion rules, contracts, requirement ids (L1…) |
| [Architecture](architecture.md) | you change mukade's code: one adapter per harness (`Harness`, `Destination`, the registry), adding or changing a harness, the test that keeps it so |
| [Tool conversion tables](tool-conversion-tables.md) | you need how one harness's tool call becomes another's |
| [Harness catalog](reference/harness-catalog.md) | you need install and store locations per OS, store layouts, lineage (`catalog/harnesses.json`) |
| [Harness session persistence reference](reference/harness-session-persistence-reference.md) | you look beyond the four: secondary research (2026-10-27, restructured 2026-11-11) on 24 harnesses, with provenance and limitations. For the four, harness pages and specification come first (their `[V]` facts supersede it) |

## Decisions

| document | read it when |
|---|---|
| [ADR index](adr/README.md) | before touching what an active record governs; how a record is Accepted |
| [ADR-0001](adr/ADR-0001-installation-scan-in-library-and-cli.md) | you work on `scan`: a library capability the CLI exposes first (Accepted) |
| [ADR-0002](adr/ADR-0002-hermes-and-deepseek-harness-read-first.md) | you plan Hermes or DeepSeek support: read-first, Hermes source-only (Proposed) |

## Verification records

Records of single runs, not guarantees for later versions, in [verification/](verification/); scripts in `verification/live-hops/`, `native-compaction-live/`, `pi-live/`, `pi-reasoning-live/`.

**Hops and reasoning:**

- [Live hops and provider reasoning](verification/live-hops-and-provider-reasoning.md): Claude Code, Codex, OpenCode (2026-10-28).
- [Codex encrypted reasoning live](verification/codex-encrypted-reasoning-live.md): Codex → Codex (2026-11-04), with negative control.
- [Pi session and reasoning](verification/pi-session-and-reasoning-verification.md): mock import, real signed reasoning.
- [Opaque reasoning preservation](verification/2026-11-02-opaque-reasoning.md): OpenCode keeps the Chat reasoning array (2026-11-02).

**Compaction, platforms, listing, code mode:**

- [Native compaction records](verification/native-compaction-live.md): §4.4's records accepted live by the four harnesses.
- [Source-model summary](verification/source-model-summary-live.md): summary runs and their isolation.
- [Linux 2026-11-11](verification/linux-2026-11-11.md): Claude Code `live` (a bug fixed), process-tree kill, suite green.
- [Listing coverage](verification/listing-coverage-2026-11-08.md): sessions whose transcripts are gone (Codex `state_5.sqlite`, Claude Code `history.jsonl`, OpenCode's other databases), `live`, the cache.
- [Codex code mode](verification/2026-11-11-codex-code-mode.md): the L10 census: 725 of 981 `exec` scripts read as calls.

## Archive

Historical intent; for behavior use the reference and records.

- [Source-model summary design](archive/design/source-model-summary.md) and [plan](archive/plans/source-model-summary-implementation.md).
- [Design drafts](superpowers/specs/): TUI (2026-10-30), Hermes and DeepSeek (2026-10-31), directory filter (2026-11-02), Codex code mode (2026-11-11); [implementation plans](superpowers/plans/) for the last two.
- [TUI mockup](mockups/tui.html); [branding assets](assets/branding/) (logo, its generation prompt).
