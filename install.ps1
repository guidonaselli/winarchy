<#
.SYNOPSIS
  Instalador idempotente de Winarchy.
.DESCRIPTION
  - Instala componentes vía winget (versiones core fijadas con winget pin).
  - Registra KOMOREBI_CONFIG_HOME / YASB_CONFIG_HOME apuntando al repo.
  - Agrega bin\ al PATH de usuario (comando `winarchy`).
  - Por defecto instala en MODO CONVIVENCIA (sin autostart, Seelen sigue mandando).
    Con -Activate registra autostart (Startup) y oculta la taskbar nativa (auto-hide).
  - Snapshot en backups\<timestamp>\ antes de tocar nada. Re-ejecutable sin daño.
.EXAMPLE
  .\install.ps1            # convivencia: instala todo, no toca autostart
  .\install.ps1 -Activate  # activa Winarchy como shell experience
#>
[CmdletBinding()]
param(
    [switch]$Activate,
    [switch]$SkipPackages
)
$ErrorActionPreference = 'Stop'
$Root = $PSScriptRoot

Import-Module (Join-Path $Root 'module\Winarchy\Winarchy.psd1') -Force

Write-Host "`n== Winarchy install ==" -ForegroundColor Cyan

# --- 0. Snapshot previo -------------------------------------------------------
$startup = [Environment]::GetFolderPath('Startup')
$snapshot = New-WinarchySnapshot -Label 'pre-install' -Path @(
    $startup,
    "$env:LOCALAPPDATA\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json"
)
Write-WinarchyOk "Snapshot previo: $snapshot"

# --- 1. Paquetes (winget, versiones core fijadas) -------------------------------
if (-not $SkipPackages) {
    $lock = Import-WinarchyToml -Path (Join-Path $Root 'versions.lock.toml')
    $coreVersions = @{ 'LGUG2Z.komorebi' = $lock['core']['komorebi']; 'AmN.yasb' = $lock['core']['yasb'] }
    foreach ($id in $lock['winget'].Keys) {
        if (-not $lock['winget'][$id]) { continue }
        $installed = winget list --id $id 2>$null | Out-String
        if ($installed -match [regex]::Escape($id)) {
            Write-WinarchyOk "$id ya instalado"
        }
        else {
            Write-WinarchyInfo "Instalando $id ..."
            $args = @('install', '--id', $id, '--silent', '--accept-package-agreements', '--accept-source-agreements')
            if ($coreVersions.ContainsKey($id)) { $args += @('--version', $coreVersions[$id]) }
            winget @args
        }
        if ($coreVersions.ContainsKey($id)) {
            winget pin add --id $id 2>$null | Out-Null   # update general nunca toca el core
        }
    }
}

# --- 2. Env vars de config → repo (fuente de verdad, estilo Omarchy) -------------
[Environment]::SetEnvironmentVariable('KOMOREBI_CONFIG_HOME', (Join-Path $Root 'config\komorebi'), 'User')
[Environment]::SetEnvironmentVariable('YASB_CONFIG_HOME', (Join-Path $Root 'config\yasb'), 'User')
$env:KOMOREBI_CONFIG_HOME = Join-Path $Root 'config\komorebi'
$env:YASB_CONFIG_HOME = Join-Path $Root 'config\yasb'
Write-WinarchyOk 'KOMOREBI_CONFIG_HOME / YASB_CONFIG_HOME registradas (User)'

# --- 3. PATH: comando winarchy ----------------------------------------------------
$binDir = Join-Path $Root 'bin'
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($userPath -notlike "*$binDir*") {
    [Environment]::SetEnvironmentVariable('Path', "$userPath;$binDir", 'User')
    Write-WinarchyOk "bin\ agregado al PATH de usuario (reabrir terminal para usar `winarchy`)"
}
else { Write-WinarchyOk 'bin\ ya estaba en PATH' }

# --- 4. Theme inicial (genera todos los configs) ------------------------------------
if (-not (Get-WinarchyCurrentTheme)) {
    Write-WinarchyInfo 'Generando configs con el theme inicial (tokyo-night)...'
    Set-WinarchyTheme -Name 'tokyo-night'
}

# --- 5. Autostart + taskbar (solo con -Activate) --------------------------------------
if ($Activate) {
    $shell = New-Object -ComObject WScript.Shell

    $komorebic = (Get-Command komorebic -ErrorAction SilentlyContinue).Source
    if ($komorebic) {
        $lnk = $shell.CreateShortcut((Join-Path $startup 'Winarchy komorebi.lnk'))
        $lnk.TargetPath = $komorebic; $lnk.Arguments = 'start'; $lnk.Save()
    }

    $yasb = (Get-Command yasbc -ErrorAction SilentlyContinue).Source
    if ($yasb) {
        $lnk = $shell.CreateShortcut((Join-Path $startup 'Winarchy YASB.lnk'))
        $lnk.TargetPath = $yasb; $lnk.Arguments = 'start'; $lnk.Save()
    }

    $ahkExe = Get-WinarchyAhkExe
    if ($ahkExe) {
        $lnk = $shell.CreateShortcut((Join-Path $startup 'Winarchy hotkeys.lnk'))
        $lnk.TargetPath = $ahkExe; $lnk.Arguments = "`"$Root\config\ahk\winarchy.ahk`""; $lnk.Save()
    }
    Write-WinarchyOk 'Autostart registrado (Startup): komorebi, YASB, AHK'

    # Taskbar nativa en auto-hide (no oculta del todo: ver design D6/riesgos)
    try {
        $stuck = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StuckRects3'
        $val = (Get-ItemProperty -Path $stuck -Name Settings).Settings
        $val[8] = $val[8] -bor 0x01
        Set-ItemProperty -Path $stuck -Name Settings -Value $val
        Stop-Process -Name explorer -Force
        Write-WinarchyOk 'Taskbar nativa en auto-hide'
    }
    catch { Write-WinarchyWarn "No pude poner la taskbar en auto-hide: $($_.Exception.Message)" }

    # Sin el delay artificial (~10 s) que Explorer impone a las apps de Startup,
    # komorebi/YASB/AHK levantan apenas inicia la sesión.
    try {
        $serialize = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize'
        if (-not (Test-Path $serialize)) { New-Item $serialize -Force | Out-Null }
        Set-ItemProperty -Path $serialize -Name 'StartupDelayInMSec' -Value 0 -Type DWord
        Write-WinarchyOk 'Delay de apps de Startup eliminado (StartupDelayInMSec=0)'
    }
    catch { Write-WinarchyWarn "No pude quitar el delay de Startup: $($_.Exception.Message)" }

    Write-WinarchyInfo 'Arrancando servicios...'
    if ($komorebic) { & $komorebic start }
    if ($yasb) { & $yasb start }
    if ($ahkExe) { Start-Process $ahkExe -ArgumentList "`"$Root\config\ahk\winarchy.ahk`"" }
}
else {
    Write-WinarchyInfo 'Modo CONVIVENCIA: sin autostart. Para activar el stack: .\install.ps1 -Activate'
    Write-WinarchyInfo 'Para migrar desde Seelen: .\scripts\migrate-from-seelen.ps1'
}

Write-Host ''
Write-WinarchyOk 'Instalación completa. Verificá con: winarchy doctor'
