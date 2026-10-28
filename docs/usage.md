# Using Mukade with each harness

This guide covers the command line on Windows PowerShell. Paths, session IDs, project IDs,
model names and provider names in the examples are **illustrative**: replace them with values
from your installation. On macOS or Linux, use the corresponding paths and `export NAME=value`
instead of `$env:NAME = 'value'`.

## Before you start

1. Build Mukade with `cargo build --release` and add `target\release` to `PATH`, or use the full
   path to `mukade.exe`. Install and configure the harnesses you intend to use separately.
2. Identify the source session and the **existing project directory** it belongs to. Conversion
   creates a new destination session; it does not move or modify the source session.
3. Set the appropriate source and destination store variables in the same PowerShell session as
   Mukade. Claude Code, Codex and Pi require an explicit root, even for standard installations.
   OpenCode may use its configured store, but supply its database and environment explicitly when
   isolation matters.
4. Run `mukade list pi`, `mukade list claude_code` or `mukade list codex` to find an ID. OpenCode
   listing through Mukade is not implemented; use its own session listing instead.
5. Examine the JSON receipt: `id`, `location`, `resume` and `manifest`, including `losses`. A
   `--resume-prompt` executes the destination harness and may make a model/API call. Without it,
   Mukade produces a resume command but does not run it. The resume runs in the destination's
   working directory (`--cwd`, or the source's recorded directory), which must exist on this machine.

The examples below use full IDs represented by `YOUR_*_SESSION_ID`. Do not type the placeholder
literally. `--cwd` chooses the destination project directory; if omitted, Mukade uses the source
session's recorded directory, when present.

## Store and executable selection

| Harness | CLI name | Store/config variable | Executable flag | Executable environment variable |
|---|---|---|---|---|
| Claude Code | `claude_code` | `CLAUDE_CONFIG_DIR` | `--claude-exe` | `MUKADE_CLAUDE_EXE` |
| Codex CLI | `codex` | `CODEX_HOME` | `--codex-exe` | `MUKADE_CODEX_EXE` |
| OpenCode | `opencode` | `OPENCODE_DB` (database file), plus its applicable `XDG_*` settings | `--opencode-exe` | `MUKADE_OPENCODE_EXE` |
| Pi | `pi` | `PI_CODING_AGENT_DIR` | `--pi-exe` | `MUKADE_PI_EXE` |

An executable override points to the **launcher or executable**, not its installation directory.
The CLI resolves the destination executable using its flag, then its `MUKADE_*_EXE` variable,
then `PATH`, then known installation locations, and finally the bare command name. The version
probe uses that resolution; an explicitly configured executable is also put into the returned
resume command. OpenCode is special: when it is the **source**, its export reader uses
`MUKADE_OPENCODE_EXE` or default resolution, not the `--opencode-exe` flag. The flag applies to
OpenCode as a destination.

`--root` changes the selected harness's store for `list`, `dump` or `write`. For `hop`, it applies
to the **source**, not a separate destination root; if source and destination are the same harness,
they share that overridden root. Set the destination variable in the environment. For OpenCode,
an existing directory passed as `--root` means its `opencode.db`; a file path is used as-is.

A `--resume-prompt` runs the destination with its store variable set to the root the session was
written to, whether it came from the environment or from `--root`. A resume command you run later
by hand does not inherit that: set the destination's variable in that shell first. For OpenCode,
also configure the appropriate `XDG_*` and `OPENCODE_CONFIG` settings where needed; a database path
alone is not a guarantee of complete isolation.

A resume prompt that starts with `-` is refused, because every harness CLI would read it as an
option; so is one with line breaks when the destination's executable is a `.cmd` launcher, which
cannot take them. Both are refused before anything is written.

## 1. Claude Code

### Standard location: read Claude and write Pi

The following paths illustrate common user-home locations; use the actual homes configured for
your harnesses. Select the source ID from the `list` output.

```powershell
$env:CLAUDE_CONFIG_DIR = Join-Path $HOME '.claude'
$env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'
mukade list claude_code
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project'
```

### Custom Claude destination

Here Pi is the source and a separate Claude home and executable are the destination. Both variables
must match the installations you intend to use.

```powershell
$env:PI_CODING_AGENT_DIR = 'D:\portable\pi\agent'
$env:CLAUDE_CONFIG_DIR = 'D:\portable\claude\config'
mukade list pi
mukade hop pi YOUR_PI_SESSION_ID claude_code --cwd 'C:\work\my-project' --claude-exe 'D:\portable\claude\bin\claude.exe'
```

Keep `CLAUDE_CONFIG_DIR` set when executing the returned `resume` arguments. The launcher and paths
are examples; Mukade does not install Claude or create its authentication configuration.

## 2. Codex CLI

### Standard location: read Codex and write Pi

```powershell
$env:CODEX_HOME = Join-Path $HOME '.codex'
$env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'
mukade list codex
mukade hop codex YOUR_CODEX_SESSION_ID pi --cwd 'C:\work\my-project'
```

### Custom Codex destination

```powershell
$env:CLAUDE_CONFIG_DIR = 'D:\portable\claude\config'
$env:CODEX_HOME = 'D:\portable\codex\home'
mukade list claude_code
mukade hop claude_code YOUR_CLAUDE_SESSION_ID codex --cwd 'C:\work\my-project' --codex-exe 'D:\portable\codex\bin\codex.exe'
```

Codex requires a destination CLI version in its session metadata. Mukade tries the chosen
executable's `--version`; if that probe fails or cannot parse a version, supply the actual version
with `--dest-version 0.164.0` (replace the example with your version). Mukade reads
`model_provider` from `$env:CODEX_HOME\config.toml` when available; `--model-provider` can state
it explicitly: only the top-level `model_provider` key counts, not a profile's or a provider
table's. Keep `CODEX_HOME` set for a resume command run later.

## 3. OpenCode

### Configured installation: read OpenCode and write Pi

Use OpenCode's own listing to obtain the ID. Run its listing from the relevant project directory;
Mukade's `list opencode` currently returns an unsupported-listing error.

```powershell
$env:OPENCODE_DB = 'C:\path\to\opencode.db'
$env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'
Set-Location 'C:\work\my-project'
opencode session list --standalone
mukade hop opencode YOUR_OPENCODE_SESSION_ID pi --cwd 'C:\work\my-project'
```

Mukade calls `opencode session export ... --standalone` to read the source. Set the `XDG_*`
variables used by your OpenCode installation in this shell as well, if applicable.

### Custom OpenCode destination

The database, configuration, XDG directories and executable below must belong to the intended
OpenCode installation. The project ID, provider and model must already be valid for that installation.

```powershell
$env:CLAUDE_CONFIG_DIR = 'D:\portable\claude\config'
$env:OPENCODE_DB = 'D:\portable\opencode\data\opencode.db'
$env:XDG_DATA_HOME = 'D:\portable\opencode\data'
$env:XDG_CONFIG_HOME = 'D:\portable\opencode\config'
$env:XDG_STATE_HOME = 'D:\portable\opencode\state'
$env:XDG_CACHE_HOME = 'D:\portable\opencode\cache'
$env:OPENCODE_CONFIG = 'D:\portable\opencode\config\opencode.json'
mukade hop claude_code YOUR_CLAUDE_SESSION_ID opencode --cwd 'C:\work\my-project' --opencode-exe 'D:\portable\opencode\opencode.exe' --project-id YOUR_OPENCODE_PROJECT_ID --model-provider YOUR_PROVIDER --model YOUR_MODEL
```

OpenCode's import needs a project ID and a complete provider/model reference. Mukade can sometimes
discover these from OpenCode's existing sessions and configuration; explicit values avoid relying
on discovery. The project-ID probe inherits the process environment, so set `OPENCODE_DB` and
`XDG_*` there even if another command uses `--root`. OpenCode imports with `--standalone`.

For a **custom OpenCode source**, configure the same environment and set its executable through the
environment variable before exporting:

```powershell
$env:MUKADE_OPENCODE_EXE = 'D:\portable\opencode\opencode.exe'
$env:PI_CODING_AGENT_DIR = 'D:\portable\pi\agent'
mukade hop opencode YOUR_OPENCODE_SESSION_ID pi --cwd 'C:\work\my-project'
```

For an OpenCode destination, the receipt's `location` names the database the session was imported
into (`OPENCODE_DB`), or says it went to OpenCode's configured store; the temporary import document
is removed after the import, on success and on failure. Use the returned ID with that store. Keep
the OpenCode environment configured when using the returned resume command.

## 4. Pi

### Standard location: read Pi and write Claude Code

```powershell
$env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'
$env:CLAUDE_CONFIG_DIR = Join-Path $HOME '.claude'
mukade list pi
mukade hop pi YOUR_PI_SESSION_ID claude_code --cwd 'C:\work\my-project'
```

### Portable Pi destination

```powershell
$env:CLAUDE_CONFIG_DIR = 'D:\portable\claude\config'
$env:PI_CODING_AGENT_DIR = 'D:\portable\pi\agent'
mukade list claude_code
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project' --pi-exe 'D:\portable\pi\bin\pi.cmd'
```

Mukade writes the new Pi session below `PI_CODING_AGENT_DIR\sessions`. The receipt's `resume`
contains the chosen launcher and the session file path. Keep `PI_CODING_AGENT_DIR` set when running
it; separately verify the portable Pi installation's provider credentials and configuration. A
separate agent directory does not by itself prove that every resource Pi reads is isolated.

## Discover a store, export and import

`detect` inspects store evidence; it does **not** identify an installation directory or configure an
executable. It can return zero or multiple candidates. An empty or ambiguous root cannot be inferred;
pass the harness explicitly when you know it. For a nonstandard OpenCode database filename, pass
`opencode` and its file path explicitly rather than relying on detection.

```powershell
mukade detect 'D:\portable\pi\agent'
mukade list --root 'D:\portable\pi\agent'
mukade dump pi YOUR_PI_SESSION_ID --root 'D:\portable\pi\agent' --output 'C:\work\session.json'
$env:CLAUDE_CONFIG_DIR = 'D:\portable\claude\config'
mukade write claude_code 'C:\work\session.json' --cwd 'C:\work\my-project' --claude-exe 'D:\portable\claude\bin\claude.exe'
```

Write the document with `--output`, not through a pipe: PowerShell re-encodes piped text with the
console's code page, and non-ASCII text (an em dash, accented letters) would reach `write` already
garbled. Exported interchange documents contain conversation data; protect them as you would the
original session. `write` requires an explicit destination harness and returns a JSON receipt. The destination
must have its own configuration and credentials; Mukade does not transfer them.

## Troubleshooting

- **No store or session found:** check the variable, `--root`, source ID and whether the intended
  project/store contains that session. Use `mukade detect <path>` for diagnostic evidence.
- **Ambiguous or empty detection:** specify the harness explicitly; a newly created empty store has
  no reliable session layout to infer.
- **Cannot run the executable:** verify the launcher path, its runtime and permissions. A custom
  OpenCode source requires `MUKADE_OPENCODE_EXE`; a destination uses its corresponding flag or env var.
- **Codex version or OpenCode project/model missing:** provide the actual destination version,
  project ID, provider or model using the flags described above.
- **Resume fails after a successful write:** the session already exists. Mukade still prints its
  receipt on stdout and saves `--manifest`, then exits with code 5: use that `id` and `location`
  instead of retrying, which would create another session. A resume that outlives its timeout is
  stopped, including a harness behind a `.cmd` launcher. Check configuration and credentials in the
  process environment. Provider availability and real portable integrations must be verified in
  your own environment.
- **Manifest could not be saved:** the session was still written, and its receipt is on stdout; the
  error names its location. Save the `manifest` field of that receipt instead of running again.
