#!/usr/bin/env sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$SCRIPT_DIR"

cargo build --release --locked
printf '\nBuild completata: %s/target/release/mukade\n' "$SCRIPT_DIR"
