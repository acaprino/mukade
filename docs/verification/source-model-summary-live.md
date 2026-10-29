# Live check: asking each harness for a summary without saving anything

| | |
|---|---|
| **Date** | 2026-10-28 |
| **Harnesses** | Claude Code 2.1.312, Codex CLI 0.164.0, OpenCode 2.1.4, Pi 1.4.0 (the installed ones) |
| **mukade** | `master` at `5ea3cce` (plan of the source-model summary) |
| **Design** | [`docs/archive/design/source-model-summary.md`](../archive/design/source-model-summary.md) |
| **Result** | one command per harness that summarizes a session and leaves the session as it was; Codex's real model not reached (usage limit), its commands checked against a local mock |

## Why

`--compaction summary` makes the source harness's own model write the summary a hop carries. The
design's first rule is that the source session is never changed, and the flags that promise that
(`--no-session-persistence`, `--ephemeral`, `--fork`) had only been read in `--help`. This run asks
the real binaries what each one writes, and whether a provider accepts the request once tools are
switched off over a history that holds tool calls.

## Method

- **One throwaway session per harness**, made for this run in an empty project folder holding one
  `README.md` (synthetic text), with the prompt `Read README.md and tell me its first heading, in
  five words or fewer.`, so that every session holds a tool call. Real models, on the user's own
  configuration: Claude Code `claude-sonnet-5` (first party), Pi `openrouter/moonshotai/kimi-k2.6`,
  OpenCode its configured default.
- **Before and after** each summarizing run: the SHA-256 of the session file and the listing of its
  folder (Claude Code, Pi, the Codex home); for OpenCode, the row count of every table of
  `opencode.db`, a digest of the source's rows in `session`, `session_v2`, `message`, `part` and
  `session_message`, and the `session_v2` rows whose `fork_session_id` is the source.
- **The prompt** is the design's v1 summary prompt, one line.
- **Environment.** The run was made from inside another agent session whose environment routes
  HTTPS through a local TLS proxy (`HTTPS_PROXY`, with its CA in `NODE_EXTRA_CA_CERTS`). Codex,
  a Rust binary, does not trust that CA and failed with `workspace routing discovery failed`; the
  harnesses were therefore started without `HTTPS_PROXY`, `NODE_EXTRA_CA_CERTS` and the parent
  session's `CLAUDE_CODE_*` variables. The Claude Code summary was run both ways; the result is the same.
- **Codex** then answered `You've hit your usage limit … try again at Nov 4th, 2026`. Its commands
  were run against a local mock of the Responses API (`model_providers.mock`, `wire_api =
  "responses"`, a scratch `CODEX_HOME`), which logs each request and streams one text reply. That
  checks what Codex writes and sends; it cannot check what OpenAI accepts.
- **Afterwards** every throwaway session was deleted by its recorded id: Claude Code's project
  folder and its `session-env/<id>`, Pi's session folder, `opencode session delete <id>`,
  `codex delete --force <id>` (the two real Codex sessions the failed runs had made).

## Results

This table records the initial harness probes. OpenCode's later deny-all check and the final
command used by mukade are recorded separately below; the initial probe did not deny its tools.

| harness | command | source after the run | tools off over a history with tool calls | reply read from |
|---|---|---|---|---|
| Claude Code | `claude -p --tools "" --resume <id> --no-session-persistence --output-format json "<prompt>"` | transcript byte-identical, no file added to the project folder **[V]** | accepted **[V]** | `result` of the JSON |
| Codex | `codex exec fork <id> --ephemeral --skip-git-repo-check -c sandbox_mode="read-only" -o <file> "<prompt>"` | `CODEX_HOME` unchanged (rollouts, `session_index.jsonl`, the SQLite files and their WAL) **[V, mock]** | not applicable: `exec` has no switch, the request offers its tools; the read-only sandbox bounds what a call could do | the `-o` file (also the last block of stdout) |
| Pi | `pi -p --fork <session file> --session-dir <temporary folder> --no-tools "<prompt>"` | file byte-identical, folder unchanged; the fork lands in the temporary folder **[V]** | accepted (`openrouter`) **[V]** | stdout |
| OpenCode | `opencode run --standalone --session <id> --fork --format json "<prompt>"`, then `opencode session delete <fork id> --standalone` | the fork adds 1 `session_v2` row (`fork_session_id` = the source, `fork_boundary` `{"type":"through",…}`), 7 `session_message` rows, 1 `event_sequence` row and 1 `instruction_state` row; after the delete every count is back, no row names the fork, the source's rows are unchanged **[V]** | no switch used; the model called none | the `text` events (the fork's id is their `sessionID`) |

Timings: Claude Code 38–45 s, Pi 7 s, OpenCode 8 s, Codex against the mock under 1 s. Claude Code's
JSON reported `total_cost_usd` 1.22 cold and 0.52 with the prompt cache warm, for a session of 56
records whose context was 184 000 tokens. Almost all of it was what the user's own hooks and plugins
add to every request, not the conversation.

## Findings

- **`codex exec resume --ephemeral` writes into the session it resumes.** Against the mock it appended
  11 records to the source rollout (`thread_settings_applied`, a turn with the prompt and the reply,
  `token_count`, `task_complete`). `--ephemeral` stops a *new* rollout from being written, not an
  existing one from growing. `codex exec fork <id> --ephemeral` leaves the home unchanged, and its
  request carries the whole source conversation. This is the command to use.
- **`claude --tools` takes several values.** Placed after `--resume`, `--tools ""` swallowed the
  prompt as a tool name and Claude Code answered `Error: No deferred tool marker found in the
  resumed session … Provide a prompt to continue the conversation.` It has to come before another
  option.
- **Claude Code's own side files still move.** With `--no-session-persistence` the transcript is
  untouched, but the SessionStart hooks the user installed rewrote
  `session-env/<id>/sessionstart-hook-*.sh` for the resumed id. That folder holds hook environment,
  not conversation.
- **The language of the summary** follows what the model reads, not only the user's words: the
  English session was summarized in Italian by Claude Code, whose context carries the user's Italian
  instructions, and in English by Pi and OpenCode.
- **An interrupted last turn** (the real Codex session whose turn failed on the usage limit, copied
  into the scratch home) forks fine: the request holds the unanswered user message, then the prompt
  (mock).
- **Codex reads stdin** (`Reading additional input from stdin...`) and Claude Code waits 3 s for it
  when stdin is left open; mukade closes stdin for every harness process (`exe::run_within`).

## Not checked

- Codex against OpenAI: whether its model summarizes or keeps working, and what a fork of a real
  rollout costs. **[U]** until the usage limit resets.
- A session larger than the model's context window. None was made; whether a harness compacts it in
  memory or fails is **[U]**. mukade must report either as a failed summary, never write half of one.
- `--model` / `-m` / `--provider` overrides: the flags exist in each `--help`; no run used them.

## End to end

`target/release/mukade.exe` at `1c38672`, one real hop per source with `--compaction summary` and
`--resume-prompt "What were we doing"`, each on a new throwaway session (deleted afterwards):

| hop | time | source after | summary | the destination answered |
|---|---|---|---|---|
| Claude Code → Pi | 77 s | file byte-identical | 1054 characters, `claude-sonnet-5`, `total_cost_usd` 0.66 | what the task was, the heading found, that nothing was left open |
| Pi → OpenCode | 38 s | file byte-identical | 903 characters, `moonshotai/kimi-k2.6` | the request and the answer given |
| OpenCode → Pi | 25 s | every table's row count as before, source rows unchanged, no fork left | 433 characters, `openai/gpt-6.1-terra` | the task, what was done, its status |
| Codex (mock) → Pi, `--keep-turns 1` | 31 s | `CODEX_HOME` unchanged | `MOCK-SUMMARY`, then the last user turn kept | (the mock's text carries nothing to answer from) |

- The manifest of each said `options.compaction: "summary"`, the harness, version and model that
  wrote the summary, and one loss line: `the whole history (N entries) travelled as a summary written
  by <harness>'s model` (`the history before <id> …` with a kept turn).
- **The first Pi → OpenCode hop wrote the session and then failed its resume (exit 5)**, for a reason
  outside the summary: the session carries the source's model (`openrouter/moonshotai/kimi-k2.6`),
  and OpenCode asked OpenRouter for 235 929 output tokens, over that endpoint's 262 144-token
  context (`This endpoint's maximum context length is 262144 tokens…`). With OpenCode's own default
  model (`--model-provider openrouter --model openai/gpt-6.1-terra`) the same hop completed.
- OpenCode's summary mentioned that a handoff summary had been asked for, and Pi's reply then said
  the user had requested the handoff. The summary describes the session; the request for it is part
  of what the model saw.

## Tools denied in OpenCode (after the review)

OpenCode's `run` has no switch for tools, and the review asked what stops the summarizing run from
editing files. Two more runs on the same day, OpenCode 2.1.4, its default model:

- `OPENCODE_CONFIG_CONTENT='{"permission":{"*":"deny"}}' opencode run … "Create a file named
  WRITTEN.txt … using your write tool"`: no `tool_use` event, no file; the model printed its call as
  text instead.
- The same configuration on `run --session <id> --fork` of a session holding a `read` call, with the
  summary prompt: a normal summary (`# Handoff summary …`).

The three throwaway sessions were deleted by id.

## Final commands

These are what `src/summary.rs` runs (`<prompt>` is always the last argument; `[…]` only when a
model is known):

| harness | argv |
|---|---|
| Claude Code | `claude -p --tools "" --resume <id> --no-session-persistence --output-format json [--model <id>] <prompt>` |
| Codex | `codex exec fork <id> --ephemeral --skip-git-repo-check -c sandbox_mode="read-only" -o <scratch>/last-message.txt [-m <id>] <prompt>` |
| Pi | `pi -p --fork <session file> --session-dir <scratch>/sessions --no-tools [--provider <p>] [--model <id>] <prompt>` |
| OpenCode | `OPENCODE_CONFIG_CONTENT='{"permission":{"*":"deny"}}' opencode run --standalone --session <id> --fork --format json [-m <provider>/<id>] <prompt>`; then `opencode session delete <fork id> --standalone` |

The two fixtures kept from this run are `fixtures/claude_code/summary-print.json` (Claude Code's
JSON reply, the warm run) and `fixtures/opencode/summary-run.jsonl` (OpenCode's events for the fork).
