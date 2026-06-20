; NSIS hook: force the uninstaller icon to match the installer icon.
; Tauri 2 leaves UNINSTALLERICON="" so the uninstaller shows the NSIS default.
; We define MUI_UNICON here with the icon path. The path is the same absolute
; path Tauri uses for INSTALLERICON (line 42 of the generated installer.nsi).
!ifndef MUI_UNICON
  !define MUI_UNICON "F:\zcode\projects\tokenstep\TokenStep\windows\src-tauri\icons\icon.ico"
!endif
