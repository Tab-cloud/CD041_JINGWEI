; Jingwei Agent NSIS lifecycle hooks.
; Installation and uninstallation must never replace a running workbench or
; either desktop-owned sidecar. We first request a normal process exit, then
; terminate only Jingwei-owned image names that remain, and finally prove that
; every installed executable can be opened for writing before NSIS continues.

!ifndef JINGWEI_WORKBENCH_IMAGE
  !define JINGWEI_WORKBENCH_IMAGE "jingwei-workbench.exe"
!endif
!ifndef JINGWEI_RUNTIME_IMAGE
  !define JINGWEI_RUNTIME_IMAGE "jingwei-agent-runtime-x86_64-pc-windows-msvc.exe"
!endif
!ifndef JINGWEI_DAEMON_IMAGE
  !define JINGWEI_DAEMON_IMAGE "jingwei-daemon-x86_64-pc-windows-msvc.exe"
!endif

Function JingweiInstalledExecutablesLocked
  Push $R0
  Push $R1
  StrCpy $R0 "0"

  IfFileExists "$INSTDIR\${JINGWEI_WORKBENCH_IMAGE}" 0 jingwei_probe_runtime
  ClearErrors
  FileOpen $R1 "$INSTDIR\${JINGWEI_WORKBENCH_IMAGE}" a
  IfErrors jingwei_locked
  FileClose $R1

jingwei_probe_runtime:
  IfFileExists "$INSTDIR\sidecars\bin\${JINGWEI_RUNTIME_IMAGE}" 0 jingwei_probe_daemon
  ClearErrors
  FileOpen $R1 "$INSTDIR\sidecars\bin\${JINGWEI_RUNTIME_IMAGE}" a
  IfErrors jingwei_locked
  FileClose $R1

jingwei_probe_daemon:
  IfFileExists "$INSTDIR\sidecars\bin\${JINGWEI_DAEMON_IMAGE}" 0 jingwei_probe_done
  ClearErrors
  FileOpen $R1 "$INSTDIR\sidecars\bin\${JINGWEI_DAEMON_IMAGE}" a
  IfErrors jingwei_locked
  FileClose $R1
  Goto jingwei_probe_done

jingwei_locked:
  StrCpy $R0 "1"

jingwei_probe_done:
  Pop $R1
  Exch $R0
FunctionEnd

Function JingweiStopInstalledProcesses
  Push $R0

  ; Let the workbench receive a normal close request first. Its ExitRequested
  ; handler shuts down the desktop-owned Runtime and Daemon children.
  nsExec::Exec '"$SYSDIR\taskkill.exe" /T /IM "${JINGWEI_WORKBENCH_IMAGE}"'
  Pop $R0
  Sleep 2500

  ; Older or crashed builds may have orphaned sidecars. Give those processes a
  ; non-forced termination attempt before escalating.
  nsExec::Exec '"$SYSDIR\taskkill.exe" /T /IM "${JINGWEI_RUNTIME_IMAGE}"'
  Pop $R0
  nsExec::Exec '"$SYSDIR\taskkill.exe" /T /IM "${JINGWEI_DAEMON_IMAGE}"'
  Pop $R0
  Sleep 1000

  ; Only Jingwei-owned image names are force-terminated, and only after the
  ; graceful window has elapsed.
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /T /IM "${JINGWEI_WORKBENCH_IMAGE}"'
  Pop $R0
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /T /IM "${JINGWEI_RUNTIME_IMAGE}"'
  Pop $R0
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /T /IM "${JINGWEI_DAEMON_IMAGE}"'
  Pop $R0
  Sleep 1000

  Pop $R0
FunctionEnd

Function JingweiEnsureInstalledProcessesStopped
  Push $R0
  Push $R1

  Call JingweiInstalledExecutablesLocked
  Pop $R0
  StrCmp $R0 "0" jingwei_ready

  IfSilent jingwei_cleanup
  MessageBox MB_OKCANCEL|MB_ICONEXCLAMATION \
    "检测到精卫 Agent 或其后台服务仍在运行。安装程序将先关闭应用并清理 Runtime/Daemon，确认文件释放后再继续。请先保存未提交的编辑。" \
    IDOK jingwei_cleanup IDCANCEL jingwei_cancelled

jingwei_cleanup:
  Call JingweiStopInstalledProcesses
  Call JingweiInstalledExecutablesLocked
  Pop $R0
  StrCmp $R0 "0" jingwei_ready

  IfSilent jingwei_cancelled
  MessageBox MB_RETRYCANCEL|MB_ICONSTOP \
    "精卫 Agent 进程已尝试退出，但安装文件仍被占用。请关闭安全软件或其他占用程序后重试；安装不会忽略该文件。" \
    IDRETRY jingwei_cleanup IDCANCEL jingwei_cancelled

jingwei_cancelled:
  StrCpy $R1 "1"
  Goto jingwei_ensure_done

jingwei_ready:
  StrCpy $R1 "0"

jingwei_ensure_done:
  StrCpy $R0 $R1
  Pop $R1
  Exch $R0
FunctionEnd

; NSIS isolates uninstaller functions under the un. namespace. Keep the same
; fail-closed lifecycle for uninstall instead of falling back to file-level
; Ignore prompts.
Function un.JingweiInstalledExecutablesLocked
  Push $R0
  Push $R1
  StrCpy $R0 "0"

  IfFileExists "$INSTDIR\${JINGWEI_WORKBENCH_IMAGE}" 0 un_jingwei_probe_runtime
  ClearErrors
  FileOpen $R1 "$INSTDIR\${JINGWEI_WORKBENCH_IMAGE}" a
  IfErrors un_jingwei_locked
  FileClose $R1

un_jingwei_probe_runtime:
  IfFileExists "$INSTDIR\sidecars\bin\${JINGWEI_RUNTIME_IMAGE}" 0 un_jingwei_probe_daemon
  ClearErrors
  FileOpen $R1 "$INSTDIR\sidecars\bin\${JINGWEI_RUNTIME_IMAGE}" a
  IfErrors un_jingwei_locked
  FileClose $R1

un_jingwei_probe_daemon:
  IfFileExists "$INSTDIR\sidecars\bin\${JINGWEI_DAEMON_IMAGE}" 0 un_jingwei_probe_done
  ClearErrors
  FileOpen $R1 "$INSTDIR\sidecars\bin\${JINGWEI_DAEMON_IMAGE}" a
  IfErrors un_jingwei_locked
  FileClose $R1
  Goto un_jingwei_probe_done

un_jingwei_locked:
  StrCpy $R0 "1"

un_jingwei_probe_done:
  Pop $R1
  Exch $R0
FunctionEnd

Function un.JingweiStopInstalledProcesses
  Push $R0
  nsExec::Exec '"$SYSDIR\taskkill.exe" /T /IM "${JINGWEI_WORKBENCH_IMAGE}"'
  Pop $R0
  Sleep 2500
  nsExec::Exec '"$SYSDIR\taskkill.exe" /T /IM "${JINGWEI_RUNTIME_IMAGE}"'
  Pop $R0
  nsExec::Exec '"$SYSDIR\taskkill.exe" /T /IM "${JINGWEI_DAEMON_IMAGE}"'
  Pop $R0
  Sleep 1000
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /T /IM "${JINGWEI_WORKBENCH_IMAGE}"'
  Pop $R0
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /T /IM "${JINGWEI_RUNTIME_IMAGE}"'
  Pop $R0
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /T /IM "${JINGWEI_DAEMON_IMAGE}"'
  Pop $R0
  Sleep 1000
  Pop $R0
FunctionEnd

Function un.JingweiEnsureInstalledProcessesStopped
  Push $R0
  Push $R1
  Call un.JingweiInstalledExecutablesLocked
  Pop $R0
  StrCmp $R0 "0" un_jingwei_ready

  IfSilent un_jingwei_cleanup
  MessageBox MB_OKCANCEL|MB_ICONEXCLAMATION \
    "检测到精卫 Agent 或其后台服务仍在运行。卸载程序将先关闭应用并清理 Runtime/Daemon。请先保存未提交的编辑。" \
    IDOK un_jingwei_cleanup IDCANCEL un_jingwei_cancelled

un_jingwei_cleanup:
  Call un.JingweiStopInstalledProcesses
  Call un.JingweiInstalledExecutablesLocked
  Pop $R0
  StrCmp $R0 "0" un_jingwei_ready

  IfSilent un_jingwei_cancelled
  MessageBox MB_RETRYCANCEL|MB_ICONSTOP \
    "精卫 Agent 进程已尝试退出，但卸载文件仍被占用。请关闭安全软件或其他占用程序后重试；卸载不会忽略该文件。" \
    IDRETRY un_jingwei_cleanup IDCANCEL un_jingwei_cancelled

un_jingwei_cancelled:
  StrCpy $R1 "1"
  Goto un_jingwei_ensure_done

un_jingwei_ready:
  StrCpy $R1 "0"

un_jingwei_ensure_done:
  StrCpy $R0 $R1
  Pop $R1
  Exch $R0
FunctionEnd

!macro JINGWEI_ENSURE_STOPPED_OR_ABORT FUNCTION_NAME
  Call ${FUNCTION_NAME}
  Pop $R0
  StrCmp $R0 "0" +3
  SetErrorLevel 2
  Abort
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro JINGWEI_ENSURE_STOPPED_OR_ABORT JingweiEnsureInstalledProcessesStopped
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro JINGWEI_ENSURE_STOPPED_OR_ABORT un.JingweiEnsureInstalledProcessesStopped
!macroend
