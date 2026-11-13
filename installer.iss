; Keycast Inno Setup Script
; Generates a signed, modern, false-positive-resistant Windows Installer

#define MyAppName "Keycast"
#define MyAppVersion "0.1.1"
#define MyAppPublisher "Keycast Project"
#define MyAppURL "https://github.com"
#define MyAppExeName "keycast.exe"

[Setup]
AppId={{8B264E24-11FA-4A73-A33B-E4F57A3C5610}}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
OutputDir=dist
OutputBaseFilename=Keycast-Setup
SetupIconFile=assets\keycast.ico
Compression=lzma2/normal
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesInstallIn64BitMode=x64compatible
ArchitecturesAllowed=x64compatible
UninstallDisplayIcon={app}\{#MyAppExeName}
UninstallDisplayName={#MyAppName}
VersionInfoVersion=0.1.1.0
VersionInfoCompany={#MyAppPublisher}
VersionInfoDescription={#MyAppName} Installer
VersionInfoProductName={#MyAppName}
VersionInfoProductVersion=0.1.1.0
VersionInfoCopyright=Copyright (C) 2026 {#MyAppPublisher}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "certs\Keycast.cer"; DestDir: "{app}"; Flags: ignoreversion
Source: "assets\keycast.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "README.md"; DestDir: "{app}"; Flags: ignoreversion isreadme
Source: "THIRD-PARTY-NOTICES\*"; DestDir: "{app}\THIRD-PARTY-NOTICES"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\{#MyAppExeName}"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
; Import the certificate into user stores so Windows verifies the Authenticode signature as Valid
Filename: "certutil.exe"; Parameters: "-user -addstore Root ""{app}\Keycast.cer"""; Flags: runhidden
Filename: "certutil.exe"; Parameters: "-user -addstore TrustedPublisher ""{app}\Keycast.cer"""; Flags: runhidden
Filename: "certutil.exe"; Parameters: "-user -addstore TrustedPeople ""{app}\Keycast.cer"""; Flags: runhidden

; Strip any Mark-of-the-Web (Zone.Identifier) alternate data streams
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -Command ""Get-ChildItem -LiteralPath '{app}' -Recurse | Unblock-File"""; Flags: runhidden

; Launch Keycast upon completion (optional checkbox)
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent
