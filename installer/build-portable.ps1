$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    $version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
    Write-Host "Building YASAFF $version (portable)..."

    if (-not (Get-Command cargo.exe -ErrorAction SilentlyContinue)) {
        throw 'Rust (cargo) was not found. Install it from https://rustup.rs, then open a new terminal.'
    }
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed.' }

    # ffmpeg (LGPL build) goes next to yasaff.exe, in a "tools" folder, where YASAFF looks before PATH.
    $ffmpegName = 'ffmpeg-n8.1-latest-win64-lgpl-8.1'
    $release = 'https://github.com/BtbN/FFmpeg-Builds/releases/download/latest'
    $cache = Join-Path $root 'target\bundle\cache'
    New-Item -ItemType Directory -Force -Path $cache | Out-Null

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

    # Assemble the portable folder, then zip it.
    $stageName = "YASAFF-$version-portable-win64"
    $stage = Join-Path $root "target\portable\$stageName"
    if (Test-Path -LiteralPath $stage) { Remove-Item -Recurse -Force -LiteralPath $stage }
    $tools = Join-Path $stage 'tools'
    New-Item -ItemType Directory -Force -Path $tools | Out-Null

    Copy-Item -LiteralPath (Join-Path $root 'target\release\yasaff.exe') -Destination $stage
    Copy-Item -LiteralPath (Join-Path $unpacked 'bin\ffmpeg.exe') -Destination $tools
    Copy-Item -LiteralPath (Join-Path $unpacked 'LICENSE.txt') -Destination (Join-Path $tools 'ffmpeg-LICENSE.txt')
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'THIRD-PARTY.txt') -Destination $tools
    Copy-Item -LiteralPath (Join-Path $root 'LICENSE') -Destination (Join-Path $stage 'LICENSE.txt')
    # Tells yasaff.exe this is the portable build; on first run it asks where to keep its data
    # and writes the answer back here ("system" or "portable") instead of asking again.
    Set-Content -LiteralPath (Join-Path $stage 'portable-storage.txt') -Value 'ask' -NoNewline

    $zipOut = Join-Path $root "target\portable\$stageName.zip"
    if (Test-Path -LiteralPath $zipOut) { Remove-Item -Force -LiteralPath $zipOut }
    Compress-Archive -Path $stage -DestinationPath $zipOut
    Write-Host ''
    Write-Host "Portable build: $zipOut"
} finally {
    Pop-Location
}
