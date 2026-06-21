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
REM version before installing the new one. Users found this confusing; we want
REM the new version to silently overwrite the old one (settings live outside
REM the install dir, so they're preserved). The template defines
REM `Function PageReinstall` as the page's SHOW callback — calling Abort
REM inside a SHOW callback skips the page entirely, and the install Section
REM then overwrites files in place (no uninstall). Tauri offers no hook point
REM before this page, so we patch the generated installer.nsi and re-run
REM makensis to regenerate the setup.exe.
set "NSI=%CD%\target\release\nsis\x64\installer.nsi"
if exist "%NSI%" (
    echo [patch] Skipping reinstall/uninstall confirmation page in installer.nsi
    powershell -NoProfile -ExecutionPolicy Bypass -Command "$f='%NSI%'; $c=Get-Content -Raw $f; if ($c -notmatch '(?m)^Function PageReinstall\r?\n  ; Uninstall previous WiX') { Write-Host '[patch] marker not found, skipping'; exit 0 }; $c = $c -replace '(?m)^(Function PageReinstall\r?\n)', \"`$1  ; [TokenStep patch] skip this page entirely (silent overwrite, no uninstall prompt)`r`n  Abort`r`n\"; Set-Content -NoNewline $f $c; Write-Host '[patch] applied'" || (
        echo [warn] NSIS patch failed; continuing with unpatched installer.
    )
    REM Re-run makensis to regenerate setup.exe with the patch applied.
    set "MAKENSIS=%LOCALAPPDATA%\tauri\NSIS\nsis.exe"
    if not exist "!MAKENSIS!" set "MAKENSIS=%LOCALAPPDATA%\tauri\NSIS\Bin\makensis.exe"
    if exist "!MAKENSIS!" (
        echo [patch] Re-running makensis to rebuild setup.exe
        "!MAKENSIS!" "%NSI%" || echo [warn] makensis re-run failed; keeping original setup.exe
    ) else (
        echo [warn] makensis not found; keeping original setup.exe
    )
) else (
    echo [warn] installer.nsi not found at %NSI%; skipping patch
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
