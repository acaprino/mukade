@AGENTS.md

For UI work, apply the design file and inspect the project files it references.
Run npx imago-check before you say UI work is done.
Every app of the suite has Sigrid's one look: use only the design system's components, never a component, class or
stylesheet of your own; a missing part is added to Sigrid. See "One look for the whole suite" in AGENTS.md.

@node_modules/@acaprino/imago/docs/design-file.md

Never run `cargo` (build, test, clippy, check, run, doc), `tauri dev` or `tauri build` on this machine: Rust is built and tested on GitHub Actions only. See "Rust is built and tested on GitHub only" in AGENTS.md.

Never compile, build or lint on this machine (`cargo`, `tauri`, `tsc`, `vite build`, bundlers, `npm run build`, `npm run verify`,
`npm install` of a git dependency that runs `prepare`): GitHub Actions does it. Only an explicit request of the owner, in that
request, to build or test locally overrides this. See "Nothing is compiled on this machine" in AGENTS.md.
