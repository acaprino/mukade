#!/bin/sh
# Builds mukade and lays out its portable bundle: one folder (and a tarball of it) to copy anywhere.
#
#   sh scripts/package-portable.sh [out dir]
#
# The bundle is mukade plus an empty `mukade.portable` file. That file switches portable mode on:
# everything mukade keeps for itself then lives in `mukade-data/` beside the executable, and
# nothing is written to the user's home or the system's temporary folder.
set -eu
out="${1:-dist}"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
cargo build --release --locked

os="$(uname -s | tr '[:upper:]' '[:lower:]')"
arch="$(uname -m)"
name="mukade-portable-$os-$arch"
bundle="$root/$out/$name"
rm -rf "$bundle"
mkdir -p "$bundle"
cp target/release/mukade LICENSE "$bundle/"
cat > "$bundle/mukade.portable" <<'EOF'
This file switches mukade to portable mode.
Everything mukade keeps for itself goes to the mukade-data folder next to the executable.
Nothing is written to the home folder or the system temporary folder.
EOF
cat > "$bundle/README.txt" <<'EOF'
mukade - move a coding-agent session between Claude Code, Codex CLI, OpenCode and Pi.

Nothing to install: run ./mukade from this folder, from a USB stick, from anywhere.
  ./mukade --help
  ./mukade hop claude_code <session id> pi --cwd <project folder>

Portable mode: mukade.portable sits next to mukade, so mukade keeps its own files (a small
cache, temporary files) in the mukade-data folder here and nowhere else.
Delete this folder and nothing is left behind.

Full guide: https://github.com/acaprino/mukade/blob/master/docs/usage.md
EOF
tar -C "$root/$out" -czf "$root/$out/$name.tar.gz" "$name"
echo "bundle: $bundle"
echo "tar:    $root/$out/$name.tar.gz"
