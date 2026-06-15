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
    Add-Check 'YASB running' (Test-WinarchyProcess 'yasb') `
        'status bar' 'yasbc start  (or launch YASB from the Start Menu)'
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

    # --- Autostart (Scheduled Tasks At-LogOn) ----------------------------------
    $autostart = Get-WinarchyAutostartStatus
    $autoMissing = @($autostart.Keys | Where-Object { -not $autostart[$_] })
    Add-Check 'autostart registered (komorebi/YASB/AHK)' ($autoMissing.Count -eq 0) `
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

    $themesOk = $true; $themesDetail = @()
    foreach ($t in Get-WinarchyThemes) {
        try {
            $data = Import-WinarchyToml -Path (Join-Path (Get-WinarchyThemesDir) "$t\theme.toml")
            $missing = Test-WinarchyThemeData -Theme $data
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
