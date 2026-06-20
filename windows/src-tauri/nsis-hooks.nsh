; NSIS hook: force the uninstaller icon to match the installer icon.
; Tauri 2 leaves UNINSTALLERICON empty, so MUI_UNICON is never set and the
; uninstaller falls back to the NSIS default. This injects MUI_UNICON so
; uninstall.exe shows our brand icon.
!ifdef MUI_ICON
  !ifndef MUI_UNICON
    !define MUI_UNICON "${MUI_ICON}"
  !endif
!endif
