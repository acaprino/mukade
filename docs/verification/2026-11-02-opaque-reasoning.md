# Opaque reasoning preservation — 2026-11-02

The OpenCode reader now retains the complete provider-owned Chat reasoning array.
Compatible same-provider OpenCode output restores it without reducing it to one
signature or ciphertext. The requirement is normative in
[session interchange §3.3](../session-interchange-spec.md#33-reasoning).

## Confirmed defects and correction

The previous reader took only `native.reasoningDetails[0]`, failed to recognize
`openai-native-responses-v1`, and discarded IDs, indices, other entries and
extension fields. The writer emitted Chat details only for the provider named
`openrouter`; a custom Gateway provider received unsupported scalar native fields.

`Opaque.chat` now retains source provider identity and the complete details array
through the canonical Session and public document. Known scalar fields remain
compatibility projections. Planning checks provider/envelope compatibility as well
as protocol. Native and aggregator OpenAI formats are distinct namespaces; mixed
or unknown declared formats cannot borrow the first entry's known protocol.
Unrepresentable conversions remain counted losses. Existing explicit `AsText`
and `Drop` policies keep their meaning; neither is native replay.

## Candidate and scope

- Source base: `4a0d97bc9da88e97050d26d5986f782d5e140024`, verified against official
  `https://github.com/acaprino/mukade` HEAD with `git ls-remote origin HEAD` on
  2026-11-02, plus this local source correction. No remote publication occurred.
- Build: `cargo build --release --locked --workspace -j 1`, CLI and TUI, version
  0.1.0. Final build completed in 1 minute 40 seconds.
- CLI: `target/release/mukade.exe`, SHA-256
  `edf188336cf58ca28a77d40e188328aa4a67258bbe5391f5216fd876d6869958`.
- TUI: `target/release/mukade-tui.exe`, SHA-256
  `2502be975060d7e87d81c18e2abc638cea632549e7a3baebba85049e510b9921`.
- No separate Mukade installation was replaced. JanuraCode's embedded converter
  remains pinned to `f81b18209fa8284308cfc5523714a9a43c9730bc`; the ordinary app
  transfer policy currently requests `as_text` on import and `drop` on Codex
  export. Updating that artifact requires JanuraCode's explicit source-identity,
  upstream and compatibility gates, not copying this local binary over it.

## Verification

The initial focused regression failed in four of five cases before the correction.
The permanent [seven regression tests](../../tests/reasoning_replay.rs) cover
summary-first arrays, multiple encrypted entries, native and OpenRouter formats,
IDs/indices/extensions, three repeated public-document/native round trips, tool
siblings, unknown destination protocol and counted incompatible-route losses.
A subsequently added native/aggregator mixed-array case also failed before the
final format classifier correction.

The full workspace passed **423 tests, zero failures or ignored tests**, with
`cargo test --locked --workspace -j 1 -- --test-threads=2`. After the final mixed-format
refinement, the affected library, reasoning replay, OpenCode reader/writer, planner
and round-trip suites were rerun: **126 passed, zero failures or ignored tests**.
The rest of the full-workspace result is reused for unchanged inputs.

The initial unrestricted parallel Cargo run exhausted Windows commit/pagefile
capacity (`os error 1455` and allocation failures); the serial build/test run
resolved that environmental failure. No machine settings or user processes were changed.

The final compiled CLI was also exercised through the real installed JanuraCode
OpenCode **2.2.3** importer (engine SHA-256
`c54dcb9f6317e165a1439f880a10b05f05836c728e175017024450a20720d77f`).
A synthetic history contained two sequential assistant/tool steps in one human
turn, each owning a summary and encrypted detail with IDs, indices and an
extension field. Mukade's `document opencode opencode` output, with an explicit
compatible destination protocol/provider, was imported using
`session import --standalone`. Reading the isolated destination SQLite database
found both arrays unchanged on their respective assistant owners and both tools.
No reasoning loss was reported.

This proves canonical conversion, actual native import and storage for that
candidate. It does **not** prove a subsequent outgoing request, upstream provider
acceptance, GUI execution or compatibility of every harness/model combination.
There were **zero paid model calls** in this Mukade check. The separate real
JanuraCode two-tool Gateway audit is recorded in its
`docs/verification/2026-11-02-gpt-reasoning-replay-probe.md`; it is not a qualification
of every converted session. Long-loop quality and monetary effects were not measured.

## Cleanup

The final native-import probe used private project/configuration/database/data
directories and removed its own validated temporary root (388,945 bytes), with
absence checked. The preceding successful native-import probe likewise removed
its own 394,903-byte root. Real user sessions and databases were not changed.
One-time edit/probe scripts and logs are removed after recording these results;
the source tests, specifications, this report and normal Cargo outputs remain.

Follow-up cleanup confirmed: 17 one-time files removed (963,274 bytes), with the session scratch directory empty. Referenced documentation headings and implementation/test-owner paths resolve. The Gateway model/provider documentation checks passed 115 tests; its runtime and live catalog were not changed.
