@echo off
setlocal
cd /d "%~dp0"

cargo build --release --locked
if errorlevel 1 exit /b %errorlevel%

echo.
echo Build completata: "%CD%\target\release\mukade.exe"
endlocal
