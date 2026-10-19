# Remove the per-user installation and autostart entry.
$ErrorActionPreference = "Stop"

Get-Process -Name "mchose-battery" -ErrorAction SilentlyContinue | Stop-Process -Force

$runKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
if (Test-Path $runKey) {
    Remove-ItemProperty -Path $runKey -Name "MCHOSE Battery" -ErrorAction SilentlyContinue
}

$shortcutPath = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\MCHOSE Battery.lnk"
Remove-Item -Force -LiteralPath $shortcutPath -ErrorAction SilentlyContinue

$destDir = Join-Path $env:LOCALAPPDATA "MCHOSE Battery"
Remove-Item -Recurse -Force -LiteralPath $destDir -ErrorAction SilentlyContinue

Write-Host "已卸载 MCHOSE Battery。"
