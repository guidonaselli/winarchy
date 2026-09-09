# Start-Ahk.ps1 — arranque de AHK (tray de Winarchy) esperando shell listo, con
# reintentos. No-op si AHK ya está corriendo.

param(
    [Parameter(Mandatory)][string]$AhkExe,
    [Parameter(Mandatory)][string]$ScriptPath
)

$exe    = $AhkExe
$script = $ScriptPath
$logDir = Join-Path $env:LOCALAPPDATA 'Winarchy'
$log    = Join-Path $logDir 'ahk-autostart.log'

New-Item -ItemType Directory -Path $logDir -Force | Out-Null

Add-Type -Namespace Win -Name FgAhk -MemberDefinition @'
[DllImport("user32.dll")] public static extern System.IntPtr GetForegroundWindow();
'@

function Test-ForegroundReady {
    [bool]((Get-Process explorer -ErrorAction SilentlyContinue) -and `
           ([Win.FgAhk]::GetForegroundWindow() -ne [System.IntPtr]::Zero))
}

function Write-Log([string]$m) {
    "{0}  {1}" -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), $m |
        Out-File -FilePath $log -Append -Encoding utf8
}

function Test-AhkRunning {
    [bool](Get-Process AutoHotkey64 -ErrorAction SilentlyContinue)
}

if (-not (Test-Path $exe)) { Write-Log "AutoHotkey64.exe no encontrado en $exe; abortando."; exit 1 }
if (Test-AhkRunning) { Write-Log 'AHK ya estaba corriendo; no hago nada.'; exit 0 }

Write-Log '--- arranque (autostart) ---'

$overallDeadline = (Get-Date).AddMinutes(2)

$attempt = 0
while ((Get-Date) -lt $overallDeadline) {
    if (Test-AhkRunning) { Write-Log 'AHK vivo; listo.'; exit 0 }
    $attempt++

    while ((Get-Date) -lt $overallDeadline -and -not (Test-ForegroundReady)) {
        Start-Sleep -Milliseconds 500
    }

    Write-Log "intento ${attempt}: lanzando AutoHotkey64.exe"
    try {
        $p = Start-Process -FilePath $exe -ArgumentList "`"$script`"" -WindowStyle Hidden -PassThru
    }
    catch {
        Write-Log "intento ${attempt}: fallo al lanzar: $($_.Exception.Message)"
        Start-Sleep -Seconds 2
        continue
    }

    Start-Sleep -Seconds 3
    if (-not $p.HasExited) {
        Write-Log "intento ${attempt}: AHK sigue vivo tras 3s. OK."
        exit 0
    }

    $code = try { $p.ExitCode } catch { '?' }
    Write-Log "intento ${attempt}: AHK salió rápido (código '$code')."
    Start-Sleep -Seconds 2
}

Write-Log "vencido el presupuesto de 2 min; AHK no quedó arriba."
exit 1
