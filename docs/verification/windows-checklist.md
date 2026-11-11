# Windows checklist — what is left to verify

Everything that could be settled on Linux is settled ([linux-2026-11-11.md](linux-2026-11-11.md)).
What is below needs **your Windows machine**: the real harnesses, the real stores, sometimes a key.

**How to use this page**

- One block = one check. Do them in order; each says how long it takes.
- Every block has the same parts: **Needs → Do → Pass if → Write down**.
- Tick the box when it passes. If it fails, stop and copy the output: that is the bug report.
- 🟢 free and safe · 🟡 costs a few model calls · 🔴 needs an API key you may not have

> Safety rule for every step: mukade **never changes a source session**. Each write makes a **new**
> session. Steps that create throwaway sessions say so, and say how to remove them.

---

## 0. Setup (5 min) 🟢

**Do**

```powershell
cd D:\path\to\mukade
git pull
.\build.bat
cargo test --workspace --no-fail-fast
.\target\release\mukade.exe harnesses
```

**Pass if**

- [ ] `cargo test` ends with **0 failed** in every suite (519 tests on Linux; Windows adds a few of its own).
- [ ] `harnesses` lists Claude Code, Codex, OpenCode and Pi with a version and a store each.

**Write down:** the four versions.

---

## 1. Explorer: export and import (5 min) 🟢

New in this round: `e` and `i` in `mukade-tui`.

**Do**

1. `.\target\release\mukade-tui.exe`
2. Pick a Claude Code session → press **`e`**.
3. The box shows `document … \mukade-data\exports\claude_code-<id>.json`. Press `esc`.
4. Press **`i`** → the path is already filled in → `tab` → choose **Pi** with `↓` → `enter` → `enter`.
5. Read the result box: `destination`, `location`, `interactive`, `losses`.
6. Run the `interactive` command it printed (`pi --session …`).

**Pass if**

- [ ] The export file exists and opens as JSON.
- [ ] The import shows a new id, and Pi opens the conversation.
- [ ] `losses` says what did not travel (it may say `none`).

**Clean up:** delete the new Pi session file shown in `location`.

---

## 2. Timeouts still stop the whole tree on Windows (3 min) 🟢

The Unix path changed; the Windows path (`taskkill /T`) did **not**, and no automated test
covers it (the tree test is Unix-only). One manual run confirms it.

**Do**

```powershell
# A fake harness that starts a child and then waits far past the deadline.
Set-Content "$env:TEMP\slowpi.cmd" "@echo off`r`nstart /b ping -n 120 127.0.0.1 >nul`r`nping -n 120 127.0.0.1 >nul"
# The summary runs the SOURCE harness, so the fake is the source (Pi) and its exe override.
mukade hop pi ANY_PI_SESSION claude_code --compaction summary --summary-timeout 3 --pi-exe "$env:TEMP\slowpi.cmd"
Get-Process ping -ErrorAction SilentlyContinue
```

**Pass if**

- [ ] The hop fails with `summary failed` after about 3 s.
- [ ] `Get-Process ping` prints **nothing** (both pings were stopped).

---

## 3. Codex Desktop "code mode" on a real thread (10 min) 🟢

New in this round: a one-call `exec` script is read as the shell or patch call it wraps (spec L10).
Only hand-made rollouts were tested so far.

**Needs:** one Codex **Desktop** thread with a few shell commands in it.

**Do**

```powershell
$env:CODEX_HOME = "$env:USERPROFILE\.codex"
mukade list codex | Select-Object -First 5
mukade dump codex YOUR_DESKTOP_THREAD_ID --output "$env:TEMP\desktop.json"
# How many calls became shell calls, and how many stayed exec?
(Get-Content "$env:TEMP\desktop.json" | ConvertFrom-Json).messages.content |
  Where-Object type -eq 'tool_use' | Group-Object name | Select-Object Count, Name
```

Then hop it to Claude Code, writing to a scratch folder:

```powershell
$env:CLAUDE_CONFIG_DIR = "$env:TEMP\claude-scratch"
mukade hop codex YOUR_DESKTOP_THREAD_ID claude_code --cwd 'C:\work\scratch'
```

**Pass if**

- [ ] Most calls are named `exec_command` (and some `apply_patch`); only multi-call scripts stay `exec`.
- [ ] The hop's manifest counts **fewer** `tools_text_form` than before this change.
- [ ] `claude --resume <id>` (with the same `CLAUDE_CONFIG_DIR`) shows Bash calls, not text blocks.

**Write down:** the `Group-Object` counts, and `tools_text_form` from the manifest.

**Clean up:** `Remove-Item -Recurse "$env:TEMP\claude-scratch"`.

---

## 4. Codex subagent parent (3 min) 🟢

New: a subagent's rollout now reads with `meta.parent` = the thread that spawned it.

**Do**

```powershell
mukade dump codex YOUR_SUBAGENT_THREAD_ID --output "$env:TEMP\sub.json"
(Get-Content "$env:TEMP\sub.json" | ConvertFrom-Json).meta.parent
```

Find a subagent id in the TUI (rows marked `↳`) or in `state_5.sqlite` `thread_spawn_edges`.

**Pass if**

- [ ] It prints the parent thread's id, the same one the TUI shows as `parent`.

---

## 5. Codex: a chain of pages (15 min) 🟢

Open since 0.165: a thread edited more than once is read by following each page back to its base.

**Do**

1. In the **Codex app**, open a new test chat. Send two messages.
2. **Edit** the second message. Then **edit it again**.
3. `mukade dump codex THE_THREAD_ID --output "$env:TEMP\pages.json"`
4. Compare the turns in `pages.json` with what the app shows.

**Pass if**

- [ ] The same turns, in the same order, the latest version of the edited message.

**Clean up:** `codex delete --force THE_THREAD_ID`.

---

## 6. Codex: summary written by Codex's real model (5 min) 🟡

Was checked against a mock only (quota).

**Do**

```powershell
$before = (Get-FileHash THE_ROLLOUT_FILE).Hash
mukade hop codex A_THROWAWAY_THREAD_ID pi --compaction summary --resume-prompt "What were we doing"
(Get-FileHash THE_ROLLOUT_FILE).Hash -eq $before
```

**Pass if**

- [ ] The last line prints `True` (the source is untouched).
- [ ] The summary in the new Pi session is a summary, not a work log.
- [ ] Pi's reply answers from the summary.

---

## 7. A session bigger than the source model's window (10 min) 🟡

**Do:** pick (or build) a session larger than its model's context window, then:

```powershell
mukade hop claude_code THE_BIG_SESSION pi --compaction summary
```

**Pass if**

- [ ] It ends with `summary failed` (exit 4: nothing changed), and **no** Pi session was written.
- [ ] It never writes half a summary.

---

## 8. Claude Code: signed thinking really validated (15 min) 🟡

`claude -p --resume` did not check earlier turns' thinking, so the tampered copy resumed too.

**Needs:** a Claude Code session whose **last assistant turn is a tool call with thinking**.

**Do**

1. `mukade hop claude_code THAT_SESSION claude_code` → note the new id.
2. Make a copy of the written file and change one character inside a `signature`.
3. Resume both with a one-line prompt: `claude -p --resume <id> "continue"`.

**Pass if**

- [ ] The untouched copy resumes.
- [ ] The tampered copy is **refused** by the provider.

**Clean up:** step 9.

---

## 9. Claude Code: remove the old test sessions (1 min) 🟢

**Do:** delete the folder `…-scratchpad-live-claude-work` under `%USERPROFILE%\.claude\projects\`.

- [ ] Done. No real project folder touched.

---

## 10. OpenCode with its own Anthropic provider (10 min) 🔴

**Needs:** an Anthropic API key configured in OpenCode.

**Do**

```powershell
mukade hop claude_code A_SESSION_WITH_THINKING opencode --model-provider anthropic --cwd 'C:\work\scratch' --resume-prompt "continue"
```

**Pass if**

- [ ] OpenCode answers, and the manifest says the reasoning travelled **natively**.

---

## 11. OpenAI encrypted reasoning through OpenRouter (10 min) 🔴

**Needs:** an OpenRouter key in OpenCode, and a Codex session with encrypted reasoning.

**Do:** `mukade hop codex THAT_THREAD opencode --model-provider openrouter --resume-prompt "continue"`

**Pass if**

- [ ] OpenCode answers; the manifest says `reasoning.encrypted` was written natively.

---

## 12. The listing cache on a cold disk (5 min) 🟢

**Do**

1. Reboot (or wait until the disk cache is cold). Delete `mukade-data\cache\`.
2. `Measure-Command { mukade list claude_code }`
3. Run it again.

**Pass if**

- [ ] The second run is far faster than the first.

**Write down:** both times, and the number of rows.

---

## 13. A 100 MB session (5 min) 🟢

**Do:** `mukade hop claude_code A_VERY_LARGE_SESSION pi --timing`

**Write down:** the `total` line. Earlier numbers: 30 MB took 440–630 ms.

---

## 14. Real-life shapes nobody has hopped yet (when they happen) 🟢

Whenever one of these shows up in your normal work, hop it once and check the result opens:

- [ ] a Claude Code session that **auto-compacted** (by threshold)
- [ ] a session that compacted **twice**
- [ ] a Claude Code session after a **rewind**, and a Pi session after **`/tree`** navigation
- [ ] a session with an **image** going to Claude Code

---

## When you are done

Send back the ticked list and the "Write down" values. Each passed block becomes a dated line
in `next.md` and a verification record, like the Linux one.
