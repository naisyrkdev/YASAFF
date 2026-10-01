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
