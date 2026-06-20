import subprocess, shutil, time, os

EXE = r'F:\zcode\projects\tokenstep\TokenStep\windows\src-tauri\target\release\tokenstep.exe'
SETUP = r'F:\zcode\projects\tokenstep\TokenStep\windows\src-tauri\target\release\bundle\nsis\TokenStep_0.1.1_x64-setup.exe'
ROOT_EXE = r'F:\zcode\projects\tokenstep\TokenStep\TokenStep.exe'
ROOT_SETUP = r'F:\zcode\projects\tokenstep\TokenStep\TokenStep_0.1.1_x64-setup.exe'

subprocess.run(['powershell','-NoProfile','-Command','Get-Process -Name tokenstep -ErrorAction SilentlyContinue | Stop-Process -Force'], capture_output=True)
time.sleep(2)

signtool = r'C:\Program Files (x86)\Windows Kits\10\bin\x64\signtool.exe'
tp = 'A9E2372BC217D83C27717553132091130C953074'
for f in [EXE, SETUP]:
    subprocess.run([signtool, 'sign', '/sha1', tp, '/fd', 'SHA256', '/tr', 'http://timestamp.digicert.com', '/td', 'SHA256', '/d', 'TokenStep', '/du', 'https://github.com/Backtthefuture/TokenStep', f], capture_output=True, text=True)
print('signed')

shutil.copy2(EXE, ROOT_EXE)
shutil.copy2(SETUP, ROOT_SETUP)
print('done')
os.remove(__file__)
