#!/usr/bin/env sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$SCRIPT_DIR"

# The command line. The desktop app builds with npm (docs/desktop.md).
cargo build --release --locked
printf '\nBuild done:\n  %s/target/release/mukade\n' "$SCRIPT_DIR"
