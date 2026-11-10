@echo off
setlocal
cd /d "%~dp0"

rem The whole workspace: the mukade CLI and the mukade-tui terminal browser.
cargo build --release --locked --workspace
set "BUILD_EXIT=%errorlevel%"

echo.
if not "%BUILD_EXIT%"=="0" (
    echo Build FALLITA ^(codice %BUILD_EXIT%^). Leggi l'errore qui sopra.
) else (
    echo Build completata:
    echo   "%CD%\target\release\mukade.exe"
    echo   "%CD%\target\release\mukade-tui.exe"
)

rem Keep the window open when launched with a double-click from Explorer.
echo.
pause
endlocal & exit /b %BUILD_EXIT%
