<#
.SYNOPSIS
  Desinstalación limpia de Winarchy: revierte autostart, PATH, env vars y taskbar.
.DESCRIPTION
  No borra el repo ni los backups. Con -RemovePackages también desinstala
  komorebi/YASB vía winget (AHK, Flow, ShareX y Terminal se conservan: son
  herramientas generales de la máquina).
#>
[CmdletBinding()]
param([switch]$RemovePackages)
$ErrorActionPreference = 'Stop'
$Root = $PSScriptRoot

Import-Module (Join-Path $Root 'module\Winarchy\Winarchy.psd1') -Force

Write-Host "`n== Winarchy uninstall ==" -ForegroundColor Cyan

# 1. Parar servicios
if (Get-Process -Name komorebi -ErrorAction SilentlyContinue) { komorebic stop 2>$null | Out-Null }
Stop-Process -Name yasb -Force -ErrorAction SilentlyContinue
Get-Process -Name 'AutoHotkey*' -ErrorAction SilentlyContinue |
    Where-Object { $_.CommandLine -like '*winarchy.ahk*' } |
    Stop-Process -Force -ErrorAction SilentlyContinue
Write-WinarchyOk 'Servicios detenidos'

# El focus-follows-mouse modo "Windows" es un flag del SO que sobrevive a komorebi
Disable-WinarchyXMouse
Write-WinarchyOk 'Focus-follows-mouse del SO desactivado'

# 2. Autostart fuera (Scheduled Tasks + .lnk legacy)
Unregister-WinarchyAutostart
Write-WinarchyOk 'Autostart eliminado'

# 3. Taskbar restaurada (quitar auto-hide)
try {
    $stuck = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StuckRects3'
    $val = (Get-ItemProperty -Path $stuck -Name Settings).Settings
    $val[8] = $val[8] -band (-bnot 0x01)
    Set-ItemProperty -Path $stuck -Name Settings -Value $val
    Stop-Process -Name explorer -Force
    Write-WinarchyOk 'Taskbar nativa restaurada'
}
catch { Write-WinarchyWarn "Taskbar: $($_.Exception.Message)" }

# Revertir el delay de Startup al default de Windows (valor ausente)
Remove-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize' `
    -Name 'StartupDelayInMSec' -ErrorAction SilentlyContinue
Write-WinarchyOk 'Delay de Startup restaurado al default'

# 3b. Skill "winarchy" fuera de los agentes de IA
Uninstall-WinarchySkill

# 4. Env vars y PATH
[Environment]::SetEnvironmentVariable('KOMOREBI_CONFIG_HOME', $null, 'User')
[Environment]::SetEnvironmentVariable('YASB_CONFIG_HOME', $null, 'User')
$binDir = Join-Path $Root 'bin'
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
[Environment]::SetEnvironmentVariable('Path', (($userPath -split ';' | Where-Object { $_ -and $_ -ne $binDir }) -join ';'), 'User')
Write-WinarchyOk 'Env vars y PATH revertidos'

# 5. Paquetes (opcional)
if ($RemovePackages) {
    foreach ($id in @('LGUG2Z.komorebi', 'AmN.yasb')) {
        winget pin remove --id $id 2>$null | Out-Null
        winget uninstall --id $id --silent
    }
    Write-WinarchyOk 'komorebi y YASB desinstalados'
}

Write-Host ''
Write-WinarchyOk 'Winarchy desinstalado. Backups conservados en backups\.'
Write-WinarchyInfo 'Si venías de Seelen: .\scripts\rollback-to-seelen.ps1 lo reactiva.'
