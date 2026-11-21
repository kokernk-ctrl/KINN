; kinn.nsi — NSIS installer for the kinn CLI (Windows)
; Built by .github/workflows/windows.yml with makensis.
; Version is kept in sync with the compiler workspace version.
; Uses only NSIS built-ins (TextFunc, WinMessages) — no third-party plugins.

!include "TextFunc.nsh"
!include "WinMessages.nsh"

!define NAME "KINN"
!define VERSION "0.2.0"
!define OUTFILE "kinn-0.2.0-x86_64-setup.exe"
!define ENVHKLM "SYSTEM\CurrentControlSet\Control\Session Manager\Environment"

Name "${NAME} ${VERSION}"
OutFile "${OUTFILE}"
InstallDir "$PROGRAMFILES64\KINN"
InstallDirRegKey HKLM "Software\KINN" "InstallDir"
RequestExecutionLevel admin
Unicode true

Page directory
Page instfiles

UninstPage uninstConfirm
UninstPage instfiles

Section "KINN CLI (required)"
  ; Paths are relative to scripts/windows/ (where makensis runs).
  SetOutPath "$INSTDIR\bin"
  File "..\..\compiler\target\release\kinn.exe"

  SetOutPath "$INSTDIR\examples"
  File "..\..\examples\hello.kin"
  File "..\..\examples\loops.kin"
  File "..\..\examples\arrays.kin"
  File "..\..\examples\for_loops.kin"
  File "..\..\examples\casts.kin"
  File "..\..\examples\stdlib.kin"

  SetOutPath "$INSTDIR\docs"
  File "..\..\docs\*.md"
  File "..\..\README.md"

  ; Append to the system PATH (deduplicated when the entry already exists).
  ReadRegStr $0 HKLM "${ENVHKLM}" "Path"
  ${WordReplace} "$0" ";$INSTDIR\bin" "" "E+1" $1
  StrCpy $0 "$1;$INSTDIR\bin"
  WriteRegExpandStr HKLM "${ENVHKLM}" "Path" "$0"

  ; Tell running processes the environment changed.
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000

  WriteRegStr HKLM "Software\KINN" "InstallDir" "$INSTDIR"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\KINN" \
    "DisplayName" "KINN ${VERSION}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\KINN" \
    "UninstallString" "$INSTDIR\uninstall.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\KINN" \
    "DisplayVersion" "${VERSION}"
SectionEnd

Section "Uninstall"
  ; Remove our PATH entry.
  ReadRegStr $0 HKLM "${ENVHKLM}" "Path"
  ${WordReplace} "$0" ";$INSTDIR\bin" "" "E+1" $1
  WriteRegExpandStr HKLM "${ENVHKLM}" "Path" "$1"
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000

  Delete "$INSTDIR\bin\kinn.exe"
  Delete "$INSTDIR\uninstall.exe"
  RMDir /r "$INSTDIR\examples"
  RMDir /r "$INSTDIR\docs"
  RMDir "$INSTDIR\bin"
  RMDir "$INSTDIR"
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\KINN"
  DeleteRegKey HKLM "Software\KINN"
SectionEnd
