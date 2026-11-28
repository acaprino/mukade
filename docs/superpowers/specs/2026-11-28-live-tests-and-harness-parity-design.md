# Local live tests and complete harness support

Status: agreed with the owner on 2026-11-28 (decisions 1–9 of the review hand-over). Built slice by slice.
Slice 1, the runner, built on 2026-11-28: [guide](../../live-tests.md), [record](../../verification/2026-11-28-live-runner.md)
(36 of 36 pairs; the chat of Hermes and DeepSeek Harness not checked, no adapter). Next: the review's fixes 2–6.

## Problem Statement

Mukade's tests run on fixtures. No test starts a real harness: whether Claude Code, Codex, OpenCode, Pi,
Hermes or DeepSeek Harness opens what mukade wrote is known only from one-off runs recorded in
`docs/verification`, each for one version that has since moved on (OpenCode 2.2.5 → 2.4.2, Pi 1.4.0 →
1.7.0, Codex 0.167 → 0.172). The owner cannot re-check a harness after it updates without hand work in
their real stores. The six harnesses are also not supported alike: chat reaches four of them, Hermes has
no sandbox install, three harnesses have no cheap read of a large session, fork links survive a hop
into two destinations only, and the Pi listing re-reads every file. The 2026-11-28 audit found bugs in
the same paths.

## Solution

One local command installs pinned copies of the six harnesses into a gitignored folder of the
repository, each with its own store, fetches the mukade that the latest green CI run built, seeds every
source store from the repository's fixtures, hops every source into every destination, reads each
result back with mukade, and has each destination harness open what was written. It prints a matrix of
pass, fail and not-checked per pair and saves the same as a report. Nothing touches the owner's real
stores and no model is called unless asked for. The buildable support gaps are then closed one by one,
each proven by a new scenario of that command, and the docs say what it verified.

## User Stories

1. As the owner, I run one command and get a pass/fail matrix of every harness pair, so I know which
   hops work today.
2. As the owner, I re-run it with the newest harness versions, so I learn when an update breaks mukade.
3. As the owner, I run it without touching my real stores, sessions, configuration or credentials.
4. As the owner, I run it without API keys or model costs; with an explicit flag and my keys, it also
   checks real resumes.
5. As the owner, I test the mukade that CI built from `develop`, or a binary I name.
6. As the owner, I delete everything it installed with one flag.
7. As the owner, I can chat with Hermes and DeepSeek Harness instances as with the other four.
8. As the owner, I install Hermes into a sandbox from the window, as the npm harnesses.
9. As the owner, I read a huge OpenCode, Hermes or DeepSeek session at the memory cost of the others.
10. As the owner, a fork hopped into Codex or OpenCode after its origin stays linked to the origin's copy.
11. As the owner, the Pi listing costs what changed, not the whole store.

## Implementation Decisions

- The runner is a Node script with no dependency beyond Node itself; npm is needed for the installs anyway.
- Installs, stores and reports live in one gitignored folder at the repository root; one flag removes it.
- The harness versions are pinned in one data file. The npm harnesses install from npm into a prefix of
  their own; Hermes installs from its git tag with `uv` into a virtual environment of its own (PyPI
  carries an older version). A flag tries the newest versions instead and reports what it found.
- Isolation uses the variables mukade itself sets for a sandbox (each adapter's sandbox description),
  plus the store variable of the harness catalog. One Rust test keeps the runner's table equal to the
  adapters', so the two cannot drift.
- The mukade binary comes from the latest green Verify run of `develop` (its uploaded artifact); a flag
  names another binary or a release.
- No model is called by default. A destination is confirmed by a command of its own that needs no model
  where one exists; otherwise by a resume against the mock providers the verification records already
  use. Real models only with an explicit flag and the owner's keys; a check that needs one and did not
  get it is reported as not checked, never as passed.
- Chat scenarios use a temporary data folder outside the repository (chat refuses one inside a
  checkout) and remove it afterwards.
- The review's fixes 2–6 come before the support gaps; fixes 7–20 after them; the docs last.
- Out of the gaps by decision: summary and native compaction into Hermes and DeepSeek Harness
  (ADR-0002); reading chats the Codex app hosts on ChatGPT (ADR-0005).

## Testing Decisions

- Seams: (1) the live runner, which treats mukade and each harness as black boxes through their command
  lines, the highest seam there is; (2) the existing Rust integration tests for every Rust change, run by
  CI on Linux and Windows; (3) the existing desktop mock and smoke tests for the window's fixes. No other
  new seam.
- A runner scenario passes on observable results only: exit codes, the session listed and read back with
  its message count, and the destination harness's own output naming the session.
- Each support gap adds the scenario that fails before it and passes after it, plus its Rust test.

## Out of Scope

- Running the runner in CI: it downloads hundreds of MB of harnesses and may need keys; CI keeps the
  fixture suite.
- macOS (dropped from CI by the owner).
- Changing how Hermes and DeepSeek Harness compact (decided limits), and reading ChatGPT-hosted chats.

## Further Notes

- Order of work: the runner (installs, every pair, native confirmation, chat), then fixes 2–6, then the
  gaps (chat for Hermes and DeepSeek, Hermes sandbox, cheap large reads, fork links into Codex and
  OpenCode, the Pi listing cache), then fixes 7–20, then the docs.
- Open facts the runner settles: which harnesses can open a session without a model, and whether Hermes'
  hooks can start a chat member.
