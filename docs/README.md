# Documentation

## Getting started

- [Command-line usage](usage.md): setup, commands, compaction, examples and troubleshooting.
- [Open questions](../next.md): outstanding verification and known limitations.

## Technical reference

- [Session interchange specification](session-interchange-spec.md): data model, conversion rules and contracts.
- [Tool conversion tables](tool-conversion-tables.md): tool mappings between harnesses.
- Harness guides: [Claude Code](harnesses/claude-code.md), [Codex](harnesses/codex.md), [OpenCode](harnesses/opencode.md) and [Pi](harnesses/pi.md).
- [Harness session persistence reference](reference/harness-session-persistence-reference.md): secondary research with its original provenance and limitations.
- [Harness catalog](reference/harness-catalog.md): the four harnesses, their own install and store locations on Windows, macOS and Linux, their store layouts and the lineage each records (`catalog/harnesses.json`).

## Verification records

These are records of particular runs, not guarantees for later harness versions. Dates, environments and limitations remain in each record.

- [Live hops and provider reasoning](verification/live-hops-and-provider-reasoning.md): Claude Code, Codex and OpenCode.
- [Pi session and reasoning verification](verification/pi-session-and-reasoning-verification.md): mock-provider import and real-provider signed reasoning, with separate results and limitations.
- [Source-model summary](verification/source-model-summary-live.md): summary runs and their isolation.

Supporting scripts are kept next to these records in `verification/live-hops/`, `verification/pi-live/` and `verification/pi-reasoning-live/`.

## Archive

Designs and plans preserve historical intent; consult the technical reference and verification records for implemented behavior and evidence.

- [Source-model summary design](archive/design/source-model-summary.md).
- [Source-model summary implementation plan](archive/plans/source-model-summary-implementation.md).
