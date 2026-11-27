<#
.SYNOPSIS
  Guest side of Run-UwpHostSandbox.ps1 -Probe: records which Windows tools still open a window without Explorer.
.DESCRIPTION
  Each command is launched, the new visible windows and a screenshot are recorded, and what it started is closed.
  Then it checks the input language cell of the dock cluster with two keyboard layouts installed.
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
Add-Type -AssemblyName System.Drawing, System.Windows.Forms, UIAutomationClient, UIAutomationTypes
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class Win {
    delegate bool EnumProc(IntPtr hwnd, IntPtr l);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p, IntPtr l);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindow(string cls, string title);
    [DllImport("user32.dll")] public static extern IntPtr GetKeyboardLayout(uint thread);
    [DllImport("user32.dll")] public static extern int GetKeyboardLayoutList(int count, IntPtr[] list);
    [DllImport("user32.dll")] public static extern int SetDisplayConfig(uint paths, IntPtr pathArray, uint modes, IntPtr modeArray, uint flags);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr LoadKeyboardLayout(string id, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, int dx, int dy, uint data, UIntPtr extra);
    public static void Click(int x, int y) { SetCursorPos(x, y); System.Threading.Thread.Sleep(300); mouse_event(2, 0, 0, 0, UIntPtr.Zero); mouse_event(4, 0, 0, 0, UIntPtr.Zero); }
    [DllImport("dwmapi.dll")] static extern int DwmGetWindowAttribute(IntPtr h, int a, out int v, int size);
    public static bool Shown(IntPtr h) { int v; return IsWindowVisible(h) && !(DwmGetWindowAttribute(h, 14, out v, 4) == 0 && v != 0); }
    public static string ClassOf(IntPtr h) { var s = new StringBuilder(256); GetClassName(h, s, 256); return s.ToString(); }
    public static string TitleOf(IntPtr h) { var s = new StringBuilder(256); GetWindowText(h, s, 256); return s.ToString(); }
    public static IntPtr[] Top() { var l = new List<IntPtr>(); EnumWindows((h, p) => { if (Shown(h)) l.Add(h); return true; }, IntPtr.Zero); return l.ToArray(); }
}
'@
[void][Win]::SetProcessDPIAware()

function Save-Screen([string]$Name) {
    $b = [Windows.Forms.SystemInformation]::VirtualScreen
    $bmp = New-Object Drawing.Bitmap $b.Width, $b.Height
    $g = [Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($b.Left, $b.Top, 0, 0, $bmp.Size); $g.Dispose()
    $bmp.Save("C:\sandbox\$Name.png", [Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
}

function Test-Launch([string]$Name, [string]$File, [string]$Arguments) {
    $before = [Win]::Top()
    $processes = @(Get-Process | ForEach-Object Id)
    $r = [ordered]@{ command = "$File $Arguments".Trim(); error = $null; windows = @() }
    try { $info = New-Object Diagnostics.ProcessStartInfo $File, $Arguments; [void][Diagnostics.Process]::Start($info) }
    catch { $r.error = $_.Exception.GetBaseException().Message; return $r }
    $new = @()
    foreach ($i in 1..16) {
        Start-Sleep -Milliseconds 500
        $new = @([Win]::Top() | Where-Object { $before -notcontains $_ })
        if ($new) { break }
    }
    if ($new) {
        Start-Sleep 2
        $new = @([Win]::Top() | Where-Object { $before -notcontains $_ })
        $r.windows = @($new | ForEach-Object {
            [uint32]$owner = 0; [void][Win]::GetWindowThreadProcessId($_, [ref]$owner)
            "$((Get-Process -Id $owner -ErrorAction SilentlyContinue).ProcessName)|$([Win]::ClassOf($_))|$([Win]::TitleOf($_))"
        })
        Save-Screen "tool-$Name"
        foreach ($h in $new) { [void][Win]::PostMessage($h, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) }
        Start-Sleep 1
    }
    Get-Process | Where-Object { $processes -notcontains $_.Id -and $_.Id -ne $PID -and $_.SessionId -eq (Get-Process -Id $PID).SessionId } | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep 1
    $r
}

function Find-InputCell {
    $cluster = [Win]::FindWindow('WinarchyShellCluster', $null)
    if ($cluster -eq [IntPtr]::Zero) { return $null }
    $root = [Windows.Automation.AutomationElement]::FromHandle($cluster)
    $all = $root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)
    @($all) | Where-Object { $_.Current.Name -match '^(Input language|Idioma de entrada)' } | Select-Object -First 1
}

function Test-InputLanguage {
    $r = [ordered]@{ layouts = [Win]::GetKeyboardLayoutList(0, $null) }
    $form = New-Object Windows.Forms.Form; $form.Text = 'language target'; $form.Show()
    $pump = { foreach ($i in 1..20) { [Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 100 } }
    & $pump
    # un clic real la deja en primer plano: la píldora muestra el layout de esa ventana
    [Win]::Click($form.Left + 100, $form.Top + 100)
    & $pump
    $r.foreground = [Win]::GetForegroundWindow() -eq $form.Handle
    $layout = { [Win]::GetKeyboardLayout(0).ToInt64() }
    $cell = Find-InputCell
    if (-not $cell) { $r.cell = 'not found'; Save-Screen 'language-missing'; $form.Close(); return $r }
    $r.before = [ordered]@{ name = $cell.Current.Name; layout = (& $layout) }
    Save-Screen 'language-before'
    $b = $cell.Current.BoundingRectangle
    [Win]::Click([int]($b.X + $b.Width / 2), [int]($b.Y + $b.Height / 2))
    & $pump
    $r.after = [ordered]@{ name = (Find-InputCell).Current.Name; layout = (& $layout) }
    Save-Screen 'language-after'
    $form.Close()
    $r
}

function Test-DriveNotice {
    $r = [ordered]@{ letter = $null; error = $null }
    $vhd = Join-Path $env:TEMP 'probe.vhd'
    Set-Content "$env:TEMP\vhd.txt" "create vdisk file=`"$vhd`" maximum=32`nattach vdisk`ncreate partition primary`nformat fs=ntfs quick`nassign letter=V"
    $r.diskpart = (& diskpart /s "$env:TEMP\vhd.txt" 2>&1 | Out-String).Trim()
    $r.letter = Test-Path V:\
    Start-Sleep -Milliseconds 1500
    Save-Screen 'drive-notice'
    $pill = [Win]::FindWindow('WinarchyShellPill', [NullString]::Value)
    $b = [Windows.Automation.AutomationElement]::FromHandle($pill).Current.BoundingRectangle
    $before = [Win]::Top()
    [Win]::Click([int]($b.X + $b.Width / 2), [int]($b.Y + $b.Height / 2))
    Start-Sleep 4
    Save-Screen 'drive-clicked'
    $r.windows = @([Win]::Top() | Where-Object { $before -notcontains $_ } | ForEach-Object { "$([Win]::ClassOf($_))|$([Win]::TitleOf($_))" })
    $r
}

$report = [ordered]@{}
try {
    New-Item -ItemType Directory -Path C:\uwproot -Force | Out-Null
    Copy-Item C:\probe\root\* C:\uwproot -Recurse -Force
    # el idioma se agrega con Explorer vivo, como en una sesión que ya lo tenía configurado
    $list = Get-WinUserLanguageList
    $list.Add('es-AR')
    Set-WinUserLanguageList $list -Force
    [void][Win]::LoadKeyboardLayout('0000040a', 1)
    Start-Sleep 3
    Get-Process explorer -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep 3
    $themePath = 'C:\uwproot\config\shell\theme.json'
    $theme = Get-Content $themePath -Raw | ConvertFrom-Json
    $theme.dock = $true
    $theme | ConvertTo-Json -Compress | Set-Content $themePath
    $shell = Start-Process C:\uwproot\winarchy-shell.exe -ArgumentList '--session-shell', '--root', 'C:\uwproot' -WindowStyle Hidden -PassThru
    Start-Sleep 5
    $report.shellAlive = -not $shell.HasExited
    $report.explorerRunning = [bool](Get-Process explorer -ErrorAction SilentlyContinue)
    $cases = @(
        'devmgmt|devmgmt.msc|', 'diskmgmt|diskmgmt.msc|', 'taskmgr|taskmgr.exe|', 'ncpa|control.exe|ncpa.cpl', 'powercfg|control.exe|powercfg.cpl',
        'eventvwr|eventvwr.msc|', 'magnify|magnify.exe|', 'osk|osk.exe|',
        'mmsys|control.exe|mmsys.cpl', 'sndvol|SndVol.exe|', 'desk|control.exe|desk.cpl', 'bthprops|control.exe|bthprops.cpl',
        'timedate|control.exe|timedate.cpl', 'appwiz|control.exe|appwiz.cpl', 'sysdm|control.exe|sysdm.cpl', 'mouse|control.exe|main.cpl',
        'intl|control.exe|intl.cpl', 'cleanmgr|cleanmgr.exe|', 'colorcpl|colorcpl.exe|', 'printers|control.exe|printers',
        'folder|explorer.exe|C:\Users', 'settings|ms-settings:|', 'settings-sound|ms-settings:sound|'
    )
    $report.tools = [ordered]@{}
    foreach ($case in $cases) {
        $name, $file, $arguments = $case -split '\|'
        $report.tools[$name] = Test-Launch $name $file $arguments
        $report.tools[$name].explorerAfter = [bool](Get-Process explorer -ErrorAction SilentlyContinue)
        $report | ConvertTo-Json -Depth 6 | Set-Content C:\sandbox\result.json
        Get-Process explorer -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    }
    # SDC_VALIDATE | SDC_TOPOLOGY_INTERNAL: la llamada del menú Proyectar sin cambiar la pantalla
    $report.projectionValidate = [Win]::SetDisplayConfig(0, [IntPtr]::Zero, 0, [IntPtr]::Zero, 0x41)
    $report.inputLanguage = Test-InputLanguage
    $report.driveNotice = Test-DriveNotice
    $report.shellAliveAtEnd = -not $shell.HasExited
    $report.status = 'done'
}
catch { $report.status = 'error'; $report.error = "$($_.Exception.Message) ($($_.InvocationInfo.ScriptLineNumber))" }
$report | ConvertTo-Json -Depth 6 | Set-Content C:\sandbox\result.json
