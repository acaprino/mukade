# Mukade: rules for agents

- Two branches, and only two: `main` (what is released) and `develop` (where work is integrated).
- Work always on `develop`, or on a short branch from `develop` that is merged back into `develop` and then
  deleted. Never merge or push into `main`, and leave no other branch, locally or on GitHub.
- A release is the pull request `develop` → `main`, opened and merged by the owner: every merge into `main`
  is a release (`.github/workflows/release.yml`, the same in Shinatama, Mukade, Xiox, Muro, Ethel and
  artcraft-web).
  The version goes up by a patch, or by a minor or major step when the pull request has the label
  `release:minor` or `release:major`; `build-release.bat` builds on Windows; the tag `v<version>` and the GitHub
  Release (the installer of `release\installer\` and one portable zip with all of `release\` in a folder; no loose
  program) are published, and `main` is merged back into
  `develop`. Nothing is published when the build fails.
- Never change the version or create release tags by hand: the workflow does it (`.github/release.json` lists
  the version files, `.github/scripts/release.mjs` writes them).

Before UI work, read node_modules/@acaprino/imago/docs/design-file.md and the component or style files it references.
Apply its tokens and confirmed design choices within the requested scope.
Use the package's im- classes and --im- tokens. Build a control of your own from the tokens, never from literal colours or sizes.
Report conflicts and keep unresolved choices explicit.
Run npx imago-check before you say UI work is done.

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
