!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "Mouse Insight"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Mouse Insight"
  RMDir /r "$APPDATA\MouseInsight\logs"
  Delete "$APPDATA\MouseInsight\config.json.*.tmp"
!macroend
