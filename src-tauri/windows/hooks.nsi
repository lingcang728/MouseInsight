; Ask a running instance to quit gracefully before CheckIfAppIsRunning falls
; back to a hard kill: `--quit` is forwarded through the single-instance
; channel, so the live process runs its full key-release shutdown. The wait
; gives that path a beat to finish; anything still alive afterwards hits the
; default kill prompt.
!macro MI_QuitRunningInstance
  ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
    !if "${INSTALLMODE}" == "currentUser"
      nsis_tauri_utils::FindProcessCurrentUser "${MAINBINARYNAME}.exe"
    !else
      nsis_tauri_utils::FindProcess "${MAINBINARYNAME}.exe"
    !endif
    Pop $R0
    ${If} $R0 = 0
      ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --quit'
      Sleep 1500
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro MI_QuitRunningInstance
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro MI_QuitRunningInstance
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "Mouse Insight"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Mouse Insight"
  RMDir /r "$APPDATA\MouseInsight\logs"
  Delete "$APPDATA\MouseInsight\config.json.*.tmp"
  Delete "$APPDATA\MouseInsight\held-keys.*.tmp"
  Delete "$APPDATA\MouseInsight\held-keys.json"
!macroend
