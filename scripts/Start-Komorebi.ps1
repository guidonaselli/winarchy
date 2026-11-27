# Start-Komorebi.ps1 — arranque resiliente de komorebi para el autostart de Winarchy.
#
# Lo lanza la Scheduled Task At-LogOn (oculto). Resuelve dos causas de que komorebi
# quedara OFFLINE al prender/desbloquear la PC:
#   a) panic temprano (Application Error 0xc0000409 = abort de Rust).
#   b) "failed call to AllowSetForegroundWindow after 5 retries" (main.rs:219): la sesión
#      no estaba lista cuando komorebi intentó tomar permiso de foreground.
# Este launcher:
#   1. Redirige stdout/stderr de komorebi a un log.
#   2. Espera a Explorer con foreground o al shell Winarchy de esta sesión. Las ventanas
#      Winarchy no activan foreground, así que komorebi valida el permiso al arrancar.
#   3. Reinicia el foreground lock, sintetiza Alt y reintenta hasta 5 min, verificando
#      que komorebi siga vivo tras 8 s.
#
# No-op si komorebi ya está corriendo.

$root = Split-Path $PSScriptRoot -Parent
$sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
$env:KOMOREBI_CONFIG_HOME = Join-Path $root 'config\komorebi'
$exe    = Join-Path $env:ProgramFiles 'komorebi\bin\komorebi.exe'
$logDir = Join-Path $env:LOCALAPPDATA 'komorebi'
$log    = Join-Path $logDir 'komorebi-winarchy.log'
if ((Test-Path $log) -and (Get-Item $log).Length -gt 512KB) { Move-Item $log "$log.1" -Force -ErrorAction SilentlyContinue }
$outLog = Join-Path $logDir 'komorebi.out.log'
$errLog = Join-Path $logDir 'komorebi.err.log'

New-Item -ItemType Directory -Path $logDir -Force | Out-Null

# P/Invoke para detectar foreground real y bajar el lock timeout. Sin esto,
# AllowSetForegroundWindow de komorebi es rechazada cuando el escritorio no asentó.
Add-Type -Namespace Win -Name Fg -MemberDefinition @'
[DllImport("user32.dll")] public static extern System.IntPtr GetForegroundWindow();
[DllImport("user32.dll", SetLastError=true)] public static extern bool SystemParametersInfo(uint a, uint b, System.IntPtr c, uint d);
[DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, System.UIntPtr extra);
'@

function Reset-ForegroundLock {
    # Sintetiza un tap de Alt (down/up). Windows suelta el foreground-lock cuando la
    # sesión recibe un evento de input, dejando que komorebi complete su
    # AllowSetForegroundWindow. Sin esto, en boots donde otra app retiene el foreground,
    # komorebi corta a los 5 reintentos (main.rs:219) durante todo el presupuesto.
    [Win.Fg]::keybd_event(0x12, 0, 0, [System.UIntPtr]::Zero)        # VK_MENU down
    [Win.Fg]::keybd_event(0x12, 0, 0x2, [System.UIntPtr]::Zero)      # VK_MENU up (KEYEVENTF_KEYUP)
}

function Test-ShellReady {
    $shells = @(Get-Process -Name explorer, 'winarchy-shell' -ErrorAction SilentlyContinue | Where-Object SessionId -eq $sessionId)
    [bool]($shells.Count -gt 0 -and (($shells.ProcessName -contains 'winarchy-shell') -or
           ([Win.Fg]::GetForegroundWindow() -ne [System.IntPtr]::Zero)))
}

function Write-Log([string]$m) {
    "{0}  {1}" -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), $m |
        Out-File -FilePath $log -Append -Encoding utf8
}

function Test-KomorebiRunning {
    [bool](Get-Process komorebi -ErrorAction SilentlyContinue)
}

if (-not (Test-Path $exe)) { Write-Log "komorebi.exe no encontrado en $exe; abortando."; exit 1 }
if (Test-KomorebiRunning)  { Write-Log 'komorebi ya estaba corriendo; no hago nada.'; exit 0 }

Write-Log '--- arranque (autostart) ---'

# komorebi deja un log por dia en %TEMP% y no borra ninguno
Get-ChildItem (Join-Path $env:TEMP 'komorebi.log.*'), (Join-Path $env:TEMP 'komorebi_plaintext.log.*') -ErrorAction SilentlyContinue |
    Where-Object LastWriteTime -LT (Get-Date).AddDays(-3) | Remove-Item -Force -ErrorAction SilentlyContinue

# Limpiar estado stale de un cierre sucio. komorebi.sock es un socket AF_UNIX; si
# komorebi crasheó, el archivo queda y el próximo arranque puede fallar al bindear
# hasta que se limpia (causa típica de "offline al boot, no levanta solo"). Seguro
# borrarlos acá porque ya confirmamos que komorebi NO está corriendo.
foreach ($stale in @('komorebi.sock', 'komorebi.hwnd.json')) {
    $f = Join-Path $logDir $stale
    if (Test-Path $f) {
        try { Remove-Item $f -Force -ErrorAction Stop; Write-Log "limpiado estado stale: $stale" }
        catch { Write-Log "no pude borrar $stale : $($_.Exception.Message)" }
    }
}

# El estado de game-mode es por sesión: las session-float-rules murieron con komorebi.
# Si el flag sobrevive al reboot, AHK arranca creyéndose en juego y deja el AccentWatch
# inerte para siempre, sin señal visible.
$stateDir = Join-Path $root 'state'
foreach ($stale in @('game-mode.flag', 'game-mode-floated')) {
    $f = Join-Path $stateDir $stale
    if (Test-Path $f) {
        Remove-Item $f -Force -ErrorAction SilentlyContinue
        Write-Log "limpiado estado de sesión stale: $stale"
    }
}

# Presupuesto total para que el shell asiente y komorebi quede arriba.
$overallDeadline = (Get-Date).AddMinutes(5)

# 1) Esperar Explorer con foreground o el shell Winarchy de esta sesión.
while ((Get-Date) -lt $overallDeadline -and -not (Test-ShellReady)) {
    Start-Sleep -Milliseconds 500
}
Start-Sleep -Seconds 2

# 2) Intentos con presupuesto de tiempo: revalidar el shell, bajar el lock timeout,
#    lanzar oculto y verificar que sobrevive >8s. Reintentamos hasta $overallDeadline.
$attempt = 0
while ((Get-Date) -lt $overallDeadline) {
    if (Test-KomorebiRunning) { Write-Log 'komorebi vivo; listo.'; exit 0 }
    $attempt++

    # Reiniciar la condición de foreground antes de cada intento.
    while ((Get-Date) -lt $overallDeadline -and -not (Test-ShellReady)) {
        Start-Sleep -Milliseconds 500
    }
    if ((Get-Date) -ge $overallDeadline) { break }
    [Win.Fg]::SystemParametersInfo(0x2001, 0, [System.IntPtr]::Zero, 0x3) | Out-Null
    Reset-ForegroundLock

    Write-Log "intento ${attempt}: lanzando komorebi.exe"
    try {
        # el redirect trunca: lo del arranque anterior (el rastro de una caida) queda en .1
        foreach ($f in $outLog, $errLog) { if ((Get-Item $f -ErrorAction SilentlyContinue).Length) { Move-Item $f "$f.1" -Force -ErrorAction SilentlyContinue } }
        # en info anota cada cambio de titulo: cientos de MB por dia
        $p = Start-Process -FilePath $exe -ArgumentList '--log-level', 'warn' -WindowStyle Hidden -PassThru `
                -RedirectStandardOutput $outLog -RedirectStandardError $errLog
    }
    catch {
        Write-Log "intento ${attempt}: fallo al lanzar: $($_.Exception.Message)"
        Start-Sleep -Seconds 2
        continue
    }

    Start-Sleep -Seconds 8
    if (-not $p.HasExited) {
        Write-Log "intento ${attempt}: komorebi sigue vivo tras 8s. OK."
        # Mitigación: en algunos boots el monitor principal (índice 0) queda enfocado en
        # el workspace 2 (índice 1) en vez del 1. No hallamos la causa raíz; forzamos el
        # workspace 0 del monitor principal solo en el arranque fresco. komorebic necesita
        # el socket ya bindeado, por eso va tras la ventana de supervivencia.
        $komorebic = Join-Path (Split-Path $exe) 'komorebic.exe'
        if (Test-Path $komorebic) {
            try {
                & $komorebic focus-monitor-workspace 0 0 *> $null
                Write-Log 'monitor principal reenfocado a workspace 0.'
            }
            catch { Write-Log "no pude reenfocar workspace 0: $($_.Exception.Message)" }

            # Desmanagea ventanas de games.toml que komorebi ya tileó en su escaneo
            # inicial (retile/ignore-rule no las desmanagean retroactivamente).
            try {
                # el módulo es de PowerShell 7 y este script corre en 5.1: la lista se pide a pwsh
                $pwsh = Get-Command pwsh -ErrorAction SilentlyContinue
                if (-not $pwsh) { throw 'pwsh is not on PATH' }
                $module = (Join-Path $root 'module\Winarchy\Winarchy.psm1').Replace("'", "''")
                $games = @(& $pwsh.Source -NoProfile -Command "Import-Module '$module' -Force; Get-WinarchyGames")
                $stateJson = & $komorebic state 2>$null | ConvertFrom-Json
                $tiledExes = @(
                    foreach ($m in $stateJson.monitors.elements) {
                        foreach ($ws in $m.workspaces.elements) {
                            foreach ($c in $ws.containers.elements) {
                                foreach ($w in $c.windows.elements) { $w.exe }
                            }
                        }
                    }
                ) | Sort-Object -Unique
                $toFree = @($tiledExes | Where-Object { $games -contains $_ })
                foreach ($gameExe in $toFree) {
                    & $komorebic eager-focus $gameExe *> $null
                    & $komorebic unmanage *> $null
                }
                if ($toFree.Count -gt 0) {
                    & $komorebic retile *> $null
                    Write-Log "desmanageadas $($toFree.Count) ventana(s) de games.toml que ya estaban tileadas ($($toFree -join ', ')) + retile."
                }
            }
            catch { Write-Log "no pude desmanagear ventanas de games.toml post-arranque: $($_.Exception.Message)" }
        }
        exit 0
    }

    # komorebi salió rápido: preservar su stderr (el siguiente intento lo sobrescribe).
    $code = try { $p.ExitCode } catch { '?' }
    Write-Log "intento ${attempt}: komorebi salió rápido (código '$code'). stderr:"
    $stderr = if (Test-Path $errLog) { (Get-Content $errLog -Raw -ErrorAction SilentlyContinue) } else { '' }
    if ($stderr) { foreach ($ln in ($stderr -split "`r?`n" | Where-Object { $_ })) { Write-Log "    | $ln" } }
    else { Write-Log '    | (stderr vacío)' }

    # Si fue el foreground, esperar un poco más a que asiente antes de reintentar.
    if ($stderr -match 'AllowSetForegroundWindow') {
        Write-Log "intento ${attempt}: foreground aún no listo; espero y reintento."
        Start-Sleep -Seconds 5
    }
    else {
        Start-Sleep -Seconds 3
    }
}

Write-Log "vencido el presupuesto de 5 min; komorebi no quedó arriba."
exit 1
