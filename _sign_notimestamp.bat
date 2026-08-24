@echo off
setlocal
set "SIGNTOOL="
for /f "delims=" %%P in ('where /r "C:\Program Files (x86)\Windows Kits\10\bin" signtool.exe 2^>nul ^| findstr x64') do set "SIGNTOOL=%%P"
if not defined SIGNTOOL (
    echo [error] signtool not found
    exit /b 1
)
set "THUMB=A9E2372BC217D83C27717553132091130C953074"
set "EXE=F:\zcode\projects\tokenstep\TokenStep\windows\src-tauri\target\release\tokenstep.exe"
set "SETUP=F:\zcode\projects\tokenstep\TokenStep\windows\src-tauri\target\release\bundle\nsis\TokenStep_0.1.9_x64-setup.exe"
echo [sign] %EXE%
"%SIGNTOOL%" sign /sha1 %THUMB% /fd SHA256 /d "TokenStep" "%EXE%"
echo [sign] %SETUP%
"%SIGNTOOL%" sign /sha1 %THUMB% /fd SHA256 /d "TokenStep Installer" "%SETUP%"
echo [deploy] Copying to repo root
copy /y "%EXE%" "F:\zcode\projects\tokenstep\TokenStep\TokenStep.exe" >nul
copy /y "%EXE%" "F:\zcode\projects\tokenstep\TokenStep\TokenStep_v0.1.9.exe" >nul
copy /y "%SETUP%" "F:\zcode\projects\tokenstep\TokenStep\TokenStep_0.1.9_x64-setup.exe" >nul
echo === Done ===
endlocal
