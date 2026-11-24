; n0te for Windows: one installer for n0te Laptop, n0te Tablet and the n0te-sync helper.
;
; Per user, no administrator rights: %LOCALAPPDATA%\Programs\n0te, registry under HKCU only.
; Built by packaging/windows/package.sh (makensis 3.08 or newer, on Linux or Windows):
;
;   makensis -DVERSION=0.32.0 -DARCH=x64 -DSTAGE=<staged files> -DOUTFILE=<setup.exe> n0te.nsi
;
; STAGE holds n0te-laptop.exe, n0te-tablet.exe, n0te-sync.exe, pdfium.dll, LICENSE.txt,
; THIRD_PARTY_NOTICES.txt and licenses\ (package.sh puts them there).
;
; Never touched, also not by the uninstaller: the library in %APPDATA%\n0te (notebooks, pairing,
; history), which belongs to the user.

Unicode true
ManifestDPIAware true
ManifestSupportedOS all
RequestExecutionLevel user
SetCompressor /SOLID lzma

!ifndef VERSION
  !error "VERSION is missing: makensis -DVERSION=0.32.0 ..."
!endif
!ifndef ARCH
  !error "ARCH is missing: -DARCH=x64 or -DARCH=arm64"
!endif
!ifndef STAGE
  !error "STAGE is missing: -DSTAGE=<folder with the files to install>"
!endif
!ifndef OUTFILE
  !define OUTFILE "n0te-${VERSION}-windows-${ARCH}-setup.exe"
!endif
; VIProductVersion needs four plain numbers: 0.32.0-beta.1 -> 0.32.0.0 (package.sh passes it).
!ifndef VERSION4
  !define VERSION4 "${VERSION}.0"
!endif

!define APP "n0te"
!define PUBLISHER "DiscoveryFox"
!define URL "https://github.com/DiscoveryFox/n0te"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\n0te"
!define RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define CLASSES "Software\Classes"
!define PROGID "n0te.Notebook"

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "Sections.nsh"
!include "FileFunc.nsh"
!include "x64.nsh"
!include "WinVer.nsh"

Name "${APP}"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\n0te"
InstallDirRegKey HKCU "Software\n0te" "InstallDir"

; Self-updates (the apps' updater runs "setup.exe /S /UPDATE /RELAUNCH=laptop|tablet"):
; /UPDATE keeps the programs that are installed and closes running n0te windows first;
; /RELAUNCH= starts that program again when the update is in.
Var Relaunch
BrandingText "n0te ${VERSION} (${ARCH})"
ShowInstDetails show
ShowUninstDetails show

; --- pages ---------------------------------------------------------------------------------

; Compile-time paths use "/" (makensis on Linux and Windows); install-time paths "\".
!define MUI_ICON "../icons/n0te.ico"
!define MUI_UNICON "../icons/n0te.ico"
!define MUI_ABORTWARNING
!define MUI_COMPONENTSPAGE_SMALLDESC
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_TEXT "$(RunNow)"
!define MUI_FINISHPAGE_RUN_FUNCTION RunApp

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${STAGE}/LICENSE.txt"
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; The first language is the fallback; Windows' display language picks German when it is German.
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "German"

LangString DescLaptop ${LANG_ENGLISH} "n0te Laptop: notebooks with keyboard and mouse (typing, PDFs, graphs, live with the tablet)."
LangString DescLaptop ${LANG_GERMAN} "n0te Laptop: Notizbücher mit Tastatur und Maus (Tippen, PDFs, Graphen, live mit dem Tablet)."
LangString DescTablet ${LANG_ENGLISH} "n0te Tablet: for pen and touch devices (preview: the pen tools are still being built)."
LangString DescTablet ${LANG_GERMAN} "n0te Tablet: für Geräte mit Stift und Touch (Vorschau: die Stift-Werkzeuge entstehen noch)."
LangString DescAutostart ${LANG_ENGLISH} "Start n0te sync when you sign in, so notebooks stay in sync with your other devices while n0te is closed."
LangString DescAutostart ${LANG_GERMAN} "n0te sync bei der Anmeldung starten: Notizbücher bleiben mit deinen anderen Geräten im Abgleich, auch wenn n0te zu ist."
LangString NameAutostart ${LANG_ENGLISH} "Sync at sign-in"
LangString NameAutostart ${LANG_GERMAN} "Sync bei der Anmeldung"
LangString RunNow ${LANG_ENGLISH} "Start n0te now"
LangString RunNow ${LANG_GERMAN} "n0te jetzt starten"
LangString NeedWin10 ${LANG_ENGLISH} "n0te needs Windows 10 or 11."
LangString NeedWin10 ${LANG_GERMAN} "n0te braucht Windows 10 oder 11."
LangString Need64 ${LANG_ENGLISH} "n0te needs a 64-bit Windows."
LangString Need64 ${LANG_GERMAN} "n0te braucht ein 64-Bit-Windows."
LangString NeedArm ${LANG_ENGLISH} "This is the installer for ARM64 PCs (Snapdragon and similar). Please use n0te-${VERSION}-windows-x64-setup.exe on this PC."
LangString NeedArm ${LANG_GERMAN} "Das ist der Installer für ARM64-PCs (Snapdragon und ähnliche). Auf diesem PC bitte n0te-${VERSION}-windows-x64-setup.exe nehmen."
LangString PreferArm ${LANG_ENGLISH} "This PC has an ARM64 processor: n0te-${VERSION}-windows-arm64-setup.exe runs faster and uses less battery. Install the x64 version anyway?"
LangString PreferArm ${LANG_GERMAN} "Dieser PC hat einen ARM64-Prozessor: n0te-${VERSION}-windows-arm64-setup.exe läuft schneller und spart Akku. Trotzdem die x64-Version installieren?"
LangString NotebookType ${LANG_ENGLISH} "n0te notebook"
LangString NotebookType ${LANG_GERMAN} "n0te-Notizbuch"

VIProductVersion "${VERSION4}"
VIAddVersionKey /LANG=${LANG_ENGLISH} "ProductName" "n0te"
VIAddVersionKey /LANG=${LANG_ENGLISH} "ProductVersion" "${VERSION}"
VIAddVersionKey /LANG=${LANG_ENGLISH} "FileVersion" "${VERSION}"
VIAddVersionKey /LANG=${LANG_ENGLISH} "FileDescription" "n0te Setup (${ARCH})"
VIAddVersionKey /LANG=${LANG_ENGLISH} "CompanyName" "${PUBLISHER}"
VIAddVersionKey /LANG=${LANG_ENGLISH} "LegalCopyright" "Copyright (C) 2026 ${PUBLISHER}. All rights reserved."

; --- sections ------------------------------------------------------------------------------

; Always: the sync helper, pdfium, the licenses, the uninstaller and its entry in Settings > Apps.
Section "-n0te sync" SecCore
  SetOutPath "$INSTDIR"
  ; A running helper holds its exe open: stop it (it is started again at the end).
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM n0te-sync.exe'
  Pop $0
  Sleep 300
  File "${STAGE}/n0te-sync.exe"
  File "${STAGE}/pdfium.dll"
  File "${STAGE}/LICENSE.txt"
  File "${STAGE}/THIRD_PARTY_NOTICES.txt"
  SetOutPath "$INSTDIR\licenses"
  File /r "${STAGE}/licenses/*"
  SetOutPath "$INSTDIR"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "Software\n0te" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\n0te" "Version" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "n0te"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "${PUBLISHER}"
  WriteRegStr HKCU "${UNINST_KEY}" "URLInfoAbout" "${URL}"
  WriteRegStr HKCU "${UNINST_KEY}" "HelpLink" "${URL}"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
SectionEnd

Section "n0te Laptop" SecLaptop
  SetOutPath "$INSTDIR"
  File "${STAGE}/n0te-laptop.exe"
  CreateShortCut "$SMPROGRAMS\n0te Laptop.lnk" "$INSTDIR\n0te-laptop.exe" "" "$INSTDIR\n0te-laptop.exe" 0
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\App Paths\n0te-laptop.exe" "" "$INSTDIR\n0te-laptop.exe"
SectionEnd

Section "n0te Tablet" SecTablet
  SetOutPath "$INSTDIR"
  File "${STAGE}/n0te-tablet.exe"
  CreateShortCut "$SMPROGRAMS\n0te Tablet.lnk" "$INSTDIR\n0te-tablet.exe" "" "$INSTDIR\n0te-tablet.exe" 0
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\App Paths\n0te-tablet.exe" "" "$INSTDIR\n0te-tablet.exe"
SectionEnd

Section "$(NameAutostart)" SecAutostart
  ; Written here directly (not through n0te-sync), so it works with any n0te-sync version.
  WriteRegStr HKCU "${RUN_KEY}" "n0te-sync" '"$INSTDIR\n0te-sync.exe" run'
SectionEnd

; After the components: take away what was unticked (an update may drop a program), then file
; associations for what is there, then start the helper again.
Section "-finish" SecFinish
  ${IfNot} ${SectionIsSelected} ${SecLaptop}
    Delete "$INSTDIR\n0te-laptop.exe"
    Delete "$SMPROGRAMS\n0te Laptop.lnk"
    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\App Paths\n0te-laptop.exe"
  ${EndIf}
  ${IfNot} ${SectionIsSelected} ${SecTablet}
    Delete "$INSTDIR\n0te-tablet.exe"
    Delete "$SMPROGRAMS\n0te Tablet.lnk"
    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\App Paths\n0te-tablet.exe"
  ${EndIf}
  ${IfNot} ${SectionIsSelected} ${SecAutostart}
    DeleteRegValue HKCU "${RUN_KEY}" "n0te-sync"
  ${EndIf}

  ; .n0te files open with n0te Laptop (n0te Tablet when only that one is installed);
  ; n0te:// links (guest invites) open n0te Tablet when it is installed, else n0te Laptop.
  ${If} ${FileExists} "$INSTDIR\n0te-laptop.exe"
    StrCpy $1 "$INSTDIR\n0te-laptop.exe"
  ${Else}
    StrCpy $1 "$INSTDIR\n0te-tablet.exe"
  ${EndIf}
  ${If} ${FileExists} "$INSTDIR\n0te-tablet.exe"
    StrCpy $2 "$INSTDIR\n0te-tablet.exe"
  ${Else}
    StrCpy $2 "$INSTDIR\n0te-laptop.exe"
  ${EndIf}

  WriteRegStr HKCU "${CLASSES}\.n0te" "" "${PROGID}"
  WriteRegStr HKCU "${CLASSES}\.n0te" "Content Type" "application/x-n0te"
  WriteRegStr HKCU "${CLASSES}\.n0te\OpenWithProgids" "${PROGID}" ""
  WriteRegStr HKCU "${CLASSES}\${PROGID}" "" "$(NotebookType)"
  WriteRegStr HKCU "${CLASSES}\${PROGID}\DefaultIcon" "" '"$1",0'
  WriteRegStr HKCU "${CLASSES}\${PROGID}\shell\open\command" "" '"$1" "%1"'

  WriteRegStr HKCU "${CLASSES}\n0te" "" "URL:n0te"
  WriteRegStr HKCU "${CLASSES}\n0te" "URL Protocol" ""
  WriteRegStr HKCU "${CLASSES}\n0te\DefaultIcon" "" '"$2",0'
  WriteRegStr HKCU "${CLASSES}\n0te\shell\open\command" "" '"$2" "%1"'

  ; "Open with" lists both programs for .n0te files.
  DeleteRegKey HKCU "${CLASSES}\Applications\n0te-laptop.exe"
  DeleteRegKey HKCU "${CLASSES}\Applications\n0te-tablet.exe"
  ${If} ${FileExists} "$INSTDIR\n0te-laptop.exe"
    WriteRegStr HKCU "${CLASSES}\Applications\n0te-laptop.exe" "FriendlyAppName" "n0te Laptop"
    WriteRegStr HKCU "${CLASSES}\Applications\n0te-laptop.exe\SupportedTypes" ".n0te" ""
    WriteRegStr HKCU "${CLASSES}\Applications\n0te-laptop.exe\shell\open\command" "" '"$INSTDIR\n0te-laptop.exe" "%1"'
    WriteRegStr HKCU "${CLASSES}\.n0te\OpenWithList\n0te-laptop.exe" "" ""
  ${Else}
    DeleteRegKey HKCU "${CLASSES}\.n0te\OpenWithList\n0te-laptop.exe"
  ${EndIf}
  ${If} ${FileExists} "$INSTDIR\n0te-tablet.exe"
    WriteRegStr HKCU "${CLASSES}\Applications\n0te-tablet.exe" "FriendlyAppName" "n0te Tablet"
    WriteRegStr HKCU "${CLASSES}\Applications\n0te-tablet.exe\SupportedTypes" ".n0te" ""
    WriteRegStr HKCU "${CLASSES}\Applications\n0te-tablet.exe\shell\open\command" "" '"$INSTDIR\n0te-tablet.exe" "%1"'
    WriteRegStr HKCU "${CLASSES}\.n0te\OpenWithList\n0te-tablet.exe" "" ""
  ${Else}
    DeleteRegKey HKCU "${CLASSES}\.n0te\OpenWithList\n0te-tablet.exe"
  ${EndIf}
  ; SHCNE_ASSOCCHANGED: Explorer picks up the new icons and handlers.
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'

  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" '"$1",0'
  ${GetSize} "$INSTDIR" "/S=0K" $0 $3 $4
  WriteRegDWORD HKCU "${UNINST_KEY}" "EstimatedSize" $0

  ; The helper runs again (hidden; it has no window of its own).
  ${If} ${SectionIsSelected} ${SecAutostart}
    ExecShell "open" "$INSTDIR\n0te-sync.exe" "run" SW_HIDE
  ${EndIf}
SectionEnd

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecLaptop} $(DescLaptop)
  !insertmacro MUI_DESCRIPTION_TEXT ${SecTablet} $(DescTablet)
  !insertmacro MUI_DESCRIPTION_TEXT ${SecAutostart} $(DescAutostart)
!insertmacro MUI_FUNCTION_DESCRIPTION_END

; --- functions -----------------------------------------------------------------------------

Function .onInit
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "/RELAUNCH=" $Relaunch
  ${If} ${Errors}
    StrCpy $Relaunch ""
  ${EndIf}
  ClearErrors
  ${GetOptions} $R0 "/UPDATE" $R1
  ${IfNot} ${Errors}
    ; The app that asked has quit already; others are asked to close, then made to.
    nsExec::Exec '"$SYSDIR\taskkill.exe" /IM n0te-laptop.exe /IM n0te-tablet.exe'
    Pop $0
    Sleep 1500
    nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM n0te-laptop.exe /IM n0te-tablet.exe'
    Pop $0
    ; Keep what is installed: a program that isn't there stays away, autostart as it was.
    ReadRegStr $R2 HKCU "Software\n0te" "InstallDir"
    ${If} $R2 != ""
      ${IfNot} ${FileExists} "$R2\n0te-laptop.exe"
      ${AndIf} ${FileExists} "$R2\n0te-tablet.exe"
        !insertmacro UnselectSection ${SecLaptop}
      ${EndIf}
      ${IfNot} ${FileExists} "$R2\n0te-tablet.exe"
      ${AndIf} ${FileExists} "$R2\n0te-laptop.exe"
        !insertmacro UnselectSection ${SecTablet}
      ${EndIf}
      ReadRegStr $R3 HKCU "${RUN_KEY}" "n0te-sync"
      ${If} $R3 == ""
        !insertmacro UnselectSection ${SecAutostart}
      ${EndIf}
    ${EndIf}
  ${EndIf}
  ${IfNot} ${AtLeastWin10}
    MessageBox MB_ICONSTOP|MB_OK "$(NeedWin10)" /SD IDOK
    Abort
  ${EndIf}
  ${If} ${IsNativeIA32}
    MessageBox MB_ICONSTOP|MB_OK "$(Need64)" /SD IDOK
    Abort
  ${EndIf}
  !if "${ARCH}" == "arm64"
    ${IfNot} ${IsNativeARM64}
      MessageBox MB_ICONSTOP|MB_OK "$(NeedArm)" /SD IDOK
      Abort
    ${EndIf}
  !else
    ${If} ${IsNativeARM64}
      MessageBox MB_ICONQUESTION|MB_YESNO "$(PreferArm)" /SD IDYES IDYES keep_x64
      Abort
      keep_x64:
    ${EndIf}
  !endif
FunctionEnd

; At least one of the two programs stays ticked: unticking the last one ticks it again.
Function .onSelChange
  ${IfNot} ${SectionIsSelected} ${SecLaptop}
  ${AndIfNot} ${SectionIsSelected} ${SecTablet}
    ${If} $0 == ${SecTablet}
      !insertmacro SelectSection ${SecTablet}
    ${Else}
      !insertmacro SelectSection ${SecLaptop}
    ${EndIf}
  ${EndIf}
FunctionEnd

; After a self-update: the program that asked for it starts again.
Function .onInstSuccess
  ${If} $Relaunch == "laptop"
  ${OrIf} $Relaunch == "tablet"
    ${If} ${FileExists} "$INSTDIR\n0te-$Relaunch.exe"
      Exec '"$INSTDIR\n0te-$Relaunch.exe"'
    ${EndIf}
  ${EndIf}
FunctionEnd

; "Start n0te now": n0te Laptop when it is installed, else n0te Tablet.
Function RunApp
  ${If} ${FileExists} "$INSTDIR\n0te-laptop.exe"
    Exec '"$INSTDIR\n0te-laptop.exe"'
  ${ElseIf} ${FileExists} "$INSTDIR\n0te-tablet.exe"
    Exec '"$INSTDIR\n0te-tablet.exe"'
  ${EndIf}
FunctionEnd

; --- uninstaller ---------------------------------------------------------------------------

Section "Uninstall"
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM n0te-sync.exe'
  Pop $0
  Sleep 300

  Delete "$INSTDIR\n0te-laptop.exe"
  Delete "$INSTDIR\n0te-tablet.exe"
  Delete "$INSTDIR\n0te-sync.exe"
  Delete "$INSTDIR\pdfium.dll"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\THIRD_PARTY_NOTICES.txt"
  RMDir /r "$INSTDIR\licenses"
  Delete "$INSTDIR\uninstall.exe"
  ; Only if empty: never a recursive delete of the install folder.
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS\n0te Laptop.lnk"
  Delete "$SMPROGRAMS\n0te Tablet.lnk"

  DeleteRegValue HKCU "${RUN_KEY}" "n0te-sync"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\App Paths\n0te-laptop.exe"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\App Paths\n0te-tablet.exe"
  DeleteRegKey HKCU "${CLASSES}\Applications\n0te-laptop.exe"
  DeleteRegKey HKCU "${CLASSES}\Applications\n0te-tablet.exe"
  DeleteRegKey HKCU "${CLASSES}\${PROGID}"
  DeleteRegKey HKCU "${CLASSES}\n0te"
  DeleteRegValue HKCU "${CLASSES}\.n0te\OpenWithProgids" "${PROGID}"
  DeleteRegKey HKCU "${CLASSES}\.n0te\OpenWithList\n0te-laptop.exe"
  DeleteRegKey HKCU "${CLASSES}\.n0te\OpenWithList\n0te-tablet.exe"
  ; .n0te itself only while it still points at n0te (another program may have taken it over).
  ReadRegStr $0 HKCU "${CLASSES}\.n0te" ""
  ${If} $0 == "${PROGID}"
    DeleteRegValue HKCU "${CLASSES}\.n0te" ""
    DeleteRegValue HKCU "${CLASSES}\.n0te" "Content Type"
  ${EndIf}
  DeleteRegKey /ifempty HKCU "${CLASSES}\.n0te\OpenWithProgids"
  DeleteRegKey /ifempty HKCU "${CLASSES}\.n0te\OpenWithList"
  DeleteRegKey /ifempty HKCU "${CLASSES}\.n0te"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'

  DeleteRegKey HKCU "${UNINST_KEY}"
  DeleteRegKey HKCU "Software\n0te"
  ; %APPDATA%\n0te (the notebooks) stays where it is.
SectionEnd
