<#
.SYNOPSIS
  Installs Winarchy in a clean Windows Sandbox as a regular (non-elevated) user and checks the result.
.DESCRIPTION
  Tests what is committed (HEAD), not the working tree: the sandbox clones a bare copy of the
  repo whose `release` branch points at HEAD.
    install     winget -> git + pwsh, clone, install.ps1 twice, Assert-Install
    selfupdate  install, then roll the checkout back to v1.6.0 (AHK/Flow unpinned, as 1.6.0
                left them) and run that version's `winarchy update --self`, Assert-Install
    boot     boot.ps1 refuses an elevated session; then, as the user (clone URL -> local bare
             repo), a clean install and a re-run with WINARCHY_ACTIVATE=1; Assert-Install -Mode active
  The sandbox runs with vGPU disabled (its DWM crashes on some host GPU drivers), UAC on with
  automatic approval and winget installed. Results land in <Out>\<timestamp>-<scenario>\.
  Needs the Windows Sandbox feature (Containers-DisposableClientVM) and its `wsb` CLI.
.EXAMPLE
  .\tests\sandbox\Start-WinarchySandbox.ps1 -Scenario boot
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('install', 'boot', 'selfupdate')][string]$Scenario,
    [switch]$Keep,
    [string]$Out = (Join-Path $env:TEMP 'winarchy-sandbox')
)
$ErrorActionPreference = 'Stop'
$root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$wsb = Join-Path $env:LOCALAPPDATA 'Microsoft\WindowsApps\wsb.exe'
if (-not (Test-Path $wsb)) { throw 'wsb.exe not found: enable Windows Sandbox (Containers-DisposableClientVM) and reboot.' }

$run = Join-Path $Out "$(Get-Date -Format 'yyyyMMdd-HHmmss')-$Scenario"
New-Item -ItemType Directory -Path $run -Force | Out-Null
git clone --quiet --bare $root (Join-Path $run 'winarchy.git')
git -C (Join-Path $run 'winarchy.git') branch -f release HEAD
Copy-Item (Join-Path $root 'boot.ps1') $run
Copy-Item $PSScriptRoot (Join-Path $run 'scripts') -Recurse

function Invoke-Guest {
    param([ValidateSet('ExistingLogin', 'System')][string]$As, [string]$Command)
    $result = & $wsb exec --raw --id $script:id -r $As -c $Command 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) { throw "wsb exec failed: $result" }
    ($result | ConvertFrom-Json).ExitCode
}

function Invoke-GuestScript {
    param([string]$As, [string]$Script, [string]$Arguments)
    $code = Invoke-Guest $As "powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\sandbox\scripts\$Script $Arguments"
    if ($code -ne 0) { throw "$Script exited with $code; see $run" }
}

function Wait-UserSession {
    foreach ($try in 1..60) {
        try { if ((Invoke-Guest ExistingLogin 'cmd /c exit 0') -eq 0) { return } } catch { }
        Start-Sleep 5
    }
    throw 'The sandbox user session did not come up.'
}

$config = @"
<Configuration>
  <vGPU>Disable</vGPU>
  <MemoryInMB>8192</MemoryInMB>
  <MappedFolders>
    <MappedFolder><HostFolder>$run</HostFolder><SandboxFolder>C:\sandbox</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder>
  </MappedFolders>
</Configuration>
"@ -replace '\r?\n\s*', ''
$script:id = ((& $wsb start --raw -c $config | Out-String) | ConvertFrom-Json).Id
Write-Host "Sandbox $id, results in $run"
try {
    Start-Process $wsb -ArgumentList 'connect', '--id', $id
    Wait-UserSession
    Invoke-GuestScript ExistingLogin 'Initialize-Sandbox.ps1'
    $null = Invoke-Guest System 'shutdown /r /t 0'
    Start-Sleep 20
    Wait-UserSession
    if ($Scenario -eq 'boot') {
        $code = Invoke-Guest System 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\sandbox\scripts\Invoke-SandboxScenario.ps1 -Scenario elevated'
        if ($code -ne 0) { throw "elevated check exited with $code; see $run" }
    }
    Invoke-GuestScript ExistingLogin 'Invoke-SandboxScenario.ps1' "-Scenario $Scenario"
    $mode = if ($Scenario -eq 'boot') { 'active' } else { 'coexistence' }
    $code = Invoke-Guest ExistingLogin "pwsh.exe -NoProfile -ExecutionPolicy Bypass -File C:\sandbox\scripts\Assert-Install.ps1 -Mode $mode"
    Get-Content (Join-Path $run 'assert.txt')
    if ($code -ne 0) { throw "Assert-Install failed; see $run" }
    Write-Host "PASS: $Scenario" -ForegroundColor Green
}
finally {
    if (-not $Keep) { & $wsb stop --id $id | Out-Null }
}
