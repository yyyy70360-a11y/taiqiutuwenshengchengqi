!macro NSIS_HOOK_PREUNINSTALL
  ; fix-shortcut-icons.ps1 keeps a root-level icon for pinned shortcuts.
  ; Remove it during uninstall so the application directory can be removed.
  SetFileAttributes "$INSTDIR\app.ico" NORMAL
  Delete "$INSTDIR\app.ico"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Retry after the generated resource cleanup has run.
  SetFileAttributes "$INSTDIR\app.ico" NORMAL
  Delete "$INSTDIR\app.ico"
  RMDir "$INSTDIR"
!macroend
