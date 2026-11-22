; fx installer (task T2.10, docs/DECISIONS.md D-045): per user, no administrator.
; Built by the CI with Inno Setup 6:
;   iscc /DAppVersion=0.2.0 /DSourceDir=<folder with the .exe files> installer\fx.iss
; Installs into %LOCALAPPDATA%\Programs\fx, adds a Start Menu shortcut, and the
; uninstaller removes everything it installed. The user's settings and state
; (%APPDATA%\fx, %LOCALAPPDATA%\fx) stay unless the user asks to remove them.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef Arch
  #define Arch "x64"
#endif
#ifndef SourceDir
  #define SourceDir "..\target\x86_64-pc-windows-msvc\release"
#endif

[Setup]
AppId={{6E7C2B5A-3F1D-4C8E-9A2B-0F4D5E6A7B8C}
AppName=fx
AppVersion={#AppVersion}
AppVerName=fx {#AppVersion}
AppPublisher=fx
AppPublisherURL=https://github.com/heapwraith/fx
AppSupportURL=https://github.com/heapwraith/fx/issues
DefaultDirName={localappdata}\Programs\fx
DefaultGroupName=fx
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
#if Arch == "arm64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif
OutputDir=..\dist
OutputBaseFilename=fx-{#AppVersion}-setup-{#Arch}
SetupIconFile=..\crates\fx-ui\resources\fx.ico
UninstallDisplayIcon={app}\fx.exe
UninstallDisplayName=fx
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
LicenseFile=..\LICENSE-MIT

[Languages]
Name: "ptbr"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\fx.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\fx-indexd.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\fx-preview.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\CHANGELOG.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\fx"; Filename: "{app}\fx.exe"
Name: "{autodesktop}\fx"; Filename: "{app}\fx.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\fx.exe"; Description: "{cm:LaunchProgram,fx}"; Flags: nowait postinstall skipifsilent
; Update from fx itself (T4.1): silent install with /RELAUNCH opens the new fx.
Filename: "{app}\fx.exe"; Flags: nowait; Check: WizardSilent and RelaunchAsked

[UninstallRun]
; The optional index service (installed with consent from inside fx) lives in
; Program Files and needs Administrator to be removed: one UAC prompt, only
; when the service exists.
Filename: "{app}\fx-indexd.exe"; Parameters: "uninstall"; Verb: "runas"; Flags: shellexec waituntilterminated runhidden; Check: IndexServiceInstalled; RunOnceId: "RemoveIndexService"

[Code]
function RelaunchAsked(): Boolean;
begin
  Result := Pos('/RELAUNCH', Uppercase(GetCmdTail)) > 0;
end;

function IndexServiceInstalled(): Boolean;
begin
  Result := RegKeyExists(HKLM, 'SYSTEM\CurrentControlSet\Services\fx-indexd');
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if (CurUninstallStep = usPostUninstall) and not UninstallSilent then
  begin
    if MsgBox('Apagar também as configurações e o estado do fx (abas, pastas fixadas, histórico)?' + #13#10 +
              'Escolha "Não" para mantê-los caso instale o fx de novo.',
              mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
    begin
      DelTree(ExpandConstant('{userappdata}\fx'), True, True, True);
      DelTree(ExpandConstant('{localappdata}\fx'), True, True, True);
    end;
  end;
end;
