; CloudStream Windows x64 installer (egui shell).
; Built by scripts/build_release.ps1 with makensis.
; Accepts Tauri-style updater flags (/P /R /UPDATE) as a silent reinstall.

!ifndef VERSION
  !error "VERSION must be defined (/DVERSION=x.y.z)"
!endif
!ifndef PAYLOAD_DIR
  !error "PAYLOAD_DIR must be defined (/DPAYLOAD_DIR=...)"
!endif
!ifndef OUT_FILE
  !error "OUT_FILE must be defined (/DOUT_FILE=...)"
!endif

Name "CloudStream"
OutFile "${OUT_FILE}"
InstallDir "$PROGRAMFILES64\CloudStream"
RequestExecutionLevel admin
Unicode true
SetCompressor /SOLID lzma

!include "MUI2.nsh"
!include "FileFunc.nsh"

!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Var SilentUpdate

Function .onInit
  StrCpy $SilentUpdate 0
  ${GetParameters} $0
  ${GetOptions} $0 "/P" $1
  IfErrors +2 0
    StrCpy $SilentUpdate 1
  ${GetOptions} $0 "/S" $1
  IfErrors +2 0
    StrCpy $SilentUpdate 1
  ${GetOptions} $0 "/UPDATE" $1
  IfErrors +2 0
    StrCpy $SilentUpdate 1
  StrCmp $SilentUpdate 1 0 +2
    SetSilent silent
FunctionEnd

Section "Install"
  SetOutPath "$INSTDIR"
  File /r "${PAYLOAD_DIR}\*.*"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateDirectory "$SMPROGRAMS\CloudStream"
  CreateShortCut "$SMPROGRAMS\CloudStream\CloudStream.lnk" "$INSTDIR\cloudstream.exe"
  CreateShortCut "$DESKTOP\CloudStream.lnk" "$INSTDIR\cloudstream.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\CloudStream" "DisplayName" "CloudStream"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\CloudStream" "DisplayIcon" "$INSTDIR\cloudstream.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\CloudStream" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\CloudStream" "UninstallString" "$INSTDIR\Uninstall.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\CloudStream" "InstallLocation" "$INSTDIR"
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\CloudStream" "NoModify" 1
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\CloudStream" "NoRepair" 1
SectionEnd

Section "Uninstall"
  Delete "$SMPROGRAMS\CloudStream\CloudStream.lnk"
  RMDir "$SMPROGRAMS\CloudStream"
  Delete "$DESKTOP\CloudStream.lnk"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir /r "$INSTDIR"
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\CloudStream"
SectionEnd
