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

## One look for the whole suite (the owner's rule, 2026-11-23)

Every app of the suite (Daodan, Xiox, Shinatama, Mukade, Muro and the others on Sigrid) has the same look and feel:
the same surfaces, shadows, spacing, type and motion, and the same component for the same need. What is the app's own
is its layout, which follows its functions, its accent and its icon.

- The suite's design system is Sigrid (`acaprino/sigrid`). The UI uses only its components: it makes no component of
  its own that draws (no stylesheet, no class, no primitive, no copy or restyling of a part of the design system); the
  app's code composes the design system's components into screens and wires them to its data.
- The app adapts to the design system, never the other way round. Sigrid changes only when a component or a primitive
  that a function needs is really missing: it is then added to Sigrid, generic, for every app. The app's former look
  is no reason to change Sigrid.
- The base of the app is Imago, the suite's shell in Xiox's style (window, title bar, frame, commands, palette, theme,
  notices), built with Sigrid; Sigrid gives the components. The app brings only its screens, made of Sigrid's
  components, and has no shell, window, frame or look of its own. What is missing in the shell is added to Imago, what
  is missing in a component is added to Sigrid. Until Imago's shell on Sigrid exists, the app keeps the shell and the
  parts it has today (Imago's) and adds none of its own.

## UI work (Imago)

Before UI work, read node_modules/@acaprino/imago/docs/design-file.md and the component or style files it references.
Apply its tokens and confirmed design choices within the requested scope.
Use the package's im- classes and --im- tokens; never build a control of your own ("One look for the whole suite").
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
