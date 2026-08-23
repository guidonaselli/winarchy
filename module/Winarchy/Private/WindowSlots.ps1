# WindowSlots.ps1 — la posición del tile dentro del workspace.
#
# komorebi ordena los containers por orden de inserción y no tiene regla declarativa para
# fijar "esta app va en el tile N": fijarlo es necesariamente imperativo. Acá vive el cálculo
# (puro, testeable contra un fixture de `komorebic state`) y su ejecución vía komorebic.
#
# Dos direcciones, según quién movió la ventana. Quién fue se deduce del estado, no del tipo
# de evento: si cambió QUÉ ventanas hay en el workspace, apareció o se fue una y hay que
# reconciliar; si son las mismas y cambió el ORDEN, lo reordenó el usuario y hay que aprenderlo.
# El tipo de evento sólo decide si vale la pena mirar el estado.

function Get-WinarchyWorkspaceExes {
    <# Exes por índice de container de un workspace. Un container stackeado cuenta como una
       sola posición: se representa por su ventana visible. #>
    param([Parameter(Mandatory)]$Workspace)
    $out = foreach ($container in $Workspace.containers.elements) {
        $window = @($container.windows.elements)[0]
        if ($window) { $window.exe } else { '' }
    }
    @($out)
}

function Get-WinarchyWorkspaceSignature {
    <# Por workspace, qué ventanas contiene (sin importar el orden). Comparar dos firmas
       distingue "apareció/se fue una ventana" de "las mismas ventanas en otro orden", que es
       lo único que hace falta para saber si corregir o aprender. #>
    param([Parameter(Mandatory)]$State)
    $signature = @{}
    $monitorIndex = -1
    foreach ($monitor in $State.monitors.elements) {
        $monitorIndex++
        $workspaceIndex = -1
        foreach ($workspace in $monitor.workspaces.elements) {
            $workspaceIndex++
            $exes = @(Get-WinarchyWorkspaceExes -Workspace $workspace | Sort-Object)
            $signature["$monitorIndex,$workspaceIndex"] = $exes -join '|'
        }
    }
    $signature
}

function Test-WinarchyWindowsAppeared {
    <# True si entre las dos firmas cambió el conjunto de ventanas de algún workspace. #>
    param($Previous, [Parameter(Mandatory)]$Current)
    if (-not $Previous) { return $false }
    foreach ($key in $Current.Keys) {
        if (-not $Previous.ContainsKey($key)) { return $true }
        if ($Previous[$key] -ne $Current[$key]) { return $true }
    }
    foreach ($key in $Previous.Keys) {
        if (-not $Current.ContainsKey($key)) { return $true }
    }
    $false
}

function Get-WinarchySlotMoves {
    <#
      Movimientos pendientes para que cada app con slot declarado quede en su posición.
      Array vacío = el estado ya cumple. Puro: no toca komorebi, sólo lee el estado.

      Se procesa por slot ascendente sobre una lista simulada. Cada movimiento se ejecuta
      después como una serie de `cycle-move`, que va swapeando con el vecino: burbujear un
      elemento hasta su destino equivale a sacarlo y reinsertarlo ahí, así que la simulación
      y la ejecución coinciden.
    #>
    param([Parameter(Mandatory)]$State, [Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Pref)
    $withSlot = @($Pref | Where-Object { $null -ne $_.Slot })
    if ($withSlot.Count -eq 0) { return @() }
    $map = Get-WinarchyMonitorIndexMap -State $State

    $moves = [System.Collections.Generic.List[object]]::new()
    $monitorIndex = -1
    foreach ($monitor in $State.monitors.elements) {
        $monitorIndex++
        $workspaceIndex = -1
        foreach ($workspace in $monitor.workspaces.elements) {
            $workspaceIndex++
            $exes = [System.Collections.Generic.List[string]]::new()
            foreach ($exe in (Get-WinarchyWorkspaceExes -Workspace $workspace)) { $exes.Add($exe) }
            if ($exes.Count -lt 2) { continue }
            # El container enfocado: los comandos navegan por índice desde ahí, porque el exe
            # no identifica una ventana (la misma app puede tener ventanas en varios monitores).
            $focusedIndex = if ($workspace.containers.PSObject.Properties['focused']) { [int]$workspace.containers.focused } else { 0 }

            $targets = @($withSlot |
                Where-Object { $map.ContainsKey($_.Monitor) -and $map[$_.Monitor] -eq $monitorIndex } |
                Where-Object { $_.Workspace -eq $workspaceIndex } |
                Sort-Object Slot)

            # un slot se reclama una sola vez por workspace
            $claimed = [System.Collections.Generic.HashSet[int]]::new()

            foreach ($target in $targets) {
                $from = -1
                for ($i = 0; $i -lt $exes.Count; $i++) {
                    if ($exes[$i] -ieq $target.Exe) { $from = $i; break }
                }
                if ($from -lt 0) { continue }
                $to = [Math]::Min($target.Slot, $exes.Count - 1)
                if (-not $claimed.Add($to)) { continue }
                if ($from -eq $to) { continue }
                $exes.RemoveAt($from)
                $exes.Insert($to, $target.Exe)
                $moves.Add([pscustomobject]@{
                        Exe       = $target.Exe
                        Monitor   = $monitorIndex
                        Workspace = $workspaceIndex
                        From      = $from
                        To        = $to
                        Focused   = $focusedIndex
                        Count     = $exes.Count
                    })
                # el foco viaja con la ventana movida
                $focusedIndex = $to
            }
        }
    }
    @($moves)
}

function Get-WinarchySlotCommands {
    <#
      Un movimiento como la secuencia de argumentos de komorebic que lo ejecuta.

      Se navega por índice —monitor, workspace y después `cycle-focus`— y NO con
      `eager-focus <exe>`: eager-focus toma la primera ventana que matchea el exe, que puede
      estar en otro monitor. Con dos ventanas de la misma app (dos Zen, por ejemplo) eso
      reordena el workspace equivocado.
    #>
    param([Parameter(Mandatory)]$Move)
    $out = , @('focus-monitor-workspace', "$($Move.Monitor)", "$($Move.Workspace)")
    $steps = ($Move.From - $Move.Focused) % $Move.Count
    if ($steps -lt 0) { $steps += $Move.Count }
    for ($i = 0; $i -lt $steps; $i++) { $out += , @('cycle-focus', 'next') }
    $direction = if ($Move.To -gt $Move.From) { 'next' } else { 'previous' }
    for ($i = 0; $i -lt [Math]::Abs($Move.To - $Move.From); $i++) { $out += , @('cycle-move', $direction) }
    @($out)
}

function Get-WinarchyFocusedHwnd {
    <# hwnd de la ventana enfocada, para devolverle el foco después de reordenar. #>
    param([Parameter(Mandatory)]$State)
    function Get-Focused($collection) {
        if (-not $collection -or -not $collection.PSObject.Properties['focused']) { return }
        # `focused` puede apuntar a un elemento que ya no está
        $elements = @($collection.elements)
        if ($collection.focused -ge $elements.Count) { return }
        $elements[$collection.focused]
    }
    $monitor = Get-Focused $State.monitors
    if (-not $monitor) { return }
    $workspace = Get-Focused $monitor.workspaces
    if (-not $workspace) { return }
    $container = Get-Focused $workspace.containers
    if (-not $container) { return }
    (Get-Focused $container.windows).hwnd
}

function Get-WinarchyWindowLocation {
    <# Dónde está una ventana ahora: monitor, workspace, índice de container y qué container
       tiene el foco en ese workspace. $null si la ventana ya no existe. #>
    param([Parameter(Mandatory)]$State, [Parameter(Mandatory)]$Hwnd)
    $monitorIndex = -1
    foreach ($monitor in $State.monitors.elements) {
        $monitorIndex++
        $workspaceIndex = -1
        foreach ($workspace in $monitor.workspaces.elements) {
            $workspaceIndex++
            $containerIndex = -1
            foreach ($container in $workspace.containers.elements) {
                $containerIndex++
                foreach ($window in $container.windows.elements) {
                    if ($window.hwnd -ne $Hwnd) { continue }
                    return [pscustomobject]@{
                        Monitor   = $monitorIndex
                        Workspace = $workspaceIndex
                        Index     = $containerIndex
                        Focused   = if ($workspace.containers.PSObject.Properties['focused']) { [int]$workspace.containers.focused } else { 0 }
                        Count     = @($workspace.containers.elements).Count
                    }
                }
            }
        }
    }
}

function Get-WinarchyFocusCommands {
    <# Comandos para devolver el foco a una ubicación concreta. #>
    param([Parameter(Mandatory)]$Location)
    $out = , @('focus-monitor-workspace', "$($Location.Monitor)", "$($Location.Workspace)")
    $steps = ($Location.Index - $Location.Focused) % $Location.Count
    if ($steps -lt 0) { $steps += $Location.Count }
    for ($i = 0; $i -lt $steps; $i++) { $out += , @('cycle-focus', 'next') }
    @($out)
}

function Get-WinarchySlotRestore {
    <#
      Dónde queda la ventana que el usuario tenía enfocada, después de aplicar los
      movimientos. Se calcula replicando los movimientos sobre los índices en vez de releer
      `komorebic state`: recién reordenado, komorebi contesta ese pedido vacío (exit 0, sin
      salida) durante un rato, y el foco del usuario no puede depender de esa lotería.
    #>
    param([Parameter(Mandatory)]$State, [Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Move)
    if ($Move.Count -eq 0) { return }
    $hwnd = Get-WinarchyFocusedHwnd -State $State
    if (-not $hwnd) { return }
    $location = Get-WinarchyWindowLocation -State $State -Hwnd $hwnd
    if (-not $location) { return }

    $index = $location.Index
    $last = $null
    foreach ($m in $Move) {
        if ($m.Monitor -ne $location.Monitor -or $m.Workspace -ne $location.Workspace) { continue }
        $last = $m
        if ($index -eq $m.From) { $index = $m.To; continue }
        # el resto se corre según por dónde pasó la ventana movida
        if ($m.From -lt $index -and $m.To -ge $index) { $index-- }
        elseif ($m.From -gt $index -and $m.To -le $index) { $index++ }
    }
    # tras el último movimiento el foco quedó en la ventana movida
    if (-not $last) { $last = @($Move)[-1] }
    [pscustomobject]@{
        Monitor   = $location.Monitor
        Workspace = $location.Workspace
        Index     = $index
        Focused   = if ($last.Monitor -eq $location.Monitor -and $last.Workspace -eq $location.Workspace) { $last.To } else { $location.Focused }
        Count     = $location.Count
    }
}

function Invoke-WinarchySlotReconcile {
    <# Lleva cada app con slot declarado a su posición. No-op si ya cumplen.
       Devuelve la cantidad de eventos de movimiento que emitió. #>
    param([switch]$Quiet, $State)
    $prefs = @(Import-WinarchyWindowPrefs)
    if (@($prefs | Where-Object { $null -ne $_.Slot }).Count -eq 0) {
        if (-not $Quiet) { Write-WinarchyInfo 'No saved slots; nothing to order.' }
        return
    }
    if (-not $State -and -not (Test-WinarchyProcess 'komorebi')) {
        if (-not $Quiet) { Write-WinarchyWarn 'komorebi is not running; nothing to order.' }
        return
    }
    $state = if ($State) { $State } else { Get-WinarchyKomorebiState }
    $moves = @(Get-WinarchySlotMoves -State $state -Pref $prefs)
    if ($moves.Count -eq 0) {
        if (-not $Quiet) { Write-WinarchyOk 'Every window is already in its slot.' }
        return
    }
    $restore = Get-WinarchySlotRestore -State $state -Move $moves
    $steps = 0
    foreach ($move in $moves) {
        foreach ($command in (Get-WinarchySlotCommands -Move $move)) {
            $null = Invoke-WinarchyKomorebic @command
            if ($command[0] -eq 'cycle-move') { $steps++ }
        }
    }
    if ($restore) {
        foreach ($command in (Get-WinarchyFocusCommands -Location $restore)) { $null = Invoke-WinarchyKomorebic @command }
    }
    if (-not $Quiet) { Write-WinarchyOk "$($moves.Count) window(s) moved to their slot." }
    # Devuelve los eventos de movimiento que este reordenamiento va a generar: el daemon los
    # descuenta para no aprender su propia corrección como si fuera del usuario.
    $steps
}

function Get-WinarchyLiveSlotKeys {
    <# Slot vivo de cada ventana, por `exe|device_id|workspace`. La clave es compuesta y no
       sólo el exe: una app puede tener ventanas en varios workspaces. #>
    param([Parameter(Mandatory)]$State)
    $live = @{}
    foreach ($monitor in $State.monitors.elements) {
        $workspaceIndex = -1
        foreach ($workspace in $monitor.workspaces.elements) {
            $workspaceIndex++
            $slot = -1
            foreach ($container in $workspace.containers.elements) {
                $slot++
                foreach ($window in $container.windows.elements) {
                    if (-not $window.exe) { continue }
                    $key = "$($window.exe)|$($monitor.device_id)|$workspaceIndex"
                    if (-not $live.ContainsKey($key)) { $live[$key] = $slot }
                }
            }
        }
    }
    $live
}

function Get-WinarchyLearnedSlots {
    <# Preferencias con el slot actualizado a la posición viva. Sólo se toca el slot: una app
       sin preferencia previa no adquiere una por haber sido movida. $null = nada cambió. #>
    param([Parameter(Mandatory)]$State, [Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Pref)
    $live = Get-WinarchyLiveSlotKeys -State $State
    $changed = $false
    # `$entry` y no `$pref`: las variables de PowerShell son case-insensitive, así que un
    # `foreach ($pref in $Pref)` le asigna cada elemento al propio parámetro, y la restricción
    # [object[]] lo reenvuelve en un array de uno.
    foreach ($entry in $Pref) {
        $key = "$($entry.Exe)|$($entry.Monitor)|$($entry.Workspace)"
        if (-not $live.ContainsKey($key)) { continue }
        if ($live[$key] -eq $entry.Slot) { continue }
        $entry.Slot = $live[$key]
        $changed = $true
    }
    if (-not $changed) { return $null }
    @($Pref)
}

function Resolve-WinarchySlotConflicts {
    <# Slots únicos por workspace: el duplicado se corre al primer libre. `Live` no se mueve. #>
    param(
        [Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Pref,
        [hashtable]$Live = @{}
    )
    $changed = $false
    $groups = $Pref | Where-Object { $null -ne $_.Slot } | Group-Object { "$($_.Monitor)|$($_.Workspace)" }
    foreach ($group in $groups) {
        $taken = [System.Collections.Generic.HashSet[int]]::new()
        $ordered = $group.Group | Sort-Object @{ Expression = { -not $Live.ContainsKey("$($_.Exe)|$($_.Monitor)|$($_.Workspace)") } }, Slot, Exe
        foreach ($entry in $ordered) {
            $slot = $entry.Slot
            while (-not $taken.Add($slot)) { $slot++ }
            if ($slot -ne $entry.Slot) { $entry.Slot = $slot; $changed = $true }
        }
    }
    $changed
}

function Update-WinarchySlotFromState {
    <# Aprende la posición que el usuario acaba de darle a una ventana. #>
    param([switch]$Quiet, $State)
    $prefs = @(Import-WinarchyWindowPrefs)
    if ($prefs.Count -eq 0) { return }
    if (-not $State -and -not (Test-WinarchyProcess 'komorebi')) { return }
    if (-not $State) { $State = Get-WinarchyKomorebiState }
    $learned = Get-WinarchyLearnedSlots -State $State -Pref $prefs
    if (-not $learned) { return }
    $null = Resolve-WinarchySlotConflicts -Pref $learned -Live (Get-WinarchyLiveSlotKeys -State $State)
    Export-WinarchyWindowPrefs -Pref $learned
    if (-not $Quiet) { Write-WinarchyOk 'Slots updated from the current arrangement.' }
    $true
}

function Test-WinarchySlotEvent {
    <#
      ¿Este evento amerita mirar el estado? Es sólo un filtro de ruido: quién movió la
      ventana lo decide la firma del workspace, no este nombre. Los nombres son el
      PascalCase del comando de komorebic que los origina, más los eventos de ventana;
      MoveResizeEnd y MouseCapture son los del arrastre con el mouse.

      Un tipo desconocido no despierta al daemon, y lo peor que puede pasar con uno nuevo
      es que un reordenamiento se note recién en el próximo evento útil.
    #>
    param([string]$Type)
    $Type -in @(
        # apareció / se fue una ventana
        'ManageWindow', 'UnmanageWindow', 'Manage', 'Unmanage', 'Show', 'Hide', 'Uncloak', 'Cloak', 'Destroy',
        # se movió una ventana
        'MoveWindow', 'CycleMoveWindow', 'Promote', 'PromoteSwap', 'PromoteWindow',
        'MouseCapture', 'MoveResizeEnd'
    )
}

function Test-WinarchyWindowSlots {
    <# El daemon vivo Y suscrito: el named pipe existe mientras la suscripción está abierta,
       y desaparece con el proceso. Más fiable que buscar un powershell.exe entre otros. #>
    Test-Path '\\.\pipe\winarchy-window-slots'
}

function Stop-WinarchyWindowSlots {
    <# Mata el reconciliador: es un pwsh entre otros, se lo identifica por su línea de
       comando. Get-Process y no CIM: en esta máquina el stack MI falla seguido. #>
    Get-Process -Name 'pwsh' -ErrorAction SilentlyContinue |
        Where-Object { $_.CommandLine -like '*Start-WindowSlots.ps1*' } |
        Stop-Process -Force -ErrorAction SilentlyContinue
}

function Start-WinarchyWindowSlots {
    <# Levanta el reconciliador si no está corriendo. #>
    if (Test-WinarchyWindowSlots) { return }
    $launcher = Join-Path (Get-WinarchyRoot) 'scripts\Start-WindowSlots.ps1'
    if (-not (Test-Path $launcher)) { return }
    $pwsh = (Get-Command pwsh -ErrorAction SilentlyContinue).Source
    if (-not $pwsh) { return }
    Start-Process $pwsh -WindowStyle Hidden -ArgumentList `
        '-NoProfile', '-WindowStyle', 'Hidden', '-ExecutionPolicy', 'Bypass', '-File', "`"$launcher`""
}
