; Brewinget NSIS uninstall hooks
; Removes the Windows Scheduled Task created by the auto-update feature
; so it doesn't orphan in Task Scheduler after the app is uninstalled.

!macro NSIS_HOOK_PREUNINSTALL
  ; /f suppresses the "are you sure?" prompt; error is ignored if task
  ; doesn't exist (user never enabled auto-update scheduling).
  ExecWait 'schtasks /delete /tn "Brewinget Auto-Update" /f'
!macroend
