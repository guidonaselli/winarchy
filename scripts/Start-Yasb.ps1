# Start-Yasb.ps1 — arranque de YASB (barra) esperando shell listo, con reintentos.
# No-op si YASB ya está corriendo.

param(
    [Parameter(Mandatory)][string]$YasbExe,
    [Parameter(Mandatory)][string]$YasbConfigHome
)

$env:YASB_CONFIG_HOME = $YasbConfigHome
$logDir = Join-Path $env:LOCALAPPDATA 'Winarchy'
$log    = Join-Path $logDir 'yasb-autostart.log'

New-Item -ItemType Directory -Path $logDir -Force | Out-Null

Add-Type -Namespace Win -Name FgYasb -MemberDefinition @'
[DllImport("user32.dll")] public static extern System.IntPtr GetForegroundWindow();
'@

function Test-ForegroundReady {
    [bool]((Get-Process explorer -ErrorAction SilentlyContinue) -and `
           ([Win.FgYasb]::GetForegroundWindow() -ne [System.IntPtr]::Zero))
}

function Write-Log([string]$m) {
    "{0}  {1}" -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), $m |
        Out-File -FilePath $log -Append -Encoding utf8
}

function Test-YasbRunning {
    [bool](Get-Process yasb -ErrorAction SilentlyContinue)
}

if (-not (Test-Path $YasbExe)) { Write-Log "yasbc.exe no encontrado en $YasbExe; abortando."; exit 1 }
if (Test-YasbRunning) { Write-Log 'YASB ya estaba corriendo; no hago nada.'; exit 0 }

Write-Log '--- arranque (autostart) ---'

$overallDeadline = (Get-Date).AddMinutes(1)

$attempt = 0
while ((Get-Date) -lt $overallDeadline) {
    if (Test-YasbRunning) { Write-Log 'YASB vivo; listo.'; exit 0 }
    $attempt++

    while ((Get-Date) -lt $overallDeadline -and -not (Test-ForegroundReady)) {
        Start-Sleep -Milliseconds 500
    }

    Write-Log "intento ${attempt}: lanzando yasbc start"
    try {
        & $YasbExe start
    }
    catch {
        Write-Log "intento ${attempt}: fallo al lanzar: $($_.Exception.Message)"
        Start-Sleep -Seconds 2
        continue
    }

    Start-Sleep -Seconds 3
    if (Test-YasbRunning) {
        Write-Log "intento ${attempt}: YASB vivo tras 3s. OK."
        exit 0
    }

    Write-Log "intento ${attempt}: YASB no quedó corriendo."
    Start-Sleep -Seconds 2
}

Write-Log "vencido el presupuesto de 1 min; YASB no quedó arriba."
exit 1
