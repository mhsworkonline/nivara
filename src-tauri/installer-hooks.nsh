; Nivara NSIS installer hooks.
; PREUNINSTALL: require the parent password before allowing uninstall.
; Delegates the prompt + verification to the installed binary
; (`nivara.exe --uninstall-gate`), which returns exit code 0 (allow) / 1 (deny).
; Deterrent only — a determined user can still remove files manually.

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToStack '"$INSTDIR\nivara.exe" --uninstall-gate'
  Pop $0
  Pop $1
  ${If} $0 != 0
    MessageBox MB_ICONSTOP "Incorrect or missing parent password. Uninstall cancelled."
    Quit
  ${EndIf}
!macroend
