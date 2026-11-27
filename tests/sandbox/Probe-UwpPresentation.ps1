<#
.SYNOPSIS
  Guest side of Run-UwpHostSandbox.ps1: decides whether a UWP app really shows a window (a running process proves nothing).
.DESCRIPTION
  Pass means a visible, uncloaked ApplicationFrameWindow with a visible CoreWindow child and non-uniform pixels.
  -Mode Disable/Restore run as System and toggle AutoRestartShell; -Mode Run runs in the interactive session.
#>
[CmdletBinding()]
param([Parameter(Mandatory)][ValidateSet('Disable', 'Run', 'Restore')][string]$Mode)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path C:\probe\guest.marker) -or -not (Test-Path C:\Users\WDAGUtilityAccount)) { throw 'Only runs inside the disposable Windows Sandbox.' }
$winlogon = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon'
function Get-AutoRestart { (Get-ItemProperty $winlogon).AutoRestartShell }

if ($Mode -eq 'Disable') {
    "$(Get-AutoRestart)" | Set-Content C:\sandbox\autorestart-original.txt
    Set-ItemProperty $winlogon AutoRestartShell 0 -Type DWord
    return
}
if ($Mode -eq 'Restore') {
    Set-ItemProperty $winlogon AutoRestartShell ([int](Get-Content C:\sandbox\autorestart-original.txt)) -Type DWord
    "$(Get-AutoRestart)" | Set-Content C:\sandbox\autorestart-restored.txt
    return
}

if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Run needs the Sandbox interactive user.' }
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
[ComImport, Guid("2e941141-7f97-4756-ba1d-9decde894a3d"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IApplicationActivationManager {
    [PreserveSig] int ActivateApplication([MarshalAs(UnmanagedType.LPWStr)] string id, [MarshalAs(UnmanagedType.LPWStr)] string args, int options, out uint pid);
}
[StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
public static class Uwp {
    delegate bool EnumProc(IntPtr hwnd, IntPtr l);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p, IntPtr l);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, EnumProc p, IntPtr l);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern IntPtr GetShellWindow();
    [DllImport("user32.dll", SetLastError = true)] public static extern bool SetShellWindow(IntPtr h);
    public static string TrySetShellWindow(IntPtr h) { var ok = SetShellWindow(h); return ok ? "registered" : "error " + Marshal.GetLastWin32Error(); }
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("dwmapi.dll")] static extern int DwmGetWindowAttribute(IntPtr h, int a, out int v, int size);
    [DllImport("dwmapi.dll")] public static extern int DwmSetWindowAttribute(IntPtr h, int a, ref int v, int size);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int w, int ht, uint flags);
[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int command);
[DllImport("user32.dll")] public static extern bool SystemParametersInfo(uint action, uint param, IntPtr value, uint flags);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern IntPtr SendMessageTimeout(IntPtr h, uint msg, IntPtr w, IntPtr l, uint flags, uint ms, out IntPtr result);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr h);
    public static bool Responds(IntPtr h) { IntPtr r; return SendMessageTimeout(h, 0, IntPtr.Zero, IntPtr.Zero, 2, 1000, out r) != IntPtr.Zero; }
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindow(string cls, string title);
    [DllImport("ntdll.dll")] public static extern int NtSuspendProcess(IntPtr process);
    [DllImport("ntdll.dll")] public static extern int NtResumeProcess(IntPtr process);
    [DllImport("kernel32.dll")] public static extern IntPtr OpenThread(uint access, bool inherit, uint id);
    [DllImport("kernel32.dll")] public static extern int SuspendThread(IntPtr thread);
    [DllImport("user32.dll")] public static extern bool RegisterHotKey(IntPtr h, int id, uint mods, uint vk);
    [DllImport("user32.dll")] public static extern bool UnregisterHotKey(IntPtr h, int id);
    [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr h, uint msg, IntPtr w, string l, uint flags, uint ms, out IntPtr result);
    public static void Chord(byte[] keys) { foreach (var k in keys) keybd_event(k, 0, 0, UIntPtr.Zero); for (int i = keys.Length - 1; i >= 0; i--) keybd_event(keys[i], 0, 2, UIntPtr.Zero); }
    public static string ClassOf(IntPtr h) { var s = new StringBuilder(256); GetClassName(h, s, 256); return s.ToString(); }
    public static int Cloaked(IntPtr h) { int v = 0; return DwmGetWindowAttribute(h, 14, out v, 4) == 0 ? v : -1; }
    public static IntPtr[] Top() { var l = new List<IntPtr>(); EnumWindows((h, p) => { l.Add(h); return true; }, IntPtr.Zero); return l.ToArray(); }
    public static IntPtr[] Children(IntPtr parent) { var l = new List<IntPtr>(); EnumChildWindows(parent, (h, p) => { l.Add(h); return true; }, IntPtr.Zero); return l.ToArray(); }
    public static int Activate(string id, out uint pid) {
        var m = (IApplicationActivationManager)Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("45BA127D-10A8-46EA-8AB7-56EA9078943C")));
        return m.ActivateApplication(id, null, 0, out pid);
    }
}
'@
[void][Uwp]::SetProcessDPIAware()

function Get-Shown([IntPtr]$Hwnd) { [Uwp]::IsWindowVisible($Hwnd) -and [Uwp]::Cloaked($Hwnd) -eq 0 }

function Get-ColorCount([IntPtr]$Hwnd, [string]$Png) {
    $r = New-Object RECT; [void][Uwp]::GetWindowRect($Hwnd, [ref]$r)
    $w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top
    if ($w -le 0 -or $h -le 0) { return 0 }
    $bmp = New-Object Drawing.Bitmap $w, $h
    $g = [Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($r.Left, $r.Top, 0, 0, $bmp.Size); $g.Dispose()
    $bmp.Save($Png, [Drawing.Imaging.ImageFormat]::Png)
    $colors = @{}
    foreach ($x in 0..39) { foreach ($y in 0..39) { $colors[$bmp.GetPixel([int](($w - 1) * $x / 39), [int](($h - 1) * $y / 39)).ToArgb()] = $true } }
    $bmp.Dispose()
    $colors.Count
}

function Get-Frames([uint32]$AppPid) {
    foreach ($frame in [Uwp]::Top() | Where-Object { [Uwp]::ClassOf($_) -eq 'ApplicationFrameWindow' }) {
        foreach ($core in [Uwp]::Children($frame) | Where-Object { [Uwp]::ClassOf($_) -eq 'Windows.UI.Core.CoreWindow' }) {
            [uint32]$owner = 0; [void][Uwp]::GetWindowThreadProcessId($core, [ref]$owner)
            if ($owner -eq $AppPid) { [pscustomobject]@{ Frame = $frame; Core = $core } }
        }
    }
}

function Test-Presentation([string]$Aumid, [string]$Name, [int]$Seconds = 10) {
    [uint32]$appPid = 0; $watch = [Diagnostics.Stopwatch]::StartNew()
    $hr = [Uwp]::Activate($Aumid, [ref]$appPid)
    $result = [ordered]@{ app = $Aumid; hr = '0x{0:X8}' -f $hr; pid = $appPid; status = 'fail'; reason = 'activation failed'; ms = $null; colors = 0 }
    if ($hr -ne 0) { return $result }
    $result.reason = 'no hosted frame'
    while ($watch.Elapsed.TotalSeconds -lt $Seconds) {
        $hit = Get-Frames $appPid | Select-Object -First 1
        if ($hit) {
            $result.reason = 'frame or CoreWindow hidden or cloaked'
            if ((Get-Shown $hit.Frame) -and (Get-Shown $hit.Core)) {
                $result.ms = [int]$watch.Elapsed.TotalMilliseconds
                Start-Sleep 1
                $result.colors = Get-ColorCount $hit.Frame "C:\sandbox\$Name.png"
                if ($result.colors -gt 3) { $result.status = 'pass'; $result.reason = '' } else { $result.reason = 'uniform pixels' }
                return $result
            }
        }
        else {
            $own = [Uwp]::Top() | Where-Object { [uint32]$o = 0; [void][Uwp]::GetWindowThreadProcessId($_, [ref]$o); $o -eq $appPid -and (Get-Shown $_) -and [Uwp]::ClassOf($_) -ne 'Windows.UI.Core.CoreWindow' }
            if ($own) { $result.status = 'no-uwp'; $result.reason = 'own top-level window, not hosted'; return $result }
        }
        Start-Sleep -Milliseconds 100
    }
    $result
}

function Get-WindowPid([IntPtr]$Hwnd) { [uint32]$o = 0; [void][Uwp]::GetWindowThreadProcessId($Hwnd, [ref]$o); $o }

function Wait-Until([scriptblock]$Condition, [double]$Seconds) {
    $watch = [Diagnostics.Stopwatch]::StartNew()
    while ($watch.Elapsed.TotalSeconds -lt $Seconds) { if (& $Condition) { return [int]$watch.Elapsed.TotalMilliseconds }; Start-Sleep -Milliseconds 100 }
    $null
}

function Get-ShellOwner {
    $h = [Uwp]::GetShellWindow()
    if ($h -eq [IntPtr]::Zero) { return 'none' }
    "$((Get-Process -Id (Get-WindowPid $h) -ErrorAction SilentlyContinue).ProcessName)"
}

function Get-HostStatus { Get-Content C:\uwproot\state\uwphost.status -Raw -ErrorAction SilentlyContinue }

function Start-UwpHost {
    Remove-Item C:\uwproot\state\uwphost.status -ErrorAction SilentlyContinue
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process C:\probe\host\winarchy-uwphost.exe -ArgumentList '--parent', $PID, '--root', 'C:\uwproot' -PassThru
    $null = $p.Handle
    $ready = Wait-Until { $p.HasExited -or "$(Get-HostStatus)" -match '\|ready\|' } 120
    @{ process = $p; ready = ($null -ne $ready -and -not $p.HasExited); ms = [int]$watch.Elapsed.TotalMilliseconds }
}

function Get-HostFacts($Run) { [ordered]@{ ready = $Run.ready; ms = $Run.ms; exited = $Run.process.HasExited; shellWindowOwner = (Get-ShellOwner); status = "$(Get-HostStatus)" } }

function Test-Lifecycle([string]$Aumid, $Apps, $HostRun) {
    $r = [ordered]@{}
    $first = Test-Presentation $Aumid 'life-open'
    if ($first.status -ne 'pass') { $r.open = $first; return $r }
    $hit = Get-Frames $first.pid | Select-Object -First 1
    [void][Uwp]::SetWindowPos($hit.Frame, [IntPtr]::Zero, 40, 40, 900, 600, 0x4004)
    $r.resizeMs = Wait-Until { $c = New-Object RECT; [void][Uwp]::GetWindowRect($hit.Core, [ref]$c); [Math]::Abs(($c.Right - $c.Left) - 900) -le 40 } 3
    $c = New-Object RECT; [void][Uwp]::GetWindowRect($hit.Core, [ref]$c); $r.coreAfterResize = "$($c.Right - $c.Left)x$($c.Bottom - $c.Top)"
    $r.resizeColors = Get-ColorCount $hit.Frame 'C:\sandbox\life-resized.png'
    if ($null -eq $r.resizeMs) {
        $f = New-Object RECT; [void][Uwp]::GetClientRect($hit.Frame, [ref]$f)
        [void][Uwp]::SetWindowPos($hit.Core, [IntPtr]::Zero, 0, 0, $f.Right, $f.Bottom, 0x4014)
        Start-Sleep 2
        $c = New-Object RECT; [void][Uwp]::GetWindowRect($hit.Core, [ref]$c); $r.coreAfterManualResize = "$($c.Right - $c.Left)x$($c.Bottom - $c.Top) of $($f.Right)x$($f.Bottom)"
        [void](Get-ColorCount $hit.Frame 'C:\sandbox\life-resized-manual.png')
    }
    [void][Uwp]::PostMessage($hit.Frame, 0x10, [IntPtr]::Zero, [IntPtr]::Zero)
    $r.closeMs = Wait-Until { -not (Get-Frames $first.pid | Where-Object { Get-Shown $_.Frame }) } 6
    $r.relaunch = Test-Presentation $Aumid 'life-relaunch'
    $second = $Apps | Where-Object { $_.AppID -match 'WindowsBackup' } | Select-Object -First 1
    if ($second) { $r.second = Test-Presentation $second.AppID 'life-second'; $r.firstStillShown = [bool](Get-Frames $r.relaunch.pid | Where-Object { Get-Shown $_.Frame }) }

    $open = Get-Frames $r.relaunch.pid | Select-Object -First 1
    $HostRun.process.Refresh()
    $cpu = $HostRun.process.TotalProcessorTime.TotalMilliseconds
    Start-Sleep 60
    $HostRun.process.Refresh()
    $r.idle = [ordered]@{ cpuMsPerMinute = [int]($HostRun.process.TotalProcessorTime.TotalMilliseconds - $cpu); workingSetMB = [int]($HostRun.process.WorkingSet64 / 1MB); privateMB = [int]($HostRun.process.PrivateMemorySize64 / 1MB) }

    $HostRun.process.Kill(); [void]$HostRun.process.WaitForExit(5000)
    Start-Sleep 3
    $r.afterKill = [ordered]@{ shellWindowOwner = (Get-ShellOwner) }
    if ($open) { $r.afterKill.frameExists = [Uwp]::IsWindow($open.Frame); $r.afterKill.coreExists = [Uwp]::IsWindow($open.Core); $r.afterKill.coreShown = (Get-Shown $open.Core) }
    $again = Start-UwpHost
    $r.restart = Get-HostFacts $again
    $r.afterRestart = Test-Presentation $Aumid 'life-after-restart'
    if ($r.afterRestart.status -ne 'pass') {
        # la app que quedó huérfana se cierra y se relanza: lo que haría el usuario
        $r.orphanShown = [bool](Get-Frames $r.relaunch.pid | Where-Object { Get-Shown $_.Frame })
        Stop-Process -Id $r.relaunch.pid -Force -ErrorAction SilentlyContinue
        Start-Sleep 3
        $r.afterRelaunch = Test-Presentation $Aumid 'life-after-orphan-closed'
    }
    # autovigilancia: solo se frena el hilo dueño de la ventana de shell; el proceso tiene que salir solo con 6
    if (-not $again.process.HasExited) {
        [uint32]$owner = 0; $thread = [Uwp]::GetWindowThreadProcessId([Uwp]::GetShellWindow(), [ref]$owner)
        [void][Uwp]::SuspendThread([Uwp]::OpenThread(2, $false, $thread))
        $watch = [Diagnostics.Stopwatch]::StartNew()
        $exited = $again.process.WaitForExit(30000)
        $r.selfWatch = [ordered]@{ exited = $exited; ms = [int]$watch.Elapsed.TotalMilliseconds; code = $(if ($exited) { $again.process.ExitCode }) }
        if (-not $exited) { $again.process.Kill() }
    }
    $r
}

# una ventana mandada al fondo (como hace komorebi) tiene que seguir por encima del escritorio de la shell
function Test-BottomOfZOrder {
    $desk = [Uwp]::FindWindow('WinarchyDesktopWallpaper', $null)
    if ($desk -eq [IntPtr]::Zero) { return [ordered]@{ desktop = 'not found' } }
    $form = New-Object Windows.Forms.Form; $form.Show(); [Windows.Forms.Application]::DoEvents()
    $above = { $z = [Uwp]::Top(); [Array]::IndexOf($z, $form.Handle) -lt [Array]::IndexOf($z, $desk) }
    $r = [ordered]@{ before = (& $above) }
    $order = { $z = [Uwp]::Top(); "of $($z.Length): shellWindow $([Array]::IndexOf($z, [Uwp]::GetShellWindow())), desktop $([Array]::IndexOf($z, $desk)), window $([Array]::IndexOf($z, $form.Handle))" }
    $r.orderBefore = (& $order)
    Start-Sleep 2; [Windows.Forms.Application]::DoEvents()
    $r.settled = (& $above)
    [void][Uwp]::SetWindowPos($form.Handle, [IntPtr]1, 0, 0, 0, 0, 0x13); Start-Sleep 2; [Windows.Forms.Application]::DoEvents()
    $r.afterWindowSentToBottom = (& $above)
    $r.orderAfter = (& $order)
    [void][Uwp]::SetWindowPos($desk, [IntPtr]1, 0, 0, 0, 0, 0x4013); Start-Sleep 1
    $r.afterDesktopSentToBottom = (& $above)
    $form.Close()
    $r
}

function Save-Screen([string]$Name) {
    $b = [Windows.Forms.SystemInformation]::VirtualScreen
    $bmp = New-Object Drawing.Bitmap $b.Width, $b.Height
    $g = [Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($b.Left, $b.Top, 0, 0, $bmp.Size); $g.Dispose()
    $bmp.Save("C:\sandbox\$Name.png", [Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
}

function Get-VisibleWindows { @([Uwp]::Top() | Where-Object { Get-Shown $_ } | ForEach-Object { "$([Uwp]::ClassOf($_))|$((Get-Process -Id (Get-WindowPid $_) -ErrorAction SilentlyContinue).ProcessName)" }) }

# combinaciones libres (true) o ya tomadas por alguien (false)
function Get-Hotkeys {
    $r = [ordered]@{}; $id = 100
    foreach ($k in @(@('Win+A', 0x8, 0x41), @('Win+N', 0x8, 0x4E), @('Win+Tab', 0x8, 0x09), @('Win+X', 0x8, 0x58), @('Win+Ctrl+D', 0xA, 0x44), @('Win+V', 0x8, 0x56), @('Alt+Tab', 0x1, 0x09))) {
        $id++; $r[$k[0]] = [Uwp]::RegisterHotKey([IntPtr]::Zero, $id, $k[1], $k[2]); [void][Uwp]::UnregisterHotKey([IntPtr]::Zero, $id)
    }
    $r
}

function Test-Shortcuts {
    $r = [ordered]@{}
    foreach ($k in @(@('Win+A', 0x41), @('Win+N', 0x4E), @('Win+I', 0x49), @('Win+V', 0x56), @('Win+Period', 0xBE), @('Win+P', 0x50), @('Win', 0))) {
        $before = Get-VisibleWindows
        [Uwp]::Chord([byte[]]$(if ($k[1]) { 0x5B, $k[1] } else { , 0x5B }))
        Start-Sleep 4
        $new = @(Get-VisibleWindows | Where-Object { $before -notcontains $_ })
        $r[$k[0]] = $new
        if ($new) { Save-Screen "key-$($k[0] -replace '\+', '-')" }
        [Uwp]::Chord([byte[]](, 0x1B)); Start-Sleep 2
    }
    $r
}

# cada lanzamiento va en un proceso aparte: contra un host colgado puede no volver nunca
function Measure-Launch([string]$Name, [string]$Script) {
    $done = "C:\sandbox\hang-$Name.txt"
    Remove-Item $done -ErrorAction SilentlyContinue
    $body = "`$w = [Diagnostics.Stopwatch]::StartNew(); try { $Script; `$r = 'ok' } catch { `$r = `$_.Exception.Message }; `"`$r|`$([int]`$w.Elapsed.TotalMilliseconds)`" | Set-Content $done"
    $info = New-Object Diagnostics.ProcessStartInfo 'powershell.exe', "-NoProfile -EncodedCommand $([Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($body)))"
    $info.UseShellExecute = $false; $info.CreateNoWindow = $true
    $child = [Diagnostics.Process]::Start($info)
    if (-not $child.WaitForExit(15000)) { $child.Kill(); return 'blocked for 15 s' }
    (Get-Content $done -ErrorAction SilentlyContinue) -join ' '
}

function Test-Hang($HostRun, [string]$Aumid, $Shell) {
    $r = [ordered]@{}
    [void][Uwp]::NtSuspendProcess($HostRun.process.Handle)
    try {
        $desk = [Uwp]::FindWindow('WinarchyDesktopWallpaper', $null)
        if ($Shell) { $r.shellRespondsWhileHung = (($desk -ne [IntPtr]::Zero) -and [Uwp]::Responds($desk)) }
        $r.hostWindowResponds = [Uwp]::Responds([Uwp]::GetShellWindow())
        $r.createProcess = Measure-Launch 'create' "`$i = New-Object Diagnostics.ProcessStartInfo 'cmd.exe', '/c exit'; `$i.UseShellExecute = `$false; [void][Diagnostics.Process]::Start(`$i)"
        $r.shellExecuteExe = Measure-Launch 'exe' "`$i = New-Object Diagnostics.ProcessStartInfo 'cmd.exe', '/c exit'; [void][Diagnostics.Process]::Start(`$i)"
        $r.shellExecuteDocument = Measure-Launch 'document' "Set-Content C:\Users\Public\hang.txt x; [void][Diagnostics.Process]::Start((New-Object Diagnostics.ProcessStartInfo 'C:\Users\Public\hang.txt'))"
        # nombres que el shell tiene que resolver, URI y AUMID: lo que lanzan AHK, la barra y Inicio
        $r.shellExecuteUnresolved = Measure-Launch 'unresolved' "[void][Diagnostics.Process]::Start((New-Object Diagnostics.ProcessStartInfo 'winarchy-no-such-program.exe'))"
        $r.shellExecuteUri = Measure-Launch 'uri' "[void][Diagnostics.Process]::Start((New-Object Diagnostics.ProcessStartInfo 'ms-settings:'))"
        $r.activateAumid = Measure-Launch 'aumid' "`$t = [Type]::GetTypeFromCLSID([Guid]'45BA127D-10A8-46EA-8AB7-56EA9078943C'); Add-Type -TypeDefinition '[System.Runtime.InteropServices.ComImport, System.Runtime.InteropServices.Guid(`"2e941141-7f97-4756-ba1d-9decde894a3d`"), System.Runtime.InteropServices.InterfaceType(System.Runtime.InteropServices.ComInterfaceType.InterfaceIsIUnknown)] public interface IAam { [System.Runtime.InteropServices.PreserveSig] int ActivateApplication([System.Runtime.InteropServices.MarshalAs(System.Runtime.InteropServices.UnmanagedType.LPWStr)] string id, [System.Runtime.InteropServices.MarshalAs(System.Runtime.InteropServices.UnmanagedType.LPWStr)] string args, int options, out uint pid); } public static class Aam { public static int Run(object o, string id) { uint pid; return ((IAam)o).ActivateApplication(id, null, 0, out pid); } }'; `$null = [Aam]::Run([Activator]::CreateInstance(`$t), '$Aumid')"
        $r.broadcast = Measure-Launch 'broadcast' "[void][Environment]::SetEnvironmentVariable('WINARCHY_HANG', '1', 'User')"
    }
    finally { [void][Uwp]::NtResumeProcess($HostRun.process.Handle) }
    Start-Sleep 2
    $r.afterResume = (Test-Presentation $Aumid 'hang-after-resume').status
    $r
}

# G3/G4: vigilante de prueba (WM_NULL de 1 s cada 2 s, 3 fallos) que mata el host colgado y lo relanza,
# con la shell sondeada cada 1 s desde otro proceso durante todo el ciclo
function Test-Watchdog($HostRun, [string]$Aumid) {
    Remove-Item C:\sandbox\sampler.stop, C:\sandbox\sampler.txt -ErrorAction SilentlyContinue
    $script = @'
Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class P { [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t); [DllImport("user32.dll")] public static extern IntPtr SendMessageTimeout(IntPtr h, uint m, IntPtr w, IntPtr l, uint f, uint ms, out IntPtr r); }'
$samples = 0; $failures = 0
while (-not (Test-Path C:\sandbox\sampler.stop)) {
    $h = [P]::FindWindow('WinarchyDesktopWallpaper', $null); [IntPtr]$out = [IntPtr]::Zero
    if ($h -eq [IntPtr]::Zero -or [P]::SendMessageTimeout($h, 0, [IntPtr]::Zero, [IntPtr]::Zero, 2, 1000, [ref]$out) -eq [IntPtr]::Zero) { $failures++ }
    $samples++
    "$samples $failures" | Set-Content C:\sandbox\sampler.txt
    Start-Sleep 1
}
'@
    $info = New-Object Diagnostics.ProcessStartInfo 'powershell.exe', "-NoProfile -EncodedCommand $([Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($script)))"
    $info.UseShellExecute = $false; $info.CreateNoWindow = $true
    $sampler = [Diagnostics.Process]::Start($info)
    $r = [ordered]@{}
    $null = Wait-Until { Test-Path C:\sandbox\sampler.txt } 30
    [void][Uwp]::NtSuspendProcess($HostRun.process.Handle)
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $hung = Get-ShellOwner
    $fails = 0
    while ($fails -lt 3 -and $watch.Elapsed.TotalSeconds -lt 30) {
        Start-Sleep 2
        if ([Uwp]::Responds([Uwp]::GetShellWindow())) { $fails = 0 } else { $fails++ }
    }
    $r.hungOwner = $hung
    $r.failures = $fails
    $r.detectedMs = [int]$watch.Elapsed.TotalMilliseconds
    $HostRun.process.Kill(); [void]$HostRun.process.WaitForExit(5000)
    $again = Start-UwpHost
    $r.restart = Get-HostFacts $again
    $r.recoveredMs = [int]$watch.Elapsed.TotalMilliseconds
    $r.afterRestart = (Test-Presentation $Aumid 'watchdog-after-restart').status
    Set-Content C:\sandbox\sampler.stop x
    [void]$sampler.WaitForExit(10000)
    $samples, $missed = (Get-Content C:\sandbox\sampler.txt) -split ' '
    $r.shellSamples = [int]$samples
    $r.shellFailures = [int]$missed
    if (-not $again.process.HasExited) { $again.process.Kill() }
    $r
}

# T2: registrar como escritorio la ventana de otro proceso
function Test-ForeignShellWindow {
    $script = 'Add-Type -AssemblyName System.Windows.Forms; $f = New-Object Windows.Forms.Form; $f.Text = "foreign-desktop"; [Windows.Forms.Application]::Run($f)'
    $info = New-Object Diagnostics.ProcessStartInfo 'powershell.exe', "-NoProfile -STA -EncodedCommand $([Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($script)))"
    $info.UseShellExecute = $false; $info.CreateNoWindow = $true
    $other = [Diagnostics.Process]::Start($info)
    $r = [ordered]@{}
    try {
        $null = Wait-Until { [Uwp]::FindWindow($null, 'foreign-desktop') -ne [IntPtr]::Zero } 20
        $r.before = Get-ShellOwner
        $r.result = [Uwp]::TrySetShellWindow([Uwp]::FindWindow($null, 'foreign-desktop'))
        $r.after = Get-ShellOwner
    }
    finally { $other.Kill() }
    $r
}

# G6: con las combinaciones que toma el host, AHK sigue recibiendo las suyas
function Test-AhkHotkeys {
    $keys = [ordered]@{ 'wintab' = @('#Tab', 0x5B, 0x09); 'winx' = @('#x', 0x5B, 0x58); 'winctrld' = @('#^d', 0x5B, 0x11, 0x44); 'alttab' = @('!Tab', 0x12, 0x09); 'wina' = @('#a', 0x5B, 0x41) }
    $body = "#SingleInstance Force`r`n" + (($keys.Keys | ForEach-Object { "$($keys[$_][0])::FileAppend('x', 'C:\Users\Public\ahk-$_')" }) -join "`r`n") + "`r`nFileAppend('x', 'C:\Users\Public\ahk-ready')`r`n"
    Set-Content C:\Users\Public\keys.ahk $body
    $ahk = Start-Process C:\probe\ahk\AutoHotkey64.exe -ArgumentList '/ErrorStdOut', 'C:\Users\Public\keys.ahk' -PassThru
    $r = [ordered]@{ ready = ($null -ne (Wait-Until { Test-Path C:\Users\Public\ahk-ready } 10)) }
    foreach ($name in $keys.Keys) {
        [Uwp]::Chord([byte[]]($keys[$name] | Select-Object -Skip 1))
        $r[$name] = ($null -ne (Wait-Until { Test-Path "C:\Users\Public\ahk-$name" } 3))
        Start-Sleep 1
    }
    $ahk.Kill()
    $r
}

function Test-Theme([string]$Aumid) {
    $p = Test-Presentation $Aumid 'theme-before'
    if ($p.status -ne 'pass') { return [ordered]@{ status = $p.reason } }
    $key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize'
    $now = (Get-ItemProperty $key).AppsUseLightTheme
    Set-ItemProperty $key AppsUseLightTheme ([int](-not $now)) -Type DWord
    [IntPtr]$out = [IntPtr]::Zero
    [void][Uwp]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [IntPtr]::Zero, 'ImmersiveColorSet', 2, 2000, [ref]$out)
    Start-Sleep 4
    $hit = Get-Frames $p.pid | Select-Object -First 1
    if ($hit) { [void](Get-ColorCount $hit.Frame 'C:\sandbox\theme-after.png') }
    Set-ItemProperty $key AppsUseLightTheme ([int]$now) -Type DWord
    [ordered]@{ wasLight = $now; captured = [bool]$hit }
}

function Test-Cost([string]$Aumid) {
    $times = foreach ($i in 1..10) {
        $p = Test-Presentation $Aumid "cost"
        if ($p.status -eq 'pass') { $p.ms }
        $hit = Get-Frames $p.pid | Select-Object -First 1
        if ($hit) { [void][Uwp]::PostMessage($hit.Frame, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) }
        Start-Sleep 3
    }
    $sorted = @($times | Sort-Object)
    if (-not $sorted) { return [ordered]@{ samples = 0 } }
    [ordered]@{ samples = $sorted.Count; p50 = $sorted[[int](($sorted.Count - 1) * 0.5)]; p95 = $sorted[[int][Math]::Ceiling(($sorted.Count - 1) * 0.95)] }
}

function Get-HostProcess { Get-Process winarchy-uwphost -ErrorAction SilentlyContinue | Select-Object -First 1 }

function Wait-ShellHost([int]$Not, [double]$Seconds) {
    $watch = [Diagnostics.Stopwatch]::StartNew()
    while ($watch.Elapsed.TotalSeconds -lt $Seconds) {
        $p = Get-HostProcess
        if ($p -and $p.Id -ne $Not -and "$(Get-HostStatus)".StartsWith("$($p.Id)|ready|")) { return [ordered]@{ pid = $p.Id; ms = [int]$watch.Elapsed.TotalMilliseconds } }
        if ((Get-ShellOwner) -eq 'winarchy-shell') { break }
        Start-Sleep -Milliseconds 100
    }
    [ordered]@{ pid = 0; ms = $null }
}

# los enlaces ms-settings: tal como los lanzan la shell y AHK (ShellExecute), con Configuración cerrada
function Test-SettingsLink([string]$Name, [scriptblock]$Open) {
    $Open.Invoke()
    $shown = Wait-Until { $app = Get-Process SystemSettings -ErrorAction SilentlyContinue | Select-Object -First 1; $app -and (Get-Frames $app.Id | Where-Object { (Get-Shown $_.Frame) -and (Get-Shown $_.Core) }) } 15
    Start-Sleep 2
    Save-Screen $Name
    [ordered]@{ shownMs = $shown }
}

# el contenido tiene que seguir al marco en mosaico, maximizado y restaurado: las capturas lo muestran
function Test-Resize {
    $app = Get-Process SystemSettings -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $app) { return 'no app' }
    $frame = (Get-Frames $app.Id | Select-Object -First 1).Frame
    $place = { param($x, $y, $w, $h) [void][Uwp]::SetWindowPos($frame, [IntPtr]::Zero, $x, $y, $w, $h, 0x14) }
    foreach ($step in @(
            @('resize-1-wide', { & $place 100 60 1700 1000 }),
            @('resize-2-half', { & $place 960 40 940 1000 }),
            @('resize-3-maximized', { [void][Uwp]::ShowWindow($frame, 3) }),
            @('resize-4-restored', { [void][Uwp]::ShowWindow($frame, 9) }),
            @('resize-5-small', { & $place 40 40 800 600 }))) {
        & $step[1]; Start-Sleep 4; Save-Screen $step[0]
    }
    $r = [ordered]@{}
    # una app que no responde durante el redimensionado no puede colgar al host
    $hostPid = (Get-HostProcess).Id
    $null = $app.Handle
    [void][Uwp]::NtSuspendProcess($app.Handle)
    try { & $place 100 60 1200 900; Start-Sleep 15; $r.hostSurvivesHungApp = ((Get-HostProcess).Id -eq $hostPid) }
    finally { [void][Uwp]::NtResumeProcess($app.Handle) }
    Start-Sleep 3
    $r.appsOpen = "$(Get-Content C:\uwproot\state\uwphost.apps -Raw)".Trim()
    [void][Uwp]::PostMessage($frame, 0x10, [IntPtr]::Zero, [IntPtr]::Zero); Start-Sleep 6
    $r.appsAfterClose = "$(Get-Content C:\uwproot\state\uwphost.apps -Raw)".Trim()
    $r.errors = @(Select-String C:\uwproot\state\uwphost.log -Pattern 'phase=bounds' | ForEach-Object Line)
    $r
}

function Test-AhkLink([string]$Name) {
    $ahk = 'C:\probe\ahk\AutoHotkey64.exe'
    if (-not (Test-Path $ahk)) { return 'no AutoHotkey' }
    $functions = [regex]::Matches((Get-Content C:\probe\winarchy.ahk -Raw), '(?ms)^(OpenSettings|SettingsLinksOpen|RunTool)\(.*?^\}').Value -join "`r`n"
    Set-Content C:\Users\Public\link.ahk -Encoding UTF8 "#Requires AutoHotkey v2.0`r`nSettingsFallback := Map()`r`nStateDir := 'C:\uwproot\state'`r`nWLog(*) => 0`r`nWinarchyText(t) => t`r`nOpenSettings('ms-settings:about')`r`n$functions"
    Test-SettingsLink $Name { Start-Process $ahk -ArgumentList '/ErrorStdOut', 'C:\Users\Public\link.ahk' }
}

# con Configuración abierta: el conmutador de Alt+Tab tal como lo pide AHK, y el theme oscuro y claro
function Test-Integration {
    $app = Get-Process SystemSettings -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $app) { return 'no app' }
    $frame = (Get-Frames $app.Id | Select-Object -First 1).Frame
    $r = [ordered]@{}
    $form = New-Object Windows.Forms.Form; $form.Text = 'otra ventana'; $form.Show(); [Windows.Forms.Application]::DoEvents()
    $ahk = 'C:\probe\ahk\AutoHotkey64.exe'
    if (Test-Path $ahk) {
        $island = [regex]::Match((Get-Content C:\probe\winarchy.ahk -Raw), '(?ms)^ShellIsland\(.*?^\}').Value
        $show = "ShellIsland('switcher.show', ',`"params`":{`"windows`":[$([long]$frame),$([long]$form.Handle)],`"index`":0}')"
        Set-Content C:\Users\Public\switch.ahk -Encoding UTF8 "#Requires AutoHotkey v2.0`r`nDllCall('ProcessIdToSessionId', 'uint', ProcessExist(), 'uint*', &sessionId := 0)`r`n$show`r`nSleep 6000`r`nShellIsland('switcher.hide')`r`n$island"
        $run = Start-Process $ahk -ArgumentList '/ErrorStdOut', 'C:\Users\Public\switch.ahk' -PassThru
        Start-Sleep 3; Save-Screen 'switcher'
        $r.switcher = $run.WaitForExit(10000)
    }
    $key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize'
    $before = Get-ItemProperty $key
    foreach ($mode in @(@('theme-dark', 0), @('theme-light', 1))) {
        foreach ($name in 'AppsUseLightTheme', 'SystemUsesLightTheme') { Set-ItemProperty $key $name $mode[1] -Type DWord }
        [IntPtr]$out = [IntPtr]::Zero
        [void][Uwp]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [IntPtr]::Zero, 'ImmersiveColorSet', 2, 2000, [ref]$out)
        Start-Sleep 5; Save-Screen $mode[0]
    }
    foreach ($name in 'AppsUseLightTheme', 'SystemUsesLightTheme') { if ($null -ne $before.$name) { Set-ItemProperty $key $name $before.$name -Type DWord } }
    $form.Close()
    $r
}

# lo que hace la shell de sesión sola: lanzar, vigilar, relanzar, rendirse y no dejar huérfanos
function Test-Supervision([string]$Aumid) {
    New-Item -ItemType Directory C:\uwproot\state\bin -Force | Out-Null
    Copy-Item C:\probe\host\winarchy-uwphost.exe C:\uwproot\state\bin
    $r = [ordered]@{}
    # la corrida sin red también cubre el aviso con las animaciones apagadas (SPI_SETCLIENTAREAANIMATION)
    if ($scenario.offline) { $r.animationsOff = [Uwp]::SystemParametersInfo(0x1043, 0, [IntPtr]::Zero, 3) }
    $start = { Start-Process C:\uwproot\winarchy-shell.exe -ArgumentList '--session-shell', '--root', 'C:\uwproot' -WindowStyle Hidden -PassThru }

    $shell = & $start
    Start-Sleep 8
    $r.withoutFlag = [ordered]@{ hostRunning = [bool](Get-HostProcess); shellWindowOwner = (Get-ShellOwner) }
    $shell.Kill(); [void]$shell.WaitForExit(5000); Start-Sleep 3

    Set-Content C:\uwproot\state\uwphost.enabled '1'
    $shell = & $start
    $first = Wait-ShellHost 0 150
    $r.enabled = [ordered]@{ readyMs = $first.ms; shellWindowOwner = (Get-ShellOwner) }
    if (-not $first.pid) {
        # sin símbolos: la shell tiene que reclamar su ventana y dejar la marca
        Start-Sleep 5
        Save-Screen 'degraded'
        $r.degraded = [ordered]@{ shellWindowOwner = (Get-ShellOwner); blockedMarker = (Test-Path C:\uwproot\state\uwphost.blocked); hostRunning = [bool](Get-HostProcess); zorder = (Test-BottomOfZOrder) }
    }
    else {
        $r.enabled.parentIsShell = ((Get-CimInstance Win32_Process -Filter "ProcessId=$($first.pid)").ParentProcessId -eq $shell.Id)
        $r.enabled.settings = (Test-Presentation $Aumid 'supervised-settings').status
        $r.enabled.zorder = Test-BottomOfZOrder
        Get-Process SystemSettings -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep 3
        $r.enabled.link = Test-SettingsLink 'link-shell' { [void][Diagnostics.Process]::Start((New-Object Diagnostics.ProcessStartInfo 'ms-settings:about')) }
        $r.enabled.integration = Test-Integration
        $r.enabled.resize = Test-Resize
        Get-Process SystemSettings -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep 3
        $r.enabled.ahkLink = Test-AhkLink 'link-ahk'

        $victim = Get-HostProcess
        $null = $victim.Handle
        [void][Uwp]::NtSuspendProcess($victim.Handle)
        $watch = [Diagnostics.Stopwatch]::StartNew()
        $terminated = $victim.WaitForExit(30000)
        $r.hang = [ordered]@{ terminated = $terminated; detectedMs = [int]$watch.Elapsed.TotalMilliseconds; shellResponds = [Uwp]::Responds([Uwp]::FindWindow('WinarchyDesktopWallpaper', $null)) }
        $next = Wait-ShellHost $victim.Id 30
        $r.hang.relaunchedMs = $next.ms
        # Configuración quedó viva del host anterior: el nuevo la cierra y el enlace vuelve a abrir
        $r.hang.orphanLink = Test-SettingsLink 'link-orphan' { [void][Diagnostics.Process]::Start((New-Object Diagnostics.ProcessStartInfo 'ms-settings:about')) }
        Get-Process SystemSettings -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep 3
        $r.hang.settingsAfter = (Test-Presentation $Aumid 'supervised-after-hang').status

        $r.kills = @()
        foreach ($n in 2..4) {
            $victim = Get-HostProcess
            $victim.Kill(); [void]$victim.WaitForExit(5000)
            $entry = [ordered]@{ failure = $n }
            if ($n -eq 3) { Start-Sleep 2; $entry.ownerDuringBackoff = (Get-ShellOwner); $entry.zorderDuringBackoff = Test-BottomOfZOrder }
            if ($n -lt 4) { $next = Wait-ShellHost $victim.Id 60; $entry.relaunched = [bool]$next.pid; $entry.waitedMs = $next.ms }
            $r.kills += $entry
        }
        $claimed = Wait-Until { (Get-ShellOwner) -eq 'winarchy-shell' } 15
        Save-Screen 'degraded'
        Start-Sleep 40
        $r.degraded = [ordered]@{ claimedMs = $claimed; hostRunning = [bool](Get-HostProcess); blockedMarker = (Test-Path C:\uwproot\state\uwphost.blocked); zorder = (Test-BottomOfZOrder) }
        $r.degraded.shellResponds = [Uwp]::Responds([Uwp]::FindWindow('WinarchyDesktopWallpaper', $null))
    }
    $shell.Kill(); [void]$shell.WaitForExit(5000); Start-Sleep 3

    if ($first.pid) {
        $shell = & $start
        $again = Wait-ShellHost 0 60
        $r.newSession = [ordered]@{ relaunched = [bool]$again.pid }
        $shell.Kill(); [void]$shell.WaitForExit(5000); Start-Sleep 3
        $r.newSession.hostDiedWithShell = -not (Get-HostProcess)
    }
    $r.explorer = [bool](Get-Process explorer -ErrorAction SilentlyContinue)
    Copy-Item C:\uwproot\state\shell-boot.log, C:\uwproot\state\uwphost.log C:\sandbox -ErrorAction SilentlyContinue
    $r
}

$report = [ordered]@{}
try {
    $scenario = Get-Content C:\probe\scenario.json -Raw | ConvertFrom-Json
    $apps = @(Get-StartApps)
    $report.packages = @(Get-AppxPackage | ForEach-Object Name | Sort-Object)
    $report.startApps = @($apps | ForEach-Object AppID | Sort-Object)

    # la sonda tiene que poder fallar: una ventana propia cloakeada no cuenta como mostrada
    $form = New-Object Windows.Forms.Form; $form.Show(); [Windows.Forms.Application]::DoEvents()
    $before = Get-Shown $form.Handle
    $one = 1; [void][Uwp]::DwmSetWindowAttribute($form.Handle, 13, [ref]$one, 4)
    $report.selfTest = [ordered]@{ shownBeforeCloak = $before; shownCloaked = (Get-Shown $form.Handle) }
    $form.Close()

    Copy-Item C:\probe\root C:\uwproot -Recurse
    $report.explorerStopped = $false
    if (-not $scenario.explorer) {
    Get-Process explorer -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep 2
    if (Get-Process explorer -ErrorAction SilentlyContinue) { throw 'Explorer came back in the guest.' }
    $report.explorerStopped = $true
    }
    if ($scenario.supervision) {
        $report.supervision = Test-Supervision 'windows.immersivecontrolpanel_cw5n1h2txyewy!microsoft.windows.immersivecontrolpanel'
        $report.status = 'done'
        return
    }
    $startShell = {
        $script:shell = Start-Process C:\uwproot\winarchy-shell.exe -ArgumentList '--session-shell', '--root', 'C:\uwproot' -WindowStyle Hidden -PassThru
        Start-Sleep 5
        $report.shell = [ordered]@{ alive = -not $script:shell.HasExited; ownsShellWindow = ((Get-WindowPid ([Uwp]::GetShellWindow())) -eq $script:shell.Id) }
    }
    if ($scenario.shell -and -not $scenario.hostFirst) { & $startShell }
    if ($scenario.host) {
        $hostRun = Start-UwpHost
        $report.host = Get-HostFacts $hostRun
        $report.host.exitCode = if ($hostRun.process.HasExited) { $hostRun.process.ExitCode }
        Start-Sleep 2
    }
    if ($scenario.shell -and $scenario.hostFirst) { & $startShell }
    if ($scenario.shell) { $report.zorder = Test-BottomOfZOrder }

    $targets = @{ settings = 'windows.immersivecontrolpanel_cw5n1h2txyewy!microsoft.windows.immersivecontrolpanel' }
    $calc = $apps | Where-Object { $_.AppID -match 'WindowsCalculator' } | Select-Object -First 1
    if ($calc) { $targets.calculator = $calc.AppID }
    if ($scenario.extras) {
        $report.hotkeys = Get-Hotkeys
        if (-not $scenario.explorer -and (Test-Path C:\probe\ahk\AutoHotkey64.exe)) { $report.ahk = Test-AhkHotkeys }
        if (-not $scenario.explorer -and -not $scenario.shell -and -not $scenario.host) { $report.foreignShellWindow = Test-ForeignShellWindow }
    }
    $report.apps = [ordered]@{}
    foreach ($name in $targets.Keys | Sort-Object) { $report.apps[$name] = Test-Presentation $targets[$name] $name }
    if ($scenario.extras -and $report.apps.settings.status -eq 'pass') {
        $report.cost = Test-Cost $targets.settings
        if ($scenario.host) {
            $report.theme = Test-Theme $targets.settings
            $report.shortcuts = Test-Shortcuts
            try { $null = [Activator]::CreateInstance([Type]::GetTypeFromCLSID([Guid]'c2f03a33-21f5-47fa-b4bb-156362a2f239')); $report.immersiveShellCom = 'available' } catch { $report.immersiveShellCom = $_.Exception.Message }
            $report.hang = Test-Hang $hostRun $targets.settings $scenario.shell
            if ($scenario.shell) { $report.watchdog = Test-Watchdog $hostRun $targets.settings }
        }
    }
    if ($scenario.host -and -not $scenario.extras -and $report.apps.settings.status -eq 'pass') { $report.lifecycle = Test-Lifecycle $targets.settings $apps $hostRun }
    if ($scenario.shell) { $report.shellWindowAtEnd = (Get-ShellOwner); $report.shellResponds = ((-not $shell.HasExited) -and [Uwp]::Responds([Uwp]::FindWindow('WinarchyDesktopWallpaper', $null))) }

    # control Win32 (el Sandbox no trae Notepad): una ventana de otro proceso, mostrada, nunca cuenta como alojada
    $script = 'Add-Type -AssemblyName System.Windows.Forms; $f = New-Object Windows.Forms.Form; $f.Text = "control"; [Windows.Forms.Application]::Run($f)'
    $info = New-Object Diagnostics.ProcessStartInfo 'powershell.exe', "-NoProfile -STA -EncodedCommand $([Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($script)))"
    $info.UseShellExecute = $false; $info.CreateNoWindow = $true
    $control = [Diagnostics.Process]::Start($info)
    $shownMs = Wait-Until { [Uwp]::Top() | Where-Object { (Get-WindowPid $_) -eq $control.Id -and (Get-Shown $_) } } 20
    $report.win32Control = [ordered]@{ shown = ($null -ne $shownMs); hostedFrames = @(Get-Frames $control.Id).Count }
    $control.Kill()
    if ($scenario.host) {
        $report.symbols = @(Get-ChildItem C:\uwproot\state\symbols -Recurse -File -ErrorAction SilentlyContinue | ForEach-Object { "$($_.FullName.Substring(11)) $($_.Length)" })
        $report.hostStatus = "$(Get-HostStatus)"
        Copy-Item C:\uwproot\state\uwphost.log C:\sandbox -ErrorAction SilentlyContinue
    }
    $report.defender = try { [ordered]@{ realtime = (Get-MpComputerStatus).RealTimeProtectionEnabled; threats = @(Get-MpThreatDetection).Count } } catch { $_.Exception.Message }
    $report.explorerAtEnd = [bool](Get-Process explorer -ErrorAction SilentlyContinue)
    $report.status = 'done'
}
catch { $report.status = 'error'; $report.error = "$_ ($($_.InvocationInfo.ScriptLineNumber))" }
finally { $report | ConvertTo-Json -Depth 6 | Set-Content C:\sandbox\result.json -Encoding UTF8 }
