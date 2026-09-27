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
#Requires -Version 7.0
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
Write-WinarchyOk "Pre-install snapshot: $snapshot"

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
catch { Write-WinarchyWarn "Could not export the existing tasks to the snapshot: $($_.Exception.Message)" }

# --- 1. Paquetes (winget, versiones core fijadas) -------------------------------
$lock = Import-WinarchyToml -Path (Join-Path $Root 'versions.lock.toml')
$coreVersions = Get-WinarchyCoreVersions -Lock $lock
# -SkipPackages (la migración de `winarchy update --self`) igual tiene que traer los
# componentes core que falten: si no, un core nuevo nunca llega a quien ya tenía Winarchy
# y el stack queda apuntando a un exe inexistente.
$packageIds = if ($SkipPackages) { @($coreVersions.Keys) } else { @($lock['winget'].Keys) }
if ($SkipPackages) { Write-WinarchyInfo 'Checking that the core components are present...' }
$failedPackages = @()
foreach ($id in $packageIds) {
    if (-not $lock['winget'][$id]) { continue }
    $installed = Get-WinarchyInstalledVersion -Id $id
    $outdated = $installed -and -not $SkipPackages -and $coreVersions.ContainsKey($id) -and
        (Test-WinarchyVersionBelow -Installed $installed -Pinned $coreVersions[$id])
    if ($installed -and -not $outdated) {
        if (-not $SkipPackages) { Write-WinarchyOk "$id already installed" }
    }
    else {
        if ($outdated) {
            Write-WinarchyInfo "Updating $id $installed -> $($coreVersions[$id]) (the version Winarchy is tested with)..."
            winget pin remove --id $id --exact --source winget 2>$null | Out-Null
        }
        else { Write-WinarchyInfo "Installing $id ..." }
        $args = @('install', '--id', $id, '--exact', '--source', 'winget', '--silent', '--accept-package-agreements', '--accept-source-agreements')
        if ($coreVersions.ContainsKey($id)) { $args += @('--version', $coreVersions[$id]) }
        winget @args
        if ($LASTEXITCODE -eq -1978334967) { Write-WinarchyWarn "$id installed; restart Windows to finish its setup." }
        elseif ($LASTEXITCODE -notin 0, -1978335135, -1978335189) {
            Write-WinarchyWarn "winget could not install $id (exit code $LASTEXITCODE)"
            $failedPackages += $id
        }
    }
    if ($coreVersions.ContainsKey($id)) {
        winget pin add --id $id --exact --source winget --accept-source-agreements 2>$null | Out-Null   # update general nunca toca el core
    }
}
# PATH del proceso con lo que acaba de instalar winget.
$env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')

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
Write-WinarchyOk 'KOMOREBI_CONFIG_HOME / YASB_CONFIG_HOME / WEZTERM_CONFIG_FILE set (User)'

# WEZTERM_CONFIG_FILE gana sobre el descubrimiento por home: si el usuario ya tenía su
# propio wezterm.lua, queda huérfano en silencio. Avisar dónde está y dónde va ahora.
foreach ($orphan in @("$env:USERPROFILE\.wezterm.lua", "$env:USERPROFILE\.config\wezterm\wezterm.lua")) {
    if (Test-Path $orphan) {
        Write-WinarchyWarn "Your previous WezTerm config ($orphan) is no longer loaded: $weztermConfig is now. Move your settings to config\wezterm\user.lua."
    }
}

# --- 3. PATH: comando winarchy ----------------------------------------------------
$binDir = Join-Path $Root 'bin'
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($userPath -notlike "*$binDir*") {
    [Environment]::SetEnvironmentVariable('Path', "$userPath;$binDir", 'User')
    Write-WinarchyOk 'bin\ added to the user PATH (open a new terminal to use winarchy)'
}
else { Write-WinarchyOk 'bin\ already on PATH' }

# --- 3b. Terminal profile: PSFzf + hook en $PROFILE de pwsh --------------------------
if (-not $SkipPackages) {
    if (-not (Get-Module -ListAvailable PSFzf)) {
        Write-WinarchyInfo 'Installing PSFzf (PSGallery)...'
        try { Install-Module PSFzf -Scope CurrentUser -Force -ErrorAction Stop }
        catch { Write-WinarchyWarn "Could not install PSFzf: $($_.Exception.Message)" }
    }
    else { Write-WinarchyOk 'PSFzf already installed' }
}
Install-WinarchyShellProfile

# --- 3c. Skill "winarchy" en los agentes de IA detectados (estilo Omarchy) ----------
Install-WinarchySkill

# --- 3d. Comandos de Winarchy visibles desde el lanzador ------------------------------
Sync-WinarchyPalette

# --- 3e. Perfil de Flow (primer arranque) ----------------------------------------------
Initialize-WinarchyFlow

# --- 4. Theme (genera todos los configs) --------------------------------------------
# Siempre re-renderiza, no solo en la primera instalacion: install.ps1 es tambien la
# migracion de `winarchy update --self`, y sin esto un cambio en templates/ nunca
# llegaba a los configs de quien ya tenia un theme seteado.
$initialTheme = Get-WinarchyCurrentTheme
if ($initialTheme) {
    Write-WinarchyInfo "Regenerating configs from the templates (theme: $initialTheme)..."
} else {
    $initialTheme = 'tokyo-night'
    Write-WinarchyInfo 'Generating configs with the initial theme (tokyo-night)...'
}
Set-WinarchyTheme -Name $initialTheme

# --- 4b. Identidad unificada: tray único + auto-updates de terceros off ---------------
# Idempotente: se reaplica en cada install/repair, así un update de Flow/ShareX/Everything
# no reintroduce su tray/auto-update. YASB ya queda configurado por su config.yaml
# (show_systray/update_check off); AHK hostea el tray del stack; komorebi no tiene tray.
Set-WinarchyFlowIdentity
Set-WinarchyShareXIdentity
Set-WinarchyEverythingIdentity
Set-WinarchyFlowKeywords
Set-WinarchyDefenderExclusions

# --- 5. Autostart + taskbar (solo con -Activate) --------------------------------------
if ($Activate) {
    $komorebic = (Get-Command komorebic -ErrorAction SilentlyContinue)?.Source
    $yasb = (Get-Command yasbc -ErrorAction SilentlyContinue)?.Source
    $ahkExe = Get-WinarchyAhkExe

    # Scheduled Tasks At-LogOn (disparo más temprano que la carpeta Startup). Migra
    # borrando los .lnk legacy; idempotente; fallback a Startup si el registro falla.
    Register-WinarchyAutostart

    # Taskbar nativa en auto-hide (no oculta del todo: ver design D6/riesgos)
    try {
        $null = Set-WinarchyTaskbarAutoHide -Enabled $true
        Write-WinarchyOk 'Windows taskbar set to auto-hide'
    }
    catch { Write-WinarchyWarn "Could not set the taskbar to auto-hide: $($_.Exception.Message)" }

    try {
        $n = Set-WinarchyWindowsHardening
        Write-WinarchyOk "Windows without Bing, suggestions and ads ($n setting(s))"
    }
    catch { Write-WinarchyWarn "Could not apply the Windows hardening: $($_.Exception.Message)" }

    # Sin el delay artificial (~10 s) que Explorer impone a las apps de Startup,
    # komorebi/YASB/AHK levantan apenas inicia la sesión.
    try {
        $serialize = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize'
        if (-not (Test-Path $serialize)) { New-Item $serialize -Force | Out-Null }
        Set-ItemProperty -Path $serialize -Name 'StartupDelayInMSec' -Value 0 -Type DWord
        Write-WinarchyOk 'Startup app delay removed (StartupDelayInMSec=0)'
    }
    catch { Write-WinarchyWarn "Could not remove the Startup delay: $($_.Exception.Message)" }

    Write-WinarchyInfo 'Starting the stack...'
    if ($komorebic) { & $komorebic start }
    if ($yasb) { & $yasb start }
    if ($ahkExe) {
        Remove-Item Env:CLAUDE_CODE_CHILD_SESSION, Env:CLAUDECODE -ErrorAction SilentlyContinue
        Start-Process $ahkExe -ArgumentList "`"$Root\config\ahk\winarchy.ahk`""
    }
}
else {
    # Migración / self-update (sin -Activate): si el autostart YA estaba activo, re-registrarlo
    # para que tome cambios en cómo se lanzan los componentes (p.ej. el launcher resiliente
    # scripts\Start-Komorebi.ps1) sin cambiar el modo de operación vigente. Si nunca estuvo
    # activo (convivencia pura), no se toca nada.
    $autostart = Get-WinarchyAutostartStatus
    if (@($autostart.Values | Where-Object { $_ }).Count -gt 0) {
        Write-WinarchyInfo 'Autostart is on: registering it again to pick up startup changes...'
        Register-WinarchyAutostart
    }
    else {
        Write-WinarchyInfo 'Coexistence mode: nothing starts at logon. To activate: .\install.ps1 -Activate'
    }
}

Write-Host ''
$failedCore = @($failedPackages | Where-Object { $coreVersions.ContainsKey($_) })
if ($failedCore) {
    Write-WinarchyErr "Core components not installed: $($failedCore -join ', '). Check the winget output above and run the installer again."
    exit 1
}
if ($failedPackages) { Write-WinarchyWarn "Not installed: $($failedPackages -join ', '). The rest of the stack works; winarchy doctor shows what is missing." }
Write-WinarchyOk 'Install complete. Check it with: winarchy doctor'
