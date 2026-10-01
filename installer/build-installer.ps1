$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    $version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
    Write-Host "Building YASAFF $version..."

    if (-not (Get-Command cargo.exe -ErrorAction SilentlyContinue)) {
        throw 'Rust (cargo) was not found. Install it from https://rustup.rs, then open a new terminal.'
    }
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed.' }

    # ffmpeg (LGPL build) goes into {app}\tools, where YASAFF looks before PATH.
    $ffmpegName = 'ffmpeg-n8.1-latest-win64-lgpl-8.1'
    $release = 'https://github.com/BtbN/FFmpeg-Builds/releases/download/latest'
    $cache = Join-Path $root 'target\bundle\cache'
    $tools = Join-Path $root 'target\bundle\tools'
    New-Item -ItemType Directory -Force -Path $cache | Out-Null
    if (Test-Path -LiteralPath $tools) { Remove-Item -Recurse -Force -LiteralPath $tools }
    New-Item -ItemType Directory -Force -Path $tools | Out-Null

    $zip = Join-Path $cache "$ffmpegName.zip"
    $sums = Join-Path $cache 'ffmpeg-checksums.sha256'
    Write-Host 'Downloading ffmpeg checksums...'
    Invoke-WebRequest -UseBasicParsing -Uri "$release/checksums.sha256" -OutFile $sums
    $line = Select-String -Path $sums -SimpleMatch "$ffmpegName.zip" | Select-Object -First 1
    if (-not $line) { throw "No checksum for $ffmpegName.zip in the ffmpeg release." }
    $expected = ($line.Line -split '\s+')[0].ToLower()
    if (-not (Test-Path -LiteralPath $zip) -or (Get-FileHash -Algorithm SHA256 -LiteralPath $zip).Hash.ToLower() -ne $expected) {
        Write-Host "Downloading $ffmpegName.zip..."
        Invoke-WebRequest -UseBasicParsing -Uri "$release/$ffmpegName.zip" -OutFile $zip
        if ((Get-FileHash -Algorithm SHA256 -LiteralPath $zip).Hash.ToLower() -ne $expected) {
            Remove-Item -Force -LiteralPath $zip
            throw 'The ffmpeg download does not match its checksum.'
        }
    }
    $unpacked = Join-Path $cache $ffmpegName
    if (Test-Path -LiteralPath $unpacked) { Remove-Item -Recurse -Force -LiteralPath $unpacked }
    Expand-Archive -LiteralPath $zip -DestinationPath $cache
    Copy-Item -LiteralPath (Join-Path $unpacked 'bin\ffmpeg.exe') -Destination $tools
    Copy-Item -LiteralPath (Join-Path $unpacked 'LICENSE.txt') -Destination (Join-Path $tools 'ffmpeg-LICENSE.txt')
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'THIRD-PARTY.txt') -Destination $tools

    $iscc = (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source
    if (-not $iscc) {
        $iscc = foreach ($v in 7, 6) {
            "$env:ProgramFiles\Inno Setup $v\ISCC.exe"
            "${env:ProgramFiles(x86)}\Inno Setup $v\ISCC.exe"
            "$env:LOCALAPPDATA\Programs\Inno Setup $v\ISCC.exe"
        }
        $iscc = $iscc | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
    }
    if (-not $iscc) {
        throw 'Inno Setup was not found. Install it with: winget install -e --id JRSoftware.InnoSetup'
    }

    & $iscc "/DAppVersion=$version" installer\yasaff.iss
    if ($LASTEXITCODE -ne 0) { throw 'Inno Setup failed.' }
    Write-Host ''
    Write-Host "Installer: $root\target\installer\YASAFF-Setup-$version.exe"
} finally {
    Pop-Location
}
