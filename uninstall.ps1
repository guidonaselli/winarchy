<#
.SYNOPSIS
  Desinstalación limpia de Winarchy: revierte autostart, PATH, env vars, pins y taskbar.
.DESCRIPTION
  No borra el repo ni los backups. Con -RemovePackages también desinstala
  komorebi/YASB vía winget (AHK, Flow, ShareX, WezTerm y Windows Terminal se
  conservan: son herramientas generales de la máquina).
  WEZTERM_CONFIG_FILE se revierte igual, así WezTerm vuelve a su config propia en vez
  de apuntar a un archivo del repo que puede ya no existir.

  -DryRun lista todo lo que haría sin tocar nada. Existe porque este script solo se
  puede probar de verdad destruyendo el setup en uso: sin dry-run quedaba permanentemente
  sin verificar.
#>
[CmdletBinding()]
param([switch]$RemovePackages, [switch]$DryRun)
$ErrorActionPreference = 'Stop'
$Root = $PSScriptRoot

Import-Module (Join-Path $Root 'module\Winarchy\Winarchy.psd1') -Force

# Pins que pone `winarchy update --core`: hay que sacarlos todos o el usuario queda con
# paquetes pinneados en winget después de desinstalar Winarchy.
$CorePinIds = @('LGUG2Z.komorebi', 'AmN.yasb', 'Flow-Launcher.Flow-Launcher', 'AutoHotkey.AutoHotkey', 'wez.wezterm')

Write-Host "`n== Winarchy uninstall ==" -ForegroundColor Cyan
if ($DryRun) { Write-WinarchyInfo 'DRY RUN: no se modifica nada.' }

function Invoke-Step {
    <# Ejecuta el paso, o solo lo describe si es dry-run. #>
    param([string]$Describe, [scriptblock]$Action, [string]$Done)
    if ($DryRun) { Write-Host "  [ ] $Describe"; return }
    try { & $Action; Write-WinarchyOk $Done }
    catch { Write-WinarchyWarn "$($Describe): $($_.Exception.Message)" }
}

Invoke-Step 'Detener komorebi, YASB y el AHK de Winarchy' {
    Stop-WinarchyWindowSlots
    if (Get-Process -Name komorebi -ErrorAction SilentlyContinue) { komorebic stop 2>$null | Out-Null }
    Stop-Process -Name yasb -Force -ErrorAction SilentlyContinue
    Get-Process -Name 'AutoHotkey*' -ErrorAction SilentlyContinue |
        Where-Object { $_.CommandLine -like '*winarchy.ahk*' } |
        Stop-Process -Force -ErrorAction SilentlyContinue
} 'Servicios detenidos'

# El focus-follows-mouse modo "Windows" es un flag del SO que sobrevive a komorebi
Invoke-Step 'Desactivar el focus-follows-mouse del SO' { Disable-WinarchyXMouse } `
    'Focus-follows-mouse del SO desactivado'

Invoke-Step 'Eliminar el autostart (Scheduled Tasks + .lnk legacy)' { Unregister-WinarchyAutostart } `
    'Autostart eliminado'

Invoke-Step 'Restaurar la taskbar nativa (quitar auto-hide)' {
    $stuck = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StuckRects3'
    $val = (Get-ItemProperty -Path $stuck -Name Settings).Settings
    $val[8] = $val[8] -band (-bnot 0x01)
    Set-ItemProperty -Path $stuck -Name Settings -Value $val
    Stop-Process -Name explorer -Force
} 'Taskbar nativa restaurada'

Invoke-Step 'Revertir el delay de Startup al default de Windows' {
    Remove-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize' `
        -Name 'StartupDelayInMSec' -ErrorAction SilentlyContinue
} 'Delay de Startup restaurado al default'

Invoke-Step 'Quitar la skill "winarchy" de los agentes de IA' { Uninstall-WinarchySkill } `
    'Skill desinstalada'

Invoke-Step 'Quitar los comandos de Winarchy del menu de inicio' { Remove-WinarchyPalette } `
    'Paleta de comandos quitada'

Invoke-Step 'Revertir env vars (KOMOREBI_CONFIG_HOME, YASB_CONFIG_HOME, WEZTERM_CONFIG_FILE) y sacar bin\ del PATH' {
    [Environment]::SetEnvironmentVariable('KOMOREBI_CONFIG_HOME', $null, 'User')
    [Environment]::SetEnvironmentVariable('YASB_CONFIG_HOME', $null, 'User')
    [Environment]::SetEnvironmentVariable('WEZTERM_CONFIG_FILE', $null, 'User')
    $binDir = Join-Path $Root 'bin'
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    [Environment]::SetEnvironmentVariable('Path', (($userPath -split ';' | Where-Object { $_ -and $_ -ne $binDir }) -join ';'), 'User')
} 'Env vars y PATH revertidos'

Invoke-Step "Quitar los pins de winget ($($CorePinIds -join ', '))" {
    foreach ($id in $CorePinIds) { winget pin remove --id $id 2>$null | Out-Null }
} 'Pins de winget eliminados'

if ($RemovePackages) {
    Invoke-Step 'Desinstalar komorebi y YASB vía winget' {
        foreach ($id in @('LGUG2Z.komorebi', 'AmN.yasb')) { winget uninstall --id $id --silent }
    } 'komorebi y YASB desinstalados'
}

Write-Host ''
if ($DryRun) {
    Write-WinarchyInfo 'DRY RUN terminado: no se modificó nada. Corré sin -DryRun para aplicarlo.'
    return
}
Write-WinarchyOk 'Winarchy desinstalado. Backups conservados en backups\.'
Write-WinarchyInfo 'Si venías de Seelen: .\scripts\rollback-to-seelen.ps1 lo reactiva.'
