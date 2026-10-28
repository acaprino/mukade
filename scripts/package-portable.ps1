# Builds mukade and lays out its portable bundle: one folder (and a zip of it) to copy anywhere.
#
#   powershell -File scripts\package-portable.ps1 [-Out dist]
#
# The bundle is mukade.exe plus an empty `mukade.portable` file. That file switches portable mode
# on: everything mukade keeps for itself then lives in `mukade-data\` beside the exe, and nothing
# is written to the user's profile or the system's temporary folder. Delete the folder and mukade
# is gone.
param([string]$Out = "dist")

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

    $name = "mukade-portable-windows-x64"
    $bundle = Join-Path (Join-Path $root $Out) $name
    if (Test-Path $bundle) { Remove-Item $bundle -Recurse -Force }
    New-Item -ItemType Directory -Force $bundle | Out-Null

    Copy-Item (Join-Path $root "target\release\mukade.exe") $bundle
    Copy-Item (Join-Path $root "LICENSE") $bundle
    Set-Content -Path (Join-Path $bundle "mukade.portable") -Encoding ascii -Value @(
        "This file switches mukade to portable mode.",
        "Everything mukade keeps for itself goes to the mukade-data folder next to mukade.exe.",
        "Nothing is written to the user profile or the system temporary folder.")
    Set-Content -Path (Join-Path $bundle "README.txt") -Encoding ascii -Value @(
        "mukade - move a coding-agent session between Claude Code, Codex CLI, OpenCode and Pi.",
        "",
        "Nothing to install: run mukade.exe from this folder, from a USB stick, from anywhere.",
        "  mukade.exe --help",
        "  mukade.exe hop claude_code <session id> pi --cwd <project folder>",
        "",
        "Portable mode: mukade.portable sits next to mukade.exe, so mukade keeps its own files",
        "(a small cache, temporary files) in the mukade-data folder here and nowhere else.",
        "Delete this folder and nothing is left behind.",
        "",
        "Full guide: https://github.com/acaprino/mukade/blob/master/docs/usage.md")

    $zip = Join-Path (Join-Path $root $Out) "$name.zip"
    if (Test-Path $zip) { Remove-Item $zip -Force }
    Compress-Archive -Path $bundle -DestinationPath $zip
    Write-Host "bundle: $bundle"
    Write-Host "zip:    $zip"
}
finally {
    Pop-Location
}
