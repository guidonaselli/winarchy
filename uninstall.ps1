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
$CorePinIds = @((Get-WinarchyCoreVersions -Lock (Import-WinarchyToml -Path (Join-Path $Root 'versions.lock.toml'))).Keys | Sort-Object)

Write-Host "`n== Winarchy uninstall ==" -ForegroundColor Cyan
if ($DryRun) { Write-WinarchyInfo 'DRY RUN: nothing is changed.' }

function Invoke-Step {
    <# Ejecuta el paso, o solo lo describe si es dry-run. #>
    param([string]$Describe, [scriptblock]$Action, [string]$Done)
    if ($DryRun) { Write-Host "  [ ] $Describe"; return }
    try { & $Action; Write-WinarchyOk $Done }
    catch { Write-WinarchyWarn "$($Describe): $($_.Exception.Message)" }
}

Invoke-Step 'Stop komorebi, YASB and the Winarchy AHK script' {
    Stop-WinarchyWindowSlots
    if (Get-Process -Name komorebi -ErrorAction SilentlyContinue) { komorebic stop 2>$null | Out-Null }
    Stop-Process -Name yasb -Force -ErrorAction SilentlyContinue
    Get-Process -Name 'AutoHotkey*' -ErrorAction SilentlyContinue |
        Where-Object { $_.CommandLine -like '*winarchy.ahk*' } |
        Stop-Process -Force -ErrorAction SilentlyContinue
} 'Stack stopped'

# El focus-follows-mouse modo "Windows" es un flag del SO que sobrevive a komorebi
Invoke-Step 'Turn off the Windows focus-follows-mouse' { Disable-WinarchyXMouse } `
    'Windows focus-follows-mouse off'

Invoke-Step 'Remove the autostart (scheduled tasks + legacy .lnk)' { Unregister-WinarchyAutostart } `
    'Autostart removed'

Invoke-Step 'Restore the Windows taskbar (turn off auto-hide)' {
    $null = Set-WinarchyTaskbarAutoHide -Enabled $false
} 'Windows taskbar restored'

Invoke-Step 'Revert the Windows hardening (Bing, suggestions, ads)' {
    $null = Set-WinarchyWindowsHardening -Revert
} 'Windows hardening reverted'

Invoke-Step 'Restore the default Startup app delay' {
    Remove-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize' `
        -Name 'StartupDelayInMSec' -ErrorAction SilentlyContinue
} 'Startup app delay restored'

Invoke-Step 'Remove the "winarchy" skill from AI coding agents' { Uninstall-WinarchySkill } `
    'Skill removed'

Invoke-Step 'Remove the Winarchy commands from the Start menu' { Remove-WinarchyPalette } `
    'Command palette removed'

Invoke-Step 'Remove the Winarchy hook from the pwsh $PROFILE' { Remove-WinarchyShellProfile } `
    'Shell profile checked'

Invoke-Step 'Remove the Winarchy Defender exclusions' { Remove-WinarchyDefenderExclusions } `
    'Defender exclusions checked'

Invoke-Step 'Remove the env vars (KOMOREBI_CONFIG_HOME, YASB_CONFIG_HOME, WEZTERM_CONFIG_FILE) and bin\ from PATH' {
    [Environment]::SetEnvironmentVariable('KOMOREBI_CONFIG_HOME', $null, 'User')
    [Environment]::SetEnvironmentVariable('YASB_CONFIG_HOME', $null, 'User')
    [Environment]::SetEnvironmentVariable('WEZTERM_CONFIG_FILE', $null, 'User')
    $binDir = Join-Path $Root 'bin'
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    [Environment]::SetEnvironmentVariable('Path', (($userPath -split ';' | Where-Object { $_ -and $_ -ne $binDir }) -join ';'), 'User')
} 'Env vars and PATH reverted'

Invoke-Step "Remove the winget pins ($($CorePinIds -join ', '))" {
    foreach ($id in $CorePinIds) { winget pin remove --id $id --exact 2>$null | Out-Null }
} 'winget pins removed'

if ($RemovePackages) {
    Invoke-Step 'Uninstall komorebi and YASB with winget' {
        foreach ($id in @('LGUG2Z.komorebi', 'AmN.yasb')) { winget uninstall --id $id --exact --silent }
    } 'komorebi and YASB uninstalled'
}

Write-Host ''
if ($DryRun) {
    Write-WinarchyInfo 'DRY RUN done: nothing was changed. Run it without -DryRun to apply it.'
    return
}
Write-WinarchyOk "Winarchy uninstalled. Backups are kept in $Root\backups; delete $Root when you no longer need them."
