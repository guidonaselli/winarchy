<#
.SYNOPSIS
  Guest side of Run-UwpHostSandbox.ps1 -Probe: shell CPU at rest and per interaction, the notice for an app that
  cannot open without Explorer, and text editing in the Start search box.
.DESCRIPTION
  The Sandbox has no GPU: the numbers compare interactions against rest, they are not frame times.
#>
[CmdletBinding()]
param([Parameter(Mandatory)][ValidateSet('Disable', 'Run', 'Restore')][string]$Mode)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path C:\probe\guest.marker) -or -not (Test-Path C:\Users\WDAGUtilityAccount)) { throw 'Only runs inside the disposable Windows Sandbox.' }
$winlogon = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon'
if ($Mode -eq 'Disable') {
    "$((Get-ItemProperty $winlogon).AutoRestartShell)" | Set-Content C:\sandbox\autorestart-original.txt
    Set-ItemProperty $winlogon AutoRestartShell 0 -Type DWord
    return
}
if ($Mode -eq 'Restore') {
    Set-ItemProperty $winlogon AutoRestartShell ([int](Get-Content C:\sandbox\autorestart-original.txt)) -Type DWord
    "$((Get-ItemProperty $winlogon).AutoRestartShell)" | Set-Content C:\sandbox\autorestart-restored.txt
    return
}
if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Run needs the Sandbox interactive user.' }
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class Win {
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll", EntryPoint = "SystemParametersInfoW")] public static extern bool SetAnimation(uint action, uint p, IntPtr on, uint flags);
}
'@
[void][Win]::SetProcessDPIAware()

function Save-Screen([string]$Name) {
    $b = [Windows.Forms.SystemInformation]::VirtualScreen
    $bmp = New-Object Drawing.Bitmap $b.Width, $b.Height
    $g = [Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($b.Left, $b.Top, 0, 0, $bmp.Size); $g.Dispose()
    $bmp.Save("C:\sandbox\$Name.png", [Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
}

function Send-Shell([string]$Command, [string]$Params = '') {
    $pipe = New-Object IO.Pipes.NamedPipeClientStream '.', "winarchy-shell-$((Get-Process -Id $PID).SessionId)", 'InOut'
    try {
        $pipe.Connect(2000)
        $extra = if ($Params) { ",`"params`":$Params" } else { '' }
        $bytes = [Text.Encoding]::UTF8.GetBytes("{`"v`":1,`"id`":1,`"cmd`":`"$Command`"$extra}`n")
        $pipe.Write($bytes, 0, $bytes.Length); $pipe.Flush()
        (New-Object IO.StreamReader $pipe).ReadLine()
    }
    finally { $pipe.Dispose() }
}

function Measure-Cpu([scriptblock]$Action, [int]$Seconds) {
    $shell.Refresh(); $before = $shell.TotalProcessorTime.TotalMilliseconds
    $watch = [Diagnostics.Stopwatch]::StartNew()
    & $Action
    $left = $Seconds * 1000 - $watch.ElapsedMilliseconds
    if ($left -gt 0) { Start-Sleep -Milliseconds $left }
    $shell.Refresh(); [int]($shell.TotalProcessorTime.TotalMilliseconds - $before)
}

$report = [ordered]@{}
try {
    New-Item -ItemType Directory -Path C:\uwproot -Force | Out-Null
    Copy-Item C:\probe\root\* C:\uwproot -Recurse -Force
    New-Item -ItemType Directory -Path C:\uwproot\state -Force | Out-Null
    Get-Process explorer -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep 3
    $themePath = 'C:\uwproot\config\shell\theme.json'
    $theme = Get-Content $themePath -Raw | ConvertFrom-Json
    $theme.dock = $true
    $theme | Add-Member -NotePropertyName urgent -NotePropertyValue '#e5484d'
    $theme | ConvertTo-Json -Compress | Set-Content $themePath
    [void][Win]::SetAnimation(0x1043, 0, [IntPtr]1, 2)
    $shell = Start-Process C:\uwproot\winarchy-shell.exe -ArgumentList '--session-shell', '--root', 'C:\uwproot' -WindowStyle Hidden -PassThru
    Start-Sleep 8
    $forms = foreach ($i in 1..6) {
        $f = New-Object Windows.Forms.Form; $f.Text = "window $i"; $f.StartPosition = 'Manual'; $f.Location = New-Object Drawing.Point (60 * $i), (50 * $i); $f.Show(); $f
    }
    $pump = { param($ms) $w = [Diagnostics.Stopwatch]::StartNew(); while ($w.ElapsedMilliseconds -lt $ms) { [Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 20 } }
    & $pump 5000
    $cpu = [ordered]@{}
    $cpu.rest20s = Measure-Cpu { & $pump 20000 } 20
    $ids = ($forms | ForEach-Object { $_.Handle.ToInt64() }) -join ','
    $cpu.switcher10steps5s = Measure-Cpu {
        foreach ($step in 0..9) { $report.switcherReply = Send-Shell 'switcher.show' "{`"windows`":[$ids],`"index`":$($step % 6)}"; & $pump 200; if ($step -eq 3) { Save-Screen 'switcher' } }
        $null = Send-Shell 'switcher.hide'
    } 5
    $cpu.start3cycles6s = Measure-Cpu {
        foreach ($n in 1..3) { $null = Send-Shell 'start.toggle'; & $pump 1000; if ($n -eq 1) { Save-Screen 'start' }; $null = Send-Shell 'start.toggle'; & $pump 800 }
    } 6
    $cpu.islandProgress5s = Measure-Cpu {
        foreach ($p in 1..10) { $report.islandReply = Send-Shell 'island.present' "{`"title`":`"volume`",`"progress`":$($p / 10)}"; & $pump 200; if ($p -eq 5) { Save-Screen 'island' } }
        $null = Send-Shell 'island.dismiss'
    } 5
    $report.cpuMs = $cpu
    foreach ($f in $forms) { $f.Close() }
    & $pump 3000
    # Configuración es UWP: desde Inicio no puede abrir y la shell tiene que decirlo
    $null = Send-Shell 'start.toggle'; & $pump 1500
    [Windows.Forms.SendKeys]::SendWait('settings'); & $pump 1500
    Save-Screen 'uwp-search'
    [Windows.Forms.SendKeys]::SendWait('{ENTER}')
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $log = 'C:\uwproot\state\shell-boot.log'
    while ($watch.ElapsedMilliseconds -lt 30000 -and -not (Select-String -Path $log -Pattern 'launch failed' -Quiet -ErrorAction SilentlyContinue)) { & $pump 100 }
    $report.noticeAfterMs = [int]$watch.ElapsedMilliseconds
    & $pump 600
    Save-Screen 'uwp-notice'
    $report.launchLog = @(Select-String -Path $log -Pattern 'launch failed' -ErrorAction SilentlyContinue | ForEach-Object Line)
    & $pump 3000
    # cada paso copia el campo entero para leer qué quedó escrito
    $keys = { param($k) [Windows.Forms.SendKeys]::SendWait($k); & $pump 300 }
    $field = { Set-Clipboard -Value '<empty>'; & $keys '^a'; & $keys '^c'; "$(Get-Clipboard -Raw)" }
    $null = Send-Shell 'start.toggle'; & $pump 1500
    $edit = [ordered]@{}
    # el campo no tiene cursor movible: las flechas recorren los resultados y se escribe al final
    & $keys 'alpha'; $edit.selectAllCopy = & $field
    & $keys '{END}'; & $keys '^v'; $edit.pasteAtEnd = & $field
    & $keys '^x'; $edit.cut = "$(Get-Clipboard -Raw)"; $edit.afterCut = & $field
    & $keys '^z'; $edit.undo = & $field
    & $keys '{HOME}'; & $keys 'x'; $edit.typeAfterNavigation = & $field
    & $keys '{LEFT}'; & $keys '{BACKSPACE}'; $edit.backspaceAfterNavigation = & $field
    & $keys '^a'; & $keys '{BACKSPACE}'; $edit.deleteSelection = & $field
    Save-Screen 'start-edit'
    $report.edit = $edit
    $report.shellAliveAtEnd = -not $shell.HasExited
    $shell.Kill()
    $report.status = 'done'
}
catch { $report.status = 'error'; $report.error = "$($_.Exception.Message) ($($_.InvocationInfo.ScriptLineNumber))" }
$report | ConvertTo-Json -Depth 4 | Set-Content C:\sandbox\result.json
