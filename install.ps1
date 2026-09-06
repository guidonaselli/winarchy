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
    "$env:LOCALAPPDATA\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json",
    # Overrides del usuario: son locales y gitignored, así que un reinstall o un checkout
    # limpio se los lleva puestos sin dejar rastro si no se respaldan acá.
    (Join-Path $Root 'config\windows.toml'),
    (Join-Path $Root 'config\ahk\user.ahk'),
    (Join-Path $Root 'config\pwsh\user.ps1'),
    (Join-Path $Root 'config\wezterm\user.lua')
)
Write-WinarchyOk "Snapshot previo: $snapshot"

# Export de las Scheduled Tasks de Winarchy (si existieran) al snapshot, para rollback.
# Vía schtasks.exe (no usa CIM/MI, que está roto en algunas máquinas).
try {
    foreach ($name in @('komorebi', 'yasb', 'ahk')) {
        $xml = & schtasks.exe /Query /TN "\Winarchy\$name" /XML 2>$null
        if ($LASTEXITCODE -eq 0 -and $xml) {
            $xml | Set-Content -Path (Join-Path $snapshot "task-$name.xml") -Encoding Unicode
        }
    }
}
catch { Write-WinarchyWarn "No pude exportar tareas previas al snapshot: $($_.Exception.Message)" }

# --- 1. Paquetes (winget, versiones core fijadas) -------------------------------
$lock = Import-WinarchyToml -Path (Join-Path $Root 'versions.lock.toml')
$coreVersions = @{
    'LGUG2Z.komorebi' = $lock['core']['komorebi']
    'AmN.yasb'        = $lock['core']['yasb']
    'wez.wezterm'     = $lock['core']['wezterm']
}
# -SkipPackages (la migración de `winarchy update --self`) igual tiene que traer los
# componentes core que falten: si no, un core nuevo nunca llega a quien ya tenía Winarchy
# y el stack queda apuntando a un exe inexistente.
$packageIds = if ($SkipPackages) { @($coreVersions.Keys) } else { @($lock['winget'].Keys) }
if ($SkipPackages) { Write-WinarchyInfo 'Verificando que los componentes core estén presentes...' }
foreach ($id in $packageIds) {
    if (-not $lock['winget'][$id]) { continue }
    $installed = winget list --id $id 2>$null | Out-String
    if ($installed -match [regex]::Escape($id)) {
        if (-not $SkipPackages) { Write-WinarchyOk "$id ya instalado" }
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

# --- 2. Env vars de config → repo (fuente de verdad, estilo Omarchy) -------------
@('KOMOREBI_CONFIG_HOME', 'YASB_CONFIG_HOME', 'WEZTERM_CONFIG_FILE') |
    ForEach-Object { "$_=$([Environment]::GetEnvironmentVariable($_, 'User'))" } |
    Set-Content -Path (Join-Path $snapshot 'env-user.txt') -Encoding UTF8

$weztermConfig = Join-Path $Root 'config\wezterm\wezterm.lua'
[Environment]::SetEnvironmentVariable('KOMOREBI_CONFIG_HOME', (Join-Path $Root 'config\komorebi'), 'User')
[Environment]::SetEnvironmentVariable('YASB_CONFIG_HOME', (Join-Path $Root 'config\yasb'), 'User')
[Environment]::SetEnvironmentVariable('WEZTERM_CONFIG_FILE', $weztermConfig, 'User')
$env:KOMOREBI_CONFIG_HOME = Join-Path $Root 'config\komorebi'
$env:YASB_CONFIG_HOME = Join-Path $Root 'config\yasb'
$env:WEZTERM_CONFIG_FILE = $weztermConfig
Write-WinarchyOk 'KOMOREBI_CONFIG_HOME / YASB_CONFIG_HOME / WEZTERM_CONFIG_FILE registradas (User)'

# WEZTERM_CONFIG_FILE gana sobre el descubrimiento por home: si el usuario ya tenía su
# propio wezterm.lua, queda huérfano en silencio. Avisar dónde está y dónde va ahora.
foreach ($orphan in @("$env:USERPROFILE\.wezterm.lua", "$env:USERPROFILE\.config\wezterm\wezterm.lua")) {
    if (Test-Path $orphan) {
        Write-WinarchyWarn "Tu config previa de WezTerm ($orphan) ya no se carga: ahora manda $weztermConfig. Portá lo tuyo a config\wezterm\user.lua."
    }
}

# --- 3. PATH: comando winarchy ----------------------------------------------------
$binDir = Join-Path $Root 'bin'
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($userPath -notlike "*$binDir*") {
    [Environment]::SetEnvironmentVariable('Path', "$userPath;$binDir", 'User')
    Write-WinarchyOk "bin\ agregado al PATH de usuario (reabrir terminal para usar `winarchy`)"
}
else { Write-WinarchyOk 'bin\ ya estaba en PATH' }

# --- 3b. Terminal profile: PSFzf + hook en $PROFILE de pwsh --------------------------
if (-not $SkipPackages) {
    if (-not (Get-Module -ListAvailable PSFzf)) {
        Write-WinarchyInfo 'Instalando PSFzf (PSGallery)...'
        try { Install-Module PSFzf -Scope CurrentUser -Force -ErrorAction Stop }
        catch { Write-WinarchyWarn "No pude instalar PSFzf: $($_.Exception.Message)" }
    }
    else { Write-WinarchyOk 'PSFzf ya instalado' }
}
Install-WinarchyShellProfile

# --- 3c. Skill "winarchy" en los agentes de IA detectados (estilo Omarchy) ----------
Install-WinarchySkill

# --- 3d. Comandos de Winarchy visibles desde el lanzador ------------------------------
Sync-WinarchyPalette

# --- 4. Theme (genera todos los configs) --------------------------------------------
# Siempre re-renderiza, no solo en la primera instalacion: install.ps1 es tambien la
# migracion de `winarchy update --self`, y sin esto un cambio en templates/ nunca
# llegaba a los configs de quien ya tenia un theme seteado.
$initialTheme = Get-WinarchyCurrentTheme
if ($initialTheme) {
    Write-WinarchyInfo "Regenerando configs desde los templates (theme: $initialTheme)..."
} else {
    $initialTheme = 'tokyo-night'
    Write-WinarchyInfo 'Generando configs con el theme inicial (tokyo-night)...'
}
Set-WinarchyTheme -Name $initialTheme

# --- 4b. Identidad unificada: tray único + auto-updates de terceros off ---------------
# Idempotente: se reaplica en cada install/repair, así un update de Flow no reintroduce
# su tray/auto-update. YASB ya queda configurado por su config.yaml (show_systray/
# update_check off); AHK hostea el tray del stack; komorebi no tiene tray.
Set-WinarchyFlowIdentity
Set-WinarchyFlowAppsKeyword

# --- 5. Autostart + taskbar (solo con -Activate) --------------------------------------
if ($Activate) {
    $komorebic = (Get-Command komorebic -ErrorAction SilentlyContinue).Source
    $yasb = (Get-Command yasbc -ErrorAction SilentlyContinue).Source
    $ahkExe = Get-WinarchyAhkExe

    # Scheduled Tasks At-LogOn (disparo más temprano que la carpeta Startup). Migra
    # borrando los .lnk legacy; idempotente; fallback a Startup si el registro falla.
    Register-WinarchyAutostart

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
    # Migración / self-update (sin -Activate): si el autostart YA estaba activo, re-registrarlo
    # para que tome cambios en cómo se lanzan los componentes (p.ej. el launcher resiliente
    # scripts\Start-Komorebi.ps1) sin cambiar el modo de operación vigente. Si nunca estuvo
    # activo (convivencia pura), no se toca nada.
    $autostart = Get-WinarchyAutostartStatus
    if (@($autostart.Values | Where-Object { $_ }).Count -gt 0) {
        Write-WinarchyInfo 'Autostart activo: re-registrando para aplicar cambios de arranque...'
        Register-WinarchyAutostart
    }
    else {
        Write-WinarchyInfo 'Modo CONVIVENCIA: sin autostart. Para activar el stack: .\install.ps1 -Activate'
        Write-WinarchyInfo 'Para migrar desde Seelen: .\scripts\migrate-from-seelen.ps1'
    }
}

Write-Host ''
Write-WinarchyOk 'Instalación completa. Verificá con: winarchy doctor'
