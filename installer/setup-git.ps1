param(
    [switch]$Install,
    [string]$Name,
    [string]$Email,
    [string]$LogFile = (Join-Path $env:TEMP 'yasaff-setup-git.log')
)

$ErrorActionPreference = 'Continue'
$ProgressPreference = 'SilentlyContinue'
$interactive = -not ($Install -or $Name -or $Email)

function Write-Log([string]$Message) {
    Write-Host $Message
    Add-Content -LiteralPath $LogFile -Value ('{0:yyyy-MM-dd HH:mm:ss}  {1}' -f (Get-Date), $Message) -Encoding UTF8
}

function Test-Email([string]$Value) {
    return $Value -match '^[^@\s"]+@[^@\s"]+\.[^@\s".]+$'
}

function Find-Git {
    $cmd = Get-Command git.exe -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    $candidates = @()
    foreach ($key in 'HKLM:\SOFTWARE\GitForWindows', 'HKCU:\SOFTWARE\GitForWindows') {
        $dir = (Get-ItemProperty -Path $key -Name InstallPath -ErrorAction SilentlyContinue).InstallPath
        if ($dir) { $candidates += Join-Path $dir 'cmd\git.exe' }
    }
    $candidates += "$env:ProgramFiles\Git\cmd\git.exe", "$env:LOCALAPPDATA\Programs\Git\cmd\git.exe"
    foreach ($path in $candidates) {
        if (Test-Path -LiteralPath $path) { return $path }
    }
    return $null
}

function Install-GitWithWinget {
    $winget = Get-Command winget.exe -ErrorAction SilentlyContinue
    if (-not $winget) {
        Write-Log 'winget is not available.'
        return $false
    }
    Write-Log 'Installing Git with winget (Windows may ask for permission)...'
    & $winget.Source install --id Git.Git --exact --source winget --silent `
        --accept-package-agreements --accept-source-agreements 2>&1 |
        ForEach-Object { Add-Content -LiteralPath $LogFile -Value "    $_" -Encoding UTF8 }
    Write-Log "winget finished with exit code $LASTEXITCODE."
    return [bool](Find-Git)
}

function Install-GitFromGitHub {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    $arch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64' -or $env:PROCESSOR_ARCHITEW6432 -eq 'ARM64') { 'arm64' } else { '64-bit' }
    Write-Log 'Looking up the latest Git for Windows on GitHub...'
    try {
        $release = Invoke-RestMethod -UseBasicParsing -ErrorAction Stop `
            -Uri 'https://api.github.com/repos/git-for-windows/git/releases/latest' `
            -Headers @{ 'User-Agent' = 'YASAFF-setup' }
    } catch {
        Write-Log "Couldn't reach GitHub: $_"
        return $false
    }
    $asset = $release.assets | Where-Object { $_.name -match "^Git-.+-$arch\.exe$" } | Select-Object -First 1
    if (-not $asset) {
        Write-Log "The release $($release.tag_name) has no installer for $arch."
        return $false
    }

    $installer = Join-Path $env:TEMP $asset.name
    Write-Log "Downloading $($asset.name)..."
    try {
        Invoke-WebRequest -UseBasicParsing -ErrorAction Stop -Uri $asset.browser_download_url -OutFile $installer
    } catch {
        Write-Log "Download failed: $_"
        return $false
    }
    if ($asset.digest -match '^sha256:([0-9a-f]{64})$') {
        $actual = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash
        if ($actual -ne $Matches[1]) {
            Write-Log "The download is damaged (SHA-256 $actual, expected $($Matches[1]))."
            Remove-Item -LiteralPath $installer -ErrorAction SilentlyContinue
            return $false
        }
    }

    Write-Log 'Running the Git installer (Windows will ask for permission)...'
    try {
        $process = Start-Process -FilePath $installer -Verb RunAs -Wait -PassThru -ErrorAction Stop `
            -ArgumentList '/VERYSILENT', '/NORESTART', '/NOCANCEL', '/SP-', '/SUPPRESSMSGBOXES'
        Write-Log "The Git installer finished with exit code $($process.ExitCode)."
    } catch {
        Write-Log "The Git installer didn't run: $_"
    }
    Remove-Item -LiteralPath $installer -ErrorAction SilentlyContinue
    return [bool](Find-Git)
}

function Read-WithDefault([string]$Prompt, [string]$Default) {
    $suffix = if ($Default) { " [$Default]" } else { '' }
    $answer = Read-Host "$Prompt$suffix"
    if ($answer) { return $answer.Trim() } else { return $Default }
}

function Exit-Script([int]$Code) {
    if ($interactive) { Read-Host 'Press Enter to close' | Out-Null }
    exit $Code
}

Write-Log '--- YASAFF Git setup ---'
$git = Find-Git
if ($git) { Write-Log "Git found: $git" }

if ($interactive) {
    if (-not $git) {
        $Install = (Read-Host 'Git is not installed. Install it now? [Y/n]') -notmatch '^n'
    }
    $currentName = if ($git) { (& $git config --global user.name) } else { '' }
    $currentEmail = if ($git) { (& $git config --global user.email) } else { '' }
    Write-Host ''
    Write-Host 'Git stamps every change you save with a name and an email address.'
    Write-Host 'If you publish your work (for example on GitHub), others can see them.'
    Write-Host 'Leave both empty to skip this.'
    $Name = Read-WithDefault 'Your name' $currentName
    do {
        $Email = Read-WithDefault 'Your email' $currentEmail
        $ok = (-not $Email) -or (Test-Email $Email)
        if (-not $ok) { Write-Host 'That doesn''t look like an email address, for example jane@example.com.' }
    } while (-not $ok)
}

if (-not $git -and $Install) {
    if (-not (Install-GitWithWinget)) {
        Install-GitFromGitHub | Out-Null
    }
    $git = Find-Git
    if (-not $git) {
        Write-Log 'Git could not be installed.'
        Exit-Script 1
    }
    Write-Log "Git installed: $git"
}

if ($Name -or $Email) {
    if (-not $Name -or -not (Test-Email $Email)) {
        Write-Log 'Both a name and a valid email are needed.'
        Exit-Script 4
    }
    if (-not $git) {
        Write-Log 'Git is not installed, so the name and email were not saved.'
        Exit-Script 2
    }
    & $git config --global user.name $Name
    $nameCode = $LASTEXITCODE
    & $git config --global user.email $Email
    if ($nameCode -ne 0 -or $LASTEXITCODE -ne 0) {
        Write-Log 'git config failed.'
        Exit-Script 3
    }
    Write-Log "Saved git name '$Name' and email '$Email'."
}

Write-Log 'Done.'
Exit-Script 0
