<#
.SYNOPSIS
  Regenerates docs\screenshots\*.png. Runs nmm-gui against a throw-away demo
  config (your real config is never touched) and lets it save its own window
  contents, so nothing else on the screen can end up in the images.
  Usage: .\installer\screenshots.ps1 [-Gui target\release\nmm-gui.exe]
#>
param([string]$Gui = 'target\release\nmm-gui.exe')
$ErrorActionPreference = 'Stop'
$root = Split-Path (Split-Path $PSScriptRoot)
Set-Location $root

# A demo home so the config path shown in the window is neutral.
$demo = Join-Path $env:SystemDrive 'nmm-demo'
New-Item -ItemType Directory -Force "$demo\.config\not-magick-mouse" | Out-Null
Copy-Item docs\demo-config.toml "$demo\.config\not-magick-mouse\config.toml" -Force
New-Item -ItemType Directory -Force docs\screenshots | Out-Null

$shots = @(
  @('global',        'global'),
  @('mouse',         'mouse:vid_1532&pid_005c'),
  @('mouse-in-app',  'mouse:vid_046d&pid_b019,app:0'),
  @('edit-dpi',      'mouse:vid_046d&pid_b019+edit:x2'),
  @('edit-shortcut', 'global+edit:middle'),
  @('settings',      'settings')
)
$old = @{ U = $env:USERPROFILE; H = $env:HOME }
try {
  foreach ($s in $shots) {
    $env:USERPROFILE = $demo; $env:HOME = $demo
    $env:NMM_GUI_PAGE = $s[1]
    $env:NMM_GUI_SHOT = Join-Path $root "docs\screenshots\$($s[0]).png"
    Start-Process $Gui -Wait
  }
} finally {
  $env:USERPROFILE = $old.U; $env:HOME = $old.H
  Remove-Item Env:NMM_GUI_PAGE, Env:NMM_GUI_SHOT -ErrorAction SilentlyContinue
  Remove-Item $demo -Recurse -Force -ErrorAction SilentlyContinue
}
Write-Host 'Screenshots written to docs\screenshots'
