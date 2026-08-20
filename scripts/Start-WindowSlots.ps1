# Start-WindowSlots.ps1 — mantiene cada ventana en el slot que el usuario le dio.
#
# Lo lanza la Scheduled Task At-LogOn (oculto, no elevado). Se suscribe a los eventos de
# komorebi por named pipe y reacciona según quién movió la ventana:
#   - apareció (o se fue) una ventana → reconcilia contra los slots de config/windows.toml.
#   - la movió el usuario             → aprende: el slot guardado pasa a ser la posición nueva.
#
# Quién fue se deduce comparando el conjunto de ventanas de cada workspace contra el de la
# ráfaga anterior, no del tipo de evento: así un gesto nuevo (o un komorebi que renombre sus
# eventos) no puede hacer que un reordenamiento del usuario se revierta.
#
# El pipe se drena en un hilo aparte y el trabajo se hace en el principal. No es adorno:
# cada evento trae el estado completo (~24 KB), así que bastan dos o tres sin leer para
# llenar el buffer del pipe, y ahí komorebi se queda bloqueado escribiendo — deja de
# contestar `komorebic state` y el reconciliador se queda ciego justo después de mover algo.
#
# Named pipe y no socket: el UDS deja un archivo en disco cuyo ciclo de vida ya nos costó
# caro (komorebi.sock stale = komorebi OFFLINE al boot). El pipe no deja residuo.
#
# La suscripción muere con komorebi, así que el loop externo espera y se re-suscribe en vez
# de dar el proceso por terminado.

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
foreach ($file in Get-ChildItem -Path (Join-Path $root 'module\Winarchy\Private') -Filter '*.ps1') {
    . $file.FullName
}

$logDir = Join-Path $env:LOCALAPPDATA 'winarchy'
New-Item -ItemType Directory -Path $logDir -Force | Out-Null
$log = Join-Path $logDir 'window-slots.log'
if ((Test-Path $log) -and (Get-Item $log).Length -gt 1MB) { Remove-Item $log -Force }

function Write-Log {
    param([string]$Message)
    "{0:yyyy-MM-dd HH:mm:ss} {1}" -f (Get-Date), $Message | Add-Content -Path $log -Encoding UTF8
}

$debounceMs = 250
$pipeName = 'winarchy-window-slots'

# Una sola instancia: el pipe es el candado. Sin esto, la tarea del autostart y un arranque
# a mano dejan dos daemons peleando por el mismo nombre de pipe, y el que pierde reintenta
# en loop llenando el log.
if (Test-WinarchyWindowSlots) {
    Write-Log 'another instance already holds the pipe; exiting'
    return
}

Write-Log 'started'

while ($true) {
    if (-not (Test-WinarchyProcess 'komorebi')) {
        Start-Sleep -Seconds 5
        continue
    }

    $server = $null
    $reader = $null
    $runspace = $null
    $pump = $null
    try {
        $server = New-Object System.IO.Pipes.NamedPipeServerStream(
            $pipeName, [System.IO.Pipes.PipeDirection]::In, 1,
            [System.IO.Pipes.PipeTransmissionMode]::Byte, [System.IO.Pipes.PipeOptions]::Asynchronous)
        $null = Invoke-WinarchyKomorebic subscribe-pipe $pipeName
        $server.WaitForConnection()
        $reader = New-Object System.IO.StreamReader($server)

        # Hilo que sólo drena el pipe: mantiene a komorebi desbloqueado mientras el hilo
        # principal reordena. Se guarda el tipo de evento, no el estado que lo acompaña.
        $events = [System.Collections.Concurrent.ConcurrentQueue[string]]::new()
        $runspace = [runspacefactory]::CreateRunspace()
        $runspace.Open()
        $runspace.SessionStateProxy.SetVariable('reader', $reader)
        $runspace.SessionStateProxy.SetVariable('events', $events)
        $pump = [powershell]::Create()
        $pump.Runspace = $runspace
        $null = $pump.AddScript({
                while ($true) {
                    $line = $reader.ReadLine()
                    if ($null -eq $line) { $events.Enqueue('!eof'); break }
                    try { $events.Enqueue(($line | ConvertFrom-Json).event.type) } catch { }
                }
            })
        $handle = $pump.BeginInvoke()
        Write-Log 'subscribed'

        # Referencia inicial ANTES del primer evento: sin esto, el primer movimiento del
        # usuario no tendría contra qué compararse y se perdería sin aprenderse.
        $signature = try { Get-WinarchyWorkspaceSignature -State (Get-WinarchyKomorebiState) } catch { $null }
        # Movimientos que emitió la propia reconciliación: mientras queden por llegar, un
        # cambio de orden es nuestro y no del usuario, así que no se aprende.
        $selfMoves = 0
        $pending = $false
        $quiet = 0

        while (-not $handle.IsCompleted) {
            $type = $null
            if ($events.TryDequeue([ref]$type)) {
                if ($type -eq '!eof') { break }
                if (Test-WinarchySlotEvent -Type $type) { $pending = $true }
                $quiet = 0
                continue
            }

            Start-Sleep -Milliseconds 50
            $quiet += 50
            if (-not $pending -or $quiet -lt $debounceMs) { continue }
            $pending = $false

            try {
                $state = Get-WinarchyKomorebiState
                $current = Get-WinarchyWorkspaceSignature -State $state
                $appeared = Test-WinarchyWindowsAppeared -Previous $signature -Current $current

                if ($appeared -or $selfMoves -gt 0) {
                    $emitted = [int](Invoke-WinarchySlotReconcile -Quiet -State $state)
                    $selfMoves = $emitted
                    if ($emitted) {
                        Write-Log "reconciled ($emitted move(s))"
                        # la referencia es el estado corregido, no el que disparó
                        $signature = Get-WinarchyWorkspaceSignature -State (Get-WinarchyKomorebiState)
                        continue
                    }
                }
                elseif ($signature) {
                    if (Update-WinarchySlotFromState -Quiet -State $state) { Write-Log 'learned new slots' }
                }
                $signature = $current
            }
            catch { Write-Log "error: $($_.Exception.Message)" }
        }
    }
    catch { Write-Log "subscription error: $($_.Exception.Message)" }
    finally {
        if ($pump) { $pump.Stop(); $pump.Dispose() }
        if ($runspace) { $runspace.Dispose() }
        if ($reader) { $reader.Dispose() }
        if ($server) { $server.Dispose() }
        try { $null = Invoke-WinarchyKomorebic unsubscribe-pipe $pipeName } catch { }
    }

    Write-Log 'disconnected; waiting for komorebi'
    Start-Sleep -Seconds 5
}
