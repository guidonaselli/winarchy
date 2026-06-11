# Doctor.ps1 — `winarchy doctor`: diagnóstico verde/rojo del stack con remediación

function Invoke-WinarchyDoctor {
    $root = Get-WinarchyRoot
    $checks = [System.Collections.Generic.List[object]]::new()
    function Add-Check {
        param([string]$Name, [bool]$Ok, [string]$Detail, [string]$Fix = '')
        $checks.Add([pscustomobject]@{ Name = $Name; Ok = $Ok; Detail = $Detail; Fix = $Fix })
    }

    # --- Procesos core vivos -------------------------------------------------
    Add-Check 'komorebi corriendo' (Test-WinarchyProcess 'komorebi') `
        'tiling window manager' 'komorebic start'
    Add-Check 'YASB corriendo' (Test-WinarchyProcess 'yasb') `
        'barra de estado' 'yasbc start  (o lanzar YASB desde el menú inicio)'
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
    Add-Check 'AHK winarchy.ahk corriendo' $ahkAlive `
        'dueño único de hotkeys' "AutoHotkey64.exe `"$root\config\ahk\winarchy.ahk`""
    Add-Check 'Flow Launcher corriendo' (Test-WinarchyProcess 'Flow.Launcher') `
        'launcher' "& `"$env:LOCALAPPDATA\FlowLauncher\Flow.Launcher.exe`""

    # --- Configs parseables ---------------------------------------------------
    $komorebiJson = Join-Path $root 'config\komorebi\komorebi.json'
    $komorebiOk = $false
    if (Test-Path $komorebiJson) {
        try { Get-Content $komorebiJson -Raw | ConvertFrom-Json | Out-Null; $komorebiOk = $true } catch { }
    }
    Add-Check 'komorebi.json válido' $komorebiOk `
        $komorebiJson 'winarchy theme set <theme>  (lo regenera)'

    $themesOk = $true; $themesDetail = @()
    foreach ($t in Get-WinarchyThemes) {
        try {
            $data = Import-WinarchyToml -Path (Join-Path (Get-WinarchyThemesDir) "$t\theme.toml")
            $missing = Test-WinarchyThemeData -Theme $data
            if ($missing.Count -gt 0) { $themesOk = $false; $themesDetail += "$t (faltan: $($missing -join ','))" }
        }
        catch { $themesOk = $false; $themesDetail += "$t (no parsea)" }
    }
    Add-Check 'themes válidos' $themesOk `
        ($(if ($themesDetail) { $themesDetail -join '; ' } else { "$((@(Get-WinarchyThemes)).Count) themes OK" })) `
        'corregir theme.toml indicado'

    # --- Enlaces / env vars ----------------------------------------------------
    $envOk = ($env:KOMOREBI_CONFIG_HOME -eq (Join-Path $root 'config\komorebi')) -and
             ($env:YASB_CONFIG_HOME -eq (Join-Path $root 'config\yasb'))
    Add-Check 'env vars de config apuntando al repo' $envOk `
        "KOMOREBI_CONFIG_HOME=$env:KOMOREBI_CONFIG_HOME; YASB_CONFIG_HOME=$env:YASB_CONFIG_HOME" `
        '.\install.ps1  (las re-registra; reabrir terminal)'

    # --- Versiones vs lockfile ---------------------------------------------------
    $lock = Import-WinarchyToml -Path (Join-Path $root 'versions.lock.toml')
    $komorebicCmd = Get-Command komorebic -ErrorAction SilentlyContinue
    $verOk = $false; $verDetail = 'komorebic no está en PATH'
    if ($komorebicCmd) {
        $installed = (& komorebic --version 2>$null | Select-Object -First 1) -replace '[^\d\.]', ''
        $verOk = $installed -eq $lock['core']['komorebi']
        $verDetail = "instalado=$installed lockfile=$($lock['core']['komorebi'])"
    }
    Add-Check 'komorebi en versión del lockfile' $verOk $verDetail `
        'winarchy update --core  (o reinstalar la versión fijada)'

    # --- Dueños de hotkeys duplicados ----------------------------------------------
    $whkd = Test-WinarchyProcess 'whkd'
    Add-Check 'sin dueños de hotkeys duplicados (whkd)' (-not $whkd) `
        $(if ($whkd) { 'whkd está corriendo — conflicto con AHK' } else { 'AHK es el único dueño' }) `
        'Stop-Process -Name whkd; quitar su autostart'

    # --- Estado de migración Seelen ---------------------------------------------------
    $seelen = Test-WinarchyProcess 'seelen-ui'
    Add-Check 'Seelen UI inactivo' (-not $seelen) `
        $(if ($seelen) { 'Seelen sigue corriendo (modo convivencia)' } else { 'migración completa o no instalado' }) `
        '.\scripts\migrate-from-seelen.ps1'

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
    Write-WinarchyInfo 'Recordatorio: komorebi NO gestiona ventanas elevadas (by design; no correr elevado).'
    if ($failed -eq 0) { Write-WinarchyOk 'Todo verde.' } else { Write-WinarchyErr "$failed chequeo(s) en rojo." }
    $failed
}
