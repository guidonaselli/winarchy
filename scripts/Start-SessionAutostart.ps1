# Start-SessionAutostart.ps1 — lo que Explorer hace al iniciar sesión y la shell de Winarchy no:
# ejecutar Run/RunOnce (HKCU y HKLM) y las carpetas Startup, respetando StartupApproved.
# Una vez por inicio de sesión (-Identity); no relanza lo que ya corre.
param([Parameter(Mandatory)][string]$Identity, [int]$DelaySeconds = 3)

$ErrorActionPreference = 'Continue'
$root = Split-Path $PSScriptRoot -Parent
$sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
$done = Join-Path $root "state\session-autostart-$sessionId"
if ((Test-Path -LiteralPath $done) -and ((Get-Content -LiteralPath $done -Raw).Trim() -eq $Identity)) { return }
New-Item -ItemType Directory -Path (Split-Path $done) -Force | Out-Null
Set-Content -LiteralPath $done -Value $Identity -Encoding ASCII
Start-Sleep -Seconds $DelaySeconds

$log = Join-Path $root 'state\session-autostart.log'
function Write-Log($text) { Add-Content -LiteralPath $log -Value ('{0} {1}' -f (Get-Date -Format s), $text) }
Set-Content -LiteralPath $log -Value $null

function Get-Disabled([string]$hive, [string]$sub, [string]$name) {
    $key = "${hive}:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\$sub"
    $bytes = (Get-ItemProperty -LiteralPath $key -Name $name -ErrorAction SilentlyContinue).$name
    [bool]($bytes -and ($bytes[0] -band 1))
}

# Parte una línea de comando como CreateProcess: entre comillas, o el prefijo más corto que exista (rutas con espacios sin comillas).
function Split-Command([string]$command) {
    $text = [Environment]::ExpandEnvironmentVariables($command.Trim())
    if ($text -match '^"([^"]+)"\s*(.*)$') { return [pscustomobject]@{ File = $matches[1]; Arguments = $matches[2] } }
    $words = $text -split ' '
    for ($n = 1; $n -le $words.Count; $n++) {
        $candidate = ($words[0..($n - 1)] -join ' ')
        foreach ($path in @($candidate, "$candidate.exe")) {
            if (Test-Path -LiteralPath $path -PathType Leaf) { return [pscustomobject]@{ File = $path; Arguments = ($words[$n..($words.Count)] -join ' ').Trim() } }
        }
    }
    [pscustomobject]@{ File = $text; Arguments = '' }
}

$entries = [System.Collections.Generic.List[object]]::new()
foreach ($spec in @(
    @{ Hive = 'HKCU'; Path = 'Software\Microsoft\Windows\CurrentVersion\Run'; Approved = 'Run'; Once = $false },
    @{ Hive = 'HKLM'; Path = 'Software\Microsoft\Windows\CurrentVersion\Run'; Approved = 'Run'; Once = $false },
    @{ Hive = 'HKLM'; Path = 'Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run'; Approved = 'Run32'; Once = $false },
    @{ Hive = 'HKCU'; Path = 'Software\Microsoft\Windows\CurrentVersion\RunOnce'; Approved = $null; Once = $true },
    @{ Hive = 'HKLM'; Path = 'Software\Microsoft\Windows\CurrentVersion\RunOnce'; Approved = $null; Once = $true }
)) {
    $key = "$($spec.Hive):\$($spec.Path)"
    $props = (Get-ItemProperty -LiteralPath $key -ErrorAction SilentlyContinue).PSObject.Properties | Where-Object { $_.Name -notmatch '^PS' }
    foreach ($p in $props) {
        if ($spec.Approved -and (Get-Disabled $spec.Hive $spec.Approved $p.Name)) { Write-Log "skip disabled: $($p.Name)"; continue }
        $entries.Add([pscustomobject]@{ Name = $p.Name; Command = [string]$p.Value; Key = $key; Once = $spec.Once })
    }
}
foreach ($spec in @(
    @{ Hive = 'HKCU'; Dir = [Environment]::GetFolderPath('Startup') },
    @{ Hive = 'HKLM'; Dir = [Environment]::GetFolderPath('CommonStartup') }
)) {
    if (-not (Test-Path -LiteralPath $spec.Dir)) { continue }
    foreach ($f in Get-ChildItem -LiteralPath $spec.Dir -File -ErrorAction SilentlyContinue | Where-Object { $_.Name -ne 'desktop.ini' }) {
        if (Get-Disabled $spec.Hive 'StartupFolder' $f.Name) { Write-Log "skip disabled: $($f.Name)"; continue }
        $entries.Add([pscustomobject]@{ Name = $f.Name; Command = $f.FullName; Key = $null; Once = $false })
    }
}

$running = @{}
# solo cuenta lo de esta sesión: un servicio con el mismo nombre (p. ej. Everything en la sesión 0) no es el cliente del usuario
foreach ($p in Get-Process -ErrorAction SilentlyContinue | Where-Object SessionId -eq $sessionId) { $running[$p.ProcessName.ToLowerInvariant()] = $true }
foreach ($e in $entries) {
    $cmd = Split-Command $e.Command
    $exeName = [IO.Path]::GetFileNameWithoutExtension($cmd.File)
    if ($cmd.File -notmatch '\.lnk$' -and $exeName -and $running.ContainsKey($exeName.ToLowerInvariant())) { Write-Log "already running: $($e.Name)"; continue }
    try {
        if ($e.Once) { Remove-ItemProperty -LiteralPath $e.Key -Name $e.Name -ErrorAction SilentlyContinue }
        if (-not (Test-Path -LiteralPath $cmd.File)) { Write-Log "missing target: $($e.Name): $($cmd.File)"; continue }
        $start = @{ FilePath = $cmd.File; WorkingDirectory = (Split-Path $cmd.File) }
        if ($cmd.Arguments) { $start.ArgumentList = $cmd.Arguments }
        Start-Process @start
        Write-Log "started: $($e.Name)"
    }
    catch { Write-Log "failed: $($e.Name): $($_.Exception.Message)" }
    Start-Sleep -Milliseconds 400
}
