# Start-Komorebi.ps1 — arranque resiliente de komorebi para el autostart de Winarchy.
#
# Lo lanza la Scheduled Task At-LogOn (oculto). Resuelve la causa de que komorebi
# quedara OFFLINE al prender la PC: arrancaba demasiado temprano y paniqueaba
# (Application Error 0xc0000409 = panic/abort de Rust), y como el `Start-Process`
# directo lo lanzaba una sola vez, sin log y sin reintento, nadie lo volvía a
# levantar hasta reiniciarlo a mano. Este launcher:
#   1. Redirige stdout/stderr de komorebi a un log (antes se perdían: no sabíamos
#      por qué paniqueaba).
#   2. Espera a que el shell (explorer) esté listo antes del primer intento.
#   3. Reintenta si komorebi sale en los primeros segundos.
#
# Hereda KOMOREBI_CONFIG_HOME del entorno (User var que registra install.ps1).
# No-op si komorebi ya está corriendo.

$exe    = Join-Path $env:ProgramFiles 'komorebi\bin\komorebi.exe'
$logDir = Join-Path $env:LOCALAPPDATA 'komorebi'
$log    = Join-Path $logDir 'komorebi-winarchy.log'
$outLog = Join-Path $logDir 'komorebi.out.log'
$errLog = Join-Path $logDir 'komorebi.err.log'

New-Item -ItemType Directory -Path $logDir -Force | Out-Null

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

# 1) Esperar a que el shell esté listo. explorer suele estar al toque, pero en el
#    arranque puede no estarlo todavía; esperamos hasta 40s y damos un settle corto.
$deadline = (Get-Date).AddSeconds(40)
while ((Get-Date) -lt $deadline -and -not (Get-Process explorer -ErrorAction SilentlyContinue)) {
    Start-Sleep -Milliseconds 500
}
Start-Sleep -Seconds 4

# 2) Intentos: lanzar oculto con redirección de logs y verificar que sobrevive >8s.
$maxAttempts = 5
for ($i = 1; $i -le $maxAttempts; $i++) {
    if (Test-KomorebiRunning) { Write-Log 'komorebi vivo; listo.'; exit 0 }

    Write-Log "intento $i/${maxAttempts}: lanzando komorebi.exe"
    try {
        $p = Start-Process -FilePath $exe -WindowStyle Hidden -PassThru `
                -RedirectStandardOutput $outLog -RedirectStandardError $errLog
    }
    catch {
        Write-Log "intento ${i}: fallo al lanzar: $($_.Exception.Message)"
        Start-Sleep -Seconds 2
        continue
    }

    Start-Sleep -Seconds 8
    if (-not $p.HasExited) { Write-Log "intento ${i}: komorebi sigue vivo tras 8s. OK."; exit 0 }

    # komorebi salió rápido: preservar su stderr (el siguiente intento lo sobrescribe).
    $code = try { $p.ExitCode } catch { '?' }
    Write-Log "intento ${i}: komorebi salió rápido (código '$code'). stderr:"
    if (Test-Path $errLog) {
        $stderr = (Get-Content $errLog -Raw -ErrorAction SilentlyContinue)
        if ($stderr) { foreach ($ln in ($stderr -split "`r?`n" | Where-Object { $_ })) { Write-Log "    | $ln" } }
        else { Write-Log '    | (stderr vacío)' }
    }
    Start-Sleep -Seconds 3
}

Write-Log "agotados $maxAttempts intentos; komorebi no quedó arriba."
exit 1
