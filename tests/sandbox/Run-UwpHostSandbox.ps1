<#
.SYNOPSIS
  Runs a probe (Probe-UwpPresentation.ps1 by default) in a disposable Windows Sandbox with winarchy-shell as the only shell.
.DESCRIPTION
  Evidence lands in state\verify\uwphost\<timestamp>. Nothing on the real desktop is touched.
  -HostDir holds a candidate uwu-reference.exe that only ever runs inside the guest; networking is enabled with it
  because it downloads public symbols on first use.
#>
[CmdletBinding()]
param([string]$HostDir, [switch]$NoShell, [switch]$HostFirst, [switch]$Extras, [switch]$Offline, [switch]$Explorer, [string[]]$HostEnv = @(), [string]$Probe = 'Probe-UwpPresentation.ps1', [string]$Exe)
$ErrorActionPreference = 'Stop'
$repo = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$wsb = Join-Path $env:LOCALAPPDATA 'Microsoft\WindowsApps\wsb.exe'
# -Exe: la shell del escritorio puede estar corriendo desde target\release, que no se puede reconstruir encima
if (-not $Exe) { $Exe = Join-Path $repo 'shell\target\release\winarchy-shell.exe' }
if (-not (Test-Path $wsb) -or -not (Test-Path $Exe)) { throw 'Windows Sandbox or release shell binary is missing.' }
$run = Join-Path $repo "state\verify\uwphost\$(Get-Date -Format 'yyyyMMdd-HHmmss')"
$in = Join-Path $run 'input'; $out = Join-Path $run 'output'; $payload = Join-Path $in 'root'
New-Item -ItemType Directory -Path (Join-Path $payload 'config\shell'), $out -Force | Out-Null
Copy-Item $Exe (Join-Path $payload 'winarchy-shell.exe')
Copy-Item (Join-Path $PSScriptRoot $Probe) $in
Set-Content (Join-Path $in 'guest.marker') 'sandbox'
if ($HostDir) {
    New-Item -ItemType Directory -Path (Join-Path $in 'host') | Out-Null
    Copy-Item (Join-Path $HostDir 'uwu-reference.exe') (Join-Path $in 'host')
}
if ($Extras) {
    # copia de solo lectura del AHK instalado: dentro del Sandbox mide qué hotkeys le quedan con el host vivo
    $ahk = Join-Path $env:LOCALAPPDATA 'Programs\AutoHotkey\v2\AutoHotkey64.exe'
    if (-not (Test-Path $ahk)) { $ahk = Join-Path $env:ProgramFiles 'AutoHotkey\v2\AutoHotkey64.exe' }
    if (Test-Path $ahk) { New-Item -ItemType Directory -Path (Join-Path $in 'ahk') | Out-Null; Copy-Item $ahk (Join-Path $in 'ahk') }
}
@{ shell = -not $NoShell; host = [bool]$HostDir; hostFirst = [bool]$HostFirst; extras = [bool]$Extras; explorer = [bool]$Explorer; env = @($HostEnv) } | ConvertTo-Json | Set-Content (Join-Path $in 'scenario.json')
@{ background = '#202020'; opacity = 1.0; foreground = '#ffffff'; accent = '#4da3ff'; muted = '#808080'; font_family = 'Segoe UI'; font_size = 13; position = 'top'; dock = $false } |
    ConvertTo-Json -Compress | Set-Content (Join-Path $payload 'config\shell\theme.json')

$folders = "<MappedFolder><HostFolder>$([Security.SecurityElement]::Escape($in))</HostFolder><SandboxFolder>C:\probe</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder>" +
    "<MappedFolder><HostFolder>$([Security.SecurityElement]::Escape($out))</HostFolder><SandboxFolder>C:\sandbox</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder>"
$config = "<Configuration><vGPU>Disable</vGPU><Networking>$(if ($HostDir -and -not $Offline) { 'Enable' } else { 'Disable' })</Networking><MemoryInMB>4096</MemoryInMB><MappedFolders>$folders</MappedFolders></Configuration>"
$id = ((& $wsb start --raw -c $config | Out-String) | ConvertFrom-Json).Id
if (-not $id) { throw 'wsb did not return a Sandbox id.' }
function Invoke-Guest([string]$As, [string]$Mode) {
    $cmd = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\probe\$Probe -Mode $Mode"
    $reply = & $wsb exec --raw --id $id -r $As -c $cmd 2>&1 | Out-String
    try { $code = ($reply | ConvertFrom-Json).ExitCode } catch { throw "wsb exec failed: $reply" }
    if ($code -ne 0) { throw "Guest $Mode exited $code; evidence: $out" }
}
try {
    Start-Process $wsb -ArgumentList 'connect', '--id', $id -WindowStyle Minimized   # la sesion de usuario solo existe con un cliente conectado
    foreach ($try in 1..60) {
        try { if ((((& $wsb exec --raw --id $id -r ExistingLogin -c 'cmd /c exit 0' 2>&1 | Out-String) | ConvertFrom-Json).ExitCode) -eq 0) { break } } catch { }
        Start-Sleep 5
    }
    Invoke-Guest System Disable
    try { Invoke-Guest ExistingLogin Run }
    finally { Invoke-Guest System Restore }
    Get-Content (Join-Path $out 'result.json') -Raw | ConvertFrom-Json | Select-Object * -ExcludeProperty packages, startApps | ConvertTo-Json -Depth 8
    "autorestart $(Get-Content (Join-Path $out 'autorestart-original.txt')) -> $(Get-Content (Join-Path $out 'autorestart-restored.txt'))"
    Write-Host "evidence: $out"
}
finally { & $wsb stop --id $id | Out-Null }
