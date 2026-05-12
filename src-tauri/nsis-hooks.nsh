; Brewinget NSIS uninstall hook
; Removes the Windows Scheduled Task created by the auto-update feature
; so it doesn't orphan in Task Scheduler after the app is uninstalled.
;
; We use the explicit $SYSDIR path (= SysWOW64 in the 32-bit NSIS installer)
; and PowerShell so the task deletion is handled by the same mechanism that
; created it (Register-ScheduledTask).  The outer NSIS string uses double
; quotes so $SYSDIR expands and $\" gives a literal " for PowerShell -Command.
; -ErrorAction SilentlyContinue keeps the uninstaller from stalling when the
; user never enabled auto-update scheduling.

!macro NSIS_HOOK_PREUNINSTALL
  ExecWait "$SYSDIR\WindowsPowerShell\v1.0\powershell.exe -NonInteractive -NoProfile -Command $\"Unregister-ScheduledTask -TaskName 'Brewinget Auto-Update' -Confirm:$$false -ErrorAction SilentlyContinue$\""
!macroend
