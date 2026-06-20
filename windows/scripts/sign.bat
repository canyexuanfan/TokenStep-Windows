@echo off
REM Sign the built TokenStep exe + NSIS installer with the self-signed
REM code-signing certificate (CN subject contains "TokenStep"), then copy
REM versioned artifacts to the repository root.
REM
REM Prereq: the certificate must exist in Cert:\CurrentUser\My.
REM To (re)create it, see windows\docs\SIGNING.md.
REM
REM Usage: scripts\sign.bat [thumbprint]
REM   If thumbprint omitted, looks up the cert by subject.
REM
REM Artifact naming convention (see windows\docs\PACKAGING.md):
REM   <root>\TokenStep.exe                      latest standalone (no version)
REM   <root>\TokenStep_v<ver>.exe               versioned standalone
REM   <root>\TokenStep_<ver>_x64-setup.exe      NSIS installer

setlocal enabledelayedexpansion
set "THUMB=%~1"

REM --- Resolve repo root (..\..  relative to this script) ---
set "ROOT=%~dp0..\.."
pushd "%ROOT%" >nul
set "ROOT=%CD%"
popd >nul

REM --- Read version from tauri.conf.json ---
set "CONF=%ROOT%\windows\src-tauri\tauri.conf.json"
set "VER="
for /f "usebackq tokens=*" %%V in (`powershell -NoProfile -Command "(Get-Content -Raw '%CONF%' | ConvertFrom-Json).version"`) do set "VER=%%V"
if "%VER%"=="" (
    echo [error] Could not read version from %CONF%.
    exit /b 1
)
echo [ver] TokenStep %VER%

REM --- Locate signtool (newest Windows Kits x64 version) ---
set "SIGNTOOL="
for /f "delims=" %%P in ('where /r "C:\Program Files (x86)\Windows Kits\10\bin" signtool.exe 2^>nul ^| findstr x64') do set "SIGNTOOL=%%P"
if not defined SIGNTOOL (
    echo [error] signtool.exe not found under Windows Kits.
    exit /b 1
)

REM --- If no thumbprint given, find it by subject via PowerShell ---
if "%THUMB%"=="" (
    for /f "usebackq tokens=*" %%T in (`powershell -NoProfile -Command "(Get-ChildItem Cert:\CurrentUser\My | Where-Object {$_.Subject -like '*TokenStep*'} | Select-Object -First 1 -ExpandProperty Thumbprint)"`) do set "THUMB=%%T"
)
if "%THUMB%"=="" (
    echo [error] No self-signed cert found in Cert:\CurrentUser\My with subject matching TokenStep.
    echo        Generate one first ^(see docs\SIGNING.md^), or pass its thumbprint as an argument.
    exit /b 1
)
echo [sign] Using certificate thumbprint: %THUMB%

set "BUILD=%ROOT%\windows\src-tauri\target\release"
set "EXE=%BUILD%\tokenstep.exe"

REM The NSIS installer name embeds the version, so resolve it dynamically.
set "SETUP="
for /f "delims=" %%S in ('dir /b "%BUILD%\bundle\nsis\TokenStep_*_x64-setup.exe" 2^>nul ^| findstr /v "uninstall"') do set "SETUP=%BUILD%\bundle\nsis\%%S"
if not defined SETUP (
    echo [warn] No NSIS installer found under %BUILD%\bundle\nsis\.
)

set "RC=0"
if exist "%EXE%" (
    echo [sign] Signing %EXE%
    "%SIGNTOOL%" sign /sha1 %THUMB% /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /d "TokenStep" /du "https://github.com/Backtthefuture/TokenStep" "%EXE%" || set "RC=1"
) else (
    echo [warn] exe not found: %EXE%
    set "RC=1"
)

if defined SETUP (
    echo [sign] Signing %SETUP%
    "%SIGNTOOL%" sign /sha1 %THUMB% /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /d "TokenStep Installer" /du "https://github.com/Backtthefuture/TokenStep" "%SETUP%" || set "RC=1"
) else (
    echo [warn] Installer not found; skipping installer signing.
)

REM --- Deploy versioned artifacts to repo root ---
echo [deploy] Copying artifacts to %ROOT%
if exist "%EXE%" (
    copy /y "%EXE%" "%ROOT%\TokenStep.exe" >nul
    copy /y "%EXE%" "%ROOT%\TokenStep_v%VER%.exe" >nul
    echo        - TokenStep.exe
    echo        - TokenStep_v%VER%.exe
)
if defined SETUP (
    copy /y "%SETUP%" "%ROOT%\TokenStep_%VER%_x64-setup.exe" >nul
    echo        - TokenStep_%VER%_x64-setup.exe
)

if not "%RC%"=="0" goto :sign_failed
echo === Signing + deploy complete. TokenStep %VER% ===
endlocal & exit /b 0

:sign_failed
echo [error] One or more files failed to sign.
endlocal & exit /b 1
