@echo off
rem Build Mukade and assemble the portable package in .\release:
rem   release\mukade-desktop.exe   the desktop app (Tauri, the UI embedded)
rem   release\mukade.exe           the command line
rem Both sit in one folder on purpose: they share release\mukade-data (exports, manifests, the chat
rem service's files), so the app reads the chat that `mukade chat start` runs from there.
rem Usage: build-release.bat   (then run release\mukade-desktop.exe)
setlocal
cd /d "%~dp0"

where npm >nul 2>nul || (echo npm is required ^(Node.js 22+^). & goto fail)
where cargo >nul 2>nul || (echo cargo is required ^(Rust^). & goto fail)

echo [1/4] Frontend dependencies
if not exist "node_modules\@tauri-apps\cli" (
    call npm ci || goto fail
)

echo [2/4] Command line ^(mukade.exe, release profile^)
cargo build --release --locked -p mukade --bin mukade || goto fail

echo [3/4] Desktop app ^(mukade-desktop.exe, the UI embedded^)
rem `tauri build` runs `npm run build` first (beforeBuildCommand) and embeds apps\desktop\dist.
pushd apps\desktop
call npx tauri build --no-bundle --features custom-protocol
set "TAURI_EXIT=%errorlevel%"
popd
if not "%TAURI_EXIT%"=="0" goto fail

echo [4/4] Copy to release
if not exist "release" mkdir "release"
rem Only the programs are replaced: release\mukade-data, if it is there, is the user's and stays.
copy /y "target\release\mukade.exe" "release\mukade.exe" >nul || (echo Could not copy mukade.exe. & goto fail)
copy /y "target\release\mukade-desktop.exe" "release\mukade-desktop.exe" >nul || (echo Could not copy mukade-desktop.exe. & goto fail)
copy /y "docs\desktop.md" "release\LEGGIMI-desktop.md" >nul
copy /y "LICENSE" "release\LICENSE" >nul

echo.
echo OK. Ready:
echo   "%CD%\release\mukade-desktop.exe"   ^(the app; needs the WebView2 runtime, built into Windows 11^)
echo   "%CD%\release\mukade.exe"           ^(the command line: list, hop, dump, write, chat ...^)
echo Data: "%CD%\release\mukade-data" ^(made on first use, beside both programs^)
echo.
pause
exit /b 0

:fail
echo.
echo BUILD FAILED. Read the error above.
echo.
pause
exit /b 1
