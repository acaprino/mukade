@AGENTS.md

For UI work, apply the design file and inspect the project files it references.
Run npx imago-check before you say UI work is done.

@node_modules/@acaprino/imago/docs/design-file.md

Never run `cargo` (build, test, clippy, check, run, doc), `tauri dev` or `tauri build` on this machine: Rust is built and tested on GitHub Actions only. See "Rust is built and tested on GitHub only" in AGENTS.md.
