; Inzel installer - compile with Inno Setup 6 AFTER running the PyInstaller step (build.bat does both)
#define MyAppName "Inzel"
#define MyAppVersion "1.0.0"
#define MyAppPublisher "Debugger Mohon"
#define MyAppURL "https://github.com/DebuggerMohon/Inzel"
#define MyAppExe "Inzel.exe"

[Setup]
AppId={{B7C1F3A2-5D4E-4F8A-9A36-2E7D1C0B94F1}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}/issues
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
LicenseFile=LICENSE
SetupIconFile=assets\inzel.ico
UninstallDisplayIcon={app}\{#MyAppExe}
OutputDir=installer
OutputBaseFilename=Inzel-Setup-{#MyAppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Installs per-user by default (no admin prompt); the user can choose "for all users" in a dialog
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
; Asks to close Inzel if it is running during install/uninstall
AppMutex=Inzel_SingleInstance_Mutex
CloseApplications=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Additional shortcuts:"
Name: "startup"; Description: "Start Inzel when Windows starts"; GroupDescription: "Startup:"; Flags: unchecked

[Files]
Source: "target\release\Inzel.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"
Name: "{group}\Uninstall {#MyAppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Inzel"; ValueData: """{app}\{#MyAppExe}"" --minimized"; Flags: uninsdeletevalue; Tasks: startup

[Run]
Filename: "{app}\{#MyAppExe}"; Description: "Launch {#MyAppName}"; Flags: nowait postinstall skipifsilent
