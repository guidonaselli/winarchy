<#
.SYNOPSIS
  Runs the Pester suite inside a disposable Windows Sandbox and records every process kill and komorebic call that
  escapes a mock, so a leaking test cannot take down the real desktop.
.DESCRIPTION
  Decoys named like the desktop processes run alongside. Results land in <Out>\<timestamp>-pester: pester.log
  (test output) and leaks.log (escaped calls with their test line, and decoys that died).
#>
[CmdletBinding()]
param([string]$Out = (Join-Path $env:TEMP 'winarchy-sandbox'), [string]$Path = 'tests')
$ErrorActionPreference = 'Stop'
$wsb = Join-Path $env:LOCALAPPDATA 'Microsoft\WindowsApps\wsb.exe'
if (-not (Test-Path $wsb)) { throw 'wsb.exe not found: enable Windows Sandbox.' }
$root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$pester = (Get-Module -ListAvailable Pester | Sort-Object Version | Select-Object -Last 1).ModuleBase
$ahk = Join-Path $env:ProgramFiles 'AutoHotkey'
if (-not (Test-Path $ahk)) { $ahk = Join-Path $env:LOCALAPPDATA 'Programs\AutoHotkey' }   # instalacion por usuario: sin esto los tests de AHK se saltean
$run = Join-Path $Out "$(Get-Date -Format 'yyyyMMdd-HHmmss')-pester"
New-Item -ItemType Directory -Path $run -Force | Out-Null

@'
$ErrorActionPreference = 'Stop'
Copy-Item C:\repo C:\work\Winarchy -Recurse -Force
$global:LeakLog = 'C:\sandbox\leaks.log'
function global:Write-Leak([string]$what) {
    $from = Get-PSCallStack | Where-Object { $_.ScriptName -like '*.Tests.ps1' } | Select-Object -First 1
    Add-Content $global:LeakLog "$what <- $(Split-Path "$($from.ScriptName)" -Leaf):$($from.ScriptLineNumber)"
}
$meta = [System.Management.Automation.CommandMetadata]::new((Get-Command Stop-Process -CommandType Cmdlet))
$params = [System.Management.Automation.ProxyCommand]::GetParamBlock($meta)
$binding = [System.Management.Automation.ProxyCommand]::GetCmdletBindingAttribute($meta)
Invoke-Expression @"
function global:Stop-Process {
    $binding
    param($params)
    process { Write-Leak "Stop-Process `$(`$PSBoundParameters.GetEnumerator() | ForEach-Object { "-`$(`$_.Key) `$(@(`$_.Value) -join ',')" })" }
}
"@
function global:komorebic { Write-Leak "komorebic $args" }
Import-Module C:\pester\Pester.psd1
# señuelos con los nombres de procesos del escritorio: alguno muerto delata un test que mata de verdad
New-Item -ItemType Directory -Path C:\decoy -Force | Out-Null
$decoys = foreach ($name in 'wezterm-gui', 'komorebi', 'winarchy-shell', 'Flow.Launcher', 'yasb', 'AutoHotkey64', 'ShareX', 'PowerToys', 'winarchy-assistant', 'pwsh') {
    Copy-Item "$env:SystemRoot\System32\PING.EXE" "C:\decoy\$name.exe"
    $p = Start-Process "C:\decoy\$name.exe" -ArgumentList '-t', '127.0.0.1' -WindowStyle Hidden -PassThru
    $null = $p.Handle
    [pscustomobject]@{ Name = $name; Process = $p }
}
$config = New-PesterConfiguration
$config.Run.Path = "C:\work\Winarchy\$env:WINARCHY_TEST_PATH"
$config.Run.PassThru = $true
$config.Output.Verbosity = 'Detailed'
$result = Invoke-Pester -Configuration $config
foreach ($d in $decoys) {
    if ($d.Process.HasExited) { Add-Content $global:LeakLog "DECOY DIED $($d.Name)#$($d.Process.Id) at $($d.Process.ExitTime.ToString('HH:mm:ss.fff'))" }
    else { $d.Process.Kill() }
}
exit $result.FailedCount + $result.FailedContainersCount
'@ | Set-Content (Join-Path $run 'probe.ps1')

$folders = @{ $run = 'C:\sandbox'; $root = 'C:\repo'; $PSHOME = 'C:\pwsh'; $pester = 'C:\pester' }
if (Test-Path $ahk) { $folders[$ahk] = 'C:\Program Files\AutoHotkey' }
$mapped = -join ($folders.GetEnumerator() | ForEach-Object {
        "<MappedFolder><HostFolder>$($_.Key)</HostFolder><SandboxFolder>$($_.Value)</SandboxFolder><ReadOnly>$(if ($_.Key -eq $run) { 'false' } else { 'true' })</ReadOnly></MappedFolder>"
    })
$config = "<Configuration><vGPU>Disable</vGPU><Networking>Disable</Networking><MemoryInMB>4096</MemoryInMB><MappedFolders>$mapped</MappedFolders></Configuration>"
$id = ((& $wsb start --raw -c $config | Out-String) | ConvertFrom-Json).Id
Write-Host "Sandbox $id, results in $run"
try {
    Start-Process $wsb -ArgumentList 'connect', '--id', $id -WindowStyle Minimized   # the user session only exists while a client is connected
    foreach ($try in 1..60) {
        try { if ((((& $wsb exec --raw --id $id -r ExistingLogin -c 'cmd /c exit 0' 2>&1 | Out-String) | ConvertFrom-Json).ExitCode) -eq 0) { break } } catch { }
        Start-Sleep 5
    }
    $cmd = "cmd /c set WINARCHY_TEST_PATH=$Path&& C:\pwsh\pwsh.exe -NoProfile -ExecutionPolicy Bypass -File C:\sandbox\probe.ps1 > C:\sandbox\pester.log 2>&1"
    $result = & $wsb exec --raw --id $id -r ExistingLogin -c $cmd 2>&1 | Out-String
    try { $code = ($result | ConvertFrom-Json).ExitCode } catch { throw "wsb exec failed: $result" }
    Get-Content (Join-Path $run 'pester.log') | Where-Object { $_ -match 'Tests Passed|\[-\]' }
    Write-Host "exit $code; escaped calls: $(@(Get-Content (Join-Path $run 'leaks.log') -ErrorAction SilentlyContinue).Count) -> $run"
}
finally { & $wsb stop --id $id | Out-Null }
