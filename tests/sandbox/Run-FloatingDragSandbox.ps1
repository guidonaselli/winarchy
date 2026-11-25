<#
.SYNOPSIS
  Win+drag in a floating workspace, against the real komorebi and the real DragMove code of the working tree.
.DESCRIPTION
  Starts a disposable Windows Sandbox that maps this working tree's config folder, the host's komorebi and
  AutoHotkey v2 (all read-only) and runs Probe-FloatingDrag.ps1 as the sandbox user. Nothing on the host is
  touched. Results land in <Out>\<timestamp>-floating-drag\.
#>
[CmdletBinding()]
param([string]$Out = (Join-Path $env:TEMP 'winarchy-sandbox'), [string]$Komorebi = "$env:ProgramFiles\komorebi\bin", [string]$Ahk = "$env:LOCALAPPDATA\Programs\AutoHotkey\v2")
$ErrorActionPreference = 'Stop'
$root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$wsb = Join-Path $env:LOCALAPPDATA 'Microsoft\WindowsApps\wsb.exe'
if (-not (Test-Path $wsb)) { throw 'wsb.exe not found: enable Windows Sandbox.' }
foreach ($p in $Komorebi, $Ahk) { if (-not (Test-Path $p)) { throw "missing: $p" } }
$run = Join-Path $Out "$(Get-Date -Format 'yyyyMMdd-HHmmss')-floating-drag"
New-Item -ItemType Directory -Path $run -Force | Out-Null
Copy-Item (Join-Path $PSScriptRoot 'Probe-FloatingDrag.ps1') $run
$config = @"
<Configuration><vGPU>Disable</vGPU><Networking>Disable</Networking><MemoryInMB>4096</MemoryInMB><MappedFolders>
<MappedFolder><HostFolder>$run</HostFolder><SandboxFolder>C:\sandbox</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder>
<MappedFolder><HostFolder>$(Join-Path $root 'config')</HostFolder><SandboxFolder>C:\repo\config</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder>
<MappedFolder><HostFolder>$Komorebi</HostFolder><SandboxFolder>C:\komorebi-bin</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder>
<MappedFolder><HostFolder>$Ahk</HostFolder><SandboxFolder>C:\ahk</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder>
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
    $result = & $wsb exec --raw --id $id -r ExistingLogin -c 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\sandbox\Probe-FloatingDrag.ps1' 2>&1 | Out-String
    try { $code = ($result | ConvertFrom-Json).ExitCode } catch { throw "wsb exec failed: $result" }
    Get-Content (Join-Path $run 'floating-drag.log') -ErrorAction SilentlyContinue
    if ($code -ne 0) { throw "floating drag probe exited $code; see $run" }
    Write-Host 'PASS: floating drag' -ForegroundColor Green
}
finally { & $wsb stop --id $id | Out-Null }
