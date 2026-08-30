@echo off
REM TokenStep for Windows - release build script.
REM Produces an NSIS installer under src-tauri\target\release\bundle\nsis\.
REM
REM Requires: Rust (stable, x86_64-pc-windows-msvc) + Tauri CLI.
REM The Tauri CLI is auto-installed via `cargo tauri` if missing.
REM
REM Post-build step: patches the generated installer.nsi to skip the
REM "uninstall previous version?" confirmation page on upgrade, then re-runs
REM makensis so the bundled setup.exe reflects the patch. See PATCH note below.

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

REM ── PATCH: skip the "reinstall / uninstall old version" confirmation page ──
REM Tauri's NSIS template shows a page asking whether to uninstall the old
REM version before installing the new one. Users found this confusing; we
REM want the new version to silently overwrite the old one (settings live
REM outside the install dir, so they're preserved). The patch itself, the
REM makensis re-run, and the nsis-output.exe → versioned setup.exe rename
REM all live in patch-nsis.ps1 — a separate file because an inline
REM `powershell -Command "... `r`n ..."` gets its escapes mangled by cmd.exe
REM ("此时不应有 `r`n", exit 255 before signing ever runs).
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0patch-nsis.ps1" || (
    echo [warn] NSIS patch step failed; continuing with unpatched installer.
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
