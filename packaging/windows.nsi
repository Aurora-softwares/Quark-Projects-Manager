Unicode true
Name "Quark Projects"
OutFile "dist\QuarkProjects-Windows-Setup.exe"
InstallDir "$LOCALAPPDATA\Programs\Quark Projects"
RequestExecutionLevel user
Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles
Section "Install"
  SetOutPath "$INSTDIR"
  File "target\release\quark-projects-manager.exe"
  CreateDirectory "$SMPROGRAMS\Quark Projects"
  CreateShortcut "$SMPROGRAMS\Quark Projects\Quark Projects.lnk" "$INSTDIR\quark-projects-manager.exe"
  CreateShortcut "$DESKTOP\Quark Projects.lnk" "$INSTDIR\quark-projects-manager.exe"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
SectionEnd
Section "Uninstall"
  Delete "$DESKTOP\Quark Projects.lnk"
  Delete "$SMPROGRAMS\Quark Projects\Quark Projects.lnk"
  RMDir "$SMPROGRAMS\Quark Projects"
  Delete "$INSTDIR\quark-projects-manager.exe"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
SectionEnd
