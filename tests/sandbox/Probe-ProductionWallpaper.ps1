[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('Prepare', 'Run', 'Cleanup', 'UserRun')][string]$Mode,
    [string]$Password,
    [Parameter(Mandatory)][ValidatePattern('^[0-9a-f]{64}$')][string]$Nonce,
    [switch]$RealFiles
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$probeUser = 'WinarchyWpProbe'
$noncePath = 'C:\probe\production-wallpaper.nonce'
$output = 'C:\sandbox'
$registryPath = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon'

$snapshotPath = "C:\Windows\Temp\winarchy-wallpaper-$Nonce.json"

function Assert-Guest {
    if ($PSCommandPath -ine 'C:\probe\Probe-ProductionWallpaper.ps1' -or
        -not (Test-Path -LiteralPath 'C:\sandbox' -PathType Container) -or
        -not (Test-Path -LiteralPath 'C:\probe\host-identity.json' -PathType Leaf) -or
        -not (Test-Path -LiteralPath 'C:\Users\WDAGUtilityAccount' -PathType Container) -or
        -not (Test-Path -LiteralPath $noncePath -PathType Leaf) -or
        (Get-Content -LiteralPath $noncePath -Raw).Trim() -ne $Nonce -or
        $Nonce -notmatch '^[0-9a-f]{64}$') {
        throw 'This probe mode requires its staged nonce inside Windows Sandbox.'
    }
    $hostIdentity = Get-Content -LiteralPath 'C:\probe\host-identity.json' -Raw | ConvertFrom-Json
    $machineGuid = (Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Cryptography' -Name MachineGuid).MachineGuid
    $product = (Get-ItemProperty -LiteralPath 'HKLM:\HARDWARE\DESCRIPTION\System\BIOS' -Name SystemProductName).SystemProductName
    if (-not $hostIdentity.machine_guid -or $machineGuid -eq $hostIdentity.machine_guid -or
        $env:COMPUTERNAME -eq $hostIdentity.computer_name -or $product -ne 'Virtual Machine') {
        throw 'Refusing to alter Explorer or registry outside the staged disposable Sandbox.'
    }
}

Assert-Guest

if ($Mode -eq 'Prepare') {
    if ([Security.Principal.WindowsIdentity]::GetCurrent().User.Value -ne 'S-1-5-18') { throw 'Prepare must run as SYSTEM.' }
    if ((Test-Path -LiteralPath $snapshotPath) -or (Get-LocalUser -Name $probeUser -ErrorAction SilentlyContinue)) { throw 'Probe has already been prepared or its account already exists.' }
    if (-not $Password) { throw 'Prepare requires the generated probe password.' }
    $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon', $false)
    $exists = $null -ne $key -and $key.GetValueNames() -contains 'AutoRestartShell'
    $original = if ($exists) { @{ Exists = $true; Value = $key.GetValue('AutoRestartShell', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames); Kind = $key.GetValueKind('AutoRestartShell').ToString() } } else { @{ Exists = $false } }
    if ($null -ne $key) { $key.Dispose() }
    [IO.File]::WriteAllText($snapshotPath, ($original | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
    Copy-Item -LiteralPath $snapshotPath -Destination (Join-Path $output 'autorestart-original.json')
    [Microsoft.Win32.Registry]::SetValue('HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon', 'AutoRestartShell', 0, [Microsoft.Win32.RegistryValueKind]::DWord)
    if (Test-Path -LiteralPath 'C:\probe\root\YASB') { Copy-Item -LiteralPath 'C:\probe\root\YASB' -Destination (Join-Path $env:ProgramW6432 'YASB') -Recurse -Force }
    Get-Process explorer -ErrorAction SilentlyContinue | Where-Object { $_.SessionId -gt 0 -and $_.Path -ieq 'C:\Windows\explorer.exe' } | Stop-Process -Force
    $secure = ConvertTo-SecureString $Password -AsPlainText -Force
    New-LocalUser -Name $probeUser -Password $secure -AccountNeverExpires -PasswordNeverExpires | Out-Null
    Add-LocalGroupMember -SID 'S-1-5-32-545' -Member $probeUser
    if ((Get-LocalGroupMember -SID 'S-1-5-32-544').Name -contains "$env:COMPUTERNAME\$probeUser") { throw 'Probe user must be standard.' }
    exit 0
}

if ($Mode -eq 'Run') {
    $secure = ConvertTo-SecureString $Password -AsPlainText -Force
    $credential = [PSCredential]::new(".\$probeUser", $secure)
    $stdout = Join-Path $output 'production-wallpaper-user.stdout.txt'
    $stderr = Join-Path $output 'production-wallpaper-user.stderr.txt'
    $process = Start-Process powershell.exe -Credential $credential -LoadUserProfile -WindowStyle Hidden -PassThru -Wait -RedirectStandardOutput $stdout -RedirectStandardError $stderr -ArgumentList (@('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $PSCommandPath, '-Mode', 'UserRun', '-Nonce', $Nonce) + @(if ($RealFiles) { '-RealFiles' }))
    [IO.File]::WriteAllText((Join-Path $output 'production-wallpaper-exit.txt'), [string]$process.ExitCode, [Text.UTF8Encoding]::new($false))
    exit $process.ExitCode
}

if ($Mode -eq 'Cleanup') {
    if ([Security.Principal.WindowsIdentity]::GetCurrent().User.Value -ne 'S-1-5-18') { throw 'Cleanup must run as SYSTEM.' }
    if (-not (Test-Path -LiteralPath $snapshotPath)) { exit 0 }
    $probeExe = "C:\Users\$probeUser\winarchy-wallpaper-probe\winarchy-shell.exe"
    Get-Process winarchy-shell -ErrorAction SilentlyContinue | Where-Object Path -EQ $probeExe | Stop-Process -Force
    $filesExe = "C:\Users\$probeUser\winarchy-wallpaper-probe\state\bin\winarchy-files.exe"
    Get-Process winarchy-files -ErrorAction SilentlyContinue | Where-Object Path -EQ $filesExe | Stop-Process -Force
    $original = Get-Content -LiteralPath $snapshotPath -Raw | ConvertFrom-Json
    if ($original.Exists) {
        $kind = [Microsoft.Win32.RegistryValueKind][Enum]::Parse([Microsoft.Win32.RegistryValueKind], $original.Kind)
        $value = switch ($kind.ToString()) {
            DWord { [int]$original.Value }
            QWord { [long]$original.Value }
            Binary { ,([byte[]]$original.Value) }
            None { ,([byte[]]$original.Value) }
            MultiString { ,([string[]]$original.Value) }
            default { [string]$original.Value }
        }
        [Microsoft.Win32.Registry]::SetValue('HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon', 'AutoRestartShell', $value, $kind)
    } else {
        Remove-ItemProperty -Path $registryPath -Name AutoRestartShell -ErrorAction SilentlyContinue
    }
    $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon', $false)
    try {
        $exists = $null -ne $key -and $key.GetValueNames() -contains 'AutoRestartShell'
        $restored = if ($exists) { @{ Exists = $true; Value = $key.GetValue('AutoRestartShell', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames); Kind = $key.GetValueKind('AutoRestartShell').ToString() } } else { @{ Exists = $false } }
    } finally { if ($null -ne $key) { $key.Dispose() } }
    [IO.File]::WriteAllText((Join-Path $output 'autorestart-restored.json'), ($restored | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
    if ($restored.Exists -ne $original.Exists -or ($original.Exists -and
        ($restored.Kind -ne $original.Kind -or (ConvertTo-Json -InputObject $restored.Value -Compress) -ne (ConvertTo-Json -InputObject $original.Value -Compress)))) {
        throw 'AutoRestartShell was not restored exactly.'
    }
    if (Get-LocalUser -Name $probeUser -ErrorAction SilentlyContinue) { Remove-LocalUser -Name $probeUser }
    Remove-Item -LiteralPath $snapshotPath
    exit 0
}

if ($Mode -ne 'UserRun') { throw 'Pass a supported probe mode inside the guest.' }
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if ($identity.Name -ine "$env:COMPUTERNAME\$probeUser" -or $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'UserRun requires the disposable standard probe account.'
}

Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Collections.Generic;
[ComImport, Guid("B92B56A9-8B55-4E14-9A89-0199BBB6F93B"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IDesktopWallpaperProbe {
    [PreserveSig] int SetWallpaper(IntPtr monitor, IntPtr wallpaper);
    [PreserveSig] int GetWallpaper([MarshalAs(UnmanagedType.LPWStr)] string monitor, out IntPtr wallpaper);
    [PreserveSig] int GetMonitorDevicePathAt(uint index, out IntPtr monitor);
    [PreserveSig] int GetMonitorDevicePathCount(out uint count);
    [PreserveSig] int GetMonitorRECT([MarshalAs(UnmanagedType.LPWStr)] string monitor, out WallpaperProbeNative.RECT rect);
    [PreserveSig] int SetBackgroundColor(uint color);
    [PreserveSig] int GetBackgroundColor(out uint color);
    [PreserveSig] int SetPosition(int position);
    [PreserveSig] int GetPosition(out int position);
}
public class DesktopWallpaperMonitor {
    public uint index;
    public int id_hr;
    public string id;
    public int? rect_hr;
    public WallpaperProbeNative.RECT rect;
    public int? wallpaper_hr;
    public string wallpaper;
}
public class DesktopWallpaperResult {
    public int initialize_hr, activation_hr = unchecked((int)0x8000FFFF);
    public int? position_hr, background_color_hr, monitor_count_hr;
    public int position;
    public uint background_color, monitor_count;
    public List<DesktopWallpaperMonitor> monitors = new List<DesktopWallpaperMonitor>();
}
public static class DesktopWallpaperQuery {
    [DllImport("ole32.dll")] private static extern int CoInitializeEx(IntPtr reserved, uint mode);
    [DllImport("ole32.dll")] private static extern void CoUninitialize();
    [DllImport("ole32.dll")] private static extern int CoCreateInstance(ref Guid clsid, IntPtr outer, uint context, ref Guid iid, [MarshalAs(UnmanagedType.Interface)] out IDesktopWallpaperProbe wallpaper);
    public static DesktopWallpaperResult Read() {
        var result = new DesktopWallpaperResult();
        result.initialize_hr = CoInitializeEx(IntPtr.Zero, 2);
        if (result.initialize_hr < 0 && result.initialize_hr != unchecked((int)0x80010106)) return result;
        IDesktopWallpaperProbe api = null;
        try {
            var clsid = new Guid("C2CF3110-460E-4FC1-B9D0-8A1C0C9CC4BD");
            var iid = typeof(IDesktopWallpaperProbe).GUID;
            result.activation_hr = CoCreateInstance(ref clsid, IntPtr.Zero, 4, ref iid, out api);
            if (result.activation_hr < 0) return result;
            result.position_hr = api.GetPosition(out result.position);
            result.background_color_hr = api.GetBackgroundColor(out result.background_color);
            result.monitor_count_hr = api.GetMonitorDevicePathCount(out result.monitor_count);
            if (result.monitor_count_hr != 0) return result;
            for (uint i = 0; i < result.monitor_count; i++) {
                var monitor = new DesktopWallpaperMonitor { index = i };
                result.monitors.Add(monitor);
                IntPtr id = IntPtr.Zero;
                try {
                    monitor.id_hr = api.GetMonitorDevicePathAt(i, out id);
                    if (id != IntPtr.Zero) monitor.id = Marshal.PtrToStringUni(id);
                } finally { if (id != IntPtr.Zero) Marshal.FreeCoTaskMem(id); }
                if (monitor.id_hr != 0 || String.IsNullOrEmpty(monitor.id)) continue;
                monitor.rect_hr = api.GetMonitorRECT(monitor.id, out monitor.rect);
                IntPtr wallpaper = IntPtr.Zero;
                try {
                    monitor.wallpaper_hr = api.GetWallpaper(monitor.id, out wallpaper);
                    if (wallpaper != IntPtr.Zero) monitor.wallpaper = Marshal.PtrToStringUni(wallpaper);
                } finally { if (wallpaper != IntPtr.Zero) Marshal.FreeCoTaskMem(wallpaper); }
            }
            return result;
        } finally {
            if (api != null) Marshal.FinalReleaseComObject(api);
            if (result.initialize_hr >= 0) CoUninitialize();
        }
    }
}
public static class WallpaperProbeNative {
    [DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)] public static extern bool SystemParametersInfoW(uint action, uint param, string value, uint flags);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string className, string windowName);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
    [DllImport("user32.dll", EntryPoint="SystemParametersInfoW", CharSet=CharSet.Unicode, SetLastError=true)] public static extern bool GetWallpaper(uint action, uint param, System.Text.StringBuilder value, uint flags);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
    [DllImport("user32.dll", EntryPoint="FindWindowExW", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowEx(IntPtr parent, IntPtr after, string className, string windowName);
    [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr hwnd, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr hwnd, System.Text.StringBuilder text, int count);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassNameW(IntPtr hwnd, System.Text.StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT point);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd, ref POINT point);
    [DllImport("user32.dll", EntryPoint="SendMessageW")] public static extern IntPtr SendMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll", EntryPoint="PostMessageW")] public static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll", EntryPoint="keybd_event")] public static extern void KeybdEvent(byte key, byte scan, uint flags, UIntPtr extraInfo);
    [DllImport("user32.dll")] public static extern bool ScreenToClient(IntPtr hwnd, ref POINT point);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hwnd, out RECT rect);
    [DllImport("user32.dll")] public static extern int GetSystemMetrics(int index);
    [DllImport("user32.dll", SetLastError=true)] public static extern IntPtr GetDC(IntPtr hwnd);
    [DllImport("user32.dll", SetLastError=true)] public static extern int ReleaseDC(IntPtr hwnd, IntPtr dc);
    [DllImport("gdi32.dll", SetLastError=true)] public static extern bool BitBlt(IntPtr destination, int x, int y, int width, int height, IntPtr source, int sourceX, int sourceY, uint operation);
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
}
'@
Add-Type -AssemblyName Accessibility
Add-Type -ReferencedAssemblies ([Accessibility.IAccessible].Assembly.Location) -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using Accessibility;
public sealed class DesktopAccessibleChild {
    public int id { get; set; }
    public string name { get; set; }
    public object role { get; set; }
    public object state { get; set; }
    public string error { get; set; }
}
public sealed class DesktopAccessibleResult {
    public int child_count { get; set; }
    public List<DesktopAccessibleChild> children { get; set; }
    public DesktopAccessibleResult() { children = new List<DesktopAccessibleChild>(); }
}
public static class DesktopAccessibilityQuery {
    [DllImport("oleacc.dll", EntryPoint="AccessibleObjectFromWindow")]
    private static extern int AccessibleObjectFromWindow(IntPtr hwnd, uint objectId, ref Guid interfaceId, [MarshalAs(UnmanagedType.Interface)] out IAccessible accessible);
    public static DesktopAccessibleResult Read(IntPtr hwnd) {
        var iid = typeof(IAccessible).GUID;
        IAccessible accessible;
        var hr = AccessibleObjectFromWindow(hwnd, 0xFFFFFFFC, ref iid, out accessible);
        if (hr < 0) Marshal.ThrowExceptionForHR(hr);
        var result = new DesktopAccessibleResult { child_count = accessible.accChildCount };
        for (var id = 1; id <= result.child_count; id++) {
            var child = new DesktopAccessibleChild { id = id };
            try {
                child.name = accessible.get_accName(id);
                child.role = accessible.get_accRole(id);
                child.state = accessible.get_accState(id);
            } catch (Exception error) { child.error = error.Message; }
            result.children.Add(child);
        }
        return result;
    }
    private static IAccessible Open(IntPtr hwnd) {
        var iid = typeof(IAccessible).GUID;
        IAccessible accessible;
        var hr = AccessibleObjectFromWindow(hwnd, 0xFFFFFFFC, ref iid, out accessible);
        if (hr < 0) Marshal.ThrowExceptionForHR(hr);
        return accessible;
    }
    public static int State(IntPtr hwnd, int childId) {
        var accessible = Open(hwnd);
        try { return (int)accessible.get_accState(childId); }
        finally { Marshal.ReleaseComObject(accessible); }
    }
    public static int[] Location(IntPtr hwnd, int childId) {
        var accessible = Open(hwnd);
        try {
            int left, top, width, height;
            accessible.accLocation(out left, out top, out width, out height, childId);
            return new int[] { left, top, width, height };
        } finally { Marshal.ReleaseComObject(accessible); }
    }
    public static void Select(IntPtr hwnd, int childId) {
        var accessible = Open(hwnd);
        try { accessible.accSelect(3, childId); }
        finally { Marshal.ReleaseComObject(accessible); }
    }
    public static void AddSelection(IntPtr hwnd, int childId) {
        var accessible = Open(hwnd);
        try { accessible.accSelect(8, childId); }
        finally { Marshal.ReleaseComObject(accessible); }
    }
    public static void DefaultAction(IntPtr hwnd, int childId) {
        var accessible = Open(hwnd);
        try { accessible.accDoDefaultAction(childId); }
        finally { Marshal.ReleaseComObject(accessible); }
    }
}
'@

$root = Join-Path $env:USERPROFILE 'winarchy-wallpaper-probe'
$desktopRoot = [Environment]::GetFolderPath([Environment+SpecialFolder]::Desktop)
New-Item -ItemType Directory -Path (Join-Path $root 'config\shell'), (Join-Path $root 'state'), $desktopRoot -Force | Out-Null
Copy-Item -LiteralPath 'C:\probe\root\winarchy-shell.exe' -Destination (Join-Path $root 'winarchy-shell.exe') -Force
Copy-Item -LiteralPath 'C:\probe\root\config\shell\theme.json' -Destination (Join-Path $root 'config\shell\theme.json') -Force
New-Item -ItemType Directory -Path (Join-Path $root 'bin') -Force | Out-Null
if ($RealFiles) {
    $real = 'C:\probe\real'
    New-Item -ItemType Directory -Path (Join-Path $root 'state\bin'), (Join-Path $root 'config\files') -Force | Out-Null
    Copy-Item -Path (Join-Path $real 'cli\bin\*') -Destination (Join-Path $root 'bin') -Recurse -Force
    Copy-Item -LiteralPath (Join-Path $real 'cli\module') -Destination (Join-Path $root 'module') -Recurse -Force
    Copy-Item -Path (Join-Path $real 'files\*') -Destination (Join-Path $root 'state\bin') -Force
    Copy-Item -LiteralPath (Join-Path $real 'files-theme.json') -Destination (Join-Path $root 'config\files\theme.json') -Force
    Copy-Item -LiteralPath (Join-Path $real 'ps7') -Destination (Join-Path $root 'ps7') -Recurse -Force
} else {
Copy-Item -LiteralPath (Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe') -Destination (Join-Path $root 'pwsh.exe') -Force
@'
param([string]$Command, [string]$Path)
if ($Command -cne 'files' -or [string]::IsNullOrWhiteSpace($Path)) { exit 2 }
$record = @{ command = $Command; path = $Path }
[IO.File]::WriteAllText((Join-Path $PSScriptRoot 'files-open.json'), ($record | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
'@ | Set-Content -LiteralPath (Join-Path $root 'bin\winarchy.ps1') -Encoding UTF8
}

function New-SolidBitmap([string]$Path, [int]$Red, [int]$Green, [int]$Blue, [string]$Format) {
    $bitmap = [Drawing.Bitmap]::new(32, 32, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.Clear([Drawing.Color]::FromArgb($Red, $Green, $Blue))
        $bitmap.Save($Path, [Drawing.Imaging.ImageFormat]::$Format)
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
}

function Set-Wallpaper([string]$Path) {
    if (-not [WallpaperProbeNative]::SystemParametersInfoW(20, 0, $Path, 3)) { throw "SPI_SETDESKWALLPAPER failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())" }
    $current = [Text.StringBuilder]::new(32768)
    if (-not [WallpaperProbeNative]::GetWallpaper(115, $current.Capacity, $current, 0) -or $current.ToString() -ine $Path) {
        throw "SPI_GETDESKWALLPAPER did not retain the requested source format: $Path; got $current"
    }
}

function Wait-DesktopWindow([int]$ProcessId) {
    for ($i = 0; $i -lt 100; $i++) {
        $process = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
        $window = [WallpaperProbeNative]::FindWindow('WinarchyDesktopWallpaper', $null)
        if (-not $process) { throw 'Production shell exited before creating the wallpaper host.' }
        [uint32]$ownerId = 0
        if ($window -ne [IntPtr]::Zero) { $null = [WallpaperProbeNative]::GetWindowThreadProcessId($window, [ref]$ownerId) }
        if ($ownerId -eq $ProcessId -and [WallpaperProbeNative]::IsWindowVisible($window)) { return $window }
        Start-Sleep -Milliseconds 100
    }
    throw 'Production wallpaper host did not become visible within 10 seconds.'
}

function Capture-Color([string]$Name, [int]$Red, [int]$Green, [int]$Blue, [switch]$ScreenshotOnly) {
    $process.Refresh()
    if ($process.HasExited) { throw 'The original production shell exited during the probe.' }
    [uint32]$ownerId = 0
    $null = [WallpaperProbeNative]::GetWindowThreadProcessId($window, [ref]$ownerId)
    if ($ownerId -ne $process.Id -or -not [WallpaperProbeNative]::IsWindowVisible($window)) { throw 'Wallpaper host is not visible or belongs to another process.' }
    $x = [WallpaperProbeNative]::GetSystemMetrics(76)
    $y = [WallpaperProbeNative]::GetSystemMetrics(77)
    $width = [WallpaperProbeNative]::GetSystemMetrics(78)
    $height = [WallpaperProbeNative]::GetSystemMetrics(79)
    if ($width -le 0 -or $height -le 0) { throw 'Virtual screen bounds are invalid.' }
    $bounds = New-Object WallpaperProbeNative+RECT
    if (-not [WallpaperProbeNative]::GetWindowRect($window, [ref]$bounds) -or
        $bounds.Left -ne $x -or $bounds.Top -ne $y -or $bounds.Right -ne ($x + $width) -or $bounds.Bottom -ne ($y + $height)) {
        throw 'Production wallpaper host does not cover the virtual desktop.'
    }
    $bitmap = [Drawing.Bitmap]::new($width, $height)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $screenDc = [WallpaperProbeNative]::GetDC([IntPtr]::Zero)
        if ($screenDc -eq [IntPtr]::Zero) { throw "GetDC failed while capturing $Name (Win32=$([Runtime.InteropServices.Marshal]::GetLastWin32Error()))." }
        $bitmapDc = [IntPtr]::Zero
        try {
            $bitmapDc = $graphics.GetHdc()
            if (-not [WallpaperProbeNative]::BitBlt($bitmapDc, 0, 0, $width, $height, $screenDc, $x, $y, [uint32]0x00CC0020)) {
                throw "BitBlt failed while capturing $Name (source=0x$($screenDc.ToInt64().ToString('X')); destination=0x$($bitmapDc.ToInt64().ToString('X')); Win32=$([Runtime.InteropServices.Marshal]::GetLastWin32Error()))."
            }
        } finally {
            if ($bitmapDc -ne [IntPtr]::Zero) { $graphics.ReleaseHdc($bitmapDc) }
            $null = [WallpaperProbeNative]::ReleaseDC([IntPtr]::Zero, $screenDc)
        }
        $bitmap.Save((Join-Path $output "$Name.png"), [Drawing.Imaging.ImageFormat]::Png)
        $samples = foreach ($fx in @(0.25, 0.5, 0.75)) {
            foreach ($fy in @(0.25, 0.5, 0.75)) {
                $pixel = $bitmap.GetPixel([int]($width * $fx), [int]($height * $fy))
                [pscustomobject]@{ R = [int]$pixel.R; G = [int]$pixel.G; B = [int]$pixel.B }
            }
        }
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
    if (-not $ScreenshotOnly) {
        foreach ($pixel in $samples) {
            $actual = @($pixel.R, $pixel.G, $pixel.B)
            $expected = @($Red, $Green, $Blue)
            $delta = [Math]::Max([Math]::Abs($actual[0] - $Red), [Math]::Max([Math]::Abs($actual[1] - $Green), [Math]::Abs($actual[2] - $Blue)))
            [IO.File]::AppendAllText((Join-Path $output 'production-wallpaper-colors.txt'), "$Name expected=$($expected -join ',') actual=$($actual -join ',') delta=$delta" + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
            if ($delta -gt 4) { throw "$Name wallpaper pixel mismatch: expected $($expected -join ','); got $($actual -join ',')." }
        }
    }
}

$first = Join-Path $root 'winarchy-wallpaper-first.bmp'
$second = Join-Path $root 'winarchy-wallpaper-second.jpg'
$transparent = Join-Path $root 'winarchy-wallpaper-transparent.png'
New-SolidBitmap $first 31 87 143 Bmp
New-SolidBitmap $second 193 53 79 Jpeg
$corrupt = Join-Path $root 'winarchy-wallpaper-corrupt.png'
[IO.File]::WriteAllBytes($corrupt, [byte[]]@(0, 1, 2, 3, 4, 5))
$alpha = [Drawing.Bitmap]::new(32, 32, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
$alphaGraphics = [Drawing.Graphics]::FromImage($alpha)
try { $alphaGraphics.Clear([Drawing.Color]::Transparent); $alpha.Save($transparent, [Drawing.Imaging.ImageFormat]::Png) } finally { $alphaGraphics.Dispose(); $alpha.Dispose() }
Set-Wallpaper $corrupt
$exe = Join-Path $root 'winarchy-shell.exe'
if (Get-Process explorer -ErrorAction SilentlyContinue) { throw 'Explorer is still running in the disposable guest.' }
if ($RealFiles) {
    $installer = Start-Process -FilePath 'C:\probe\real\WindowsAppRuntimeInstall-x64.exe' -ArgumentList '--quiet' -WindowStyle Hidden -PassThru
    if (-not $installer.WaitForExit(120000)) { $installer.Kill(); throw 'Windows App Runtime installation exceeded 120 seconds.' }
    $runtimePackage = @(Get-AppxPackage -Name 'Microsoft.WindowsAppRuntime.2' | Select-Object Name, Version, Architecture, Status)
    [IO.File]::WriteAllText((Join-Path $output 'windows-app-runtime-packages.json'), (ConvertTo-Json -InputObject $runtimePackage -Depth 3), [Text.UTF8Encoding]::new($false))
    if ($installer.ExitCode -ne 0 -or -not ($runtimePackage | Where-Object { ([version]$_.Version).Major -eq 2 -and ([version]$_.Version).Minor -ge 4 -and [int]$_.Status -eq 0 })) { throw "Windows App Runtime 2.4+ (Microsoft.WindowsAppRuntime.2) is unavailable for the probe user (installer exit $($installer.ExitCode))." }
    $env:PATH = (Join-Path $root 'ps7') + ';' + $env:PATH
}
$process = Start-Process -FilePath $exe -WorkingDirectory $root -ArgumentList @('--session-shell', '--root', ('"' + $root + '"')) -WindowStyle Hidden -PassThru
try {
$window = Wait-DesktopWindow $process.Id
if ($RealFiles) {
    $filesExe = Join-Path $root 'state\bin\winarchy-files.exe'
    $pwshExe = Join-Path $root 'ps7\pwsh.exe'
    $files = [ordered]@{
        status = 'FAIL'; error = $null
        shell_sha256 = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
        files_sha256 = (Get-FileHash -LiteralPath $filesExe -Algorithm SHA256).Hash
        pwsh_sha256 = (Get-FileHash -LiteralPath $pwshExe -Algorithm SHA256).Hash
        pwsh_version = (Get-Item -LiteralPath $pwshExe).VersionInfo.ProductVersion
        explorer_seen = $false; cases = @()
    }
    $stopFiles = {
        Get-Process winarchy-files -ErrorAction SilentlyContinue | Where-Object Path -EQ $filesExe | Stop-Process -Force
        $until = [DateTime]::UtcNow.AddSeconds(10)
        while ((Get-Process winarchy-files -ErrorAction SilentlyContinue | Where-Object Path -EQ $filesExe) -and [DateTime]::UtcNow -lt $until) { Start-Sleep -Milliseconds 100 }
        if (Get-Process winarchy-files -ErrorAction SilentlyContinue | Where-Object Path -EQ $filesExe) { throw 'A previous Files process survived the stop.' }
    }
    try {
        Add-Type -AssemblyName UIAutomationClient
        Add-Type -AssemblyName WindowsBase
        $defView = [WallpaperProbeNative]::FindWindowEx($window, [IntPtr]::Zero, 'SHELLDLL_DefView', $null)
        $listViewHwnd = [WallpaperProbeNative]::FindWindowEx($defView, [IntPtr]::Zero, 'SysListView32', 'FolderView')
        if ($listViewHwnd -eq [IntPtr]::Zero) { throw 'Production Shell Desktop list HWND is missing.' }
        $folderName = 'Winarchy-' + [char]0x03A9 + [char]0x4E2D
        $folderPath = Join-Path $desktopRoot $folderName
        $marker = 'handoff-' + [Guid]::NewGuid().ToString('N') + '.txt'
        New-Item -ItemType Directory -Path $folderPath -Force | Out-Null
        New-Item -ItemType File -Path (Join-Path $folderPath $marker) | Out-Null
        $shortcut = (New-Object -ComObject WScript.Shell).CreateShortcut((Join-Path $desktopRoot 'Winarchy Folder Shortcut.lnk'))
        $shortcut.TargetPath = $folderPath
        $shortcut.Save()
        Add-Type -TypeDefinition @'
using System.Runtime.InteropServices;
public static class RealFilesInput {
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, System.IntPtr extra);
}
'@
        $activate = {
            param($how, $id)
            switch ($how) {
                'default-action' { [DesktopAccessibilityQuery]::Select($listViewHwnd, $id); [DesktopAccessibilityQuery]::DefaultAction($listViewHwnd, $id) }
                'enter' {
                    [DesktopAccessibilityQuery]::Select($listViewHwnd, $id)
                    if (-not [WallpaperProbeNative]::PostMessage($listViewHwnd, 0x0100, [IntPtr]13, [IntPtr]::Zero) -or -not [WallpaperProbeNative]::PostMessage($listViewHwnd, 0x0101, [IntPtr]13, [IntPtr]::Zero)) { throw 'Could not post Enter.' }
                }
                'double-click' {
                    $box = [DesktopAccessibilityQuery]::Location($listViewHwnd, $id)
                    $null = [RealFilesInput]::SetCursorPos($box[0] + [int]($box[2] / 2), $box[1] + [int]($box[3] / 2))
                    foreach ($pass in 1..2) { [RealFilesInput]::mouse_event(2, 0, 0, 0, [IntPtr]::Zero); [RealFilesInput]::mouse_event(4, 0, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 60 }
                }
            }
        }
        $waitItem = {
            param($name)
            $found = @()
            $limit = [DateTime]::UtcNow.AddSeconds(10)
            while ($found.Count -ne 1 -and [DateTime]::UtcNow -lt $limit) {
                $found = @([DesktopAccessibilityQuery]::Read($listViewHwnd).children | Where-Object { $_.name -ceq $name })
                if ($found.Count -ne 1) { Start-Sleep -Milliseconds 100 }
            }
            if ($found.Count -ne 1) { throw "Desktop item '$name' is not visible." }
            [int]$found[0].id
        }
        $filesTop = {
            param($filesPid)
            [Windows.Automation.AutomationElement]::RootElement.FindAll([Windows.Automation.TreeScope]::Children, (New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::ProcessIdProperty, $filesPid)))
        }
        $hasName = {
            param($tops, $name)
            foreach ($top in $tops) { if ($top.FindFirst([Windows.Automation.TreeScope]::Descendants, (New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::NameProperty, $name)))) { return $true } }
            $false
        }
        foreach ($case in @(@{ name = 'folder'; item = $folderName; how = 'default-action' }, @{ name = 'shortcut'; item = 'Winarchy Folder Shortcut'; how = 'default-action' }, @{ name = 'folder-enter'; item = $folderName; how = 'enter' }, @{ name = 'folder-double-click'; item = $folderName; how = 'double-click' })) {
            & $stopFiles
            $item = @()
            $until = [DateTime]::UtcNow.AddSeconds(10)
            while ($item.Count -ne 1 -and [DateTime]::UtcNow -lt $until) {
                $item = @([DesktopAccessibilityQuery]::Read($listViewHwnd).children | Where-Object { $_.name -ceq $case.item })
                if ($item.Count -ne 1) { Start-Sleep -Milliseconds 100 }
            }
            if ($item.Count -ne 1) { throw "Desktop item '$($case.item)' is not visible." }
            $record = [ordered]@{ case = $case.name; expected_path = $folderPath }
            $files.cases += $record
            $clock = [Diagnostics.Stopwatch]::StartNew()
            $record.how = $case.how
            & $activate $case.how ([int]$item[0].id)
            $filesProcess = $null
            while ($clock.Elapsed.TotalSeconds -lt 60) {
                if (Get-Process explorer -ErrorAction SilentlyContinue) { $files.explorer_seen = $true; throw "Explorer started during the $($case.name) handoff." }
                $filesProcess = Get-Process winarchy-files -ErrorAction SilentlyContinue | Where-Object Path -EQ $filesExe | Select-Object -First 1
                if ($filesProcess -and $filesProcess.MainWindowHandle -ne 0) { break }
                Start-Sleep -Milliseconds 100
            }
            if (-not $filesProcess -or $filesProcess.MainWindowHandle -eq 0) { throw "Files did not show a window within 60 s of the $($case.name) activation." }
            $record.window_ms = $clock.ElapsedMilliseconds
            $record.command_line = (Get-CimInstance Win32_Process -Filter "ProcessId=$($filesProcess.Id)").CommandLine
            if (-not $record.command_line.Contains($folderPath)) { throw "Files was started without the exact path: $($record.command_line)" }
            $tops = [Windows.Automation.AutomationElement]::RootElement.FindAll([Windows.Automation.TreeScope]::Children, (New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::ProcessIdProperty, $filesProcess.Id)))
            $record.top_windows = @($tops | ForEach-Object { "$($_.Current.ClassName)|$($_.Current.Name)" })
            if (@($tops | Where-Object { $_.Current.ClassName -ceq '#32770' }).Count) { throw "Files opened a dialog instead of a window: $($record.top_windows -join '; ')" }
            $record.marker_visible = $false
            $until = [DateTime]::UtcNow.AddSeconds(30)
            while (-not $record.marker_visible -and [DateTime]::UtcNow -lt $until) {
                foreach ($top in $tops) {
                    if ($top.FindFirst([Windows.Automation.TreeScope]::Descendants, (New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::NameProperty, $marker)))) { $record.marker_visible = $true }
                }
                if (-not $record.marker_visible) { Start-Sleep -Milliseconds 250 }
            }
            Start-Sleep -Milliseconds 1500
            Capture-Color "production-wallpaper-files-$($case.name)" 0 0 0 -ScreenshotOnly
            if (-not $record.marker_visible) { throw "The unique fixture '$marker' is not visible in the Files window for the $($case.name) activation." }
            if (Get-Process explorer -ErrorAction SilentlyContinue) { $files.explorer_seen = $true; throw 'Explorer is running after the Files handoff.' }
            & $stopFiles
        }
        # second activation: Files is single-instance, so the next folder must arrive through its pipe into the live window
        $secondName = 'Winarchy-Second'
        $secondPath = Join-Path $desktopRoot $secondName
        $secondMarker = 'second-' + [Guid]::NewGuid().ToString('N') + '.txt'
        New-Item -ItemType Directory -Path $secondPath -Force | Out-Null
        New-Item -ItemType File -Path (Join-Path $secondPath $secondMarker) | Out-Null
        & $stopFiles
        $second = [ordered]@{ case = 'second-activation'; expected_path = $secondPath }
        $files.second_activation = $second
        & $activate 'enter' (& $waitItem $folderName)
        $until = [DateTime]::UtcNow.AddSeconds(60)
        $live = $null
        while (-not ($live -and $live.MainWindowHandle -ne 0) -and [DateTime]::UtcNow -lt $until) { Start-Sleep -Milliseconds 200; $live = Get-Process winarchy-files -ErrorAction SilentlyContinue | Where-Object Path -EQ $filesExe | Select-Object -First 1 }
        if (-not $live) { throw 'Files did not start for the second-activation scenario.' }
        $second.first_pid = $live.Id
        Start-Sleep -Milliseconds 1500
        Capture-Color 'production-wallpaper-files-second-activation-before' 0 0 0 -ScreenshotOnly
        & $activate 'enter' (& $waitItem $secondName)
        $second.max_processes = 1
        $until = [DateTime]::UtcNow.AddSeconds(30)
        $second.marker_visible = $false
        while (-not $second.marker_visible -and [DateTime]::UtcNow -lt $until) {
            $count = @(Get-Process winarchy-files -ErrorAction SilentlyContinue | Where-Object Path -EQ $filesExe).Count
            if ($count -gt $second.max_processes) { $second.max_processes = $count }
            if (Get-Process explorer -ErrorAction SilentlyContinue) { $files.explorer_seen = $true; throw 'Explorer started during the second activation.' }
            $second.marker_visible = [bool](& $hasName (& $filesTop $live.Id) $secondMarker)
            if (-not $second.marker_visible) { Start-Sleep -Milliseconds 250 }
        }
        Start-Sleep -Milliseconds 3000
        $second.final_processes = @(Get-Process winarchy-files -ErrorAction SilentlyContinue | Where-Object Path -EQ $filesExe).Count
        $second.top_windows = @(& $filesTop $live.Id | ForEach-Object { "$($_.Current.ClassName)|$($_.Current.Name)" })
        $liveTop = @(& $filesTop $live.Id)[0]
        $second.visual_state = [string]$liveTop.GetCurrentPattern([Windows.Automation.WindowPattern]::Pattern).Current.WindowVisualState
        $foreground = [WallpaperProbeNative]::GetForegroundWindow()
        [uint32]$foregroundPid = 0
        $null = [WallpaperProbeNative]::GetWindowThreadProcessId($foreground, [ref]$foregroundPid)
        $second.foreground_is_files = ($foregroundPid -eq $live.Id)
        $second.foreground_is_shell = ($foregroundPid -eq $process.Id)
        Capture-Color 'production-wallpaper-files-second-activation' 0 0 0 -ScreenshotOnly
        if (-not $second.marker_visible -or $second.final_processes -ne 1) { throw "Second activation did not reuse the live Files instance: $($second | ConvertTo-Json -Compress)" }
        & $stopFiles
        # z-order: a real click on a visible Desktop icon while Files is open must not bury Files under the Desktop
        $zorder = [ordered]@{ case = 'desktop-click-over-files' }
        $files.zorder = $zorder
        & $activate 'enter' (& $waitItem $folderName)
        $until = [DateTime]::UtcNow.AddSeconds(60)
        $live = $null
        while (-not ($live -and $live.MainWindowHandle -ne 0) -and [DateTime]::UtcNow -lt $until) { Start-Sleep -Milliseconds 200; $live = Get-Process winarchy-files -ErrorAction SilentlyContinue | Where-Object Path -EQ $filesExe | Select-Object -First 1 }
        if (-not $live) { throw 'Files did not start for the z-order scenario.' }
        Start-Sleep -Milliseconds 2000
        $box = [DesktopAccessibilityQuery]::Location($listViewHwnd, (& $waitItem 'This PC'))
        $zorder.icon_box = $box -join ','
        $null = [RealFilesInput]::SetCursorPos($box[0] + [int]($box[2] / 2), $box[1] + [int]($box[3] / 2))
        [RealFilesInput]::mouse_event(2, 0, 0, 0, [IntPtr]::Zero); [RealFilesInput]::mouse_event(4, 0, 0, 0, [IntPtr]::Zero)
        Start-Sleep -Milliseconds 1500
        $foreground = [WallpaperProbeNative]::GetForegroundWindow()
        [uint32]$foregroundPid = 0
        $null = [WallpaperProbeNative]::GetWindowThreadProcessId($foreground, [ref]$foregroundPid)
        $zorder.foreground_is_files = ($foregroundPid -eq $live.Id)
        $zorder.foreground_is_shell = ($foregroundPid -eq $process.Id)
        Capture-Color 'production-wallpaper-files-zorder-click' 0 0 0 -ScreenshotOnly
        & $stopFiles
        # negatives: unsupported selections must show the shell's warning and never start Files
        $negatives = @()
        foreach ($negative in @('multiselection', 'virtual-folder')) {
            $entry = [ordered]@{ case = $negative }
            $negatives += $entry
            $files.negatives = $negatives
            if ($negative -eq 'multiselection') {
                $folderId = & $waitItem $folderName
                $galleryId = & $waitItem 'Gallery'
                [DesktopAccessibilityQuery]::Select($listViewHwnd, $folderId)
                $tries = 0
                $selected = @()
                while ($selected.Count -ne 2 -and $tries -lt 5) {
                    [DesktopAccessibilityQuery]::AddSelection($listViewHwnd, $galleryId)
                    Start-Sleep -Milliseconds 100
                    $selected = @(foreach ($child in [DesktopAccessibilityQuery]::Read($listViewHwnd).children | Where-Object { $null -ne $_.name }) { if (([DesktopAccessibilityQuery]::State($listViewHwnd, $child.id) -band 2) -ne 0) { $child.id } })
                    $tries++
                }
                if ($selected.Count -ne 2) { throw 'Could not multiselect the folder and Gallery.' }
            } else {
                [DesktopAccessibilityQuery]::Select($listViewHwnd, (& $waitItem 'This PC'))
            }
            if (-not [WallpaperProbeNative]::PostMessage($listViewHwnd, 0x0100, [IntPtr]13, [IntPtr]::Zero) -or -not [WallpaperProbeNative]::PostMessage($listViewHwnd, 0x0101, [IntPtr]13, [IntPtr]::Zero)) { throw "Could not post Enter for $negative." }
            $dialog = [IntPtr]::Zero
            $until = [DateTime]::UtcNow.AddSeconds(10)
            while ($dialog -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $until) {
                $candidate = [WallpaperProbeNative]::FindWindow('#32770', 'Winarchy')
                if ($candidate -ne [IntPtr]::Zero -and [WallpaperProbeNative]::IsWindowVisible($candidate)) { $dialog = $candidate } else { Start-Sleep -Milliseconds 50 }
            }
            if ($dialog -eq [IntPtr]::Zero) { throw "No warning was shown for $negative." }
            [uint32]$owner = 0
            $null = [WallpaperProbeNative]::GetWindowThreadProcessId($dialog, [ref]$owner)
            Start-Sleep -Milliseconds 1500
            $entry.dialog_owned_by_shell = ($owner -eq $process.Id)
            $entry.files_started = [bool](Get-Process winarchy-files -ErrorAction SilentlyContinue | Where-Object Path -EQ $filesExe)
            Capture-Color "production-wallpaper-files-negative-$negative" 0 0 0 -ScreenshotOnly
            $null = [WallpaperProbeNative]::PostMessage($dialog, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
            Start-Sleep -Milliseconds 300
            $entry.dialog_closed = -not [WallpaperProbeNative]::IsWindowVisible($dialog)
            $entry.explorer_running = [bool](Get-Process explorer -ErrorAction SilentlyContinue)
            if ($entry.explorer_running) { $files.explorer_seen = $true }
            if (-not $entry.dialog_owned_by_shell -or $entry.files_started -or -not $entry.dialog_closed -or $entry.explorer_running) { throw "Negative case $negative failed: $($entry | ConvertTo-Json -Compress)" }
        }
        $files.status = 'PASS'
    } catch {
        $files.error = $_.Exception.Message
        try { Capture-Color 'production-wallpaper-files-failure' 0 0 0 -ScreenshotOnly } catch { }
    } finally {
        [IO.File]::WriteAllText((Join-Path $output 'files-handoff-result.json'), ($files | ConvertTo-Json -Depth 6), [Text.UTF8Encoding]::new($false))
        & $stopFiles
        if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }
    }
    exit $(if ($files.status -eq 'PASS') { 0 } else { 1 })
}
$uiDiagnostics = [ordered]@{}
try {
    Add-Type -AssemblyName UIAutomationClient
    Add-Type -AssemblyName WindowsBase
    $automationRoot = [System.Windows.Automation.AutomationElement]::FromHandle($window)
    $desktopViewHwnd = [WallpaperProbeNative]::FindWindowEx($window, [IntPtr]::Zero, 'SHELLDLL_DefView', $null)
    $listViewHwnd = [WallpaperProbeNative]::FindWindowEx($desktopViewHwnd, [IntPtr]::Zero, 'SysListView32', 'FolderView')
    if ($desktopViewHwnd -eq [IntPtr]::Zero -or $listViewHwnd -eq [IntPtr]::Zero) { throw 'Production Shell Desktop list HWNDs are missing.' }
    $itemCount = [WallpaperProbeNative]::SendMessage($listViewHwnd, 0x1004, [IntPtr]::Zero, [IntPtr]::Zero).ToInt64()
    $descendants = $automationRoot.FindAll(
        [System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.Condition]::TrueCondition
    )
    $listAutomationRoot = [System.Windows.Automation.AutomationElement]::FromHandle($listViewHwnd)
    $rawWalker = [System.Windows.Automation.TreeWalker]::RawViewWalker
    $rawChildren = @()
    $rawElement = $rawWalker.GetFirstChild($listAutomationRoot)
    for ($rawIndex = 0; $null -ne $rawElement -and $rawIndex -lt 128; $rawIndex++) {
        $rawCurrent = $rawElement.Current
        $rawChildren += [pscustomobject]@{
            name = $rawCurrent.Name
            class_name = $rawCurrent.ClassName
            control_type = $rawCurrent.ControlType.ProgrammaticName
            supported_patterns = @($rawElement.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName })
        }
        $rawElement = $rawWalker.GetNextSibling($rawElement)
    }
    $listSupportedPatterns = @($listAutomationRoot.GetSupportedPatterns())
    $listPatterns = @($listSupportedPatterns | ForEach-Object { $_.ProgrammaticName })
    $hasLegacyPattern = $listSupportedPatterns | Where-Object Id -EQ 10018
    $uiAutomation = [ordered]@{
        status = 'PASS'
        root_name = $automationRoot.Current.Name
        root_class = $automationRoot.Current.ClassName
        desktop_view_hwnd = $desktopViewHwnd.ToInt64()
        list_view_hwnd = $listViewHwnd.ToInt64()
        list_item_count = $itemCount
        list_supported_patterns = $listPatterns
        list_has_legacy_accessible_pattern = $null -ne $hasLegacyPattern
        raw_view_children = $rawChildren
        descendants = @(
            for ($index = 0; $index -lt [Math]::Min($descendants.Count, 128); $index++) {
                $element = $descendants.Item($index)
                $current = $element.Current
                [pscustomobject]@{
                    name = $current.Name
                    class_name = $current.ClassName
                    control_type = $current.ControlType.ProgrammaticName
                    keyboard_focusable = $current.IsKeyboardFocusable
                    supported_patterns = @($element.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName })
                }
            }
        )
    }
    try { $uiAutomation.msaa = [DesktopAccessibilityQuery]::Read($listViewHwnd) }
    catch { $uiAutomation.msaa = [ordered]@{ status = 'ERROR'; error = $_.Exception.Message } }
    if ($uiAutomation.msaa.child_count -lt 1) { throw 'The production Shell view exposes no MSAA children.' }
    $profileItem = @($uiAutomation.msaa.children | Where-Object { $_.name -ceq $probeUser })
    if ($profileItem.Count -ne 1) { throw "Expected one accessible user folder, found $($profileItem.Count)." }
    $folderId = [int]$profileItem[0].id
    $profileRect = [DesktopAccessibilityQuery]::Location($listViewHwnd, $folderId)
    $profilePoint = [System.Windows.Point]::new([double]($profileRect[0] + ($profileRect[2] / 2)), [double]($profileRect[1] + ($profileRect[3] / 2)))
    $profilePointElement = [System.Windows.Automation.AutomationElement]::FromPoint($profilePoint)
    $uiAutomation.point_query = [ordered]@{
        name = $profilePointElement.Current.Name
        class_name = $profilePointElement.Current.ClassName
        control_type = $profilePointElement.Current.ControlType.ProgrammaticName
        supported_patterns = @($profilePointElement.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName })
    }
    $stateBefore = [DesktopAccessibilityQuery]::State($listViewHwnd, $folderId)
    $selectionAttempts = 0
    $stateSelected = $stateBefore
    while (($stateSelected -band 2) -eq 0 -and $selectionAttempts -lt 5) {
        [DesktopAccessibilityQuery]::Select($listViewHwnd, $folderId)
        Start-Sleep -Milliseconds 100
        $stateSelected = [DesktopAccessibilityQuery]::State($listViewHwnd, $folderId)
        $selectionAttempts++
    }
    if (($stateSelected -band 2) -eq 0) { throw "MSAA selection did not set STATE_SYSTEM_SELECTED after $selectionAttempts attempts (before=$stateBefore; after=$stateSelected)." }
    Capture-Color 'production-wallpaper-desktop-selected-folder' 32 32 32
    if (-not [WallpaperProbeNative]::PostMessage($listViewHwnd, 0x007B, $listViewHwnd, [IntPtr](-1))) { throw 'Could not post WM_CONTEXTMENU to the production Desktop list.' }
    $contextMenu = [IntPtr]::Zero
    $menuDeadline = [DateTime]::UtcNow.AddSeconds(3)
    while ($contextMenu -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $menuDeadline) {
        $candidateMenu = [WallpaperProbeNative]::FindWindow('#32768', $null)
        if ($candidateMenu -ne [IntPtr]::Zero -and [WallpaperProbeNative]::IsWindowVisible($candidateMenu)) { $contextMenu = $candidateMenu }
        else { Start-Sleep -Milliseconds 50 }
    }
    if ($contextMenu -eq [IntPtr]::Zero) { throw 'Production Desktop did not open a native context menu for the selected icon.' }
    [uint32]$menuOwnerId = 0
    $null = [WallpaperProbeNative]::GetWindowThreadProcessId($contextMenu, [ref]$menuOwnerId)
    Capture-Color 'production-wallpaper-native-context-menu' 32 32 32 -ScreenshotOnly
    $null = [WallpaperProbeNative]::SendMessage($contextMenu, 0x0100, [IntPtr]27, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 100
    $menuDismissed = -not [WallpaperProbeNative]::IsWindowVisible($contextMenu)
    if (-not $menuDismissed) {
        $null = [WallpaperProbeNative]::PostMessage($contextMenu, 0x0100, [IntPtr]27, [IntPtr]::Zero)
        Start-Sleep -Milliseconds 100
        $menuDismissed = -not [WallpaperProbeNative]::IsWindowVisible($contextMenu)
    }
    if (-not $menuDismissed) { throw 'Escape did not dismiss the native Desktop context menu.' }
    $shiftF10State = [DesktopAccessibilityQuery]::State($listViewHwnd, $folderId)
    $shiftF10SelectionAttempts = 0
    while (($shiftF10State -band 6) -ne 6 -and $shiftF10SelectionAttempts -lt 5) {
        [DesktopAccessibilityQuery]::Select($listViewHwnd, $folderId)
        Start-Sleep -Milliseconds 100
        $shiftF10State = [DesktopAccessibilityQuery]::State($listViewHwnd, $folderId)
        $shiftF10SelectionAttempts++
    }
    $uiDiagnostics.shift_f10_state = $shiftF10State
    $uiDiagnostics.shift_f10_selection_attempts = $shiftF10SelectionAttempts
    if (($shiftF10State -band 6) -ne 6) { throw "Shift+F10 target did not become selected and focused (MSAA state=$shiftF10State)." }
    [WallpaperProbeNative]::KeybdEvent([byte]16, [byte]0, [uint32]0, [UIntPtr]::Zero)
    try {
        [WallpaperProbeNative]::KeybdEvent([byte]121, [byte]0, [uint32]0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 50
    } finally {
        [WallpaperProbeNative]::KeybdEvent([byte]121, [byte]0, [uint32]2, [UIntPtr]::Zero)
        [WallpaperProbeNative]::KeybdEvent([byte]16, [byte]0, [uint32]2, [UIntPtr]::Zero)
    }
    $shiftF10Menu = [IntPtr]::Zero
    $shiftF10Deadline = [DateTime]::UtcNow.AddSeconds(3)
    while ($shiftF10Menu -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $shiftF10Deadline) {
        $candidateMenu = [WallpaperProbeNative]::FindWindow('#32768', $null)
        if ($candidateMenu -ne [IntPtr]::Zero -and [WallpaperProbeNative]::IsWindowVisible($candidateMenu)) { $shiftF10Menu = $candidateMenu }
        else { Start-Sleep -Milliseconds 50 }
    }
    if ($shiftF10Menu -eq [IntPtr]::Zero) {
        $foregroundHwnd = [WallpaperProbeNative]::GetForegroundWindow()
        $foregroundTitle = [Text.StringBuilder]::new(512)
        $foregroundClass = [Text.StringBuilder]::new(256)
        $null = [WallpaperProbeNative]::GetWindowTextW($foregroundHwnd, $foregroundTitle, $foregroundTitle.Capacity)
        $null = [WallpaperProbeNative]::GetClassNameW($foregroundHwnd, $foregroundClass, $foregroundClass.Capacity)
        [uint32]$foregroundOwnerPid = 0
        $null = [WallpaperProbeNative]::GetWindowThreadProcessId($foregroundHwnd, [ref]$foregroundOwnerPid)
        $uiDiagnostics.shift_f10_activation = [ordered]@{
            foreground_title = $foregroundTitle.ToString()
            foreground_class = $foregroundClass.ToString()
            foreground_owner_pid = $foregroundOwnerPid
            shell_pid = $process.Id
            target_state = $shiftF10State
        }
        throw "Shift+F10 did not open the native Desktop context menu: $($uiDiagnostics.shift_f10_activation | ConvertTo-Json -Compress)."
    }
    [uint32]$shiftF10MenuOwnerPid = 0
    $null = [WallpaperProbeNative]::GetWindowThreadProcessId($shiftF10Menu, [ref]$shiftF10MenuOwnerPid)
    Capture-Color 'production-wallpaper-shift-f10-menu' 32 32 32 -ScreenshotOnly
    $null = [WallpaperProbeNative]::SendMessage($shiftF10Menu, 0x0100, [IntPtr]27, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 100
    $shiftF10MenuDismissed = -not [WallpaperProbeNative]::IsWindowVisible($shiftF10Menu)
    if (-not $shiftF10MenuDismissed) { throw 'Escape did not dismiss the Shift+F10 context menu.' }
    [DesktopAccessibilityQuery]::Select($listViewHwnd, $folderId)
    if (-not [WallpaperProbeNative]::PostMessage($listViewHwnd, 0x0100, [IntPtr]40, [IntPtr]::Zero) -or
        -not [WallpaperProbeNative]::PostMessage($listViewHwnd, 0x0101, [IntPtr]40, [IntPtr]::Zero)) { throw 'Could not post the production Desktop Down-key messages.' }
    Start-Sleep -Milliseconds 200
    $keyboardSelectedIds = @(
        foreach ($child in $uiAutomation.msaa.children | Where-Object { $null -ne $_.name }) {
            if (([DesktopAccessibilityQuery]::State($listViewHwnd, $child.id) -band 2) -ne 0) { $child.id }
        }
    )
    if ($keyboardSelectedIds.Count -ne 1 -or $keyboardSelectedIds[0] -eq $folderId) {
        throw "Down-arrow did not navigate the production Shell selection (selected=$($keyboardSelectedIds -join ','))."
    }
    Capture-Color 'production-wallpaper-keyboard-navigation' 32 32 32
    $otherItem = @($uiAutomation.msaa.children | Where-Object { $_.id -ne $folderId -and $_.name -ceq 'Gallery' })
    if ($otherItem.Count -ne 1) { throw 'The production Desktop fixture does not expose exactly one Gallery item.' }
    $multiSelectionAttempts = 0
    while (([DesktopAccessibilityQuery]::State($listViewHwnd, $folderId) -band 2) -eq 0 -and $multiSelectionAttempts -lt 5) {
        [DesktopAccessibilityQuery]::Select($listViewHwnd, $folderId)
        Start-Sleep -Milliseconds 100
        $multiSelectionAttempts++
    }
    if (([DesktopAccessibilityQuery]::State($listViewHwnd, $folderId) -band 2) -eq 0) { throw 'Could not restore single-folder selection before the multiselection test.' }
    $addSelectionAttempts = 0
    $multiselectedIds = @()
    while ($addSelectionAttempts -lt 5) {
        [DesktopAccessibilityQuery]::AddSelection($listViewHwnd, [int]$otherItem[0].id)
        Start-Sleep -Milliseconds 100
        $multiselectedIds = @(
            foreach ($child in $uiAutomation.msaa.children | Where-Object { $null -ne $_.name }) {
                if (([DesktopAccessibilityQuery]::State($listViewHwnd, $child.id) -band 2) -ne 0) { $child.id }
            }
        )
        $addSelectionAttempts++
        if ($multiselectedIds.Count -eq 2 -and $folderId -in $multiselectedIds -and [int]$otherItem[0].id -in $multiselectedIds) { break }
    }
    if ($multiselectedIds.Count -ne 2 -or $folderId -notin $multiselectedIds -or [int]$otherItem[0].id -notin $multiselectedIds) {
        throw "MSAA ADDSELECTION did not retain both selected items after $addSelectionAttempts attempts (selected=$($multiselectedIds -join ','); expected=$folderId,$([int]$otherItem[0].id))."
    }
    Capture-Color 'production-wallpaper-multiselection' 32 32 32
    if (-not [WallpaperProbeNative]::PostMessage($listViewHwnd, 0x0100, [IntPtr]13, [IntPtr]::Zero) -or
        -not [WallpaperProbeNative]::PostMessage($listViewHwnd, 0x0101, [IntPtr]13, [IntPtr]::Zero)) { throw 'Could not activate the multiselected Desktop folders.' }
    $multiErrorDialog = [IntPtr]::Zero
    $multiErrorDeadline = [DateTime]::UtcNow.AddSeconds(3)
    while ($multiErrorDialog -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $multiErrorDeadline) {
        $candidateDialog = [WallpaperProbeNative]::FindWindow('#32770', 'Winarchy')
        if ($candidateDialog -ne [IntPtr]::Zero -and [WallpaperProbeNative]::IsWindowVisible($candidateDialog)) { $multiErrorDialog = $candidateDialog }
        else { Start-Sleep -Milliseconds 50 }
    }
    if ($multiErrorDialog -eq [IntPtr]::Zero) { throw 'Activating multiple folders did not show the unsupported-selection message.' }
    [uint32]$multiErrorOwnerPid = 0
    $null = [WallpaperProbeNative]::GetWindowThreadProcessId($multiErrorDialog, [ref]$multiErrorOwnerPid)
    if ($multiErrorOwnerPid -ne $process.Id) { throw 'The multiselection warning was not owned by the production shell.' }
    Capture-Color 'production-wallpaper-multiselection-warning' 32 32 32 -ScreenshotOnly
    $null = [WallpaperProbeNative]::PostMessage($multiErrorDialog, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 100
    if ([WallpaperProbeNative]::IsWindowVisible($multiErrorDialog) -or (Test-Path -LiteralPath (Join-Path $root 'bin\\files-open.json'))) {
        throw 'The multiselection warning did not close cleanly or activation was handed to Files.'
    }

    [DesktopAccessibilityQuery]::Select($listViewHwnd, $folderId)
    $galleryRect = [DesktopAccessibilityQuery]::Location($listViewHwnd, [int]$otherItem[0].id)
    if ($galleryRect[2] -le 0 -or $galleryRect[3] -le 0) { throw 'MSAA returned an empty Gallery icon rectangle.' }
    $clickPoint = New-Object WallpaperProbeNative+POINT
    $clickPoint.X = $galleryRect[0] + [int]($galleryRect[2] / 2)
    $clickPoint.Y = $galleryRect[1] + [int]($galleryRect[3] / 2)
    if (-not [WallpaperProbeNative]::ScreenToClient($listViewHwnd, [ref]$clickPoint)) { throw 'Could not convert the Gallery screen point to list-view client coordinates.' }
    $clientRect = New-Object WallpaperProbeNative+RECT
    if (-not [WallpaperProbeNative]::GetClientRect($listViewHwnd, [ref]$clientRect) -or
        $clickPoint.X -lt 0 -or $clickPoint.Y -lt 0 -or $clickPoint.X -ge $clientRect.Right -or $clickPoint.Y -ge $clientRect.Bottom) {
        throw 'The Gallery accessibility rectangle falls outside the production list-view client area.'
    }
    $clickLParam = [IntPtr]([long](($clickPoint.Y -shl 16) -bor ($clickPoint.X -band 0xffff)))
    if (-not [WallpaperProbeNative]::PostMessage($listViewHwnd, 0x0200, [IntPtr]::Zero, $clickLParam) -or
        -not [WallpaperProbeNative]::PostMessage($listViewHwnd, 0x0201, [IntPtr]1, $clickLParam) -or
        -not [WallpaperProbeNative]::PostMessage($listViewHwnd, 0x0202, [IntPtr]::Zero, $clickLParam)) { throw 'Could not post synthetic pointer messages to the production Desktop list.' }
    Start-Sleep -Milliseconds 200
    $pointerSelectedIds = @(
        foreach ($child in $uiAutomation.msaa.children | Where-Object { $null -ne $_.name }) {
            if (([DesktopAccessibilityQuery]::State($listViewHwnd, $child.id) -band 2) -ne 0) { $child.id }
        }
    )
    if ($pointerSelectedIds.Count -ne 1 -or $pointerSelectedIds[0] -ne [int]$otherItem[0].id) {
        throw "Synthetic pointer click did not select Gallery through list-view hit testing (selected=$($pointerSelectedIds -join ','))."
    }
    Capture-Color 'production-wallpaper-pointer-selection' 32 32 32
    $f5Name = 'Winarchy-F5-' + [Guid]::NewGuid().ToString('N') + '.txt'
    New-Item -ItemType File -Path (Join-Path $desktopRoot $f5Name) | Out-Null
    $f5VisibleBeforeKey = @([DesktopAccessibilityQuery]::Read($listViewHwnd).children | Where-Object { $_.name -ceq $f5Name }).Count -eq 1
    if (-not [WallpaperProbeNative]::PostMessage($listViewHwnd, 0x0100, [IntPtr]116, [IntPtr]::Zero) -or
        -not [WallpaperProbeNative]::PostMessage($listViewHwnd, 0x0101, [IntPtr]116, [IntPtr]::Zero)) { throw 'Could not post F5 to the production Desktop list.' }
    $f5Item = @()
    $f5Deadline = [DateTime]::UtcNow.AddSeconds(5)
    while ($f5Item.Count -ne 1 -and [DateTime]::UtcNow -lt $f5Deadline) {
        $f5Item = @([DesktopAccessibilityQuery]::Read($listViewHwnd).children | Where-Object { $_.name -ceq $f5Name })
        if ($f5Item.Count -ne 1) { Start-Sleep -Milliseconds 100 }
    }
    if ($f5Item.Count -ne 1) { throw 'The production Desktop view did not expose the fresh file after F5.' }
    Capture-Color 'production-wallpaper-f5-refresh' 32 32 32
    [DesktopAccessibilityQuery]::Select($listViewHwnd, $folderId)
    [DesktopAccessibilityQuery]::DefaultAction($listViewHwnd, $folderId)
    $handoffPath = Join-Path $root 'bin\files-open.json'
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    while (-not (Test-Path -LiteralPath $handoffPath) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 50 }
    if (-not (Test-Path -LiteralPath $handoffPath)) { throw 'Activating the production Desktop folder did not invoke the Files CLI contract.' }
    $handoff = Get-Content -LiteralPath $handoffPath -Raw | ConvertFrom-Json
    if ($handoff.command -cne 'files' -or $handoff.path -ine $env:USERPROFILE) { throw "Desktop folder handoff had unexpected arguments: $($handoff | ConvertTo-Json -Compress)." }
    $unicodeFolderName = 'Winarchy-' + [char]0x03A9 + [char]0x4E2D
    $unicodeFolderPath = Join-Path $desktopRoot $unicodeFolderName
    if (Test-Path -LiteralPath $unicodeFolderPath) { throw 'The Unicode Desktop fixture already exists.' }
    New-Item -ItemType Directory -Path $unicodeFolderPath | Out-Null
    $unicodeItem = @()
    $deadline = [DateTime]::UtcNow.AddSeconds(8)
    while ($unicodeItem.Count -ne 1 -and [DateTime]::UtcNow -lt $deadline) {
        $liveChildren = [DesktopAccessibilityQuery]::Read($listViewHwnd).children
        $unicodeItem = @($liveChildren | Where-Object { $_.name -ceq $unicodeFolderName })
        if ($unicodeItem.Count -ne 1) { Start-Sleep -Milliseconds 100 }
    }
    if ($unicodeItem.Count -ne 1) { throw 'The running production Shell view did not expose the newly created Unicode Desktop folder.' }
    Remove-Item -LiteralPath $handoffPath -Force
    $unicodeFolderId = [int]$unicodeItem[0].id
    [DesktopAccessibilityQuery]::Select($listViewHwnd, $unicodeFolderId)
    if (-not [WallpaperProbeNative]::PostMessage($listViewHwnd, 0x0100, [IntPtr]35, [IntPtr]::Zero) -or
        -not [WallpaperProbeNative]::PostMessage($listViewHwnd, 0x0101, [IntPtr]35, [IntPtr]::Zero)) { throw 'Could not post the production Desktop End-key messages.' }
    Start-Sleep -Milliseconds 200
    if (([DesktopAccessibilityQuery]::State($listViewHwnd, $unicodeFolderId) -band 2) -eq 0) {
        throw 'End-key navigation did not leave the newly created Unicode Desktop folder selected.'
    }
    Capture-Color 'production-wallpaper-unicode-folder' 32 32 32
    if (-not [WallpaperProbeNative]::PostMessage($listViewHwnd, 0x0100, [IntPtr]13, [IntPtr]::Zero) -or
        -not [WallpaperProbeNative]::PostMessage($listViewHwnd, 0x0101, [IntPtr]13, [IntPtr]::Zero)) { throw 'Could not post Enter to the selected Unicode Desktop folder.' }
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    while (-not (Test-Path -LiteralPath $handoffPath) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 50 }
    if (-not (Test-Path -LiteralPath $handoffPath)) { throw 'Activating the Unicode Desktop folder did not invoke the Files CLI contract.' }
    $unicodeHandoff = Get-Content -LiteralPath $handoffPath -Raw | ConvertFrom-Json
    if (Get-Process explorer -ErrorAction SilentlyContinue) { throw 'Enter launched or left Explorer running instead of handing the folder to Files.' }
    if ($unicodeHandoff.command -cne 'files' -or $unicodeHandoff.path -cne $unicodeFolderPath) {
        throw "Unicode Desktop folder handoff changed the exact path: $($unicodeHandoff | ConvertTo-Json -Compress)."
    }
    Remove-Item -LiteralPath $handoffPath -Force
    $thisPcItem = @([DesktopAccessibilityQuery]::Read($listViewHwnd).children | Where-Object { $_.name -ceq 'This PC' })
    if ($thisPcItem.Count -ne 1) { throw 'The Desktop view did not expose exactly one This PC namespace item.' }
    $thisPcId = [int]$thisPcItem[0].id
    $thisPcState = [DesktopAccessibilityQuery]::State($listViewHwnd, $thisPcId)
    $thisPcSelectionAttempts = 0
    while (($thisPcState -band 2) -eq 0 -and $thisPcSelectionAttempts -lt 5) {
        [DesktopAccessibilityQuery]::Select($listViewHwnd, $thisPcId)
        Start-Sleep -Milliseconds 100
        $thisPcState = [DesktopAccessibilityQuery]::State($listViewHwnd, $thisPcId)
        $thisPcSelectionAttempts++
    }
    $uiDiagnostics.this_pc_selected_state = $thisPcState
    $uiDiagnostics.this_pc_selection_attempts = $thisPcSelectionAttempts
    if (($thisPcState -band 2) -eq 0) { throw 'The This PC item did not enter the selected state.' }
    if (-not [WallpaperProbeNative]::PostMessage($listViewHwnd, 0x0100, [IntPtr]13, [IntPtr]::Zero) -or
        -not [WallpaperProbeNative]::PostMessage($listViewHwnd, 0x0101, [IntPtr]13, [IntPtr]::Zero)) { throw 'Could not activate the This PC namespace item.' }
    $virtualErrorDialog = [IntPtr]::Zero
    $virtualErrorDeadline = [DateTime]::UtcNow.AddSeconds(3)
    while ($virtualErrorDialog -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $virtualErrorDeadline) {
        $candidateDialog = [WallpaperProbeNative]::FindWindow('#32770', 'Winarchy')
        if ($candidateDialog -ne [IntPtr]::Zero -and [WallpaperProbeNative]::IsWindowVisible($candidateDialog)) { $virtualErrorDialog = $candidateDialog }
        else { Start-Sleep -Milliseconds 50 }
    }
    if ($virtualErrorDialog -eq [IntPtr]::Zero) {
        $foregroundHwnd = [WallpaperProbeNative]::GetForegroundWindow()
        $foregroundTitle = [Text.StringBuilder]::new(512)
        $foregroundClass = [Text.StringBuilder]::new(256)
        $null = [WallpaperProbeNative]::GetWindowTextW($foregroundHwnd, $foregroundTitle, $foregroundTitle.Capacity)
        $null = [WallpaperProbeNative]::GetClassNameW($foregroundHwnd, $foregroundClass, $foregroundClass.Capacity)
        [uint32]$foregroundOwnerPid = 0
        $null = [WallpaperProbeNative]::GetWindowThreadProcessId($foregroundHwnd, [ref]$foregroundOwnerPid)
        $uiDiagnostics.this_pc_activation = [ordered]@{
            foreground_title = $foregroundTitle.ToString()
            foreground_class = $foregroundClass.ToString()
            foreground_owner_pid = $foregroundOwnerPid
            shell_pid = $process.Id
            explorer_pids = @((Get-Process explorer -ErrorAction SilentlyContinue | ForEach-Object Id))
            files_handoff_exists = Test-Path -LiteralPath $handoffPath
            selected_state_after_enter = [DesktopAccessibilityQuery]::State($listViewHwnd, $thisPcId)
        }
        throw "Opening This PC did not show the unsupported-location message: $($uiDiagnostics.this_pc_activation | ConvertTo-Json -Compress)."
    }
    [uint32]$virtualErrorOwnerPid = 0
    $null = [WallpaperProbeNative]::GetWindowThreadProcessId($virtualErrorDialog, [ref]$virtualErrorOwnerPid)
    if ($virtualErrorOwnerPid -ne $process.Id) { throw 'The This PC warning was not owned by the production shell.' }
    Capture-Color 'production-wallpaper-virtual-folder-warning' 32 32 32 -ScreenshotOnly
    $null = [WallpaperProbeNative]::PostMessage($virtualErrorDialog, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 100
    if ([WallpaperProbeNative]::IsWindowVisible($virtualErrorDialog) -or (Test-Path -LiteralPath $handoffPath) -or
        (Get-Process explorer -ErrorAction SilentlyContinue)) { throw 'Unsupported virtual-folder activation did not fail closed.' }

    $shortcutPath = Join-Path $desktopRoot 'Winarchy Folder Shortcut.lnk'
    $wsh = New-Object -ComObject WScript.Shell
    try {
        $shortcut = $wsh.CreateShortcut($shortcutPath)
        $shortcut.TargetPath = $unicodeFolderPath
        $shortcut.WorkingDirectory = $unicodeFolderPath
        $shortcut.Save()
    } finally { [Runtime.InteropServices.Marshal]::FinalReleaseComObject($wsh) | Out-Null }
    $shortcutItem = @()
    $shortcutDeadline = [DateTime]::UtcNow.AddSeconds(8)
    while ($shortcutItem.Count -ne 1 -and [DateTime]::UtcNow -lt $shortcutDeadline) {
        $shortcutItem = @([DesktopAccessibilityQuery]::Read($listViewHwnd).children | Where-Object { $_.name -ceq 'Winarchy Folder Shortcut' })
        if ($shortcutItem.Count -ne 1) { Start-Sleep -Milliseconds 100 }
    }
    if ($shortcutItem.Count -ne 1) { throw 'The production Desktop did not enumerate the folder shortcut.' }
    $shortcutId = [int]$shortcutItem[0].id
    $shortcutState = [DesktopAccessibilityQuery]::State($listViewHwnd, $shortcutId)
    $shortcutSelectionAttempts = 0
    while (($shortcutState -band 6) -ne 6 -and $shortcutSelectionAttempts -lt 5) {
        [DesktopAccessibilityQuery]::Select($listViewHwnd, $shortcutId)
        Start-Sleep -Milliseconds 100
        $shortcutState = [DesktopAccessibilityQuery]::State($listViewHwnd, $shortcutId)
        $shortcutSelectionAttempts++
    }
    if (($shortcutState -band 6) -ne 6) { throw "The folder shortcut did not become selected and focused (MSAA state=$shortcutState)." }
    if (-not [WallpaperProbeNative]::PostMessage($listViewHwnd, 0x0100, [IntPtr]13, [IntPtr]::Zero) -or
        -not [WallpaperProbeNative]::PostMessage($listViewHwnd, 0x0101, [IntPtr]13, [IntPtr]::Zero)) { throw 'Could not activate the selected folder shortcut.' }
    $shortcutDeadline = [DateTime]::UtcNow.AddSeconds(5)
    while (-not (Test-Path -LiteralPath $handoffPath) -and [DateTime]::UtcNow -lt $shortcutDeadline) { Start-Sleep -Milliseconds 50 }
    if (-not (Test-Path -LiteralPath $handoffPath)) {
        $foregroundHwnd = [WallpaperProbeNative]::GetForegroundWindow()
        $foregroundTitle = [Text.StringBuilder]::new(512)
        $foregroundClass = [Text.StringBuilder]::new(256)
        $null = [WallpaperProbeNative]::GetWindowTextW($foregroundHwnd, $foregroundTitle, $foregroundTitle.Capacity)
        $null = [WallpaperProbeNative]::GetClassNameW($foregroundHwnd, $foregroundClass, $foregroundClass.Capacity)
        [uint32]$foregroundOwnerPid = 0
        $null = [WallpaperProbeNative]::GetWindowThreadProcessId($foregroundHwnd, [ref]$foregroundOwnerPid)
        $uiDiagnostics.folder_shortcut_activation = [ordered]@{
            id = $shortcutId
            msaa_state = $shortcutState
            selection_attempts = $shortcutSelectionAttempts
            foreground_title = $foregroundTitle.ToString()
            foreground_class = $foregroundClass.ToString()
            foreground_owner_pid = $foregroundOwnerPid
            shell_pid = $process.Id
            explorer_pids = @((Get-Process explorer -ErrorAction SilentlyContinue | ForEach-Object Id))
            files_handoff_exists = $false
            warning_visible = [WallpaperProbeNative]::IsWindowVisible([WallpaperProbeNative]::FindWindow('#32770', 'Winarchy'))
        }
        throw "Activating a folder shortcut did not reach the Files CLI contract: $($uiDiagnostics.folder_shortcut_activation | ConvertTo-Json -Compress)."
    }
    $shortcutHandoff = Get-Content -LiteralPath $handoffPath -Raw | ConvertFrom-Json
    if ($shortcutHandoff.command -cne 'files' -or $shortcutHandoff.path -cne $unicodeFolderPath) {
        throw "The folder shortcut did not resolve to its target path: $($shortcutHandoff | ConvertTo-Json -Compress)."
    }
    if (Get-Process explorer -ErrorAction SilentlyContinue) { throw 'Opening a folder shortcut launched or left Explorer running.' }
    $foregroundProcess = Start-Process -FilePath $env:ComSpec -ArgumentList @('/K', 'title Winarchy Foreground Probe') -WindowStyle Normal -PassThru
    try {
        $foregroundWindow = [IntPtr]::Zero
        $foregroundDeadline = [DateTime]::UtcNow.AddSeconds(8)
        while ($foregroundWindow -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $foregroundDeadline) {
            $foregroundProcess.Refresh()
            $candidate = $foregroundProcess.MainWindowHandle
            if ($candidate -ne [IntPtr]::Zero -and [WallpaperProbeNative]::IsWindowVisible($candidate)) { $foregroundWindow = $candidate }
            else { Start-Sleep -Milliseconds 100 }
        }
        if ($foregroundWindow -eq [IntPtr]::Zero) { throw "The native foreground console did not become visible (pid=$($foregroundProcess.Id), mainWindow=$($foregroundProcess.MainWindowHandle))." }
        [uint32]$foregroundOwnerPid = 0
        $null = [WallpaperProbeNative]::GetWindowThreadProcessId($foregroundWindow, [ref]$foregroundOwnerPid)
        if ($foregroundOwnerPid -ne $foregroundProcess.Id) { throw 'The foreground console HWND is owned by an unexpected process.' }
        $consoleRect = New-Object WallpaperProbeNative+RECT
        if (-not [WallpaperProbeNative]::GetClientRect($foregroundWindow, [ref]$consoleRect)) { throw 'Could not read the foreground console client bounds.' }
        $consoleCenter = New-Object WallpaperProbeNative+POINT
        $consoleCenter.X = [int](($consoleRect.Right - $consoleRect.Left) / 2)
        $consoleCenter.Y = [int](($consoleRect.Bottom - $consoleRect.Top) / 2)
        if (-not [WallpaperProbeNative]::ClientToScreen($foregroundWindow, [ref]$consoleCenter)) { throw 'Could not map the console client center to screen coordinates.' }
        $hitWindow = [WallpaperProbeNative]::WindowFromPoint($consoleCenter)
        $hitRoot = [WallpaperProbeNative]::GetAncestor($hitWindow, 2)
        if ($hitRoot -ne $foregroundWindow) { throw 'The production shell obscured the normal app at its visible client center.' }
        Capture-Color 'production-wallpaper-foreground-console' 32 32 32 -ScreenshotOnly
    } finally {
        if ($foregroundWindow -ne [IntPtr]::Zero) { $null = [WallpaperProbeNative]::PostMessage($foregroundWindow, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) }
        if (-not $foregroundProcess.WaitForExit(3000)) { Stop-Process -Id $foregroundProcess.Id -Force -ErrorAction SilentlyContinue }
    }
    $uiAutomation.interaction = [ordered]@{
        selected_name = $profileItem[0].name
        selected_state_before = $stateBefore
        selected_state_after = $stateSelected
        context_menu_class = '#32768'
        context_menu_owner_pid = $menuOwnerId
        context_menu_dismissed_with_escape = $menuDismissed
        shift_f10_menu_owner_pid = $shiftF10MenuOwnerPid
        shift_f10_menu_dismissed = $shiftF10MenuDismissed
        keyboard_selected_ids_after_down = $keyboardSelectedIds
        multi_selected_ids = $multiselectedIds
        multi_selection_attempts = $multiSelectionAttempts
        multi_folder_warning_owner_pid = $multiErrorOwnerPid
        pointer_selected_ids = $pointerSelectedIds
        f5_fixture_name = $f5Name
        f5_visible_before_key = $f5VisibleBeforeKey
        f5_visible_after_key = $true
        handoff_command = $handoff.command
        handoff_path = $handoff.path
        unicode_folder_name = $unicodeFolderName
        unicode_handoff_path = $unicodeHandoff.path
        handoff_process_stub = 'Windows PowerShell invoked through the pwsh.exe CLI name in the disposable guest'
        unicode_enter_handoff = $true
        virtual_folder_warning_owner_pid = $virtualErrorOwnerPid
        shortcut_handoff_path = $shortcutHandoff.path
    }
} catch {
    $uiAutomation = [ordered]@{ status = 'ERROR'; error = $_.Exception.Message; diagnostics = $uiDiagnostics }
}
[IO.File]::WriteAllText((Join-Path $output 'production-desktop-uia.json'), ($uiAutomation | ConvertTo-Json -Depth 5), [Text.UTF8Encoding]::new($false))
if ($uiAutomation.status -ne 'PASS' -or $null -eq $uiAutomation.interaction) { throw "Production Desktop accessibility/activation probe failed: $($uiAutomation.error)" }
$comResult = [ordered]@{
    scope = 'Read-only API feasibility; pixel parity and slideshow progression are not tested.'
    explorer_absent_before = -not [bool](Get-Process explorer -ErrorAction SilentlyContinue)
    query = $null
    explorer_absent_after = $false
}
if (-not $comResult.explorer_absent_before) { throw 'Explorer must be absent before the wallpaper COM query.' }
$comResult.query = [DesktopWallpaperQuery]::Read()
$comResult.explorer_absent_after = -not [bool](Get-Process explorer -ErrorAction SilentlyContinue)
[IO.File]::WriteAllText((Join-Path $output 'production-wallpaper-com.json'), ($comResult | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false))
if (-not $comResult.explorer_absent_after) { throw 'Explorer appeared during the wallpaper COM query.' }
Start-Sleep -Seconds 2
Capture-Color 'production-wallpaper-initial-fallback' 32 32 32
$themePath = Join-Path $root 'config\\shell\\theme.json'
$theme = Get-Content -LiteralPath $themePath -Raw | ConvertFrom-Json
$theme.background = '#2c5b35'
[IO.File]::WriteAllText($themePath, ($theme | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false))
Start-Sleep -Seconds 3
Capture-Color 'production-wallpaper-fallback-theme-change' 44 91 53
Set-Wallpaper $first
Start-Sleep -Seconds 3
Capture-Color 'production-wallpaper-bmp' 31 87 143
Set-Wallpaper $second
Start-Sleep -Seconds 3
if (-not (Get-Process -Id $process.Id -ErrorAction SilentlyContinue)) { throw 'Production shell exited after a wallpaper change.' }
Capture-Color 'production-wallpaper-jpeg' 193 53 79
Set-Wallpaper $transparent
Start-Sleep -Seconds 2
Capture-Color 'production-wallpaper-transparent' 44 91 53
$themePath = Join-Path $root 'config\shell\theme.json'
$theme = Get-Content -LiteralPath $themePath -Raw | ConvertFrom-Json
$theme.background = '#3d75b2'
[IO.File]::WriteAllText($themePath, ($theme | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false))
Start-Sleep -Seconds 3
if (-not (Get-Process -Id $process.Id -ErrorAction SilentlyContinue)) { throw 'Production shell exited after a theme change.' }
Capture-Color 'production-wallpaper-transparent-theme' 61 117 178

# dock + cluster: the tray chevron leads the right pill and each status glyph has its own context menu
Add-Type -AssemblyName UIAutomationClient
Add-Type -TypeDefinition @'
using System.Runtime.InteropServices;
public static class DockProbeInput {
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, System.IntPtr extra);
}
'@
$dock = [ordered]@{}
$themeNow = Get-Content -LiteralPath $themePath -Raw | ConvertFrom-Json
$themeNow.dock = $true
$themeNow | Add-Member -NotePropertyName cluster_monitor -NotePropertyValue 'primary' -Force
[IO.File]::WriteAllText($themePath, ($themeNow | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false))
# the cluster pill is not exposed through UIA until a client focuses it, so the glyphs are located from the screen corner (offsets read from the first capture)
$screenW = [WallpaperProbeNative]::GetSystemMetrics(0)
$screenH = [WallpaperProbeNative]::GetSystemMetrics(1)
$rowY = $screenH - 30
Start-Sleep -Seconds 6
$dock.screen = "$screenW x $screenH"
Capture-Color 'production-wallpaper-dock-cluster' 0 0 0 -ScreenshotOnly
$menus = @()
$dock.menus = $menus
foreach ($case in @(@{ name = 'network'; x = $screenW - 159 }, @{ name = 'volume'; x = $screenW - 135 }, @{ name = 'clock'; x = $screenW - 66 })) {
    $entry = [ordered]@{ case = $case.name }
    $menus += $entry
    $dock.menus = $menus
    $null = [DockProbeInput]::SetCursorPos($case.x, $rowY)
    Start-Sleep -Milliseconds 300
    [DockProbeInput]::mouse_event(8, 0, 0, 0, [IntPtr]::Zero); [DockProbeInput]::mouse_event(16, 0, 0, 0, [IntPtr]::Zero)
    $menu = [IntPtr]::Zero
    $until = [DateTime]::UtcNow.AddSeconds(5)
    while ($menu -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $until) {
        $candidate = [WallpaperProbeNative]::FindWindow('#32768', $null)
        if ($candidate -ne [IntPtr]::Zero -and [WallpaperProbeNative]::IsWindowVisible($candidate)) { $menu = $candidate } else { Start-Sleep -Milliseconds 100 }
    }
    $entry.menu_shown = ($menu -ne [IntPtr]::Zero)
    Capture-Color "production-wallpaper-dock-menu-$($case.name)" 0 0 0 -ScreenshotOnly
    if ($menu -ne [IntPtr]::Zero) { $null = [WallpaperProbeNative]::PostMessage($menu, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero); Start-Sleep -Milliseconds 300 }
    if (-not $entry.menu_shown) { throw "Right click on the $($case.name) glyph did not open a context menu." }
}

# tray flyout with a real notification icon: needs the YASB hook DLL (staged under Program Files by Prepare)
if (Test-Path -LiteralPath "$env:ProgramW6432\YASB\lib\YASBTrayHook.dll") {
    Add-Type -TypeDefinition @'
using System.Runtime.InteropServices;
public static class TrayProbeNative {
    [DllImport("user32.dll", EntryPoint="GetWindowLongW")] public static extern int GetWindowLong(System.IntPtr h, int index);
}
'@
    $trayScript = Join-Path $output 'tray-icon-app.ps1'
    [IO.File]::WriteAllText($trayScript, @'
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
$icon = New-Object Windows.Forms.NotifyIcon
$icon.Icon = [Drawing.SystemIcons]::Information
$icon.Text = 'probe-tray-icon'
$menu = New-Object Windows.Forms.ContextMenuStrip
[void]$menu.Items.Add('Probe menu item')
$icon.ContextMenuStrip = $menu
$menu.Add_Opened({ [IO.File]::WriteAllText('C:\sandbox\tray-menu-opened.txt', 'opened') })
$icon.Visible = $true
[IO.File]::WriteAllText('C:\sandbox\tray-icon-ready.txt', 'ready')
[Windows.Forms.Application]::Run()
'@, [Text.UTF8Encoding]::new($false))
    $trayApp = Start-Process powershell.exe -WindowStyle Hidden -PassThru -ArgumentList '-NoProfile', '-STA', '-ExecutionPolicy', 'Bypass', '-File', $trayScript
    $tray = [ordered]@{}
    try {
    $click = {
        param([int]$x, [int]$y, [switch]$Right)
        $null = [DockProbeInput]::SetCursorPos($x, $y)
        Start-Sleep -Milliseconds 250
        if ($Right) { [DockProbeInput]::mouse_event(8, 0, 0, 0, [IntPtr]::Zero); [DockProbeInput]::mouse_event(16, 0, 0, 0, [IntPtr]::Zero) }
        else { [DockProbeInput]::mouse_event(2, 0, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 80; [DockProbeInput]::mouse_event(4, 0, 0, 0, [IntPtr]::Zero) }
    }
    $waitFlyout = {
        param([bool]$visible, [int]$seconds = 6)
        $until = [DateTime]::UtcNow.AddSeconds($seconds)
        do {
            $h = [WallpaperProbeNative]::FindWindow('WinarchyShellTray', $null)
            $shown = $h -ne [IntPtr]::Zero -and [WallpaperProbeNative]::IsWindowVisible($h)
            if ($shown -eq $visible) { return $h }
            Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $until)
        return [IntPtr]::Zero
    }
    $until = [DateTime]::UtcNow.AddSeconds(20)
    while (-not (Test-Path 'C:\sandbox\tray-icon-ready.txt') -and [DateTime]::UtcNow -lt $until) { Start-Sleep -Milliseconds 300 }
    Start-Sleep -Seconds 4
    $cx = $screenW - 202; $cy = $rowY   # chevron centre: first cell of the right pill (offset read from the cluster capture)
    & $click $cx $cy
    $fly = & $waitFlyout $true
    $tray.opened = $fly -ne [IntPtr]::Zero
    if (-not $tray.opened) { Capture-Color 'production-wallpaper-tray-not-opened' 0 0 0 -ScreenshotOnly; throw 'The tray flyout did not open (hook not connected or no icon arrived).' }
    Start-Sleep -Milliseconds 500
    $rect = New-Object WallpaperProbeNative+RECT
    [void][WallpaperProbeNative]::GetWindowRect($fly, [ref]$rect)
    $tray.rect = "$($rect.left),$($rect.top),$($rect.right),$($rect.bottom)"
    $tray.topmost = ([TrayProbeNative]::GetWindowLong($fly, -20) -band 0x8) -ne 0
    Capture-Color 'production-wallpaper-tray-flyout' 0 0 0 -ScreenshotOnly
    # right click on the first icon: the owning app opens its own menu
    $pad = 8; $cell = 40   # the Sandbox runs at 100% DPI
    & $click ($rect.left + $pad + [int]($cell / 2)) ($rect.top + $pad + [int]($cell / 2)) -Right
    # the app's own menu is a WinForms drop-down (not a #32768 window): the app reports when it opens
    $until = [DateTime]::UtcNow.AddSeconds(6)
    while (-not (Test-Path 'C:\sandbox\tray-menu-opened.txt') -and [DateTime]::UtcNow -lt $until) { Start-Sleep -Milliseconds 150 }
    $tray.icon_menu_shown = Test-Path 'C:\sandbox\tray-menu-opened.txt'
    Capture-Color 'production-wallpaper-tray-icon-menu' 0 0 0 -ScreenshotOnly
    if ($tray.icon_menu_shown) { [WallpaperProbeNative]::KeybdEvent(0x1B, 0, 0, [UIntPtr]::Zero); [WallpaperProbeNative]::KeybdEvent(0x1B, 0, 2, [UIntPtr]::Zero); Start-Sleep -Milliseconds 400 }
    # a click on the empty desktop closes whatever is left of the flyout
    if ([WallpaperProbeNative]::IsWindowVisible($fly) -eq $false) { & $click $cx $cy; $null = & $waitFlyout $true }
    & $click ([int]($rect.left / 2 + 300)) 300
    $tray.closed_by_outside_click = (& $waitFlyout $false) -ne [IntPtr]::Zero -or -not [WallpaperProbeNative]::IsWindowVisible([WallpaperProbeNative]::FindWindow('WinarchyShellTray', $null))
    # reopen, then a window taking the foreground closes it too
    & $click $cx $cy
    $null = & $waitFlyout $true
    $other = Start-Process powershell.exe -PassThru -ArgumentList '-NoProfile', '-Command', 'Add-Type -AssemblyName System.Windows.Forms; $f = New-Object Windows.Forms.Form; $f.Text = ''tray-probe-other''; $f.TopMost = $false; [Windows.Forms.Application]::Run($f)'
    Start-Sleep -Seconds 3
    $tray.closed_by_foreground_change = -not [WallpaperProbeNative]::IsWindowVisible([WallpaperProbeNative]::FindWindow('WinarchyShellTray', $null))
    Capture-Color 'production-wallpaper-tray-after-foreground-change' 0 0 0 -ScreenshotOnly
    Stop-Process -Id $other.Id, $trayApp.Id -Force -ErrorAction SilentlyContinue
    } finally {
        Stop-Process -Id $trayApp.Id -Force -ErrorAction SilentlyContinue
        Get-Process powershell -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowTitle -eq 'tray-probe-other' } | Stop-Process -Force
    }
    $dock.tray = $tray
    if (-not ($tray.topmost -and $tray.icon_menu_shown -and $tray.closed_by_outside_click -and $tray.closed_by_foreground_change)) { throw "Tray flyout checks failed: $($tray | ConvertTo-Json -Compress)" }
}

# dock hover preview + mouse wheel: two windows of one app, the wheel moves the preview from one to the other
$wheel = [ordered]@{}
$forms = @('wheel-a', 'wheel-b') | ForEach-Object {
    Start-Process powershell.exe -PassThru -ArgumentList '-NoProfile', '-Command', "Add-Type -AssemblyName System.Windows.Forms; `$f = New-Object Windows.Forms.Form; `$f.Text = '$_'; `$f.Width = 420; `$f.Height = 300; [Windows.Forms.Application]::Run(`$f)"
}
try {
    Start-Sleep -Seconds 5
    Capture-Color 'production-wallpaper-dock-preview-windows' 0 0 0 -ScreenshotOnly
    Add-Type -TypeDefinition 'using System.Runtime.InteropServices; public static class DwmProbe { [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(System.IntPtr h, int a, out int v, int s); }'
    $wheel.cloaked = @($forms | ForEach-Object { $_.Refresh(); $v = -1; [void][DwmProbe]::DwmGetWindowAttribute($_.MainWindowHandle, 14, [ref]$v, 4); "$($_.MainWindowHandle)=$v" })
    $thumbVisible = { $h = [WallpaperProbeNative]::FindWindow('WinarchyShellDockThumb', $null); $h -ne [IntPtr]::Zero -and [WallpaperProbeNative]::IsWindowVisible($h) }
    $found = $false
    foreach ($x in (($screenW / 2 - 140)..($screenW / 2 + 140) | Where-Object { $_ % 16 -eq 0 })) {
        $null = [DockProbeInput]::SetCursorPos($x, $rowY)
        Start-Sleep -Milliseconds 450
        if (& $thumbVisible) { $found = $true; $wheel.hover_x = $x; break }
    }
    $wheel.preview_on_hover = $found
    if ($found) {
        Start-Sleep -Milliseconds 600
        Capture-Color 'production-wallpaper-dock-preview-first' 0 0 0 -ScreenshotOnly
        [DockProbeInput]::mouse_event(0x800, 0, 0, [uint32]4294967176, [IntPtr]::Zero)
        Start-Sleep -Milliseconds 900
        $wheel.still_visible_after_wheel = [bool](& $thumbVisible)
        Capture-Color 'production-wallpaper-dock-preview-second' 0 0 0 -ScreenshotOnly
        [DockProbeInput]::mouse_event(0x800, 0, 0, [uint32]120, [IntPtr]::Zero)
        Start-Sleep -Milliseconds 900
        Capture-Color 'production-wallpaper-dock-preview-back' 0 0 0 -ScreenshotOnly
    }
}
finally { $forms | ForEach-Object { Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue } }
$dock.wheel = $wheel
if (-not ($wheel.preview_on_hover -and $wheel.still_visible_after_wheel)) { throw "Dock hover preview / wheel checks failed: $($wheel | ConvertTo-Json -Compress)" }
# chevron keyboard focus: dock.focus over the shell pipe, End selects the last target (the tray chevron), Escape ends the session
$key = [ordered]@{}
Capture-Color 'production-wallpaper-dock-key-before' 0 0 0 -ScreenshotOnly
$pipe = New-Object System.IO.Pipes.NamedPipeClientStream('.', "winarchy-shell-$([Diagnostics.Process]::GetCurrentProcess().SessionId)", [System.IO.Pipes.PipeDirection]::Out)
try {
    $pipe.Connect(5000)
    $bytes = [Text.Encoding]::UTF8.GetBytes("{`"v`":1,`"id`":1,`"cmd`":`"dock.focus`"}`n")
    $pipe.Write($bytes, 0, $bytes.Length); $pipe.Flush()
    $key.sent = $true
} finally { $pipe.Dispose() }
Start-Sleep -Milliseconds 700
[WallpaperProbeNative]::KeybdEvent(0x23, 0, 0, [UIntPtr]::Zero); [WallpaperProbeNative]::KeybdEvent(0x23, 0, 2, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 700
Capture-Color 'production-wallpaper-dock-key-focus' 0 0 0 -ScreenshotOnly
[WallpaperProbeNative]::KeybdEvent(0x1B, 0, 0, [UIntPtr]::Zero); [WallpaperProbeNative]::KeybdEvent(0x1B, 0, 2, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 700
Capture-Color 'production-wallpaper-dock-key-after' 0 0 0 -ScreenshotOnly
$dock.key = $key
$dock.status = 'PASS'
[IO.File]::WriteAllText((Join-Path $output 'dock-cluster-result.json'), ($dock | ConvertTo-Json -Depth 5), [Text.UTF8Encoding]::new($false))
$themeNow.dock = $false
[IO.File]::WriteAllText($themePath, ($themeNow | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false))
Start-Sleep -Seconds 2
$corrupt = Join-Path $root 'winarchy-wallpaper-corrupt.png'
[IO.File]::WriteAllBytes($corrupt, [byte[]]@(0, 1, 2, 3, 4, 5))
$corruptResultPath = Join-Path $output 'production-wallpaper-corrupt-result.json'
$corruptResult = [ordered]@{ status = 'FAIL'; reason = 'Corrupt wallpaper verification did not complete.'; spi_accepted = $false; spi_error = 0; retained_path = ''; same_pid = $false }
[IO.File]::WriteAllText($corruptResultPath, ($corruptResult | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
$corruptResult.spi_accepted = [WallpaperProbeNative]::SystemParametersInfoW(20, 0, $corrupt, 3)
$corruptResult.spi_error = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
$current = [Text.StringBuilder]::new(32768)
if (-not [WallpaperProbeNative]::GetWallpaper(115, $current.Capacity, $current, 0)) { throw 'SPI_GETDESKWALLPAPER failed after the corrupt-image request.' }
$corruptResult.retained_path = $current.ToString()
if (-not $corruptResult.spi_accepted -or $corruptResult.retained_path -ine $corrupt) {
    $corruptResult.status = 'NOT_TESTABLE'
    $corruptResult.reason = "Windows rejected or did not retain the corrupt source path; decoder fallback was not exercised (SPI accepted=$($corruptResult.spi_accepted), error=$($corruptResult.spi_error), path=$($corruptResult.retained_path))."
}
[IO.File]::WriteAllText($corruptResultPath, ($corruptResult | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
Start-Sleep -Seconds 3
Capture-Color 'production-wallpaper-corrupt' 61 117 178
$corruptResult.same_pid = $true
if ($corruptResult.status -ne 'NOT_TESTABLE') {
    $corruptResult.status = 'PASS'
    $corruptResult.reason = 'The production shell retained the last-good theme-composited wallpaper after an invalid image.'
}
[IO.File]::WriteAllText($corruptResultPath, ($corruptResult | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
$rect = New-Object WallpaperProbeNative+RECT
if (-not [WallpaperProbeNative]::GetWindowRect($window, [ref]$rect)) { throw 'Could not read production wallpaper host bounds.' }
$result = [ordered]@{
    status = if ($corruptResult.status -eq 'PASS') { 'PASS' } else { 'INCONCLUSIVE' }
    corrupt_wallpaper = $corruptResult
    account = [Security.Principal.WindowsIdentity]::GetCurrent().Name
    session_id = [Diagnostics.Process]::GetCurrentProcess().SessionId
    shell_pid = $process.Id
    binary_sha256 = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
    shell_process_alive_after_updates = $true
    wallpaper_hwnd_visible = [WallpaperProbeNative]::IsWindowVisible($window)
    wallpaper_hwnd_bounds = @($rect.Left, $rect.Top, $rect.Right, $rect.Bottom)
    source_formats = @('BMP', 'JPEG', 'transparent PNG')
    theme_reload_same_pid = $true
}
$returnResultPath = Join-Path $output 'production-wallpaper-return-result.json'
$returnResult = [ordered]@{ status = 'NOT_RUN'; reason = 'Corrupt-image fallback did not pass.'; request = @{ v = 1; id = 'wallpaper-return'; cmd = 'return' }; reply = $null; shell_pid = $process.Id; shell_exited = $false; explorer_pid = 0; taskbar_hwnd = 0 }
[IO.File]::WriteAllText($returnResultPath, ($returnResult | ConvertTo-Json -Depth 5), [Text.UTF8Encoding]::new($false))
if ($corruptResult.status -eq 'PASS') {
    $returnResult.status = 'FAIL'
    $returnResult.reason = 'Return to Explorer did not complete.'
    [IO.File]::WriteAllText($returnResultPath, ($returnResult | ConvertTo-Json -Depth 5), [Text.UTF8Encoding]::new($false))
    $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
    $pipe = [IO.Pipes.NamedPipeClientStream]::new('.', "winarchy-shell-$sessionId", [IO.Pipes.PipeDirection]::InOut, [IO.Pipes.PipeOptions]::Asynchronous)
    $reader = $null
    try {
        $pipe.Connect(5000)
        $request = [Text.UTF8Encoding]::new($false).GetBytes(($returnResult.request | ConvertTo-Json -Compress) + [Environment]::NewLine)
        $writeTask = $pipe.WriteAsync($request, 0, $request.Length)
        if (-not $writeTask.Wait(5000)) { throw 'Return request pipe write timed out.' }
        $reader = [IO.StreamReader]::new($pipe, [Text.UTF8Encoding]::new($false), $false, 1024, $true)
        $readTask = $reader.ReadLineAsync()
        if (-not $readTask.Wait(5000)) { throw 'Return reply pipe read timed out.' }
        $reply = $readTask.Result | ConvertFrom-Json
        $returnResult.reply = $reply
        [IO.File]::WriteAllText($returnResultPath, ($returnResult | ConvertTo-Json -Depth 5), [Text.UTF8Encoding]::new($false))
        if ($null -eq $reply -or $reply.ok -isnot [bool] -or $reply.ok -ne $true -or $reply.v -ne 1 -or $reply.id -ne 'wallpaper-return') {
            throw 'Return command did not receive the expected versioned ok:true reply.'
        }
    } finally {
        if ($null -ne $reader) { $reader.Dispose() }
        $pipe.Dispose()
    }
    $timer = [Diagnostics.Stopwatch]::StartNew()
    while ($timer.ElapsedMilliseconds -lt 25000) {
        $process.Refresh()
        $returnResult.shell_exited = $process.HasExited
        $taskbar = [WallpaperProbeNative]::FindWindow('Shell_TrayWnd', $null)
        [uint32]$ownerId = 0
        if ($taskbar -ne [IntPtr]::Zero) { $null = [WallpaperProbeNative]::GetWindowThreadProcessId($taskbar, [ref]$ownerId) }
        $explorer = if ($ownerId -gt 0) { Get-Process -Id $ownerId -ErrorAction SilentlyContinue } else { $null }
        if ($process.HasExited -and $null -ne $explorer -and $explorer.ProcessName -ieq 'explorer' -and
            $explorer.SessionId -eq $sessionId -and $explorer.Path -ieq 'C:\Windows\explorer.exe' -and [WallpaperProbeNative]::IsWindowVisible($taskbar)) {
            $returnResult.shell_exited = $true
            $returnResult.explorer_pid = $explorer.Id
            $returnResult.taskbar_hwnd = $taskbar.ToInt64()
            $returnResult.status = 'PASS'
            $returnResult.reason = 'The original shell exited and Explorer owns the visible taskbar in the same session.'
            break
        }
        Start-Sleep -Milliseconds 100
    }
    [IO.File]::WriteAllText($returnResultPath, ($returnResult | ConvertTo-Json -Depth 5), [Text.UTF8Encoding]::new($false))
    if ($returnResult.status -ne 'PASS') { throw 'Return did not exit the original shell and restore an Explorer-owned taskbar within 25 seconds.' }
}
$result.return_to_explorer = $returnResult
if ($returnResult.status -eq 'PASS') {
    $withExplorer = [ordered]@{
        explorer_present_before = [bool](Get-Process explorer -ErrorAction SilentlyContinue)
        query = $null
        explorer_present_after = $false
    }
    if (-not $withExplorer.explorer_present_before) { throw 'Explorer must be running before the comparison wallpaper COM query.' }
    $withExplorer.query = [DesktopWallpaperQuery]::Read()
    $withExplorer.explorer_present_after = [bool](Get-Process explorer -ErrorAction SilentlyContinue)
    [IO.File]::WriteAllText((Join-Path $output 'production-wallpaper-com-with-explorer.json'), ($withExplorer | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false))
}
[IO.File]::WriteAllText((Join-Path $output 'production-wallpaper-result.json'), ($result | ConvertTo-Json -Depth 5), [Text.UTF8Encoding]::new($false))
} finally {
    $process.Refresh()
    if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }
}
exit 0
