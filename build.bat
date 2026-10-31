@echo off
setlocal
cd /d "%~dp0"

rem The whole workspace: the mukade CLI and the mukade-tui terminal browser.
cargo build --release --locked --workspace
if errorlevel 1 exit /b %errorlevel%

echo.
echo Build completata:
echo   "%CD%\target\release\mukade.exe"
echo   "%CD%\target\release\mukade-tui.exe"
endlocal
