# Doctor.ps1 — `winarchy doctor`: diagnóstico verde/rojo del stack con remediación

function Invoke-WinarchyDoctor {
    $root = Get-WinarchyRoot
    $checks = [System.Collections.Generic.List[object]]::new()
    function Add-Check {
        param([string]$Name, [bool]$Ok, [string]$Detail, [string]$Fix = '')
        $checks.Add([pscustomobject]@{ Name = $Name; Ok = $Ok; Detail = $Detail; Fix = $Fix })
    }

    # --- Procesos core vivos -------------------------------------------------
    Add-Check 'komorebi running' (Test-WinarchyProcess 'komorebi') `
        'tiling window manager' 'komorebic start'
    # Un solo proceso yasb: con >1 vivo, cada uno mantiene su suscripción al named pipe
    # de komorebi y las barras pelean el foco mandando FocusWorkspaceNumber(0) — el
    # sintoma es "salto al workspace 1" al minimizar o abrir un juego.
    $yasbCount = @(Get-Process -Name 'yasb' -ErrorAction SilentlyContinue).Count
    Add-Check 'YASB running (single instance)' ($yasbCount -eq 1) `
        $(if ($yasbCount -eq 0) { 'status bar down' } elseif ($yasbCount -eq 1) { 'status bar' } else { "$yasbCount yasb processes — duplicate komorebi subscribers" }) `
        $(if ($yasbCount -gt 1) { 'winarchy reload  (kill+start leaves a single subscriber)' } else { 'yasbc start  (or launch YASB from the Start Menu)' })
    # Primero por PID file (winarchy.ahk lo escribe al arrancar; CommandLine depende
    # de WMI/CIM, que no está disponible en todos los entornos)
    $ahkAlive = $false
    $ahkPidFile = Join-Path $root 'state\ahk.pid'
    if (Test-Path $ahkPidFile) {
        $ahkPid = (Get-Content $ahkPidFile -Raw).Trim() -as [int]
        $ahkProc = if ($ahkPid) { Get-Process -Id $ahkPid -ErrorAction SilentlyContinue } else { $null }
        $ahkAlive = $ahkProc -and $ahkProc.ProcessName -like 'AutoHotkey*'
    }
    if (-not $ahkAlive) {
        $ahkAlive = $null -ne (Get-Process -Name 'AutoHotkey*' -ErrorAction SilentlyContinue |
            Where-Object { $_.CommandLine -like '*winarchy.ahk*' })
    }
    Add-Check 'AHK winarchy.ahk running' $ahkAlive `
        'sole hotkey owner' "AutoHotkey64.exe `"$root\config\ahk\winarchy.ahk`""
    Add-Check 'Flow Launcher running' (Test-WinarchyProcess 'Flow.Launcher') `
        'launcher' "& `"$env:LOCALAPPDATA\FlowLauncher\Flow.Launcher.exe`""
    # Solo importa si hay slots declarados: sin ellos el daemon no tendría nada que hacer.
    $slotPrefs = @(Import-WinarchyWindowPrefs | Where-Object { $null -ne $_.Slot })
    if ($slotPrefs.Count -gt 0) {
        Add-Check 'window slots daemon running' (Test-WinarchyWindowSlots) `
            "keeps $($slotPrefs.Count) window(s) in their tile position" `
            'winarchy reload   (or run scripts\Start-WindowSlots.ps1 hidden)'
    }

    # --- Estado de sesión stale ------------------------------------------------
    # komorebi.sock es AF_UNIX: si komorebi crasheó, el archivo queda y el próximo
    # arranque no bindea hasta limpiarlo (komorebi "offline al boot, no levanta solo").
    $sock = Join-Path $env:LOCALAPPDATA 'komorebi\komorebi.sock'
    $staleSock = (Test-Path $sock) -and -not (Test-WinarchyProcess 'komorebi')
    Add-Check 'no stale komorebi socket' (-not $staleSock) `
        $(if ($staleSock) { "$sock left behind by a dirty exit" } else { 'clean' }) `
        'Remove-Item "$env:LOCALAPPDATA\komorebi\komorebi.sock"; then .\scripts\Start-Komorebi.ps1'

    # game-mode.flag es estado de sesión: si sobrevive a un crash/reboot, AHK arranca
    # creyéndose en juego y deja el AccentWatch inerte sin ninguna señal visible.
    $gameFlag = Test-Path (Get-WinarchyGameModeFlag)
    $anyGameRunning = $false
    if ($gameFlag) {
        $running = (Get-Process -ErrorAction SilentlyContinue).ProcessName
        $anyGameRunning = $null -ne (Get-WinarchyGames | Where-Object { $running -contains ($_ -replace '\.exe$') })
    }
    Add-Check 'no orphan game-mode flag' (-not ($gameFlag -and -not $anyGameRunning)) `
        $(if (-not $gameFlag) { 'game-mode off' } elseif ($anyGameRunning) { 'game-mode on with a registered game running' } else { 'flag set but no games.toml process alive' }) `
        'winarchy game-mode off'

    # Una preferencia que apunta a un monitor desconectado no se aplica y hoy eso es mudo:
    # la app aparece donde cae y parece que el stack se olvidó de la preferencia.
    $prefs = @(Import-WinarchyWindowPrefs)
    $orphanPrefs = @()
    if ($prefs.Count -gt 0 -and (Test-WinarchyProcess 'komorebi')) {
        $monitorMap = Get-WinarchyMonitorIndexMap -State (Get-WinarchyKomorebiState)
        $orphanPrefs = @($prefs | Where-Object { -not $monitorMap.ContainsKey($_.Monitor) })
    }
    Add-Check 'window placements resolve' ($orphanPrefs.Count -eq 0) `
        $(if ($prefs.Count -eq 0) { 'none saved' } elseif ($orphanPrefs.Count -eq 0) { "$($prefs.Count) placement(s) OK" } else { "monitor missing for: $($orphanPrefs.Exe -join ', ')" }) `
        'winarchy layout list  (then re-save or: winarchy layout forget <exe>)'

    # --- Autostart (Scheduled Tasks At-LogOn) ----------------------------------
    $autostart = Get-WinarchyAutostartStatus
    $autoMissing = @($autostart.Keys | Where-Object { -not $autostart[$_] })
    Add-Check 'autostart registered' ($autoMissing.Count -eq 0) `
        $(if ($autoMissing) { "missing: $($autoMissing -join ', ')" } else { 'At-LogOn tasks present' }) `
        '.\install.ps1 -Activate  (registers the At-LogOn tasks)'

    # --- Configs parseables ---------------------------------------------------
    $komorebiJson = Join-Path $root 'config\komorebi\komorebi.json'
    $komorebiOk = $false
    if (Test-Path $komorebiJson) {
        try { Get-Content $komorebiJson -Raw | ConvertFrom-Json | Out-Null; $komorebiOk = $true } catch { }
    }
    Add-Check 'komorebi.json valid' $komorebiOk `
        $komorebiJson 'winarchy theme set <theme>  (regenerates it)'

    # Las reglas de ASC que pierden contra una capa superior no son un error (para eso
    # existe la precedencia), pero tienen que ser visibles: si una ventana se porta raro,
    # esta lista es el primer lugar donde mirar.
    $ascPath = Get-WinarchyAscPath
    if (Test-Path $ascPath) {
        $lock = Import-WinarchyToml -Path (Join-Path $root 'versions.lock.toml')
        $pinned = $lock.ContainsKey('asc') -and (Get-FileHash $ascPath -Algorithm SHA256).Hash.ToLower() -eq $lock['asc']['sha256']
        Add-Check 'community app rules vendored' $pinned `
            "$(Get-WinarchyAscAppCount) community app rules, pinned $($lock['asc']['date']) ($($lock['asc']['commit'].Substring(0, 7)))" `
            'winarchy rules sync --apply  (re-pins the vendored copy)'
        $collisions = @(Get-WinarchyAppRuleCollisions)
        Add-Check 'app rule collisions' $true `
            $(if ($collisions.Count -eq 0) { 'no ASC rule overridden' }
              else { "$($collisions.Count) ASC rule(s) overridden: $((($collisions | Select-Object -First 4).App | Sort-Object -Unique) -join ', ')" }) `
            'winarchy rules collisions  (full list)'
    }

    $themesOk = $true; $themesDetail = @()
    foreach ($t in Get-WinarchyThemes) {
        try {
            $data = Import-WinarchyToml -Path (Join-Path (Get-WinarchyThemesDir) "$t\theme.toml")
            $missing = @(Test-WinarchyThemeData -Theme $data)
            if ($missing.Count -gt 0) { $themesOk = $false; $themesDetail += "$t (missing: $($missing -join ','))" }
        }
        catch { $themesOk = $false; $themesDetail += "$t (does not parse)" }
    }
    Add-Check 'themes valid' $themesOk `
        ($(if ($themesDetail) { $themesDetail -join '; ' } else { "$((@(Get-WinarchyThemes)).Count) themes OK" })) `
        'fix the theme.toml listed above'

    # --- Enlaces / env vars ----------------------------------------------------
    $envOk = ($env:KOMOREBI_CONFIG_HOME -eq (Join-Path $root 'config\komorebi')) -and
             ($env:YASB_CONFIG_HOME -eq (Join-Path $root 'config\yasb'))
    Add-Check 'config env vars pointing at the repo' $envOk `
        "KOMOREBI_CONFIG_HOME=$env:KOMOREBI_CONFIG_HOME; YASB_CONFIG_HOME=$env:YASB_CONFIG_HOME" `
        '.\install.ps1  (re-registers them; reopen the terminal)'

    # --- Versiones vs lockfile ---------------------------------------------------
    $lock = Import-WinarchyToml -Path (Join-Path $root 'versions.lock.toml')
    $komorebicCmd = Get-Command komorebic -ErrorAction SilentlyContinue
    $verOk = $false; $verDetail = 'komorebic is not on PATH'
    if ($komorebicCmd) {
        $installed = (& komorebic --version 2>$null | Select-Object -First 1) -replace '[^\d\.]', ''
        $verOk = $installed -eq $lock['core']['komorebi']
        $verDetail = "installed=$installed lockfile=$($lock['core']['komorebi'])"
    }
    Add-Check 'komorebi at lockfile version' $verOk $verDetail `
        'winarchy update --core  (or reinstall the pinned version)'

    # Todo lo pinneado se verifica. Antes solo se miraba komorebi, así que YASB se quedó
    # cuatro versiones atrás sin que nada lo dijera (winget oculta los paquetes pinneados).
    foreach ($pinned in @(
        @{ Name = 'YASB'; Key = 'yasb'; Path = (Get-Command yasb -ErrorAction SilentlyContinue).Source },
        @{ Name = 'Flow Launcher'; Key = 'flow'; Path = "$env:LOCALAPPDATA\FlowLauncher\Flow.Launcher.exe" },
        @{ Name = 'AutoHotkey v2'; Key = 'ahk'; Path = Get-WinarchyAhkExe }
    )) {
        $found = $pinned.Path -and (Test-Path $pinned.Path)
        $installed = if ($found) { (Get-Item $pinned.Path).VersionInfo.ProductVersion } else { $null }
        # Flow reporta 4 componentes (2.1.3.0) contra el 2.1.3 del lockfile
        $matchesLock = $installed -and $installed.StartsWith($lock['core'][$pinned.Key])
        Add-Check "$($pinned.Name) at lockfile version" ([bool]$matchesLock) `
            $(if ($installed) { "installed=$installed lockfile=$($lock['core'][$pinned.Key])" } else { 'not found on disk' }) `
            'winarchy update --core  (or reinstall the pinned version)'
    }

    # ShareX no se verificaba y de él dependen cinco hotkeys, `winarchy screenshot` y la
    # creación de webapps: sin él esos atajos fallan en silencio.
    $sharex = @("$env:ProgramFiles\ShareX\ShareX.exe", "${env:ProgramFiles(x86)}\ShareX\ShareX.exe") |
        Where-Object { Test-Path $_ } | Select-Object -First 1
    Add-Check 'ShareX installed' ([bool]$sharex) `
        $(if ($sharex) { "screenshots, screen recording and webapps — $((Get-Item $sharex).VersionInfo.ProductVersion)" } else { 'missing: SUPER+Shift+S/W/P/V/G and `winarchy screenshot` will fail' }) `
        'winget install ShareX.ShareX'

    # --- Dueños de hotkeys duplicados ----------------------------------------------
    $whkd = Test-WinarchyProcess 'whkd'
    Add-Check 'no duplicate hotkey owners (whkd)' (-not $whkd) `
        $(if ($whkd) { 'whkd is running — conflicts with AHK' } else { 'AHK is the sole owner' }) `
        'Stop-Process -Name whkd; remove its autostart'

    # --- Terminal profile (informativo) -----------------------------------------------
    $shellTools = @('starship', 'fzf', 'zoxide', 'eza', 'bat')
    $missingTools = @($shellTools | Where-Object {
        -not (Get-Command $_ -CommandType Application -ErrorAction SilentlyContinue)
    })
    Add-Check 'shell tools installed' ($missingTools.Count -eq 0) `
        $(if ($missingTools) { "missing: $($missingTools -join ', ')" } else { ($shellTools -join ', ') + ' + PSFzf' }) `
        '.\install.ps1  (installs them via winget)'
    Add-Check 'pwsh profile hook installed' (Test-WinarchyShellProfileHook) `
        (Get-WinarchyShellProfilePath) '.\install.ps1  (adds the managed block to $PROFILE)'

    # --- Estado de migración Seelen ---------------------------------------------------
    $seelen = Test-WinarchyProcess 'seelen-ui'
    Add-Check 'Seelen UI inactive' (-not $seelen) `
        $(if ($seelen) { 'Seelen is still running (coexistence mode)' } else { 'migration complete or not installed' }) `
        '.\scripts\migrate-from-seelen.ps1'

    # --- Versión de Winarchy mismo (informativo, best-effort) -------------------------
    $ver = Get-WinarchyVersion
    $verCheck = Test-WinarchyUpdateAvailable
    $verDetail = "installed=$($ver.Version)$(if ($ver.Describe) { " ($($ver.Describe))" })"
    if ($verCheck.Latest) {
        $verDetail += "; latest release=$($verCheck.Latest)"
        if ($verCheck.Available) { $verDetail += ' — run: winarchy update --self' }
    }
    Add-Check 'Winarchy up to date' (-not $verCheck.Available) $verDetail 'winarchy update --self'

    # --- Recordatorio permanente -----------------------------------------------------
    Write-Host ''
    Write-Host '  winarchy doctor' -ForegroundColor Cyan
    Write-Host '  ---------------'
    $failed = 0
    foreach ($c in $checks) {
        if ($c.Ok) { Write-Host ("  [OK] {0,-42} {1}" -f $c.Name, $c.Detail) -ForegroundColor Green }
        else {
            $failed++
            Write-Host ("  [XX] {0,-42} {1}" -f $c.Name, $c.Detail) -ForegroundColor Red
            if ($c.Fix) { Write-Host ("       fix: {0}" -f $c.Fix) -ForegroundColor Yellow }
        }
    }
    Write-Host ''
    Write-WinarchyInfo 'Reminder: komorebi does NOT manage elevated windows (by design; never run it elevated).'
    if ($failed -eq 0) { Write-WinarchyOk 'All green.' } else { Write-WinarchyErr "$failed check(s) failing." }
    $failed
}
