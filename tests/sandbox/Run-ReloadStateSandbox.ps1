<#
.SYNOPSIS
  What komorebi's live view state survives a rewrite of its watched config file.
.DESCRIPTION
  Starts a disposable Windows Sandbox with the host's komorebi mapped read-only and runs Probe-ReloadState.ps1.
  Nothing on the host is touched, but the sandbox client window opens minimized. Results land in
  <Out>\<timestamp>-reload-state\reload-state.json.
#>
[CmdletBinding()]
param([string]$Out = (Join-Path $env:TEMP 'winarchy-sandbox'), [string]$Komorebi = "$env:ProgramFiles\komorebi\bin")
$ErrorActionPreference = 'Stop'
$wsb = Join-Path $env:LOCALAPPDATA 'Microsoft\WindowsApps\wsb.exe'
if (-not (Test-Path $wsb)) { throw 'wsb.exe not found: enable Windows Sandbox.' }
if (-not (Test-Path $Komorebi)) { throw "missing: $Komorebi" }
$run = Join-Path $Out "$(Get-Date -Format 'yyyyMMdd-HHmmss')-reload-state"
New-Item -ItemType Directory -Path $run -Force | Out-Null
Copy-Item (Join-Path $PSScriptRoot 'Probe-ReloadState.ps1') $run
$config = @"
<Configuration><vGPU>Disable</vGPU><Networking>Disable</Networking><MemoryInMB>4096</MemoryInMB><MappedFolders>
<MappedFolder><HostFolder>$run</HostFolder><SandboxFolder>C:\sandbox</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder>
<MappedFolder><HostFolder>$Komorebi</HostFolder><SandboxFolder>C:\komorebi-bin</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder>
</MappedFolders></Configuration>
"@ -replace '\r?\n', ''
$id = ((& $wsb start --raw -c $config | Out-String) | ConvertFrom-Json).Id
Write-Host "Sandbox $id, results in $run"
try {
    Start-Process $wsb -ArgumentList 'connect', '--id', $id -WindowStyle Minimized   # the user session only exists while a client is connected
    foreach ($try in 1..60) {
        try { if ((((& $wsb exec --raw --id $id -r ExistingLogin -c 'cmd /c exit 0' 2>&1 | Out-String) | ConvertFrom-Json).ExitCode) -eq 0) { break } } catch { }
        Start-Sleep 5
    }
    $result = & $wsb exec --raw --id $id -r ExistingLogin -c 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\sandbox\Probe-ReloadState.ps1' 2>&1 | Out-String
    try { $code = ($result | ConvertFrom-Json).ExitCode } catch { throw "wsb exec failed: $result" }
    Get-Content (Join-Path $run 'reload-state.log') -ErrorAction SilentlyContinue
    if ($code -ne 0) { throw "reload state probe exited $code; see $run" }
    Write-Host "PASS: $(Join-Path $run 'reload-state.json')" -ForegroundColor Green
}
finally { & $wsb stop --id $id | Out-Null }
