@AGENTS.md

For UI work, apply the design file and inspect the project files it references.
Run npx imago-check and npx --no sigrid-check apps/desktop/src before you say UI work is done.

@node_modules/@acaprino/imago/docs/design-file.md

Never run `cargo` (build, test, clippy, check, run, doc), `tauri dev` or `tauri build` on this machine: Rust is built and tested on GitHub Actions only. See "Rust is built and tested on GitHub only" in AGENTS.md. This holds on the owner's own computer; in a cloud session it does not apply (AGENTS.md, "Where the no-build rules apply").

Never compile, build or lint on this machine (`cargo`, `tauri`, `tsc`, `vite build`, bundlers, `npm run build`, `npm run verify`,
`npm install` of a git dependency that runs `prepare`): GitHub Actions does it. Only an explicit request of the owner, in that
request, to build or test locally overrides this. See "Nothing is compiled on this machine" in AGENTS.md. This holds on the owner's own computer; in a cloud session it does not apply (AGENTS.md, "Where the no-build rules apply").
