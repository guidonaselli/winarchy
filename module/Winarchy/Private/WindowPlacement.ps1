# WindowPlacement.ps1 — recordar en qué monitor/workspace vive cada app.
# El usuario acomoda las ventanas, `winarchy layout save` captura el estado vivo de
# komorebi a config/windows.toml, y `layout apply` lo reproduce con reglas de komorebi.

function Get-WinarchyWindowPrefsPath { Join-Path (Get-WinarchyRoot) 'config\windows.toml' }

function Get-WinarchyKomorebiState {
    <# Estado vivo de komorebi como objeto. Falla claro si komorebi no está corriendo.
       Reintenta: consultado justo después de reordenar, komorebi puede tardar más de lo que
       komorebic espera y contestar vacío (o con un timeout de socket) aunque esté sano. #>
    if (-not (Test-WinarchyProcess 'komorebi')) {
        throw 'komorebi is not running; there is no live state to read.'
    }
    foreach ($attempt in 1, 2, 3) {
        $raw = Invoke-WinarchyKomorebic state
        if ($raw.Trim()) { return $raw | ConvertFrom-Json }
        Start-Sleep -Milliseconds (300 * $attempt)
    }
    throw 'komorebic state returned nothing after 3 attempts.'
}

function Add-WinarchyWindowRulesToKomorebiJson {
    <#
      Inyecta las preferencias de ubicación como `initial_workspace_rules` dentro del
      komorebi.json generado.

      Van en el config y no por `komorebic initial-workspace-rule` porque las reglas de
      runtime NO sobreviven a un reload: cuando komorebi.json cambia de verdad, el reload
      las limpia en un momento indeterminado posterior, así que aplicarlas y verificar es
      una carrera que se pierde seguido. Dentro del config, el reload las aplica.

      El índice de monitor se resuelve contra el estado vivo (las preferencias guardan
      device_id). Sin komorebi corriendo no hay a qué resolver: se devuelve el JSON tal
      cual y las reglas entran en la próxima regeneración.

      Si komorebi corre pero no da su estado, no devuelve nada: publicar sin la vista viva
      pisaría los layouts, así que quien llama deja el komorebi.json que está.
    #>
    param([Parameter(Mandatory)][string]$Json, [switch]$SkipLiveView)
    if (-not (Test-WinarchyProcess 'komorebi')) { return $Json }
    try {
        $state = Get-WinarchyKomorebiState
        $map = Get-WinarchyMonitorIndexMap -State $state
    }
    catch {
        if (-not (Test-Path (Join-Path (Get-WinarchyRoot) 'config\komorebi\komorebi.json'))) { return $Json }
        Write-WinarchyWarn "komorebi.json not rewritten (`winarchy theme set` applies it later): $($_.Exception.Message)"
        return
    }

    $config = $Json | ConvertFrom-Json
    # display_index_preferences con el orden vivo de los monitores
    if ($map.Count -gt 1) {
        $order = [ordered]@{}
        foreach ($id in ($map.Keys | Sort-Object { $map[$_] })) { $order["$($map[$id])"] = $id }
        $config | Add-Member -NotePropertyName display_index_preferences -NotePropertyValue ([pscustomobject]$order) -Force
    }

    $prefs = @(Import-WinarchyWindowPrefs)
    foreach ($pref in $prefs) {
        if (-not $map.ContainsKey($pref.Monitor)) { continue }
        $monitor = $config.monitors[$map[$pref.Monitor]]
        if (-not $monitor -or $pref.Workspace -ge @($monitor.workspaces).Count) { continue }
        $workspace = $monitor.workspaces[$pref.Workspace]
        $key = if ($pref.Pin) { 'workspace_rules' } else { 'initial_workspace_rules' }
        $rule = [pscustomobject]@{ kind = 'Exe'; id = $pref.Exe; matching_strategy = 'Equals' }
        if ($workspace.PSObject.Properties[$key]) {
            $workspace.$key = @($workspace.$key) + $rule
        }
        else {
            $workspace | Add-Member -NotePropertyName $key -NotePropertyValue @($rule)
        }
    }
    $savedModes = @(Import-WinarchyWorkspaceLayouts)
    $defaultMode = Import-WinarchyDefaultMode
    $explicit = [System.Collections.Generic.HashSet[string]]::new()
    foreach ($saved in $savedModes) {
        if (-not $map.ContainsKey($saved.Monitor)) { continue }
        $monitor = $config.monitors[$map[$saved.Monitor]]
        if (-not $monitor -or $saved.Workspace -ge @($monitor.workspaces).Count) { continue }
        $null = $explicit.Add("$($map[$saved.Monitor]):$($saved.Workspace)")
        Set-WinarchyWorkspaceConfigMode -Workspace $monitor.workspaces[$saved.Workspace] -Layout $saved.Layout -Mode $saved.Mode
    }
    if ($defaultMode -ne 'tiling') {
        for ($m = 0; $m -lt @($config.monitors).Count; $m++) {
            for ($w = 0; $w -lt @($config.monitors[$m].workspaces).Count; $w++) {
                if ($explicit.Contains("${m}:$w")) { continue }
                Set-WinarchyWorkspaceConfigMode -Workspace $config.monitors[$m].workspaces[$w] -Layout 'BSP' -Mode $defaultMode
            }
        }
    }
    Set-WinarchyConfigLiveView -Config $config -State $state -Map $map -GlobalOnly:$SkipLiveView
    $config | ConvertTo-Json -Depth 50
}

function Set-WinarchyConfigLiveView {
    <# La recarga estática de komorebi pisa layout, flip, paddings, tiling y apilar/mosaico de ventanas nuevas
       con lo que diga el archivo: se escribe lo vivo antes de publicarlo.
       -GlobalOnly deja los workspaces como vienen (`layout apply`); el modo global no tiene preferencia guardada. #>
    param([Parameter(Mandatory)]$Config, [Parameter(Mandatory)]$State, [Parameter(Mandatory)][hashtable]$Map, [switch]$GlobalOnly)
    $behaviour = $State.PSObject.Properties['new_window_behaviour']?.Value
    if ($behaviour) { $Config | Add-Member -NotePropertyName window_container_behaviour -NotePropertyValue $behaviour -Force }
    else { $Config.PSObject.Properties.Remove('window_container_behaviour') }
    if ($GlobalOnly) { return }
    foreach ($monitor in $State.monitors.elements) {
        if (-not $monitor.device_id -or -not $Map.ContainsKey($monitor.device_id)) { continue }
        $target = $Config.monitors[$Map[$monitor.device_id]]
        $index = -1
        foreach ($live in $monitor.workspaces.elements) {
            $index++
            if (-not $target -or $index -ge @($target.workspaces).Count) { continue }
            $workspace = $target.workspaces[$index]
            $layout = $live.layout.PSObject.Properties['Default']?.Value
            if ($layout) { $workspace | Add-Member -NotePropertyName layout -NotePropertyValue $layout -Force }
            if ($live.tile -eq $false) { $workspace | Add-Member -NotePropertyName tile -NotePropertyValue $false -Force }
            else { $workspace.PSObject.Properties.Remove('tile') }
            foreach ($key in 'layout_flip', 'container_padding', 'workspace_padding') {
                $value = $live.PSObject.Properties[$key]?.Value
                if ($null -ne $value) { $workspace | Add-Member -NotePropertyName $key -NotePropertyValue $value -Force }
                else { $workspace.PSObject.Properties.Remove($key) }
            }
        }
    }
}

function Invoke-WinarchyKomorebiStaticReload {
    <# Reescribe komorebi.json con la vista viva: el watcher de komorebi lo recarga sin reconstruir el WM. #>
    param([Parameter(Mandatory)][string]$ConfigPath)
    try {
        $state = Get-WinarchyKomorebiState
        $config = Get-Content -LiteralPath $ConfigPath -Raw | ConvertFrom-Json
        Set-WinarchyConfigLiveView -Config $config -State $state -Map (Get-WinarchyMonitorIndexMap -State $state)
        Set-WinarchyFileAtomic -Path $ConfigPath -Value ($config | ConvertTo-Json -Depth 50)
    }
    catch {
        # sin vista viva no se escribe: la recarga pisaría los layouts; el resto del reload sigue
        Write-WinarchyWarn "komorebi config not reloaded: $($_.Exception.Message)"
        return
    }
    Restore-WinarchyFloatingApps -ConfigPath $ConfigPath
    Write-WinarchyStaleIgnoreWarning -ConfigPath $ConfigPath
}

function Get-WinarchyWindowPlacements {
    <# Aplana el estado de komorebi a (Exe, Monitor, Workspace, WorkspaceName, Slot).
       Slot es el índice del container dentro del workspace: la posición del tile.
       El monitor se identifica por device_id: el índice depende del orden en que Windows
       enumera las pantallas y cambia al reconectar o cambiar de puerto.
       Si una app tiene ventanas en más de un workspace, gana la primera. #>
    param([Parameter(Mandatory)]$State)
    $seen = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    $out = foreach ($monitor in $State.monitors.elements) {
        $index = -1
        foreach ($workspace in $monitor.workspaces.elements) {
            $index++
            $slot = -1
            foreach ($container in $workspace.containers.elements) {
                $slot++
                foreach ($window in $container.windows.elements) {
                    if (-not $window.exe) { continue }
                    if (-not $seen.Add($window.exe)) { continue }
                    [pscustomobject]@{
                        Exe           = $window.exe
                        Monitor       = $monitor.device_id
                        Workspace     = $index
                        WorkspaceName = $workspace.name
                        Slot          = $slot
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

function Get-WinarchyWorkspaceMode {
    <# tiling | scrolling | floating de un workspace del estado de komorebi: `tile` apagado es
       floating, el layout Scrolling es scrolling, todo lo demás es tiling. #>
    param([Parameter(Mandatory)]$Workspace)
    if ($Workspace.PSObject.Properties['tile'] -and $Workspace.tile -eq $false) { return 'floating' }
    if ($Workspace.layout.PSObject.Properties['Default']?.Value -eq 'Scrolling') { return 'scrolling' }
    'tiling'
}

function Set-WinarchyWorkspaceConfigMode {
    <# Traduce un modo guardado a las claves del workspace de komorebi.json. Un workspace
       con `tile: false` no se reacomoda solo: es el floating de Windows. #>
    param([Parameter(Mandatory)]$Workspace, [string]$Layout, [Parameter(Mandatory)][string]$Mode)
    $name = if ($Mode -eq 'scrolling') { 'Scrolling' } elseif ($Layout) { $Layout } else { 'BSP' }
    $Workspace | Add-Member -NotePropertyName layout -NotePropertyValue $name -Force
    if ($Mode -eq 'floating') { $Workspace | Add-Member -NotePropertyName tile -NotePropertyValue $false -Force }
}

function Get-WinarchyLiveWorkspaceLayouts {
    <# Workspaces vivos que se apartan de lo de fábrica: un modo distinto del default o un layout
       que no sea BSP. Los Custom se ignoran: no hay forma de volcarlos a un nombre. #>
    param([Parameter(Mandatory)]$State, [string]$DefaultMode = 'tiling')
    foreach ($monitor in $State.monitors.elements) {
        $index = -1
        foreach ($workspace in $monitor.workspaces.elements) {
            $index++
            if (-not $monitor.device_id) { continue }
            $name = $workspace.layout.PSObject.Properties['Default']?.Value
            $mode = Get-WinarchyWorkspaceMode -Workspace $workspace
            $customLayout = $mode -eq 'tiling' -and $name -and $name -ne 'BSP'
            if ($mode -ne $DefaultMode -or $customLayout) {
                [pscustomobject]@{ Monitor = $monitor.device_id; Workspace = $index; Layout = $(if ($name) { $name } else { 'BSP' }); Mode = $mode }
            }
        }
    }
}

function Import-WinarchyDefaultMode {
    $path = Get-WinarchyWindowPrefsPath
    if (-not (Test-Path $path)) { return 'tiling' }
    $value = (Import-WinarchyToml -Path $path)['default_mode']
    if ($value -in 'tiling', 'scrolling', 'floating') { $value } else { 'tiling' }
}

function Import-WinarchyWorkspaceLayouts {
    $path = Get-WinarchyWindowPrefsPath
    if (-not (Test-Path $path)) { return }
    $data = Import-WinarchyToml -Path $path
    if (-not $data.ContainsKey('workspaces')) { return }
    foreach ($entry in $data['workspaces']) {
        if (-not $entry['monitor'] -or -not ($entry['layout'] -or $entry['mode'])) { continue }
        $layout = if ($entry['layout']) { $entry['layout'] } else { 'BSP' }
        $mode = if ($entry['mode']) { $entry['mode'] } elseif ($layout -eq 'Scrolling') { 'scrolling' } else { 'tiling' }
        [pscustomobject]@{ Monitor = $entry['monitor']; Workspace = [int]$entry['workspace']; Layout = $layout; Mode = $mode }
    }
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
    # Windows reasigna el sufijo UID del device_id al reconectar o reordenar pantallas, y el id guardado deja de
    # coincidir con un monitor que sigue ahí. El modelo (lo que va antes del primer '-') es estable: un id guardado
    # sin coincidencia exacta se resuelve por modelo, solo si hay un único monitor vivo de ese modelo.
    $path = Get-WinarchyWindowPrefsPath
    if (Test-Path $path) {
        $data = Import-WinarchyToml -Path $path
        $saved = foreach ($section in $data.Values) { if ($section -is [System.Collections.IEnumerable] -and $section -isnot [string]) { foreach ($entry in $section) { if ($entry -is [System.Collections.IDictionary] -and $entry['monitor']) { $entry['monitor'] } } } }
        foreach ($id in @($saved | Select-Object -Unique)) {
            if ($map.ContainsKey($id)) { continue }
            $model = ($id -split '-')[0]
            $candidates = @($map.Keys | Where-Object { ($_ -split '-')[0] -eq $model })
            if ($candidates.Count -eq 1) { $map[$id] = $map[$candidates[0]] }
        }
    }
    $map
}

function Get-WinarchyUnplaceableExes {
    <# Exes que no tiene sentido ubicar: juegos/launchers y lo que komorebi ya ignora.
       Son justamente los que no queremos capturar como preferencia. #>
    $exes = [System.Collections.Generic.List[string]]::new()
    foreach ($exe in 'explorer.exe', 'WindowsTerminal.exe', 'wezterm-gui.exe', 'conhost.exe') { $exes.Add($exe) }
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
            Slot          = if ($entry.ContainsKey('slot')) { [int]$entry['slot'] } else { $null }
            Pin           = $entry.ContainsKey('pin') -and $entry['pin']
        }
    }
}

function Export-WinarchyWindowPrefs {
    param([Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Pref, [object[]]$Layout, [string]$DefaultMode)
    # sin -Layout / -DefaultMode se conservan los guardados: quien solo toca ventanas no debe borrarlos
    if (-not $PSBoundParameters.ContainsKey('Layout')) { $Layout = @(Import-WinarchyWorkspaceLayouts) }
    if (-not $PSBoundParameters.ContainsKey('DefaultMode')) { $DefaultMode = Import-WinarchyDefaultMode }
    $lines = [System.Collections.Generic.List[string]]::new()
    $lines.Add('# Preferencias de ubicación de ventanas de ESTA máquina.')
    $lines.Add('# Se captura con `winarchy layout save` y se reproduce con `winarchy layout apply`.')
    $lines.Add('# El monitor va por device_id porque el índice cambia al reconectar pantallas.')
    $lines.Add('# `pin = true` fija la ventana permanentemente en vez de solo al abrirla.')
    $lines.Add('# `slot` es la posición del tile dentro del workspace (0 = el primero); solo se guarda y')
    $lines.Add('# se aplica en apps con `pin = true`. El resto se ubica al abrir y nadie las reordena.')
    $lines.Add('# `[[workspaces]]` guarda el modo (tiling / scrolling / floating) y el layout de los workspaces que se apartan del default.')
    $lines.Add('# `default_mode` es el modo de los workspaces sin entrada: tiling, scrolling o floating.')
    if ($DefaultMode -ne 'tiling') { $lines.Add("default_mode = `"$DefaultMode`"") }
    foreach ($p in ($Pref | Sort-Object Exe)) {
        $lines.Add('')
        $lines.Add('[[windows]]')
        $lines.Add("exe = `"$($p.Exe)`"")
        $lines.Add("monitor = `"$($p.Monitor)`"")
        $lines.Add("workspace = $($p.Workspace)")
        if ($p.WorkspaceName) { $lines.Add("workspace_name = `"$($p.WorkspaceName)`"") }
        if ($null -ne $p.Slot) { $lines.Add("slot = $($p.Slot)") }
        if ($p.Pin) { $lines.Add('pin = true') }
    }
    foreach ($l in ($Layout | Sort-Object Monitor, Workspace)) {
        $lines.Add('')
        $lines.Add('[[workspaces]]')
        $lines.Add("monitor = `"$($l.Monitor)`"")
        $lines.Add("workspace = $($l.Workspace)")
        if ($l.Layout) { $lines.Add("layout = `"$($l.Layout)`"") }
        if ($l.Mode) { $lines.Add("mode = `"$($l.Mode)`"") }
    }
    Set-WinarchyFileAtomic -Path (Get-WinarchyWindowPrefsPath) -Value ($lines -join "`r`n")
}

function Save-WinarchyWorkspaceLayouts {
    <# Layouts a guardar: los vivos reemplazan a los de los monitores conectados; los de
       monitores desconectados se conservan. Con -Keep (captura de una sola app) no se toca nada. #>
    param([Parameter(Mandatory)]$State, [switch]$Keep, [string]$DefaultMode = (Import-WinarchyDefaultMode))
    $saved = @(Import-WinarchyWorkspaceLayouts)
    if ($Keep) { return $saved }
    $map = Get-WinarchyMonitorIndexMap -State $State
    @($saved | Where-Object { -not $map.ContainsKey($_.Monitor) }) + @(Get-WinarchyLiveWorkspaceLayouts -State $State -DefaultMode $DefaultMode)
}

function Save-WinarchyWorkspaceModes {
    <# Guarda el modo vivo de cada workspace sin tocar las preferencias de ventanas, y deja
       komorebi.json al día para que un reinicio de komorebi (sin reload) lo restaure. #>
    $state = Get-WinarchyKomorebiState
    Export-WinarchyWindowPrefs -Pref @(Import-WinarchyWindowPrefs) -Layout @(Save-WinarchyWorkspaceLayouts -State $state)
    Update-WinarchyKomorebiRules -NoReload
}

function Set-WinarchyDefaultMode {
    <# default_mode de los workspaces sin entrada propia; queda en windows.toml y entra con el próximo render. #>
    param([Parameter(Mandatory)][ValidateSet('tiling', 'scrolling', 'floating')][string]$Mode)
    Export-WinarchyWindowPrefs -Pref @(Import-WinarchyWindowPrefs) -DefaultMode $Mode
    Write-WinarchyOk "default_mode = $Mode"
    Write-WinarchyInfo 'Applies to workspaces without their own mode on the next: winarchy layout apply'
}

function Get-WinarchyWorkspaceModes {
    <# Modo vivo de cada workspace, por monitor. #>
    $state = Get-WinarchyKomorebiState
    $m = -1
    foreach ($monitor in $state.monitors.elements) {
        $m++
        $w = -1
        foreach ($workspace in $monitor.workspaces.elements) {
            $w++
            [pscustomobject]@{ Monitor = $m; Workspace = $w; Name = $workspace.name; Mode = Get-WinarchyWorkspaceMode -Workspace $workspace }
        }
    }
}

function Save-WinarchyWindowLayout {
    <# Captura la disposición actual. Mergea aditivo: guardar con media sesión abierta no
       debe borrar las preferencias de apps que en ese momento no estaban corriendo.
       -Exe captura una sola app; -WhatIf muestra qué se guardaría sin escribir. #>
    param([string]$Exe, [switch]$WhatIf)
    if ($Exe -and $Exe -notlike '*.exe') { $Exe = "$Exe.exe" }
    $state = Get-WinarchyKomorebiState
    $skip = @(Get-WinarchyUnplaceableExes)
    $live = @(Get-WinarchyWindowPlacements -State $state |
        Where-Object { $skip -notcontains $_.Exe } |
        Where-Object { -not $Exe -or $_.Exe -ieq $Exe })
    if ($live.Count -eq 0) {
        if ($Exe) { Write-WinarchyWarn "'$Exe' is not a managed window right now." }
        else { Write-WinarchyWarn 'No managed windows to capture.' }
        return
    }
    if ($WhatIf) {
        Write-WinarchyInfo 'Would save:'
        foreach ($p in ($live | Sort-Object Exe)) {
            Write-Host ("    {0,-34} workspace {1} ({2}), slot {3}" -f $p.Exe, $p.Workspace, $p.WorkspaceName, $p.Slot)
        }
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
        if (-not $placement.Pin) { $placement.Slot = $null }
        $prefs += $placement
    }
    $null = Resolve-WinarchySlotConflicts -Pref $prefs -Live (Get-WinarchyLiveSlotKeys -State $state)
    $layouts = @(Save-WinarchyWorkspaceLayouts -State $state -Keep:([bool]$Exe))
    Export-WinarchyWindowPrefs -Pref $prefs -Layout $layouts
    Write-WinarchyOk "Layout saved: $added new, $updated updated ($($prefs.Count) total) → $(Get-WinarchyWindowPrefsPath)"
    Write-WinarchyInfo 'Apply it with: winarchy layout apply'
}

function Invoke-WinarchyLayoutApply {
    <# Regenera komorebi.json con las preferencias adentro y lo recarga. #>
    param([switch]$Quiet)
    $prefs = @(Import-WinarchyWindowPrefs)
    if ($prefs.Count -eq 0 -and @(Import-WinarchyWorkspaceLayouts).Count -eq 0) {
        if (-not $Quiet) { Write-WinarchyInfo 'No saved window preferences.' }
        return
    }
    if (-not (Test-WinarchyProcess 'komorebi')) {
        if (-not $Quiet) { Write-WinarchyWarn 'komorebi is not running; preferences not applied.' }
        return
    }
    $map = Get-WinarchyMonitorIndexMap -State (Get-WinarchyKomorebiState)
    foreach ($pref in @($prefs | Where-Object { -not $map.ContainsKey($_.Monitor) })) {
        Write-WinarchyWarn "'$($pref.Exe)': monitor '$($pref.Monitor)' not connected; rule skipped."
    }
    Update-WinarchyKomorebiRules -SkipLiveView
    if (-not $Quiet) {
        $applied = @($prefs | Where-Object { $map.ContainsKey($_.Monitor) }).Count
        Write-WinarchyOk "$applied window placement rule(s) written to komorebi.json and reloaded."
    }
}

function Get-WinarchyWindowLayout {
    <# Preferencias guardadas, marcando las que no resuelven a un monitor conectado. #>
    $prefs = @(Import-WinarchyWindowPrefs)
    $layouts = @(Import-WinarchyWorkspaceLayouts)
    if ($prefs.Count -eq 0 -and $layouts.Count -eq 0) {
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
        $slot = if ($null -ne $pref.Slot) { ", slot $($pref.Slot)" } else { '' }
        Write-Host ("  {0,-34} {1}, workspace {2}{3}{4}{5}" -f $pref.Exe, $where, $pref.Workspace, $name, $slot, $pin)
    }
    foreach ($l in ($layouts | Sort-Object Monitor, Workspace)) {
        $where = if ($map.ContainsKey($l.Monitor)) { "monitor $($map[$l.Monitor])" } else { 'monitor NOT CONNECTED' }
        Write-Host ("  {0,-34} {1}, workspace {2}" -f "layout $($l.Layout)", $where, $l.Workspace)
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
