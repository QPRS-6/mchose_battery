# Install MCHOSE Battery into %LOCALAPPDATA% and start it with Windows.
# Works from a GitHub Actions artifact (exe next to this script) or from the
# project root after `cargo build --release`:
#   powershell -ExecutionPolicy Bypass -File packaging\windows\install.ps1
param(
    [string]$Exe = ""
)

$ErrorActionPreference = "Stop"

if (-not $Exe) {
    $local = Join-Path $PSScriptRoot "mchose-battery.exe"
    $built = Join-Path $PSScriptRoot "..\..\target\release\mchose-battery.exe"
    if (Test-Path -LiteralPath $local) {
        $Exe = $local
    } elseif (Test-Path -LiteralPath $built) {
        $Exe = $built
    } else {
        throw "找不到 mchose-battery.exe。请把它放在 install.ps1 同一目录，或先运行 cargo build --release。"
    }
}
if (-not (Test-Path -LiteralPath $Exe)) {
    throw "找不到 $Exe。请先在 Windows 上运行 cargo build --release。"
}

$destDir = Join-Path $env:LOCALAPPDATA "MCHOSE Battery"
New-Item -ItemType Directory -Force -Path $destDir | Out-Null
$dest = Join-Path $destDir "mchose-battery.exe"
Copy-Item -Force -LiteralPath $Exe -Destination $dest
Unblock-File -LiteralPath $dest -ErrorAction SilentlyContinue

$runKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
New-Item -Path $runKey -Force | Out-Null
Set-ItemProperty -Path $runKey -Name "MCHOSE Battery" -Value "`"$dest`""

# Optional Start Menu shortcut.
$startMenu = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
$shortcutPath = Join-Path $startMenu "MCHOSE Battery.lnk"
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath = $dest
$shortcut.WorkingDirectory = $destDir
$shortcut.Description = "MCHOSE mouse battery"
$shortcut.Save()

Get-Process -Name "mchose-battery" -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Process -FilePath $dest
Write-Host "已安装并启动：$dest"
Write-Host "开机启动已写入 HKCU Run；可在托盘菜单里关闭。"
