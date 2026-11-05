#if Ver < EncodeVer(6, 3, 0)
  #error Inno Setup 6.3 or newer is needed (for ExecAndCaptureOutput)
#endif

#ifndef AppVersion
  #define AppVersion "0.1.9"
#endif

[Setup]
AppId={{6E0B8A51-3C1F-4D7B-9A55-2F1E7C8D4B90}
AppName=YASAFF
AppVersion={#AppVersion}
AppPublisher=naisyrkdev
AppPublisherURL=https://github.com/naisyrkdev/YASAFF
DefaultDirName={autopf}\YASAFF
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\target\installer
OutputBaseFilename=YASAFF-Setup-{#AppVersion}
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\yasaff.exe
WizardStyle=modern
WizardImageFile=wizard-large-164.bmp,wizard-large-328.bmp
WizardSmallImageFile=wizard-small-55.bmp,wizard-small-110.bmp
Compression=lzma2
SolidCompression=yes
CloseApplications=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "..\target\release\yasaff.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "setup-git.ps1"; DestDir: "{app}\tools"; Flags: ignoreversion
; ffmpeg and its license, fetched by build-installer.ps1
Source: "..\target\bundle\tools\*"; DestDir: "{app}\tools"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\YASAFF"; Filename: "{app}\yasaff.exe"
Name: "{autodesktop}\YASAFF"; Filename: "{app}\yasaff.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\yasaff.exe"; Description: "{cm:LaunchProgram,YASAFF}"; Flags: nowait postinstall skipifsilent

[Code]
const
  GitLog = '%TEMP%\yasaff-setup-git.log';

var
  GitPage: TInputOptionWizardPage;
  IdentityPage: TInputQueryWizardPage;
  GitPath: String;

function GitInDir(const Dir: String): String;
begin
  Result := '';
  if (Dir <> '') and FileExists(AddBackslash(Dir) + 'cmd\git.exe') then
    Result := AddBackslash(Dir) + 'cmd\git.exe';
end;
function FindGit: String;
var
  Dir: String;
begin
  Result := FileSearch('git.exe', GetEnv('PATH'));
  if (Result = '') and RegQueryStringValue(HKLM64, 'SOFTWARE\GitForWindows', 'InstallPath', Dir) then
    Result := GitInDir(Dir);
  if (Result = '') and RegQueryStringValue(HKLM32, 'SOFTWARE\GitForWindows', 'InstallPath', Dir) then
    Result := GitInDir(Dir);
  if (Result = '') and RegQueryStringValue(HKCU, 'SOFTWARE\GitForWindows', 'InstallPath', Dir) then
    Result := GitInDir(Dir);
  if Result = '' then
    Result := GitInDir(ExpandConstant('{commonpf64}\Git'));
  if Result = '' then
    Result := GitInDir(ExpandConstant('{localappdata}\Programs\Git'));
end;

function GitConfigValue(const Key: String): String;
var
  ResultCode: Integer;
  Output: TExecOutput;
begin
  Result := '';
  if GitPath = '' then
    Exit;
  if ExecAndCaptureOutput(GitPath, 'config --global ' + Key, '', SW_HIDE, ewWaitUntilTerminated, ResultCode, Output)
    and (ResultCode = 0) and (GetArrayLength(Output.StdOut) > 0) then
    Result := Trim(Output.StdOut[0]);
end;

function IsValidEmail(const S: String): Boolean;
var
  At: Integer;
  Domain: String;
begin
  Result := False;
  if (Pos(' ', S) > 0) or (Pos('"', S) > 0) then
    Exit;
  At := Pos('@', S);
  if At < 2 then
    Exit;
  Domain := Copy(S, At + 1, Length(S));
  if (Domain = '') or (Pos('@', Domain) > 0) then
    Exit;
  Result := (Pos('.', Domain) > 1) and (Domain[Length(Domain)] <> '.');
end;
function QuoteArg(const S: String): String;
var
  I: Integer;
begin
  Result := S;
  I := Length(S);
  while (I > 0) and (S[I] = '\') do
  begin
    Result := Result + '\';
    I := I - 1;
  end;
  Result := '"' + Result + '"';
end;

procedure InitializeWizard;
begin
  GitPath := FindGit;

  GitPage := CreateInputOptionPage(wpSelectTasks,
    'Git', 'Install Git and tell it who you are.',
    'Git is a free tool that downloads projects and keeps track of versions of files. ' +
    'Setup can install it and set it up for your Windows account.' + #13#10#13#10 +
    'If you''re not sure, leave both boxes ticked.',
    False, False);
  GitPage.Add('Install Git (downloads about 70 MB from the internet)');
  GitPage.Add('Set my name and email for Git');

  IdentityPage := CreateInputQueryPage(GitPage.ID,
    'Your name and email for Git', 'Git stamps every change you save with them.',
    'Git needs a name and an email address to save changes. They don''t have to be ' +
    'your real ones, but if you publish your work (for example on GitHub), others will ' +
    'see them. If you have a GitHub account, use its email address.');
  IdentityPage.Add('&Name (for example Jane Doe):', False);
  IdentityPage.Add('&Email (for example jane@example.com):', False);
  IdentityPage.Values[0] := GitConfigValue('user.name');
  IdentityPage.Values[1] := GitConfigValue('user.email');

  if GitPath <> '' then
  begin
    GitPage.CheckListBox.ItemCaption[0] := 'Install Git (already installed: ' + GitPath + ')';
    GitPage.Values[0] := False;
    GitPage.CheckListBox.ItemEnabled[0] := False;
  end
  else
    GitPage.Values[0] := True;
  GitPage.Values[1] := not WizardSilent and ((IdentityPage.Values[0] = '') or (IdentityPage.Values[1] = ''));
end;

function ShouldSkipPage(PageID: Integer): Boolean;
begin
  Result := (PageID = IdentityPage.ID) and not GitPage.Values[1];
end;

function NextButtonClick(CurPageID: Integer): Boolean;
var
  GitName, GitEmail: String;
begin
  Result := True;
  if (CurPageID = GitPage.ID) and GitPage.Values[1] and (GitPath = '') and not GitPage.Values[0] then
  begin
    MsgBox('Git has to be installed to save your name and email. Tick "Install Git" too, ' +
      'or untick "Set my name and email for Git".', mbError, MB_OK);
    Result := False;
  end
  else if CurPageID = IdentityPage.ID then
  begin
    GitName := Trim(IdentityPage.Values[0]);
    GitEmail := Trim(IdentityPage.Values[1]);
    if GitName = '' then
    begin
      MsgBox('Please enter your name.', mbError, MB_OK);
      Result := False;
    end
    else if Pos('"', GitName) > 0 then
    begin
      MsgBox('Please leave out double quotes (") in your name.', mbError, MB_OK);
      Result := False;
    end
    else if not IsValidEmail(GitEmail) then
    begin
      MsgBox('That doesn''t look like an email address. It should look like jane@example.com.', mbError, MB_OK);
      Result := False;
    end;
  end;
end;

function UpdateReadyMemo(Space, NewLine, MemoUserInfoInfo, MemoDirInfo, MemoTypeInfo,
  MemoComponentsInfo, MemoGroupInfo, MemoTasksInfo: String): String;
begin
  Result := MemoDirInfo;
  if MemoTasksInfo <> '' then
    Result := Result + NewLine + NewLine + MemoTasksInfo;
  if GitPage.Values[0] or GitPage.Values[1] then
  begin
    Result := Result + NewLine + NewLine + 'Git:';
    if GitPage.Values[0] then
      Result := Result + NewLine + Space + 'Install Git';
    if GitPage.Values[1] then
      Result := Result + NewLine + Space + 'Name: ' + Trim(IdentityPage.Values[0]) +
        NewLine + Space + 'Email: ' + Trim(IdentityPage.Values[1]);
  end;
end;

procedure RunGitSetup;
var
  Params, Problem: String;
  ResultCode: Integer;
begin
  if not GitPage.Values[0] and not GitPage.Values[1] then
    Exit;

  Params := '-NoProfile -ExecutionPolicy Bypass -File ' + QuoteArg(ExpandConstant('{app}\tools\setup-git.ps1'));
  if GitPage.Values[0] then
    Params := Params + ' -Install';
  if GitPage.Values[1] then
    Params := Params + ' -Name ' + QuoteArg(Trim(IdentityPage.Values[0])) +
      ' -Email ' + QuoteArg(Trim(IdentityPage.Values[1]));

  if GitPage.Values[0] then
    WizardForm.StatusLabel.Caption := 'Installing Git. This can take a few minutes, and Windows may ask for permission...'
  else
    WizardForm.StatusLabel.Caption := 'Saving your name and email for Git...';
  WizardForm.FilenameLabel.Caption := '';
  WizardForm.ProgressGauge.Style := npbstMarquee;
  try
    if not ExecAsOriginalUser(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'), Params, '',
      SW_HIDE, ewWaitUntilTerminated, ResultCode) then
      ResultCode := -1;
  finally
    WizardForm.ProgressGauge.Style := npbstNormal;
  end;
  Log(Format('setup-git.ps1 exit code: %d', [ResultCode]));

  case ResultCode of
    0: Problem := '';
    1: Problem := 'Git could not be installed. You can install it later from https://git-scm.com/downloads/win.';
    2: Problem := 'Git is not installed, so your name and email were not saved.';
    3: Problem := 'Git is installed, but your name and email could not be saved.';
  else
    Problem := Format('Setting up Git did not finish (code %d).', [ResultCode]);
  end;
  if Problem <> '' then
    SuppressibleMsgBox(Problem + #13#10#13#10 +
      'YASAFF itself is installed. To try again later, run the "setup-git.ps1" script in the ' +
      '"tools" folder next to yasaff.exe (right-click it, then "Run with PowerShell").' + #13#10#13#10 +
      'Details are in ' + GitLog, mbError, MB_OK, IDOK);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    RunGitSetup;
end;
