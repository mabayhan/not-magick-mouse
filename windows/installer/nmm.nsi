; Installer for not-magick-mouse, built with NSIS (`makensis`), which runs on Linux.
; Build it with windows/installer/package.sh; it passes these defines:
;   VERSION, BIN_DIR (folder with nmm.exe + nmm-gui.exe), README, ICON, OUTFILE
;
; Silent install:  setup.exe /S
;   /D=D:\Tools\nmm           install elsewhere (must be last)
;   /CONFIGPATH=D:\nmm\c.toml shortcuts and autostart pass --config to the app
;   /AUTOSTART=1              also enable "Start with Windows"
; Silent uninstall: "C:\Program Files\not-magick-mouse\uninstall.exe" /S

Unicode true
SetCompressor /SOLID lzma
RequestExecutionLevel admin
ManifestDPIAware true

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"

!define APP "not-magick-mouse"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP}"

Name "${APP}"
OutFile "${OUTFILE}"
InstallDir "$PROGRAMFILES64\${APP}"
InstallDirRegKey HKLM "Software\${APP}" "InstallDir"
BrandingText "${APP} ${VERSION}"
VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${APP}"
VIAddVersionKey "FileDescription" "${APP} installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "not-magick-mouse contributors"

!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"
!define MUI_ABORTWARNING

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
; Launched through explorer.exe so the app runs as the normal user, not elevated.
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_TEXT "Start ${APP} now"
!define MUI_FINISHPAGE_RUN_FUNCTION LaunchApp
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

Var ConfigArg

Function LaunchApp
  Exec '"$WINDIR\explorer.exe" "$SMPROGRAMS\${APP}\${APP}.lnk"'
FunctionEnd

Section "Application" SecApp
  SectionIn RO
  SetRegView 64
  SetShellVarContext all

  ; Upgrading over a running copy: stop it so the files can be replaced.
  nsExec::Exec 'taskkill /F /IM nmm.exe /IM nmm-gui.exe'
  Pop $0
  Sleep 300

  SetOutPath "$INSTDIR"
  File "${BIN_DIR}\nmm.exe"
  File "${BIN_DIR}\nmm-gui.exe"
  File "${README}"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateDirectory "$SMPROGRAMS\${APP}"
  CreateShortcut "$SMPROGRAMS\${APP}\${APP}.lnk" "$INSTDIR\nmm.exe" "$ConfigArg" "$INSTDIR\nmm.exe" 0
  CreateShortcut "$SMPROGRAMS\${APP}\${APP} Settings.lnk" "$INSTDIR\nmm-gui.exe" "$ConfigArg" "$INSTDIR\nmm-gui.exe" 0

  ; Settings > Apps entry.
  WriteRegStr HKLM "Software\${APP}" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayName" "${APP}"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${UNINST_KEY}" "Publisher" "not-magick-mouse contributors"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\nmm.exe"
  WriteRegStr HKLM "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKLM "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKLM "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${UNINST_KEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKLM "${UNINST_KEY}" "EstimatedSize" "$0"
SectionEnd

; Off by default (/o). HKCU Run of the installing user.
Section /o "Start with Windows" SecAutostart
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${APP}" '"$INSTDIR\nmm.exe" $ConfigArg'
SectionEnd

; Defined after the sections so ${SecAutostart} exists.
Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "${APP} needs 64-bit Windows."
    Abort
  ${EndIf}
  SetRegView 64

  ; Optional custom config file, passed on as --config.
  StrCpy $ConfigArg ""
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "/CONFIGPATH=" $R1
  ${IfNot} ${Errors}
    StrCpy $ConfigArg '--config "$R1"'
  ${EndIf}

  ; /AUTOSTART=1 selects the optional section (needed for silent installs).
  ClearErrors
  ${GetOptions} $R0 "/AUTOSTART=" $R1
  ${IfNot} ${Errors}
  ${AndIf} $R1 == "1"
    SectionSetFlags ${SecAutostart} ${SF_SELECTED}
  ${EndIf}
FunctionEnd

LangString DESC_App ${LANG_ENGLISH} "The tray daemon, the settings GUI and Start Menu shortcuts."
LangString DESC_Autostart ${LANG_ENGLISH} "Start the tray daemon when you sign in."
!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecApp} $(DESC_App)
  !insertmacro MUI_DESCRIPTION_TEXT ${SecAutostart} $(DESC_Autostart)
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Function un.onInit
  SetRegView 64
FunctionEnd

; The config file (~/.config/not-magick-mouse) is never removed.
Section "Uninstall"
  SetRegView 64
  SetShellVarContext all
  nsExec::Exec 'taskkill /F /IM nmm.exe /IM nmm-gui.exe'
  Pop $0
  Sleep 300

  Delete "$INSTDIR\nmm.exe"
  Delete "$INSTDIR\nmm-gui.exe"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS\${APP}\${APP}.lnk"
  Delete "$SMPROGRAMS\${APP}\${APP} Settings.lnk"
  RMDir "$SMPROGRAMS\${APP}"

  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${APP}"
  DeleteRegKey HKLM "${UNINST_KEY}"
  DeleteRegKey HKLM "Software\${APP}"
SectionEnd
