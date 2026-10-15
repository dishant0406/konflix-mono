; Inno Setup script for the Windows installer. bundle-windows.ps1 builds it with
; /DAppVersion=<version> after it has laid the app out in target\release\bundle\Akidev.
; The install is per user (no administrator prompt) and the uninstaller removes it again.
#ifndef AppVersion
  #error Pass /DAppVersion=<version>
#endif

[Setup]
AppId={{7C1B7E4A-5E0F-4B8E-9C3D-5A1D3E0000A1}
AppName=Akidev
AppVersion={#AppVersion}
AppPublisher=Akidev
AppPublisherURL=https://akidev.live
DefaultDirName={autopf}\Akidev
DefaultGroupName=Akidev
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayIcon={app}\Akidev.exe
SetupIconFile=..\app\assets\akidev.ico
OutputDir=..\target\release\bundle
OutputBaseFilename=Akidev-{#AppVersion}-x86_64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
DisableProgramGroupPage=yes

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "..\target\release\bundle\Akidev\Akidev.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\bundle\Akidev\libmpv-2.dll"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Akidev"; Filename: "{app}\Akidev.exe"
Name: "{autodesktop}\Akidev"; Filename: "{app}\Akidev.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\Akidev.exe"; Description: "Open Akidev"; Flags: nowait postinstall skipifsilent
