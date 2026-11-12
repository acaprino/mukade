# Windows checklist — what is left to verify

Each block below is one open check that needs **your Windows machine**: the real harnesses, the
real stores, sometimes a key. Everything Linux could settle is settled
([linux-2026-11-11.md](linux-2026-11-11.md)). `next.md` cites these steps as [n].

**How to use this page**

1. Do the blocks in order, in one PowerShell window (step 0 sets it up). Each says how long it takes.
2. Each block: **Needs** (if any) → **Do** → **Pass if** → **Write down** / **Clean up** (if any).
3. Tick the box when it passes. If it fails, stop and copy the output: that is the bug report.

🟢 free and safe · 🟡 costs a few model calls · 🔴 needs an API key you may not have

> Safety rule for every step: mukade **never changes a source session**. Each write makes a **new**
> session. Steps that create throwaway sessions say so, and say how to remove them.

---

## 0. Setup (5 min) 🟢

**Do**

```powershell
cd D:\path\to\mukade
git pull
.\build.bat                       # press a key when it pauses at the end
cargo test --workspace --no-fail-fast
# Every later step runs in this window, with these:
$env:PATH = "$PWD\target\release;$env:PATH"
$env:CLAUDE_CONFIG_DIR = "$env:USERPROFILE\.claude"
$env:CODEX_HOME = "$env:USERPROFILE\.codex"
$env:PI_CODING_AGENT_DIR = "$env:USERPROFILE\.pi\agent"
$env:OPENCODE_DB = "$env:USERPROFILE\.local\share\opencode\opencode.db"
mukade harnesses
```

The `mukade` commands below read and write only the stores these variables name.

**Pass if**

- [ ] `cargo test` ends with **0 failed** in every suite (519 tests on Linux; Windows adds a few of its own).
- [ ] `harnesses` lists Claude Code, Codex, OpenCode and Pi with a version and a store each.

**Write down:** the four versions.

---

## 1. Explorer: export and import (5 min) 🟢

New in this round: `e` and `i` in `mukade-tui`.

**Do**

1. `mukade-tui`
2. Pick a Claude Code session → press **`e`**.
3. The box shows `document … \mukade-data\exports\claude_code-<id>.json`. Press `esc`.
4. Press **`i`** → the path is already filled in → `tab` → `↓` until **Pi** → `enter` → `enter`.
5. Read the result box: `destination`, `location`, `interactive`, `losses`.
6. Run the `interactive` command it printed (`pi --session …`).

**Pass if**

- [ ] The export file exists and opens as JSON.
- [ ] The import shows a new id, and Pi opens the conversation.
- [ ] `losses` says what did not travel (it may say `none`).

**Clean up:** delete the new Pi session file shown in `location`.

---

## 2. Timeouts still stop the whole tree on Windows (3 min) 🟢

The Unix path changed; the Windows path (`taskkill /T`) did **not**, and no automated test covers
it (the tree test is Unix-only). One manual run confirms it.

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

A code-mode `exec` script of literal `text(await tools.X(...))` statements is read as its calls,
each with its own result; `apply_patch` is read as a write or edit (spec L10). The read side is
checked on every rollout of this machine ([census](2026-11-11-codex-code-mode.md): 725 of 981
scripts split). Left: one live hop.

Every Desktop rollout declares PowerShell, so its shell calls translate only toward a destination
that runs PowerShell ([tables §4.1](../tool-conversion-tables.md#41-shell)): Codex on Windows does,
with no flag; Pi does only with a `shellPath` naming pwsh (without one it runs Git Bash).

**Needs:** one Codex **Desktop** thread with a few shell commands and a file edit in it.

**Do**

```powershell
mukade hop codex YOUR_DESKTOP_THREAD_ID codex --cwd 'C:\work\scratch'
codex resume <the id the receipt names>
```

Pi as well, with its shell set to pwsh in a scratch agent folder:

```powershell
$env:PI_CODING_AGENT_DIR = "$env:TEMP\pi-scratch"
New-Item -ItemType Directory -Force $env:PI_CODING_AGENT_DIR | Out-Null
'{"shellPath":"pwsh"}' | Set-Content -Encoding ascii "$env:PI_CODING_AGENT_DIR\settings.json"
mukade hop codex YOUR_DESKTOP_THREAD_ID pi --cwd 'C:\work\scratch'
pi --session <the file the receipt names>
```

**Pass if**

- [ ] Both manifests' losses name far fewer text-form calls than the thread has `exec` calls.
- [ ] Codex shows the shell commands as `exec_command` calls with their output (the edits stay
  text: Codex's patch form is not written).
- [ ] Pi shows the shell commands as `bash` calls with their output, and the edits as `edit` calls.

**Write down:** each manifest's `tools_text_form` and the thread's `exec` count.

**Clean up:**

```powershell
codex delete --force <the id the Codex receipt names>
Remove-Item -Recurse "$env:TEMP\pi-scratch"
$env:PI_CODING_AGENT_DIR = "$env:USERPROFILE\.pi\agent"
```

---

## 4. Codex subagent parent (3 min) 🟢

New: a subagent's rollout now reads with `meta.parent` = the thread that spawned it.

**Needs:** a subagent id: a row marked `↳` in the TUI, or a row of `state_5.sqlite` `thread_spawn_edges`.

**Do**

```powershell
mukade dump codex YOUR_SUBAGENT_THREAD_ID --output "$env:TEMP\sub.json"
(Get-Content "$env:TEMP\sub.json" | ConvertFrom-Json).meta.parent
```

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

Checked against a mock only so far (quota).

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

**Clean up:** delete the new Pi session file the receipt names.

---

## 7. A session bigger than the source model's window (10 min) 🟡

**Do:** pick (or build) a session larger than its model's context window, then:

```powershell
mukade hop claude_code THE_BIG_SESSION pi --compaction summary
$LASTEXITCODE
```

**Pass if**

- [ ] It ends with `summary failed` and exit code **4** (nothing changed), and **no** Pi session was written.
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

**Clean up:** delete the written file and its copy (the receipt's `location` names the first). The
new session sits in the source's project folder, not in the one step 9 removes.

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

**Clean up:** `opencode session delete <new id> --standalone`.

---

## 11. OpenAI encrypted reasoning through OpenRouter (10 min) 🔴

**Needs:** an OpenRouter key in OpenCode, and a Codex session with encrypted reasoning.

**Do:** `mukade hop codex THAT_THREAD opencode --model-provider openrouter --resume-prompt "continue"`

**Pass if**

- [ ] OpenCode answers; the manifest says `reasoning.encrypted` was written natively.

**Clean up:** `opencode session delete <new id> --standalone`.

---

## 12. The listing cache on a cold disk (5 min) 🟢

**Do**

1. Reboot (or wait until the disk cache is cold). Delete `target\release\mukade-data\cache\`.
2. `Measure-Command { mukade list claude_code }`
3. Run it again.

**Pass if**

- [ ] The second run is far faster than the first.

**Write down:** both times, and the number of rows.

---

## 13. A 100 MB session (5 min) 🟢

**Do:** `mukade hop claude_code A_VERY_LARGE_SESSION pi --timing`

**Write down:** the `total` line. Earlier numbers: 30 MB took 440–630 ms.

**Clean up:** delete the new Pi session file the receipt names.

---

## 14. Real-life shapes nobody has hopped yet (when they happen) 🟢

Whenever one of these shows up in your normal work, hop it once and check the result opens:

- [ ] a Claude Code session that **auto-compacted** (by threshold)
- [ ] a session that compacted **twice**
- [ ] a Claude Code session after a **rewind**, and a Pi session after **`/tree`** navigation
- [ ] a session with an **image** going to Claude Code

---

## 15. Claude Code shows a subagent that `hop --children` wrote (5 min) 🟢

New: `hop --children` writes each subagent into the written session's `subagents/` folder, in the
shape Claude Code writes. mukade reads it back; whether Claude Code lists or resumes it is not
checked. No prompt is sent.

**Needs:** a Claude Code session with subagents (`mukade list claude_code --children` shows rows with a
`parent`), and a scratch config folder.

**Do**

```powershell
$env:CLAUDE_CONFIG_DIR = 'C:\work\rehearsal\claude'      # an empty scratch destination
mukade hop claude_code YOUR_CLAUDE_SESSION_ID claude_code --children --root "$HOME\.claude" --cwd 'C:\work\my-project' --manifest C:\work\rehearsal\manifest.json
mukade list claude_code --children                       # the children sit under the new id
Get-ChildItem -Recurse "$env:CLAUDE_CONFIG_DIR\projects" -Filter 'agent-*'    # the files written
cd 'C:\work\my-project'; claude --resume NEW_SESSION_ID  # send nothing
```

In Claude Code, with the session open:

1. Press `Ctrl+O` for the transcript view and scroll to the first spawn call (`Task` or `Agent`).
   The call is text form and ends with a line `→ <child id>`: that id is the child's in `list`.
2. Open `/resume`: the children must **not** appear as sessions of their own.
3. Quit, then run `mukade list claude_code --children` again: the session and its children are
   unchanged.

**Pass if**

- [ ] `list` shows each child with `parent` = the new id and `kind` `spawn`.
- [ ] `Get-ChildItem` shows an `agent-<id>.jsonl` and an `agent-<id>.meta.json` per child under
      `projects\<project>\NEW_SESSION_ID\subagents\`.
- [ ] Claude Code opens the session without an error, and the `→ <child id>` line of step 1 is there.
- [ ] The children are not listed in `/resume`.
- [ ] Anything Claude Code shows of the agents (a block, a count, a refusal) is written down below.

**Write down:** what Claude Code showed, and its version.

---

## When you are done

Send back the ticked list and the "Write down" values. Each passed block becomes a dated line in
`next.md` and a verification record, like the Linux one.
