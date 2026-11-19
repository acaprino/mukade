#!/usr/bin/env sh
# Build Mukade and assemble the portable package in ./release (build-release.bat for Windows):
#   release/mukade-ui        the desktop app (Tauri, the UI embedded)
#   release/mukade           the command line
# Both sit in one folder: they share release/mukade-data (exports, manifests, the chat service).
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$SCRIPT_DIR"

command -v npm >/dev/null || { echo "npm is required (Node.js 22+)"; exit 1; }
command -v cargo >/dev/null || { echo "cargo is required (Rust)"; exit 1; }

echo "[1/4] Frontend dependencies"
[ -d node_modules/@tauri-apps/cli ] || npm ci

echo "[2/4] Command line (mukade, release profile)"
cargo build --release --locked -p mukade --bin mukade

echo "[3/4] Desktop app (mukade-ui, the UI embedded)"
(cd apps/desktop && npx tauri build --no-bundle --features custom-protocol)

echo "[4/4] Copy to release"
mkdir -p release
# Only the programs are replaced: release/mukade-data, if it is there, is the user's and stays.
cp target/release/mukade release/mukade
cp target/release/mukade-ui release/mukade-ui
cp docs/desktop.md release/LEGGIMI-desktop.md
cp LICENSE release/LICENSE

printf '\nOK. Ready:\n  %s/release/mukade-ui\n  %s/release/mukade\nData: %s/release/mukade-data (made on first use)\n' \
  "$SCRIPT_DIR" "$SCRIPT_DIR" "$SCRIPT_DIR"
