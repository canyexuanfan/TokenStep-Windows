@echo off
REM Sign the built TokenStep exe + NSIS installer with the self-signed
REM code-signing certificate (CN subject contains "TokenStep").
REM
REM Prereq: the certificate must exist in Cert:\CurrentUser\My.
REM To (re)create it, see windows\docs\SIGNING.md.
REM
REM Usage: scripts\sign.bat [thumbprint]
REM   If thumbprint omitted, looks up the cert by subject.

setlocal enabledelayedexpansion
set "THUMB=%~1"

REM Locate signtool (newest Windows Kits x64 version).
set "SIGNTOOL="
for /f "delims=" %%P in ('where /r "C:\Program Files (x86)\Windows Kits\10\bin" signtool.exe 2^>nul ^| findstr x64') do set "SIGNTOOL=%%P"
if not defined SIGNTOOL (
    echo [error] signtool.exe not found under Windows Kits.
    exit /b 1
)

REM If no thumbprint given, find it by subject via PowerShell.
if "%THUMB%"=="" (
    for /f "usebackq tokens=*" %%T in (`powershell -NoProfile -Command "(Get-ChildItem Cert:\CurrentUser\My | Where-Object {$_.Subject -like '*TokenStep*'} | Select-Object -First 1 -ExpandProperty Thumbprint)"`) do set "THUMB=%%T"
)
if "%THUMB%"=="" (
    echo [error] No self-signed cert found in Cert:\CurrentUser\My with subject matching TokenStep.
    echo        Generate one first ^(see docs\SIGNING.md^), or pass its thumbprint as an argument.
    exit /b 1
)
echo [sign] Using certificate thumbprint: %THUMB%

set "ROOT=%~dp0..\.."
set "EXE=%ROOT%\windows\src-tauri\target\release\tokenstep.exe"
set "SETUP=%ROOT%\windows\src-tauri\target\release\bundle\nsis\TokenStep_0.1.0_x64-setup.exe"

set "RC=0"
if exist "%EXE%" (
    echo [sign] Signing %EXE%
    "%SIGNTOOL%" sign /sha1 %THUMB% /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /d "TokenStep" /du "https://github.com/Backtthefuture/TokenStep" "%EXE%" || set "RC=1"
) else (
    echo [warn] exe not found: %EXE%
)

if exist "%SETUP%" (
    echo [sign] Signing %SETUP%
    "%SIGNTOOL%" sign /sha1 %THUMB% /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /d "TokenStep Installer" /du "https://github.com/Backtthefuture/TokenStep" "%SETUP%" || set "RC=1"
) else (
    echo [warn] installer not found: %SETUP%
)

if "%RC%"=="0" (
    echo === Signing complete ===
    "%SIGNTOOL%" verify /pa /all "%SETUP%" 2>nul
) else (
    echo [error] One or more files failed to sign.
)
endlocal & exit /b %RC%
