<#
.SYNOPSIS
  Builds the release binaries and produces, in dist\:
    not-magick-mouse-<version>-x86_64.msi         installer (cargo-wix, WiX Toolset v3)
    not-magick-mouse-<version>-portable.zip       nmm.exe + nmm-gui.exe, no install needed

.NOTES
  Requires: cargo install cargo-wix, and WiX Toolset v3 (https://github.com/wixtoolset/wix3/releases).
  If WiX is not on PATH, set the WIX environment variable to the folder that contains bin\candle.exe.
#>
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
Set-Location $root
$env:Path += ";$env:USERPROFILE\.cargo\bin"

cargo build --release
if ($LASTEXITCODE) { throw 'build failed' }

$version = (Select-String -Path daemon\Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$dist = Join-Path $root 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null

# MSI
cargo wix -p nmm --no-build --nocapture
if ($LASTEXITCODE) { throw 'cargo wix failed' }
$msi = Join-Path $dist "not-magick-mouse-$version-x86_64.msi"
Copy-Item "target\wix\nmm-$version-x86_64.msi" $msi -Force

# Portable zip
$stage = Join-Path $dist "portable"
Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item target\release\nmm.exe, target\release\nmm-gui.exe, README.md $stage
$zip = Join-Path $dist "not-magick-mouse-$version-portable.zip"
Remove-Item $zip -ErrorAction SilentlyContinue
Compress-Archive -Path "$stage\*" -DestinationPath $zip
Remove-Item $stage -Recurse -Force

Write-Host "Created:`n  $msi`n  $zip"
