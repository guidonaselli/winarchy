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

function Get-ExeName([string]$command) {
    $expanded = [Environment]::ExpandEnvironmentVariables($command.Trim())
    $path = if ($expanded -match '^"([^"]+)"') { $matches[1] } elseif ($expanded -match '^(.+?\.(?:exe|bat|cmd|lnk))(\s|$)') { $matches[1] } else { $expanded }
    [IO.Path]::GetFileNameWithoutExtension($path)
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
foreach ($p in Get-Process -ErrorAction SilentlyContinue) { $running[$p.ProcessName.ToLowerInvariant()] = $true }
foreach ($e in $entries) {
    $exeName = Get-ExeName $e.Command
    if ($e.Command -notmatch '\.lnk"?$' -and $exeName -and $running.ContainsKey($exeName.ToLowerInvariant())) { Write-Log "already running: $($e.Name)"; continue }
    try {
        if ($e.Once) { Remove-ItemProperty -LiteralPath $e.Key -Name $e.Name -ErrorAction SilentlyContinue }
        $target = [Environment]::ExpandEnvironmentVariables($e.Command)
        if (Test-Path -LiteralPath $target) { Start-Process -FilePath $target -WorkingDirectory (Split-Path $target) }
        else { Start-Process -FilePath "$env:SystemRoot\System32\cmd.exe" -ArgumentList '/c', "start `"`" $target" -WindowStyle Hidden }
        Write-Log "started: $($e.Name)"
    }
    catch { Write-Log "failed: $($e.Name): $($_.Exception.Message)" }
    Start-Sleep -Milliseconds 400
}
