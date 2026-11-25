# Runs inside Windows Sandbox (Windows PowerShell 5.1) as the default user. Drives the real komorebi and the
# real DragMove/DragIsFree code sliced out of config\ahk\winarchy.ahk: Win+drag in a floating workspace must
# leave the window where it was dropped; in a tiled workspace it must still snap back.
$ErrorActionPreference = 'Stop'
$out = 'C:\sandbox'
$log = Join-Path $out 'floating-drag.log'
function Log($m) { "{0:HH:mm:ss} {1}" -f (Get-Date), $m | Tee-Object -FilePath $log -Append }
trap { Log "ERROR: $_ at $($_.InvocationInfo.PositionMessage)"; exit 1 }
$komorebiBin = 'C:\komorebi-bin'
$ahk = 'C:\ahk\AutoHotkey64.exe'
$env:KOMOREBI_CONFIG_HOME = 'C:\komorebi-cfg'
New-Item -ItemType Directory -Path $env:KOMOREBI_CONFIG_HOME -Force | Out-Null
$config = @{ monitors = @(@{ workspaces = @(@{ name = '1'; layout = 'BSP' }, @{ name = '2'; layout = 'BSP' }) }) } | ConvertTo-Json -Depth 6
[IO.File]::WriteAllText("$env:KOMOREBI_CONFIG_HOME\komorebi.json", $config, [Text.UTF8Encoding]::new($false))
$komorebic = "$komorebiBin\komorebic.exe"

# the real functions, sliced from winarchy.ahk
$source = Get-Content -LiteralPath 'C:\repo\config\ahk\winarchy.ahk' -Raw
function Get-AhkBlock([string]$name) {
    $m = [regex]::Match($source, "(?ms)^$name\([^\r\n]*\{\r?\n.*?^\}\r?\n")
    if (-not $m.Success) { throw "function $name not found in winarchy.ahk" }
    $m.Value
}
$blocks = 'KomorebiState', 'FocusedWorkspace', 'Komorebic', 'DragTarget', 'DragIsFree', 'DragMove' | ForEach-Object { Get-AhkBlock $_ }
# control: the behaviour before the fix, where every drop was treated as tiled
$legacyBlocks = $blocks | ForEach-Object { if ($_ -like 'DragIsFree*') { "DragIsFree(hwnd) {`r`n    return false`r`n}`r`n" } else { $_ } }
function New-Harness([string[]]$functionBlocks) {
$functions = $functionBlocks -join "`r`n"
@"
#Requires AutoHotkey v2.0
#SingleInstance Force
KomorebicExe := 'C:\komorebi-bin\komorebic.exe'
$functions
hwnd := Integer(A_Args[1]), dx := Integer(A_Args[2]), dy := Integer(A_Args[3])
WinGetPos(&wx, &wy, , , hwnd)
x0 := wx + 40, y0 := wy + 10
MouseMove(x0, y0, 0)
n := 0
held() {
    global n, x0, y0, dx, dy
    n += 1
    MouseMove(x0 + dx * Min(n, 20) // 20, y0 + dy * Min(n, 20) // 20, 0)
    return n < 30
}
DragMove(hwnd, x0, y0, held)
Sleep(1500)
WinGetPos(&fx, &fy, , , hwnd)
FileAppend(fx ',' fy, 'C:\sandbox\drag-result.txt')
"@
}

Add-Type -Namespace Probe -Name Win -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool SetWindowPos(System.IntPtr h, System.IntPtr after, int x, int y, int w, int cx, uint flags);
[DllImport("user32.dll")] public static extern bool GetWindowRect(System.IntPtr h, out RECT r);
public struct RECT { public int L, T, R, B; }
'@
function Get-Pos($p) { $r = New-Object Probe.Win+RECT; [void][Probe.Win]::GetWindowRect($p.MainWindowHandle, [ref]$r); @($r.L, $r.T) }
# a plain WinForms window per process (the sandbox has no Notepad)
function New-TestWindow([string]$title) {
    $p = Start-Process powershell.exe -WindowStyle Normal -PassThru -ArgumentList '-NoProfile', '-Command', "Add-Type -AssemblyName System.Windows.Forms; `$f = New-Object Windows.Forms.Form; `$f.Text = '$title'; [Windows.Forms.Application]::Run(`$f)"
    Wait-Window $p
    $p
}
function Stop-TestWindows { Get-Process powershell -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowTitle -like 'drag-*' } | Stop-Process -Force }
function Wait-Window($p) { foreach ($i in 1..50) { $p.Refresh(); if ($p.MainWindowHandle -ne 0) { return }; Start-Sleep -Milliseconds 200 }; throw 'window did not appear' }

Log 'starting komorebi'
$komorebi = Start-Process "$komorebiBin\komorebi.exe" -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $out 'komorebi.out.txt') -RedirectStandardError (Join-Path $out 'komorebi.err.txt')
foreach ($i in 1..40) { if ((& $komorebic state 2>$null) -match '"monitors"') { break }; Start-Sleep -Milliseconds 500 }
if (-not ((& $komorebic state 2>$null) -match '"monitors"')) { Log "komorebi alive=$(-not $komorebi.HasExited) state=$(& $komorebic state 2>&1 | Out-String)"; throw 'komorebi never answered' }
Log 'komorebi up'

$results = [ordered]@{}
function Invoke-Case([string]$name, [bool]$floating, [int[]]$delta, [switch]$Legacy) {
    [IO.File]::WriteAllText('C:\ahk-harness.ahk', (New-Harness $(if ($Legacy) { $legacyBlocks } else { $blocks })), [Text.UTF8Encoding]::new($false))
    Stop-TestWindows
    & $komorebic workspace-tiling 0 0 $(if ($floating) { 'disable' } else { 'enable' }) | Out-Null
    $a = New-TestWindow 'drag-a'
    $b = New-TestWindow 'drag-b'
    Start-Sleep -Seconds 2
    if ($floating) {
        # two windows at known places, as the user's two WezTerm
        [void][Probe.Win]::SetWindowPos($a.MainWindowHandle, [IntPtr]::Zero, 100, 100, 600, 400, 0x4)
        [void][Probe.Win]::SetWindowPos($b.MainWindowHandle, [IntPtr]::Zero, 800, 200, 600, 400, 0x4)
        Start-Sleep -Milliseconds 800
    }
    $before = Get-Pos $a
    Remove-Item C:\sandbox\drag-result.txt -ErrorAction SilentlyContinue
    & $ahk C:\ahk-harness.ahk $a.MainWindowHandle $delta[0] $delta[1]
    foreach ($i in 1..40) { if (Test-Path C:\sandbox\drag-result.txt) { break }; Start-Sleep -Milliseconds 250 }
    $after = (Get-Content C:\sandbox\drag-result.txt -ErrorAction SilentlyContinue) -split ','
    $results[$name] = [ordered]@{ before = $before -join ','; after = $after -join ','; delta = $delta -join ',' }
    Log "$name before=$($before -join ',') after=$($after -join ',')"
    Stop-TestWindows
}
Invoke-Case 'floating-workspace' $true @(250, 120)
Invoke-Case 'tiled-workspace' $false @(30, 20)
Invoke-Case 'floating-workspace-before-the-fix' $true @(250, 120) -Legacy
$results | ConvertTo-Json | Set-Content (Join-Path $out 'floating-drag-result.json')
Stop-Process -Id $komorebi.Id -Force -ErrorAction SilentlyContinue
$f = $results['floating-workspace']
$control = $results['floating-workspace-before-the-fix']
if ($control.after -ne $control.before) { Log 'FAIL control: the pre-fix behaviour did not snap back, so this probe does not discriminate'; exit 1 }
$moved = ($f.after -split ',')[0] -eq ([int](($f.before -split ',')[0]) + 250) -and ($f.after -split ',')[1] -eq ([int](($f.before -split ',')[1]) + 120)
if ($moved) { Log 'PASS floating window stayed where it was dropped'; exit 0 } else { Log 'FAIL floating window did not stay'; exit 1 }
