#ifndef StageDir
  #error StageDir is required
#endif
#ifndef OutputDir
  #error OutputDir is required
#endif
#ifndef AppVersion
  #error AppVersion is required
#endif

[Setup]
; Match the upstream Mousehop application identifier.
AppId=com.mousehop.Mousehop
AppName=MotionDuo
AppVersion={#AppVersion}
DefaultDirName={localappdata}\Programs\Mousehop
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputDir}
OutputBaseFilename=MotionDuo-{#AppVersion}-x64-Setup
SetupIconFile=..\assets\brand\motionduo.ico
UninstallDisplayIcon={app}\bin\motionduo.exe
LicenseFile={#StageDir}\LICENSE
Compression=lzma2
SolidCompression=yes
DisableProgramGroupPage=yes
CloseApplications=no
RestartApplications=no

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#StageDir}\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion

[Icons]
Name: "{userprograms}\MotionDuo"; Filename: "{app}\bin\motionduo.exe"; WorkingDir: "{app}\bin"; IconFilename: "{app}\bin\motionduo.exe"
Name: "{userdesktop}\MotionDuo"; Filename: "{app}\bin\motionduo.exe"; WorkingDir: "{app}\bin"; IconFilename: "{app}\bin\motionduo.exe"; Tasks: desktopicon
Name: "{userprograms}\Uninstall MotionDuo"; Filename: "{uninstallexe}"; IconFilename: "{app}\bin\motionduo.exe"

[Code]
function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if FileExists(ExpandConstant('{app}\bin\mousehop.exe')) or
     FileExists(ExpandConstant('{app}\bin\mousehop-mygo.exe')) then
    Result := 'Back up your installation and user data, then uninstall the old version before installing MotionDuo.';
end;

procedure RemoveOwnedLegacyShortcut(const ShortcutPath, ExpectedTarget: String);
var
  Shell, Link: Variant;
begin
  if not FileExists(ShortcutPath) then exit;
  try
    Shell := CreateOleObject('WScript.Shell');
    Link := Shell.CreateShortcut(ShortcutPath);
    if (CompareText(ExpandFileName(Link.TargetPath), ExpandFileName(ExpectedTarget)) = 0) and
       (Link.Arguments = '') then begin
      if not DeleteFile(ShortcutPath) then
        Log('Could not remove legacy shortcut: ' + ShortcutPath);
    end else
      Log('Leaving unrelated legacy shortcut: ' + ShortcutPath);
  except
    Log('Could not inspect legacy shortcut: ' + ShortcutPath);
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then begin
    RemoveOwnedLegacyShortcut(ExpandConstant('{userprograms}\Mousehop.lnk'), ExpandConstant('{app}\bin\mousehop.exe'));
    RemoveOwnedLegacyShortcut(ExpandConstant('{userdesktop}\Mousehop.lnk'), ExpandConstant('{app}\bin\mousehop.exe'));
    RemoveOwnedLegacyShortcut(ExpandConstant('{userprograms}\Uninstall Mousehop.lnk'), ExpandConstant('{uninstallexe}'));
  end;
end;
