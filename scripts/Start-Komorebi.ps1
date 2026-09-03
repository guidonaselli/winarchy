# Start-Komorebi.ps1 — arranque resiliente de komorebi para el autostart de Winarchy.
#
# Lo lanza la Scheduled Task At-LogOn (oculto). Resuelve dos causas de que komorebi
# quedara OFFLINE al prender/desbloquear la PC:
#   a) panic temprano (Application Error 0xc0000409 = abort de Rust).
#   b) "failed call to AllowSetForegroundWindow after 5 retries" (main.rs:219): komorebi
#      arranca antes de que la sesión tenga una ventana de foreground real. Pasa típico
#      al desbloquear rápido: el LogonTrigger dispara y el escritorio no asentó todavía.
# Antes el launcher cortaba a los 5 intentos en <1 min y quedaba muerto hasta reiniciar
# a mano. Este launcher:
#   1. Redirige stdout/stderr de komorebi a un log.
#   2. Espera a que exista foreground real (explorer + GetForegroundWindow != 0), no solo
#      a que explorer exista, y baja el foreground lock timeout a 0.
#   3. Reintenta con presupuesto de tiempo (~5 min) y re-espera el foreground en cada
#      vuelta, en vez de rendirse a los pocos segundos.
#
# No-op si komorebi ya está corriendo.

$root = Split-Path $PSScriptRoot -Parent
$env:KOMOREBI_CONFIG_HOME = Join-Path $root 'config\komorebi'
$exe    = Join-Path $env:ProgramFiles 'komorebi\bin\komorebi.exe'
$logDir = Join-Path $env:LOCALAPPDATA 'komorebi'
$log    = Join-Path $logDir 'komorebi-winarchy.log'
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

function Test-ForegroundReady {
    [bool]((Get-Process explorer -ErrorAction SilentlyContinue) -and `
           ([Win.Fg]::GetForegroundWindow() -ne [System.IntPtr]::Zero))
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

# Presupuesto total para que el escritorio asiente y komorebi quede arriba. Cubre el
# caso de desbloqueo rápido, donde el foreground tarda en existir.
$overallDeadline = (Get-Date).AddMinutes(5)

# 1) Esperar foreground real (explorer + ventana de foreground != 0), no solo explorer.
while ((Get-Date) -lt $overallDeadline -and -not (Test-ForegroundReady)) {
    Start-Sleep -Milliseconds 500
}
Start-Sleep -Seconds 2

# 2) Intentos con presupuesto de tiempo: re-esperar foreground en cada vuelta, bajar el
#    lock timeout, lanzar oculto y verificar que sobrevive >8s. No nos rendimos a los
#    pocos segundos: reintentamos hasta $overallDeadline.
$attempt = 0
while ((Get-Date) -lt $overallDeadline) {
    if (Test-KomorebiRunning) { Write-Log 'komorebi vivo; listo.'; exit 0 }
    $attempt++

    # Asegurar la precondición de AllowSetForegroundWindow antes de cada intento.
    while ((Get-Date) -lt $overallDeadline -and -not (Test-ForegroundReady)) {
        Start-Sleep -Milliseconds 500
    }
    [Win.Fg]::SystemParametersInfo(0x2001, 0, [System.IntPtr]::Zero, 0x3) | Out-Null
    Reset-ForegroundLock

    Write-Log "intento ${attempt}: lanzando komorebi.exe"
    try {
        $p = Start-Process -FilePath $exe -WindowStyle Hidden -PassThru `
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

            # Apps con ignore_rules (juegos, overlays) que ya arrancaron con Windows antes
            # de que komorebi terminara de levantar (los ~8s+ de espera de arriba) quedan
            # tileadas: komorebi las gestionó en su escaneo inicial y las ignore_rules por
            # exe no se reevalúan solas sobre ventanas preexistentes. Un retile fuerza esa
            # reevaluación para que las reglas del komorebi.json ya generado se apliquen
            # también a lo que abrió antes de tiempo, no solo a lo que abra después.
            try {
                & $komorebic retile *> $null
                Write-Log 'retile forzado post-arranque (aplica ignore_rules a ventanas preexistentes).'
            }
            catch { Write-Log "no pude forzar retile post-arranque: $($_.Exception.Message)" }
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
