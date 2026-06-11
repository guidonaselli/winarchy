<#
.SYNOPSIS
  Rollback Winarchy → Seelen UI (válido en cualquier punto del período de prueba).
.DESCRIPTION
  1. Detiene el stack Winarchy y quita su autostart (sin desinstalar nada).
  2. Restaura el autostart de Seelen (Run keys / shortcuts respaldados por la migración).
  3. Relanza Seelen.
#>
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$Root = Split-Path $PSScriptRoot -Parent

Import-Module (Join-Path $Root 'module\Winarchy\Winarchy.psd1') -Force

Write-Host "`n== Rollback Winarchy → Seelen ==" -ForegroundColor Cyan

# 1. Parar Winarchy y sacar su autostart
if (Get-Process -Name komorebi -ErrorAction SilentlyContinue) { komorebic stop 2>$null | Out-Null }
Stop-Process -Name yasb -Force -ErrorAction SilentlyContinue
Get-Process -Name 'AutoHotkey*' -ErrorAction SilentlyContinue |
    Where-Object { $_.CommandLine -like '*winarchy.ahk*' } |
    Stop-Process -Force -ErrorAction SilentlyContinue
$startup = [Environment]::GetFolderPath('Startup')
Remove-Item (Join-Path $startup 'Winarchy *.lnk') -Force -ErrorAction SilentlyContinue
# El focus-follows-mouse modo "Windows" es un flag del SO que sobrevive a komorebi
Disable-WinarchyXMouse
Write-WinarchyOk 'Stack Winarchy detenido y sin autostart'

# Taskbar nativa de vuelta (Seelen la maneja a su modo)
try {
    $stuck = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StuckRects3'
    $val = (Get-ItemProperty -Path $stuck -Name Settings).Settings
    $val[8] = $val[8] -band (-bnot 0x01)
    Set-ItemProperty -Path $stuck -Name Settings -Value $val
    Stop-Process -Name explorer -Force
}
catch { Write-WinarchyWarn "Taskbar: $($_.Exception.Message)" }

# 2. Restaurar autostart de Seelen
$stateDir = Get-WinarchyStateDir
foreach ($bak in Get-ChildItem $stateDir -Filter 'seelen-run-*.bak' -ErrorAction SilentlyContinue) {
    $name = $bak.BaseName -replace '^seelen-run-', ''
    $value = (Get-Content $bak.FullName -Raw).Trim()
    Set-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name $name -Value $value
    Remove-Item $bak.FullName -Force
    Write-WinarchyOk "Run key de Seelen restaurada: $name"
}
$movedDir = Join-Path $stateDir 'startup-moved'
if (Test-Path $movedDir) {
    foreach ($lnk in Get-ChildItem $movedDir -File) {
        Move-Item $lnk.FullName (Join-Path $startup $lnk.Name) -Force
        Write-WinarchyOk "Startup restaurado: $($lnk.Name)"
    }
}

# 3. Relanzar Seelen
$seelenExe = @(
    "$env:LOCALAPPDATA\Programs\seelen-ui\seelen-ui.exe",
    "$env:ProgramFiles\Seelen UI\seelen-ui.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if ($seelenExe) { Start-Process $seelenExe; Write-WinarchyOk 'Seelen UI relanzado' }
else { Write-WinarchyWarn 'No encontré seelen-ui.exe; lanzalo manualmente o reiniciá sesión.' }

Write-Host ''
Write-WinarchyOk 'Rollback completo. Winarchy queda instalado pero inactivo.'
