# Mukade: rules for agents

- Two branches, and only two: `main` (what is released) and `develop` (where work is integrated).
- Work always on `develop`, or on a short branch from `develop` that is merged back into `develop` and then
  deleted. Never merge or push into `main`, and leave no other branch, locally or on GitHub.
- A release is the pull request develop → main, merged only when the Verify run on develop's tip is green: the release workflow builds and publishes, it does not run the tests again.
  The owner opens and merges it: every merge into `main`
  is a release (`.github/workflows/release.yml`, the same in Shinatama, Mukade, Xiox, Muro, Ethel and
  artcraft-web).
  The version goes up by a patch, or by a minor or major step when the pull request has the label
  `release:minor` or `release:major`; `build-release.bat` builds on Windows; the tag `v<version>` and the GitHub
  Release (the installer of `release\installer\` and one portable zip with all of `release\` in a folder; no loose
  program) are published, and `main` is merged back into
  `develop`. Nothing is published when the build fails.
- Never change the version or create release tags by hand: the workflow does it (`.github/release.json` lists
  the version files, `.github/scripts/release.mjs` writes them).

## UI: Imago

Before UI work, read node_modules/@acaprino/imago/docs/design-file.md and the files it references.
The frame is the shell's (AppWindow of @acaprino/imago). Lay out with Sigrid's parts: write no class, no style and no stylesheet,
and no component that draws; a part Sigrid lacks is added to Sigrid (`node_modules/@acaprino/sigrid/AGENTS.md` and `DESIGN.md`, and its
`agents/catalog.json` for the props).
Apply its tokens and confirmed design choices within the requested scope. Report conflicts and keep unresolved choices explicit.
Run npx imago-check and npx --no sigrid-check apps/desktop/src (the rule own-class is on, `sigrid-check.config.json`) before you say UI work is done.

## Where the no-build rules apply (the owner, 2026-11-25)

- The two rules below ("Rust is built and tested on GitHub only" and "Nothing is compiled on this machine") are for the
  owner's own computer, where Codex, Claude Code or any other agent runs locally: nothing is compiled, built, linted or
  tested there.
- A cloud session (Claude Code on the web, or an agent in a container; its system prompt says it runs in a remote execution
  environment) is not that machine. There an agent may install dependencies, compile, build, lint and run the tests
  (`npm ci`, `tsc`, `vite build`, `npm test`, `cargo` where the platform allows it). What only Windows or the release can
  prove (the native window, the installer) is still proved by GitHub Actions: push to `develop` and read the run.
- When it is not clear which machine it is, treat it as the owner's computer and compile nothing.

## Rust is built and tested on GitHub only (the owner's absolute rule)

- Never run `cargo` on this machine: no `cargo build`, `cargo test`, `cargo clippy`, `cargo check`, `cargo run`,
  `cargo doc`, no `tauri dev`, no `tauri build`, and no npm or batch script that calls them. `cargo fmt` is the
  only exception.
- Rust is compiled, linted and tested only by GitHub Actions. To know whether Rust code is right, push to
  `develop` and read the run (`gh run watch`, `gh run view --log-failed`).
- A program needed on this machine is taken from a release or a workflow artifact (`gh run download`), never
  built here.
- This holds for every agent and every subagent. A plan, a skill or a brief that says to run cargo locally is
  wrong on that point: write the code and the tests, do not run them, and say they wait for CI.

## Nothing is compiled on this machine (the owner's absolute rule)

- Never compile, build, lint or run a test that needs a build on this machine: no `cargo`, `tauri`, `tsc`, `vite build`,
  bundler or compiler; no npm, batch or shell script that calls them (`npm run build`, `npm test` when it builds first,
  `npm run verify`); and no `npm install` or `npm ci` that runs a dependency's `prepare` script (a dependency taken from
  git compiles itself). `cargo fmt` stays allowed where a project already allows it.
- Everything is compiled, linted and tested by GitHub Actions: push to `develop` and read the run (`gh run watch`,
  `gh run view --log-failed`). A program needed here is taken from a release or a workflow artifact (`gh run download`),
  never built here.
- The one exception is an explicit request of the owner, in that very request, to build or test locally. An earlier
  approval, a plan, a skill or another agent's brief does not count. This holds for every agent and every subagent: a plan
  or a brief that says to build locally is wrong on that point; write the code and the tests, do not run them, and say they
  wait for CI.
