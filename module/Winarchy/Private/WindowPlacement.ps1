# WindowPlacement.ps1 — recordar en qué monitor/workspace vive cada app.
# El usuario acomoda las ventanas, `winarchy layout save` captura el estado vivo de
# komorebi a config/windows.toml, y `layout apply` lo reproduce con reglas de komorebi.

function Get-WinarchyWindowPrefsPath { Join-Path (Get-WinarchyRoot) 'config\windows.toml' }

function Get-WinarchyKomorebiState {
    <# Estado vivo de komorebi como objeto. Falla claro si komorebi no está corriendo. #>
    if (-not (Test-WinarchyProcess 'komorebi')) {
        throw 'komorebi is not running; there is no live state to read.'
    }
    $raw = & komorebic state 2>$null | Out-String
    if (-not $raw.Trim()) { throw 'komorebic state returned nothing.' }
    $raw | ConvertFrom-Json
}

function Get-WinarchyWindowPlacements {
    <# Aplana el estado de komorebi a (Exe, Monitor, Workspace, WorkspaceName).
       El monitor se identifica por device_id: el índice depende del orden en que Windows
       enumera las pantallas y cambia al reconectar o cambiar de puerto.
       Si una app tiene ventanas en más de un workspace, gana la primera. #>
    param([Parameter(Mandatory)]$State)
    $seen = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    $out = foreach ($monitor in $State.monitors.elements) {
        $index = -1
        foreach ($workspace in $monitor.workspaces.elements) {
            $index++
            foreach ($container in $workspace.containers.elements) {
                foreach ($window in $container.windows.elements) {
                    if (-not $window.exe) { continue }
                    if (-not $seen.Add($window.exe)) { continue }
                    [pscustomobject]@{
                        Exe           = $window.exe
                        Monitor       = $monitor.device_id
                        Workspace     = $index
                        WorkspaceName = $workspace.name
                        Pin           = $false
                    }
                }
            }
        }
    }
    # Sin envolver en `, @(...)`: eso preserva el array al asignar pero lo pasa como un
    # unico elemento al pipear. Los llamadores envuelven con @().
    $out
}

function Get-WinarchyMonitorIndexMap {
    <# device_id → índice de monitor, resuelto contra el estado vivo. #>
    param([Parameter(Mandatory)]$State)
    $map = @{}
    $index = -1
    foreach ($monitor in $State.monitors.elements) {
        $index++
        if ($monitor.device_id) { $map[$monitor.device_id] = $index }
    }
    $map
}

function Get-WinarchyUnplaceableExes {
    <# Exes que no tiene sentido ubicar: juegos/launchers y lo que komorebi ya ignora.
       Son justamente los que no queremos capturar como preferencia. #>
    $exes = [System.Collections.Generic.List[string]]::new()
    foreach ($game in Get-WinarchyGames) { $exes.Add($game) }
    $komorebiJson = Join-Path (Get-WinarchyRoot) 'config\komorebi\komorebi.json'
    if (Test-Path $komorebiJson) {
        try {
            $config = Get-Content $komorebiJson -Raw | ConvertFrom-Json
            foreach ($rule in $config.ignore_rules) {
                if ($rule.kind -eq 'Exe' -and $rule.id) { $exes.Add($rule.id) }
            }
        }
        catch { }
    }
    $exes | Sort-Object -Unique
}

function Import-WinarchyWindowPrefs {
    <# Preferencias guardadas. Array vacío si todavía no hay archivo. #>
    $path = Get-WinarchyWindowPrefsPath
    if (-not (Test-Path $path)) { return }
    $data = Import-WinarchyToml -Path $path
    if (-not $data.ContainsKey('windows')) { return }
    foreach ($entry in $data['windows']) {
        if (-not $entry['exe']) { continue }
        [pscustomobject]@{
            Exe           = $entry['exe']
            Monitor       = $entry['monitor']
            Workspace     = [int]$entry['workspace']
            WorkspaceName = if ($entry.ContainsKey('workspace_name')) { $entry['workspace_name'] } else { '' }
            Pin           = $entry.ContainsKey('pin') -and $entry['pin']
        }
    }
}

function Export-WinarchyWindowPrefs {
    param([Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Pref)
    $lines = [System.Collections.Generic.List[string]]::new()
    $lines.Add('# Preferencias de ubicación de ventanas de ESTA máquina.')
    $lines.Add('# Se captura con `winarchy layout save` y se reproduce con `winarchy layout apply`.')
    $lines.Add('# El monitor va por device_id porque el índice cambia al reconectar pantallas.')
    $lines.Add('# `pin = true` fija la ventana permanentemente en vez de solo al abrirla.')
    foreach ($p in ($Pref | Sort-Object Exe)) {
        $lines.Add('')
        $lines.Add('[[windows]]')
        $lines.Add("exe = `"$($p.Exe)`"")
        $lines.Add("monitor = `"$($p.Monitor)`"")
        $lines.Add("workspace = $($p.Workspace)")
        if ($p.WorkspaceName) { $lines.Add("workspace_name = `"$($p.WorkspaceName)`"") }
        if ($p.Pin) { $lines.Add('pin = true') }
    }
    Set-Content -Path (Get-WinarchyWindowPrefsPath) -Value ($lines -join "`r`n") -Encoding UTF8 -NoNewline
}

function Save-WinarchyWindowLayout {
    <# Captura la disposición actual. Mergea aditivo: guardar con media sesión abierta no
       debe borrar las preferencias de apps que en ese momento no estaban corriendo. #>
    $state = Get-WinarchyKomorebiState
    $skip = Get-WinarchyUnplaceableExes
    $live = @(Get-WinarchyWindowPlacements -State $state | Where-Object { $skip -notcontains $_.Exe })
    if ($live.Count -eq 0) {
        Write-WinarchyWarn 'No managed windows to capture.'
        return
    }
    $prefs = @(Import-WinarchyWindowPrefs)
    $added = 0; $updated = 0
    foreach ($placement in $live) {
        $existing = $prefs | Where-Object { $_.Exe -ieq $placement.Exe } | Select-Object -First 1
        if ($existing) {
            # el pin es una decisión del usuario, no algo que la captura deba pisar
            $placement.Pin = $existing.Pin
            $prefs = @($prefs | Where-Object { $_.Exe -ine $placement.Exe })
            $updated++
        }
        else { $added++ }
        $prefs += $placement
    }
    Export-WinarchyWindowPrefs -Pref $prefs
    Write-WinarchyOk "Layout saved: $added new, $updated updated ($($prefs.Count) total) → $(Get-WinarchyWindowPrefsPath)"
}

function Invoke-WinarchyLayoutApply {
    <# Traduce las preferencias a reglas de komorebi. Best-effort: nunca debe tumbar un
       reload ni un `theme set`. #>
    param([switch]$Quiet)
    $prefs = @(Import-WinarchyWindowPrefs)
    if ($prefs.Count -eq 0) {
        if (-not $Quiet) { Write-WinarchyInfo 'No saved window preferences.' }
        return
    }
    if (-not (Test-WinarchyProcess 'komorebi')) {
        if (-not $Quiet) { Write-WinarchyWarn 'komorebi is not running; preferences not applied.' }
        return
    }
    $map = Get-WinarchyMonitorIndexMap -State (Get-WinarchyKomorebiState)
    $resolved = @($prefs | Where-Object { $map.ContainsKey($_.Monitor) })
    $skipped = @($prefs | Where-Object { -not $map.ContainsKey($_.Monitor) })

    # Limpiar antes de aplicar: las reglas se agregan, no se reemplazan, así que sin esto
    # cada reload acumula reglas contradictorias. clear-workspace-rules es por
    # monitor+workspace, no global.
    $touched = @{}
    foreach ($pref in $resolved) {
        $key = "$($map[$pref.Monitor]),$($pref.Workspace)"
        if (-not $touched.ContainsKey($key)) {
            $touched[$key] = $true
            komorebic clear-workspace-rules $map[$pref.Monitor] $pref.Workspace 2>$null | Out-Null
        }
    }
    foreach ($pref in $resolved) {
        # initial: la ventana nace donde el usuario la dejó y después la mueve libremente.
        # La permanente la devolvería a su sitio cada vez, que es una jaula, no una preferencia.
        $verb = if ($pref.Pin) { 'workspace-rule' } else { 'initial-workspace-rule' }
        komorebic $verb exe $pref.Exe $map[$pref.Monitor] $pref.Workspace 2>$null | Out-Null
    }
    foreach ($pref in $skipped) {
        Write-WinarchyWarn "'$($pref.Exe)': monitor '$($pref.Monitor)' not connected; rule skipped."
    }
    if (-not $Quiet) {
        Write-WinarchyOk "$($resolved.Count) window rule(s) applied$(if ($skipped.Count) { ", $($skipped.Count) skipped" })."
    }
}

function Get-WinarchyWindowLayout {
    <# Preferencias guardadas, marcando las que no resuelven a un monitor conectado. #>
    $prefs = @(Import-WinarchyWindowPrefs)
    if ($prefs.Count -eq 0) {
        Write-WinarchyInfo "No saved preferences. Arrange your windows and run: winarchy layout save"
        return
    }
    $map = @{}
    if (Test-WinarchyProcess 'komorebi') {
        $map = Get-WinarchyMonitorIndexMap -State (Get-WinarchyKomorebiState)
    }
    foreach ($pref in ($prefs | Sort-Object Exe)) {
        $where = if ($map.ContainsKey($pref.Monitor)) { "monitor $($map[$pref.Monitor])" } else { 'monitor NOT CONNECTED' }
        $name = if ($pref.WorkspaceName) { " ($($pref.WorkspaceName))" } else { '' }
        $pin = if ($pref.Pin) { ' [pinned]' } else { '' }
        Write-Host ("  {0,-34} {1}, workspace {2}{3}{4}" -f $pref.Exe, $where, $pref.Workspace, $name, $pin)
    }
}

function Remove-WinarchyWindowPref {
    param([Parameter(Mandatory)][string]$Exe)
    if ($Exe -notlike '*.exe') { $Exe = "$Exe.exe" }
    $prefs = @(Import-WinarchyWindowPrefs)
    $kept = @($prefs | Where-Object { $_.Exe -ine $Exe })
    if ($kept.Count -eq $prefs.Count) {
        Write-WinarchyWarn "'$Exe' has no saved preference."
        return
    }
    Export-WinarchyWindowPrefs -Pref $kept
    Write-WinarchyOk "'$Exe' forgotten."
}
