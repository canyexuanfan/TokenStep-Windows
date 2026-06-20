@echo off
REM TokenStep for Windows - release build script.
REM Produces an NSIS installer under src-tauri\target\release\bundle\nsis\.
REM
REM Requires: Rust (stable, x86_64-pc-windows-msvc) + Tauri CLI.
REM The Tauri CLI is auto-installed via `cargo tauri` if missing.

setlocal enabledelayedexpansion
cd /d "%~dp0\..\src-tauri"

echo === TokenStep Windows build ===

REM Ensure the Tauri CLI is available; install if not.
where tauri >nul 2>&1
if errorlevel 1 (
    echo [setup] Tauri CLI not found on PATH; installing via cargo...
    cargo install tauri-cli --version "^2.0" || (
        echo [error] Failed to install Tauri CLI.
        exit /b 1
    )
)

echo [build] Compiling release bundle (this may take several minutes)...
cargo tauri build || (
    echo [error] Build failed.
    exit /b 1
)

REM Sign the exe + installer with the self-signed cert (if present).
echo [sign] Attempting to sign the build...
call "%~dp0sign.bat" || echo [warn] Signing skipped or failed (continuing).

echo.
echo === Build complete ===
echo Installer(s) written to:
dir /b "%CD%\target\release\bundle\nsis\*.exe" 2>nul

REM Show the final repo-root artifacts (signed + versioned) from sign.bat.
echo.
echo Final release artifacts in repo root:
set "ROOT=%~dp0..\.."
pushd "%ROOT%" >nul
for %%F in (TokenStep.exe TokenStep_v*.exe TokenStep_*_x64-setup.exe) do (
    if exist "%%F" echo   - %%F
)
popd >nul
echo.
echo See windows\docs\PACKAGING.md for the naming convention.
endlocal
