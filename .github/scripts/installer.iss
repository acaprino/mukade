; The installer of a release, shared by Shinatama, Mukade, Xiox and Muro: the same file in each. Every file of release\
; (release\installer\ aside) goes into the install folder, for the current user (no administrator rights: the programs
; write their data beside them, which a folder of Program Files would refuse). The uninstaller removes the files it
; installed and nothing else, so what a program made beside them (its data, logs, the webview's cache) stays.
; build-release.bat (step 3) builds it with Inno Setup 6 into release\installer\<App>-<Version>-setup.exe:
;   ISCC /DApp=<name> /DVersion=<x.y.z> /DExe=<main program> /DSource=<release folder> /O<output folder> installer.iss

#ifndef App
  #error App is missing: /DApp=<name>
#endif
#ifndef Version
  #error Version is missing: /DVersion=<x.y.z>
#endif
#ifndef Exe
  #error Exe is missing: /DExe=<main program>
#endif
#ifndef Source
  #error Source is missing: /DSource=<release folder>
#endif

[Setup]
; One id per app, never changed: a new version installs over the old one, in its folder.
AppId=acaprino.{#App}
AppName={#App}
AppVersion={#Version}
AppVerName={#App} {#Version}
AppPublisher=Alfio Caprino
PrivilegesRequired=lowest
DefaultDirName={autopf}\{#App}
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputBaseFilename={#App}-{#Version}-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\{#Exe}
; A program of the install folder still running is closed (Restart Manager) before its files are replaced.
CloseApplications=yes
RestartApplications=no

[Tasks]
Name: desktopicon; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#Source}\*"; Excludes: "\installer\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\{#App}"; Filename: "{app}\{#Exe}"
Name: "{autodesktop}\{#App}"; Filename: "{app}\{#Exe}"; Tasks: desktopicon

[Run]
; shellexec: a program that asks for administrator rights starts through UAC.
Filename: "{app}\{#Exe}"; Description: "{cm:LaunchProgram,{#App}}"; Flags: nowait postinstall skipifsilent shellexec
