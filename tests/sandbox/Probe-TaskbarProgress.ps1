<#
.SYNOPSIS
  Guest side of Run-UwpHostSandbox.ps1 -Probe: taskbar progress and overlay icons on the dock without Explorer.
.DESCRIPTION
  A second process calls ITaskbarList3 on its own window while winarchy-shell is the only shell; each state is captured
  with a dark and a light theme. Then the shell is closed, Explorer returns and the same calls reach its taskbar.
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
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindow(string cls, string title);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr GetProp(IntPtr h, string name);
    [DllImport("user32.dll", EntryPoint = "SystemParametersInfoW")] public static extern bool GetAnimation(uint action, uint p, out int on, uint flags);
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

$child = @'
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
[ComImport, Guid("ea1afb91-9e28-4b86-90e9-9e9f8a5eefaf"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface ITaskbarList3 {
    void HrInit(); void AddTab(IntPtr h); void DeleteTab(IntPtr h); void ActivateTab(IntPtr h); void SetActiveAlt(IntPtr h);
    void MarkFullscreenWindow(IntPtr h, [MarshalAs(UnmanagedType.Bool)] bool full);
    void SetProgressValue(IntPtr h, ulong done, ulong total); void SetProgressState(IntPtr h, int state);
    void RegisterTab(IntPtr tab, IntPtr mdi); void UnregisterTab(IntPtr tab); void SetTabOrder(IntPtr tab, IntPtr before); void SetTabActive(IntPtr tab, IntPtr mdi, uint reserved);
    void ThumbBarAddButtons(IntPtr h, uint n, IntPtr buttons); void ThumbBarUpdateButtons(IntPtr h, uint n, IntPtr buttons); void ThumbBarSetImageList(IntPtr h, IntPtr list);
    void SetOverlayIcon(IntPtr h, IntPtr icon, [MarshalAs(UnmanagedType.LPWStr)] string text);
}
public static class Taskbar {
    public static string Apply(IntPtr h, ulong value, int state, IntPtr icon) {
        try {
            var t = (ITaskbarList3)Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("56FDF344-FD6D-11d0-958A-006097C9A090")));
            t.HrInit(); t.SetProgressValue(h, value, 100); t.SetProgressState(h, state); t.SetOverlayIcon(h, icon, "warn");
            return "ok";
        }
        catch (Exception e) { return e.Message; }
    }
}
"@
$form = New-Object Windows.Forms.Form; $form.Text = 'progress target'; $form.Show()
while ($true) {
    [Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 100
    if (Test-Path C:\Users\Public\progress.cmd.txt) {
        $value, $state, $overlay = (Get-Content C:\Users\Public\progress.cmd.txt) -split ' '
        Remove-Item C:\Users\Public\progress.cmd.txt
        $icon = if ($overlay -eq '1') { [Drawing.SystemIcons]::Warning.Handle } else { [IntPtr]::Zero }
        [Taskbar]::Apply($form.Handle, [uint64]$value, [int]$state, $icon) | Set-Content C:\Users\Public\progress.ack.txt
    }
}
'@

function Send-Progress([string]$Command) {
    Remove-Item C:\Users\Public\progress.ack.txt -ErrorAction SilentlyContinue
    Set-Content C:\Users\Public\progress.cmd.txt $Command
    foreach ($i in 1..100) { if (Test-Path C:\Users\Public\progress.ack.txt) { break }; Start-Sleep -Milliseconds 100 }
    Start-Sleep -Milliseconds 800
    "$(Get-Content C:\Users\Public\progress.ack.txt -ErrorAction SilentlyContinue)"
}

function Start-Child {
    Set-Content C:\Users\Public\progress-child.ps1 $child
    $info = New-Object Diagnostics.ProcessStartInfo 'powershell.exe', '-NoProfile -STA -ExecutionPolicy Bypass -File C:\Users\Public\progress-child.ps1'
    $info.UseShellExecute = $false; $info.CreateNoWindow = $true
    [Diagnostics.Process]::Start($info)
}

$report = [ordered]@{}
try {
    New-Item -ItemType Directory -Path C:\uwproot -Force | Out-Null
    Copy-Item C:\probe\root\* C:\uwproot -Recurse -Force
    Get-Process explorer -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep 3
    $themePath = 'C:\uwproot\config\shell\theme.json'
    $theme = Get-Content $themePath -Raw | ConvertFrom-Json
    $theme.dock = $true
    $theme | Add-Member -NotePropertyName urgent -NotePropertyValue '#e5484d'
    # el Sandbox puede venir con las animaciones apagadas: sin esto el barrido no se mide
    $on = 0; [void][Win]::GetAnimation(0x1042, 0, [ref]$on, 0)
    $report.clientAnimationAtStart = [bool]$on
    [void][Win]::SetAnimation(0x1043, 0, [IntPtr]1, 2)
    $theme | ConvertTo-Json -Compress | Set-Content $themePath
    $shell = Start-Process C:\uwproot\winarchy-shell.exe -ArgumentList '--session-shell', '--root', 'C:\uwproot' -WindowStyle Hidden -PassThru
    Start-Sleep 5
    $report.shellAlive = -not $shell.HasExited
    $tray = [Win]::FindWindow('Shell_TrayWnd', [NullString]::Value)
    $report.taskbandProperty = [Win]::GetProp($tray, 'TaskbandHWND') -eq $tray
    $app = Start-Child
    Start-Sleep 8
    # valor, estado TBPF, insignia
    $states = [ordered]@{ normal = '42 2 0'; paused = '70 8 0'; error = '55 4 1'; indeterminate = '0 1 0'; cleared = '0 0 0' }
    $report.calls = [ordered]@{}
    foreach ($name in $states.Keys) {
        $report.calls[$name] = Send-Progress $states[$name]; Save-Screen "dock-dark-$name"
        if ($name -eq 'indeterminate') { Start-Sleep -Milliseconds 450; Save-Screen 'dock-dark-indeterminate-later' }
    }
    $theme.background = '#f2f2f2'; $theme.foreground = '#1a1a1a'; $theme.accent = '#005fb8'; $theme.muted = '#6a6a6a'
    $theme | ConvertTo-Json -Compress | Set-Content $themePath
    Start-Sleep 3
    foreach ($name in 'normal', 'error') { $report.calls["light-$name"] = Send-Progress $states[$name]; Save-Screen "dock-light-$name" }
    # costo en reposo con un progreso determinado: no queda ningún timer de barrido
    $null = Send-Progress $states.normal
    $shell.Refresh(); $cpu = $shell.TotalProcessorTime.TotalMilliseconds; Start-Sleep 20; $shell.Refresh()
    $report.idleCpuMsDeterminate = [int]($shell.TotalProcessorTime.TotalMilliseconds - $cpu)
    $null = Send-Progress $states.indeterminate
    $shell.Refresh(); $cpu = $shell.TotalProcessorTime.TotalMilliseconds; Start-Sleep 20; $shell.Refresh()
    $report.sweepCpuMs20s = [int]($shell.TotalProcessorTime.TotalMilliseconds - $cpu)
    $report.shellAliveAtEnd = -not $shell.HasExited
    $app.Kill()
    $shell.Kill(); [void]$shell.WaitForExit(5000)
    Start-Process explorer.exe
    Start-Sleep 10
    $explorerTray = [Win]::FindWindow('Shell_TrayWnd', [NullString]::Value)
    $report.explorerTaskband = [Win]::GetProp($explorerTray, 'TaskbandHWND') -ne [IntPtr]::Zero
    $app = Start-Child
    Start-Sleep 8
    $report.calls.explorer = Send-Progress $states.normal
    Save-Screen 'explorer-normal'
    $app.Kill()
    $report.status = 'done'
}
catch { $report.status = 'error'; $report.error = "$($_.Exception.Message) ($($_.InvocationInfo.ScriptLineNumber))" }
$report | ConvertTo-Json -Depth 4 | Set-Content C:\sandbox\result.json
