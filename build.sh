#!/usr/bin/env sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$SCRIPT_DIR"

# The whole workspace: the mukade CLI and the mukade-tui terminal browser.
cargo build --release --locked --workspace
printf '\nBuild completata:\n  %s/target/release/mukade\n  %s/target/release/mukade-tui\n' "$SCRIPT_DIR" "$SCRIPT_DIR"
