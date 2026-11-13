; Installer hooks (bundle.windows.nsis.installerHooks in tauri.conf.json).

!macro NSIS_HOOK_PREINSTALL
  ; Up to 0.1.33 the app ran as familiar.exe. The installer only closes the exe under its
  ; current name, so close the old one from this folder (and nothing else of that name);
  ; the installer then deletes it.
  nsExec::Exec `powershell -NoProfile -NonInteractive -Command "Get-Process familiar -ErrorAction SilentlyContinue | Where-Object { $$_.Path -eq '$INSTDIR\familiar.exe' } | Stop-Process -Force"`
  Sleep 800
  Delete "$INSTDIR\plugin\adapters\claude-code\bin\familiar-emit.exe"
!macroend
