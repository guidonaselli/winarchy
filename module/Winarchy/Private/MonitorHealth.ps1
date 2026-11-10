# MonitorHealth.ps1 — detecta y repara el mapa de monitores corrupto de komorebi.
# Tras apagar y prender una pantalla, komorebi puede quedarse con un monitor duplicado encima
# de otro y sin el real: nada se puede mover ahí hasta reiniciarlo.

function Get-WinarchyScreenCount {
    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.Screen]::AllScreens.Count
}

function Get-WinarchyKomorebiMonitors {
    @(Invoke-WinarchyKomorebic monitor-information | ConvertFrom-Json)
}

function Test-WinarchyKomorebiMonitors {
    <# Sano = un monitor de komorebi por pantalla real y ninguno compartiendo origen con otro. #>
    param(
        [Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Monitors,
        [Parameter(Mandatory)][int]$ScreenCount
    )
    if ($Monitors.Count -ne $ScreenCount) { return $false }
    $origins = @($Monitors | ForEach-Object { "$($_.size.left),$($_.size.top)" } | Sort-Object -Unique)
    $origins.Count -eq $Monitors.Count
}

function Test-WinarchyMonitorHealth {
    <# $true también cuando no se puede juzgar: sin evidencia no se reinicia nada. #>
    try { Test-WinarchyKomorebiMonitors -Monitors (Get-WinarchyKomorebiMonitors) -ScreenCount (Get-WinarchyScreenCount) }
    catch { $true }
}

function Repair-WinarchyKomorebiMonitors {
    <#
      Reinicia komorebi si perdió un monitor. Devuelve $true si lo reinició.
      -Throttled (el watchdog de AHK): a lo sumo un reinicio cada 5 minutos, para que una
      pantalla que parpadea no lo deje reiniciando en loop.
    #>
    param([switch]$Throttled)
    if (-not (Test-WinarchyProcess 'komorebi')) { return $false }
    if (Test-WinarchyMonitorHealth) { return $false }
    # komorebi tarda varios segundos en reconciliar un cambio de pantallas: un mapa raro a
    # mitad de camino no es un mapa roto.
    Start-Sleep -Seconds 5
    if (Test-WinarchyMonitorHealth) { return $false }

    # las session-float-rules del game mode mueren con komorebi
    if (Test-Path (Get-WinarchyGameModeFlag)) {
        Write-WinarchyWarn 'komorebi lost track of a monitor; not restarting it while game mode is on. Run: winarchy reload'
        return $false
    }
    $stamp = Join-Path (Get-WinarchyStateDir) 'monitor-heal'
    if ($Throttled -and (Test-Path $stamp) -and ((Get-Date) - (Get-Item $stamp).LastWriteTime).TotalMinutes -lt 5) {
        return $false
    }
    Set-Content -Path $stamp -Value (Get-Date -Format o)

    $null = Invoke-WinarchyKomorebic stop
    $deadline = (Get-Date).AddSeconds(10)
    while ((Test-WinarchyProcess 'komorebi') -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 200 }
    # matarlo a la fuerza dejaría ventanas ocultas y el socket stale: mejor fallar a la vista
    if (Test-WinarchyProcess 'komorebi') { throw 'komorebi lost track of a monitor and did not stop when asked; run: komorebic stop, then winarchy reload' }
    # el que llama puede venir de un entorno anterior a la instalación (AHK arrancado antes)
    $env:KOMOREBI_CONFIG_HOME = Join-Path (Get-WinarchyRoot) 'config\komorebi'
    Start-WinarchyKomorebi
    $deadline = (Get-Date).AddSeconds(20)
    $answered = $false
    while (-not $answered -and (Get-Date) -lt $deadline) {
        try { $answered = [bool](Invoke-WinarchyKomorebic state).Trim() } catch { }
        if (-not $answered) { Start-Sleep -Milliseconds 500 }
    }
    if (-not $answered) { throw 'komorebi was stopped to recover a lost monitor and did not come back; run: winarchy doctor' }
    Write-WinarchyOk 'komorebi had lost track of a monitor; restarted it.'
    $true
}
