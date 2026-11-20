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

For desktop UI work, read `node_modules/@acaprino/imago/docs/design-file.md` and the referenced components. Use Imago's controls, behaviours and tokens. Keep workflow-specific components in Mukade and build them from the shared tokens.
