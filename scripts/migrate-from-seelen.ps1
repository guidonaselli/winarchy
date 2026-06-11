<#
.SYNOPSIS
  Migración Seelen UI → Winarchy (paso 3 del Migration Plan del design).
.DESCRIPTION
  1. Backup completo de la config de Seelen a backups\<ts>-seelen\.
  2. Detiene Seelen y deshabilita su autostart (Run keys + Startup + su propio flag).
  3. Activa el autostart de Winarchy (install.ps1 -Activate).
  4. Corre winarchy doctor.
  NO desinstala Seelen: eso es manual, recién tras una semana en verde
  (rollback disponible en cualquier momento: rollback-to-seelen.ps1).
#>
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$Root = Split-Path $PSScriptRoot -Parent

Import-Module (Join-Path $Root 'module\Winarchy\Winarchy.psd1') -Force

Write-Host "`n== Migración Seelen → Winarchy ==" -ForegroundColor Cyan

# 1. Backup de Seelen
$seelenConfig = "$env:APPDATA\com.seelen.seelen-ui"
if (Test-Path $seelenConfig) {
    $snapshot = New-WinarchySnapshot -Path @($seelenConfig) -Label 'seelen'
    Write-WinarchyOk "Config de Seelen respaldada en $snapshot"
}
else {
    Write-WinarchyWarn 'Config de Seelen no encontrada (¿ya migrado o no instalado?).'
}

# 2. Parar Seelen y sacar su autostart
Stop-Process -Name 'seelen-ui' -Force -ErrorAction SilentlyContinue
foreach ($runKey in @('HKCU:\Software\Microsoft\Windows\CurrentVersion\Run')) {
    $props = Get-ItemProperty -Path $runKey -ErrorAction SilentlyContinue
    if ($props) {
        foreach ($name in ($props.PSObject.Properties.Name | Where-Object { $_ -match 'seelen' })) {
            # se guarda el valor para poder restaurarlo en el rollback
            $value = $props.$name
            Set-Content -Path (Join-Path (Get-WinarchyStateDir) "seelen-run-$name.bak") -Value $value -Encoding UTF8
            Remove-ItemProperty -Path $runKey -Name $name
            Write-WinarchyOk "Autostart de Seelen removido: $name"
        }
    }
}
$startup = [Environment]::GetFolderPath('Startup')
$wsh = New-Object -ComObject WScript.Shell
$legacy = @(Get-ChildItem $startup -File -ErrorAction SilentlyContinue | Where-Object {
    # Seelen + cualquier script AHK legado (ej. win-space-launcher.ahk):
    # AHK v2 queda con un solo dueño, winarchy.ahk
    $_.Name -match 'seelen' -or
    ($_.Extension -eq '.ahk' -and $_.Name -notmatch 'winarchy') -or
    ($_.Extension -eq '.lnk' -and $_.Name -notmatch '^Winarchy ' -and
        "$($wsh.CreateShortcut($_.FullName).TargetPath) $($wsh.CreateShortcut($_.FullName).Arguments)" -match 'AutoHotkey|\.ahk')
})
$movedDir = Join-Path (Get-WinarchyStateDir) 'startup-moved'
if ($legacy) { New-Item -ItemType Directory -Force -Path $movedDir | Out-Null }
foreach ($item in $legacy) {
    Move-Item $item.FullName (Join-Path $movedDir $item.Name) -Force
    Write-WinarchyOk "Startup legado movido a state\startup-moved\: $($item.Name)"
}

# 3. Activar Winarchy
& (Join-Path $Root 'install.ps1') -Activate -SkipPackages

# 4. Verificación
Write-WinarchyInfo 'Corriendo doctor...'
$failed = Invoke-WinarchyDoctor
if ($failed -eq 0) {
    Write-WinarchyOk 'Migración completa y en verde. Seelen queda instalado pero inactivo durante el período de prueba.'
}
else {
    Write-WinarchyWarn 'Hay chequeos en rojo: NO desinstales Seelen todavía. Rollback: .\scripts\rollback-to-seelen.ps1'
}
