@echo off
rem Mukade: build mukade-ui.exe (desktop app, the UI embedded) and mukade.exe (command line),
rem assemble them in .\release, install into D:\Software\Mukade,
rem clean what the build staged. The programs sit in one folder on purpose: they share mukade-data
rem beside them (exports, manifests, the chat service's files), so the app reads the chat that
rem `mukade chat start` runs. Chat refuses a data folder inside the clone: use the install.
rem
rem   build-release.bat                   build, .\release, install, clean
rem   build-release.bat --no-install      stop at .\release
rem   build-release.bat --target <dir>    install folder (default D:\Software\Mukade)
rem   build-release.bat --ci              accepted for compatibility; every release uses npm ci
rem   build-release.bat --jobs N          parallel Rust compilations (CARGO_BUILD_JOBS)
rem
rem The same script, with the same steps and options, is in Shinatama, Mukade, Xiox, Muro and
rem Ethel: only the PROJECT parts (the settings below and the subroutines at the end) differ.
rem   1. tools, version, npm dependencies in NPM_DIR (npm ci from the lockfile)
rem   2. build                                                          PROJECT :build
rem   3. .\release made from nothing: the programs and their files      PROJECT :assemble
rem      then release\installer\<App>-<version>-setup.exe from it (.github\scripts\installer.iss,
rem      Inno Setup 6; without it a build here goes on with no installer, in CI it is an error)
rem   4. install: .\release is copied over D:\Software\Mukade, made if missing. What only the
rem      install holds (data, mukade-data, webview ...) stays as it is; nothing is backed up.
rem      release\installer\, where there is one, holds installers to hand out: it is not installed.
rem      While a program of the install folder is running, it waits for it to close.
rem   5. clean: what the build staged is removed, on success and on failure. PROJECT :clean_project
rem      Cargo's target\ stays (the build cache: the next build is incremental), node_modules stays.
rem
rem Errors always leave through a label: with setlocal on, "exit /b 1" inside a parenthesized
rem block returns 0 to whoever ran the script.

setlocal EnableExtensions
set "ROOT=%~dp0"
set "SELF=%~nx0"
set "RELEASE=%ROOT%release"
cd /d "%ROOT%" || goto :fail

rem ==== PROJECT: settings ========================================================================
set "APP=Mukade"
rem The program that must be in .\release once it is assembled.
set "APP_EXE=mukade-ui.exe"
rem Tools beyond cargo, node, npm and git.
set "TOOLS="
rem Parallel Rust jobs when neither CARGO_BUILD_JOBS nor --jobs is given (empty: cargo decides).
set "DEFAULT_JOBS="
rem The folder (relative to the repo) with package.json and node_modules.
set "NPM_DIR=."
rem ===============================================================================================

set "INSTALL=D:\Software\%APP%"
set "DO_INSTALL=1"

:args
if "%~1"=="" goto :args_done
if /i "%~1"=="--no-install" (set "DO_INSTALL=0" & shift & goto :args)
if /i "%~1"=="--ci" (shift & goto :args)
if /i "%~1"=="--target" goto :arg_target
if /i "%~1"=="--jobs" goto :arg_jobs
goto :usage
:arg_target
if "%~2"=="" goto :usage
set "INSTALL=%~f2"
shift & shift & goto :args
:arg_jobs
if "%~2"=="" goto :usage
set "CARGO_BUILD_JOBS=%~2"
shift & shift & goto :args
:args_done
if "%INSTALL:~-1%"=="\" set "INSTALL=%INSTALL:~0,-1%"
if not defined CARGO_BUILD_JOBS if defined DEFAULT_JOBS set "CARGO_BUILD_JOBS=%DEFAULT_JOBS%"

rem --- 1. Tools and dependencies -----------------------------------------------------------------
echo.
echo [1/5] Tools and dependencies
for %%T in (cargo node npm git %TOOLS%) do (
  where %%T >nul 2>nul || (echo [ERROR] %%T not found in PATH. & goto :fail)
)
rem Tauri's build scripts compile the Windows resources with rc.exe (embed-resource). Its own Windows SDK
rem lookup can name an rc.exe that is not there, and only RC_<target> or RC come before it: unless
rem RC_x86_64_pc_windows_msvc is already set, point it at the rc.exe of the newest SDK that has one.
if not defined RC_x86_64_pc_windows_msvc call :find_rc
set "VERSION=?"
rem The version of the release system: its first version file (.github\release.json).
for /f "usebackq delims=" %%V in (`node .github\scripts\release.mjs current`) do set "VERSION=%%V"
set "COMMIT=?"
for /f "usebackq delims=" %%C in (`git rev-parse --short HEAD`) do set "COMMIT=%%C"
set "DIRTY="
git diff --quiet HEAD -- 2>nul || set "DIRTY=, uncommitted changes"
echo     %APP% %VERSION%, commit %COMMIT%%DIRTY%
if defined CARGO_BUILD_JOBS echo     %CARGO_BUILD_JOBS% parallel Rust jobs
if "%DO_INSTALL%"=="1" echo     install into %INSTALL%
if not "%DO_INSTALL%"=="1" echo     no install: .\release only

pushd "%NPM_DIR%" || goto :fail
call npm ci
set "RC=%ERRORLEVEL%"
popd
if not "%RC%"=="0" goto :fail

rem --- 2. Build ----------------------------------------------------------------------------------
echo.
echo [2/5] Build
call :build
if errorlevel 1 goto :fail

rem --- 3. Release folder -------------------------------------------------------------------------
echo.
echo [3/5] %RELEASE%
if exist "%RELEASE%\" rmdir /s /q "%RELEASE%"
if exist "%RELEASE%\" goto :release_busy
call :assemble
if errorlevel 1 goto :fail
if not exist "%RELEASE%\%APP_EXE%" goto :release_missing
call :installer
if errorlevel 1 goto :fail

rem --- 4. Install --------------------------------------------------------------------------------
echo.
if "%DO_INSTALL%"=="1" goto :do_install
echo [4/5] Install skipped ^(--no-install^)
goto :installed
:do_install
echo [4/5] Install into %INSTALL%
call :install
if errorlevel 1 goto :fail
:installed

rem --- 5. Clean ----------------------------------------------------------------------------------
echo.
echo [5/5] Clean
call :clean_project

echo.
echo === Done: %APP% %VERSION% (%COMMIT%%DIRTY%)
echo     release    %RELEASE%
if "%DO_INSTALL%"=="1" echo     installed  %INSTALL%\%APP_EXE%
call :pause_if_clicked
endlocal
exit /b 0

rem --- Common subroutines ------------------------------------------------------------------------

:install
if not exist "%INSTALL%\" mkdir "%INSTALL%" || goto :sub_fail
set "SHOWN="
:install_wait
set "RUNNING=0"
for /f "usebackq delims=" %%N in (`powershell -NoProfile -NonInteractive -Command "$d = [IO.Path]::GetFullPath('%INSTALL%') + '\'; @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.Path -and $_.Path.StartsWith($d, [StringComparison]::OrdinalIgnoreCase) }).Count"`) do set "RUNNING=%%N"
if "%RUNNING%"=="0" goto :install_copy
if not defined SHOWN echo     %APP% is running from %INSTALL%: close it, the install goes on by itself.
set "SHOWN=1"
ping -n 4 127.0.0.1 >nul
goto :install_wait
:install_copy
robocopy "%RELEASE%" "%INSTALL%" /E /XD "%RELEASE%\installer" /R:10 /W:2 /NFL /NDL /NJH /NP
if errorlevel 8 (echo [ERROR] copy into %INSTALL% failed: a file may still be in use. & goto :sub_fail)
exit /b 0

:installer
rem release\installer\<App>-<version>-setup.exe from release\ (.github\scripts\installer.iss, Inno Setup 6). Without
rem Inno Setup a build on this machine goes on with no installer; in CI it is an error: the release hands it out.
set "ISCC="
for /f "delims=" %%I in ('where ISCC.exe 2^>nul') do if not defined ISCC set "ISCC=%%I"
for %%I in ("%ProgramFiles(x86)%\Inno Setup 6\ISCC.exe" "%ProgramFiles%\Inno Setup 6\ISCC.exe" "%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe") do if not defined ISCC if exist "%%~I" set "ISCC=%%~I"
if defined ISCC goto :installer_build
if defined CI (echo [ERROR] Inno Setup 6 ^(ISCC.exe^) not found: the release needs its installer. & goto :sub_fail)
echo     installer skipped: Inno Setup 6 not found ^(winget install JRSoftware.InnoSetup^)
exit /b 0
:installer_build
"%ISCC%" /Q "/DApp=%APP%" "/DVersion=%VERSION%" "/DExe=%APP_EXE%" "/DSource=%RELEASE%" "/O%RELEASE%\installer" "%ROOT%.github\scripts\installer.iss"
if errorlevel 1 (echo [ERROR] the installer was not built. & goto :sub_fail)
for %%F in ("%RELEASE%\installer\*") do echo     installer\%%~nxF  %%~zF bytes
exit /b 0

:find_rc
set "KITS=%ProgramFiles(x86)%\Windows Kits\10\bin"
set "RC_DIR="
for /f "delims=" %%D in ('dir /b /ad /o-n "%KITS%\10.*" 2^>nul') do if not defined RC_DIR if exist "%KITS%\%%D\x64\rc.exe" set "RC_DIR=%KITS%\%%D\x64"
if not defined RC_DIR echo     rc.exe not found in a Windows SDK: Tauri builds that recompile their resources will fail.
if defined RC_DIR set "RC_x86_64_pc_windows_msvc=%RC_DIR%\rc.exe"
if defined RC_DIR echo     rc.exe: %RC_DIR%\rc.exe
exit /b 0

:pause_if_clicked
rem Started with a double-click from Explorer: keep the window open to read the result. Never in CI.
if defined CI exit /b 0
echo %CMDCMDLINE% | "%SystemRoot%\System32\find.exe" /i "%SELF%" >nul && pause
exit /b 0

:sub_fail
exit /b 1

:release_busy
echo [ERROR] %RELEASE% could not be removed: a file in it is in use.
goto :fail

:release_missing
echo [ERROR] %RELEASE%\%APP_EXE% was not produced.
goto :fail

:usage
echo [ERROR] invalid arguments: %*
echo Usage: %SELF% [--no-install] [--target ^<folder^>] [--ci] [--jobs N]
call :pause_if_clicked
endlocal
exit /b 1

:fail
echo.
echo [ERROR] %APP% build-release stopped: read the error above.
call :clean_project
call :pause_if_clicked
endlocal
exit /b 1

rem ==== PROJECT: build, release folder, clean ===================================================
rem Each subroutine returns 0 when done; errors go to :sub_fail. The working folder is the repo.

:build
echo     mukade.exe ^(command line, release^)
cargo build --release --locked -p mukade --bin mukade || goto :sub_fail
echo     mukade-ui.exe ^(Tauri, release, custom-protocol; the page through beforeBuildCommand^)
pushd "apps\desktop" || goto :sub_fail
call "%ROOT%node_modules\.bin\tauri.cmd" build --no-bundle --features custom-protocol -- --locked
set "RC=%ERRORLEVEL%"
popd
if not "%RC%"=="0" goto :sub_fail
exit /b 0

:assemble
mkdir "%RELEASE%" || goto :sub_fail
copy /y "target\release\mukade-ui.exe" "%RELEASE%\" >nul || goto :sub_fail
copy /y "target\release\mukade.exe" "%RELEASE%\" >nul || goto :sub_fail
copy /y "docs\desktop.md" "%RELEASE%\README-desktop.md" >nul || goto :sub_fail
copy /y "LICENSE" "%RELEASE%\LICENSE" >nul || goto :sub_fail
for %%F in ("%RELEASE%\*") do echo     %%~nxF  %%~zF bytes
exit /b 0

:clean_project
rem The build stages nothing outside cargo's target\ and .\release.
exit /b 0
