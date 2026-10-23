; Nivara NSIS installer hooks.
; PREUNINSTALL: require the parent password before allowing uninstall.
; Delegates the prompt + verification to the installed binary
; (`nivara.exe --uninstall-gate`), which returns exit code 0 (allow) / 1 (deny).
; Deterrent only — a determined user can still remove files manually.

!macro NSIS_HOOK_PREUNINSTALL
  ; During a version upgrade Tauri runs the OLD uninstaller silently to remove
  ; the previous files before installing the new ones. Don't demand the password
  ; on that path, or every update would prompt/stall. Only gate a real,
  ; interactive uninstall (Programs & Features). Note: a silent uninstall
  ; (uninstall.exe /S) therefore bypasses the prompt — this is a deterrent, and
  ; that limitation is documented in CLAUDE.md.
  IfSilent nivara_skip_pw_gate 0
  nsExec::ExecToStack '"$INSTDIR\nivara.exe" --uninstall-gate'
  Pop $0
  Pop $1
  ${If} $0 != 0
    MessageBox MB_ICONSTOP "Incorrect or missing parent password. Uninstall cancelled."
    Quit
  ${EndIf}
  nivara_skip_pw_gate:
!macroend

; POSTUNINSTALL: never leave Task Manager / cmd disabled behind (Nivara's
; optional restriction toggle writes these HKCU policy values).
!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Policies\System" "DisableTaskMgr"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Policies\System" "DisableCMD"
!macroend

; PREINSTALL / POSTINSTALL: in-place upgrades overwrite files, so the running
; background monitor must be stopped first (its exe would be locked). If it was
; running, start it again after the new files are in place.
Var NivaraMonWasRunning

!macro NSIS_HOOK_PREINSTALL
  StrCpy $NivaraMonWasRunning 0
  nsExec::ExecToStack 'cmd /c tasklist /FI "IMAGENAME eq nivara-monitor.exe" | find /I "nivara-monitor.exe"'
  Pop $0
  Pop $1
  ${If} $0 = 0
    StrCpy $NivaraMonWasRunning 1
    nsExec::Exec 'taskkill /F /IM nivara-monitor.exe'
    Sleep 1000
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ${If} $NivaraMonWasRunning = 1
    Exec '"$INSTDIR
ivara-monitor.exe"'
  ${EndIf}
!macroend
