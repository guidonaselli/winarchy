param([Parameter(Mandatory)][string]$MetadataPath, [Parameter(Mandatory)][string]$ResultPath, [switch]$ScrollOnly, [switch]$RecreateHost, [switch]$RecreateWindow)
$ErrorActionPreference = 'Stop'
$report = [ordered]@{ passed = $false }
try {
    Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
    if (-not $ScrollOnly) { Add-Type -AssemblyName System.Drawing.Common }
    Add-Type @'
using System;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public sealed class ProbeDesktop : SafeHandleZeroOrMinusOneIsInvalid {
    public ProbeDesktop() : base(true) {}
    protected override bool ReleaseHandle() { return ProbeNative.CloseDesktop(handle); }
}
public static class ProbeNative {
    [DllImport("user32.dll", SetLastError=true)] public static extern ProbeDesktop OpenInputDesktop(uint flags, bool inherit, uint access);
    [DllImport("user32.dll", SetLastError=true)] public static extern bool CloseDesktop(IntPtr desktop);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hwnd);
    [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
    [DllImport("user32.dll", SetLastError=true)] public static extern IntPtr GetThreadDesktop(uint thread);
    [DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)] public static extern bool GetUserObjectInformation(IntPtr handle, int index, System.Text.StringBuilder value, uint bytes, out uint needed);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
}
'@
    $meta = Get-Content -LiteralPath $MetadataPath -Raw | ConvertFrom-Json
    $report.expectedPid = $meta.pid
    $report.scrollOnly = [bool]$ScrollOnly
    $report.destroyedMenuWindow = $false
    $RecreateHost = $RecreateHost -or $RecreateWindow
    $report.recreatedHost = [bool]$RecreateHost
    if ($RecreateHost -and -not $ScrollOnly) { throw 'Host lifetime probe requires the isolated desktop mode' }
    if ($ScrollOnly) {
        $desktop = [System.Text.StringBuilder]::new(256)
        [uint32]$needed = 0
        if (-not [ProbeNative]::GetUserObjectInformation([ProbeNative]::GetThreadDesktop([ProbeNative]::GetCurrentThreadId()), 2, $desktop, 512, [ref]$needed)) { throw 'Cannot read UIA client desktop' }
        $report.desktop = $desktop.ToString()
        $inputDesktop = [ProbeNative]::OpenInputDesktop(0, $false, 1)
        try {
            if ($inputDesktop.IsInvalid) { throw 'Cannot open input desktop' }
            $inputName = [System.Text.StringBuilder]::new(256)
            if (-not [ProbeNative]::GetUserObjectInformation($inputDesktop.DangerousGetHandle(), 2, $inputName, 512, [ref]$needed)) { throw 'Cannot read input desktop name' }
            $report.inputDesktop = $inputName.ToString()
            if ($report.desktop -ieq $report.inputDesktop) { throw 'UIA client must never run on the input desktop' }
            if ($report.desktop -cne $meta.desktop) { throw 'UIA client must inherit the isolated desktop' }
        } finally { $inputDesktop.Dispose() }
    }
    $report.windows = @(foreach ($role in @('menu', 'first', 'second')) {
        $handle = [IntPtr]$meta.$role
        [uint32]$ownerPid = 0
        [void][ProbeNative]::GetWindowThreadProcessId($handle, [ref]$ownerPid)
        @{ role = $role; hwnd = $handle.ToInt64(); isWindow = [ProbeNative]::IsWindow($handle); pid = $ownerPid }
    })
    foreach ($window in $report.windows) {
        if (-not $window.isWindow -or $window.pid -ne $meta.pid) { throw "Probe HWND does not belong to the test process: $($window.role)" }
    }
    $root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$meta.menu)
    if ($root.Current.ControlType -ne [System.Windows.Automation.ControlType]::Menu) { throw 'Root must expose Menu' }
    if ($root.Current.Name -cne $meta.rootName) { throw 'Unexpected Menu name' }
    $all = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
    $report.nodes = @($all | ForEach-Object { @{ name = $_.Current.Name; role = $_.Current.ControlType.ProgrammaticName } })
    $rows = @($all | Where-Object { $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::MenuItem })
    if ($rows.Count -ne 2) { throw "Expected exactly 2 MenuItems, got $($rows.Count)" }
    if ($rows[0].Current.Name -cne $meta.rowNames[0] -or $rows[1].Current.Name -cne $meta.rowNames[1]) { throw 'Unexpected MenuItem names' }
    $buttons = @($all | Where-Object { $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button })
    if ($buttons.Count -ne 2) { throw 'Expected exactly 2 close Buttons' }
    $bounds = $root.Current.BoundingRectangle
    if ($bounds.Width -le 0 -or $bounds.Height -le 0) { throw 'Menu bounds must be nonempty' }
    $before = $rows[1].Current.BoundingRectangle
    $report.initialBounds = [ordered]@{}
    foreach ($entry in @(@{ name = 'root'; rect = $bounds }, @{ name = 'rowA'; rect = $rows[0].Current.BoundingRectangle }, @{ name = 'rowB'; rect = $before })) {
        $report.initialBounds[$entry.name] = @{ left = $entry.rect.Left; top = $entry.rect.Top; width = $entry.rect.Width; height = $entry.rect.Height }
    }
    if (-not $before.IsEmpty) { throw 'Offscreen row must have empty bounds before ScrollItem' }
    $scroll = $rows[1].GetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern)
    $scroll.ScrollIntoView()
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        Start-Sleep -Milliseconds 50
        $after = $rows[1].Current.BoundingRectangle
    } until (($after.Width -gt 0 -and $after.Height -gt 0) -or [DateTime]::UtcNow -ge $deadline)
    if ($after.Width -le 0 -or $after.Height -le 0) {
        $report.scrollTimeout = [ordered]@{}
        try {
            $foreground = [ProbeNative]::GetForegroundWindow()
            [uint32]$menuPid = 0
            [void][ProbeNative]::GetWindowThreadProcessId([IntPtr]$meta.menu, [ref]$menuPid)
            $report.scrollTimeout.menuIsWindow = [ProbeNative]::IsWindow([IntPtr]$meta.menu)
            $report.scrollTimeout.menuPid = $menuPid
            $report.scrollTimeout.foregroundHwnd = $foreground.ToInt64()
            $report.scrollTimeout.menuIsForeground = $foreground -eq [IntPtr]$meta.menu
            try { $report.scrollTimeout.originalRuntimeId = @($rows[1].GetRuntimeId()) }
            catch { $report.scrollTimeout.originalRuntimeIdError = $_.Exception.Message }
            $report.scrollTimeout.originalBounds = @{ isEmpty = $after.IsEmpty; left = $after.Left; top = $after.Top; width = $after.Width; height = $after.Height }
            if ($report.scrollTimeout.menuIsWindow -and $menuPid -eq $meta.pid) {
                $freshRoot = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$meta.menu)
                $condition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, $meta.rowNames[1])
                $freshRow = $freshRoot.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
                $report.scrollTimeout.freshRowFound = $null -ne $freshRow
                if ($null -ne $freshRow) {
                    $freshBounds = $freshRow.Current.BoundingRectangle
                    $freshId = @($freshRow.GetRuntimeId())
                    $report.scrollTimeout.freshRuntimeId = $freshId
                    $report.scrollTimeout.sameRuntimeId = ($freshId -join ',') -ceq ($report.scrollTimeout.originalRuntimeId -join ',')
                    $report.scrollTimeout.freshBounds = @{ isEmpty = $freshBounds.IsEmpty; left = $freshBounds.Left; top = $freshBounds.Top; width = $freshBounds.Width; height = $freshBounds.Height }
                    $report.scrollTimeout.sameBounds = $freshBounds.Equals($after)
                }
            }
        } catch { $report.scrollTimeout.diagnosticError = $_.Exception.ToString() }
        throw 'ScrollItem did not move the second row into view'
    }
    if ($after.Top -lt $bounds.Top -or $after.Bottom -gt $bounds.Bottom -or $after.Left -lt $bounds.Left -or $after.Right -gt $bounds.Right) { throw 'Scrolled row is outside the menu' }
    $report.scroll = @{ beforeWidth = $before.Width; beforeHeight = $before.Height; afterWidth = $after.Width; afterHeight = $after.Height }
    if ($RecreateHost) {
        $close = @($buttons | Where-Object { $_.Current.Name -ceq $meta.closeName })
        if ($close.Count -ne 1) { throw 'Expected the uniquely named close Button for probe B' }
        $staleInvoke = $close[0].GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
        $request = [System.IO.Path]::ChangeExtension($MetadataPath, 'recreate')
        $ready = [System.IO.Path]::ChangeExtension($MetadataPath, 'ready')
        Set-Content -LiteralPath $request -Value 'recreate'
        $deadline = [DateTime]::UtcNow.AddSeconds(5)
        while (-not (Test-Path -LiteralPath $ready) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 20 }
        if (-not (Test-Path -LiteralPath $ready)) { throw 'Host recreation did not complete' }
        $replacement = Get-Content -LiteralPath $ready -Raw | ConvertFrom-Json
        if ([bool]$replacement.destroyedMenuWindow -ne [bool]$RecreateWindow) { throw 'Unexpected menu window lifetime' }
        $report.destroyedMenuWindow = [bool]$replacement.destroyedMenuWindow
        $report.originalMenuHwnd = $meta.menu
        $report.replacementMenuHwnd = $replacement.menu
        [uint32]$replacementPid = 0
        [void][ProbeNative]::GetWindowThreadProcessId([IntPtr]$replacement.menu, [ref]$replacementPid)
        if (-not [ProbeNative]::IsWindow([IntPtr]$replacement.menu) -or $replacementPid -ne $meta.pid) { throw 'Replacement menu HWND does not belong to the test process' }
        try {
            $staleInvoke.Invoke()
            $report.staleProviderRejected = $false
        } catch {
            $errorCause = $_.Exception
            while ($null -ne $errorCause.InnerException) { $errorCause = $errorCause.InnerException }
            if ($errorCause -isnot [System.Windows.Automation.ElementNotAvailableException]) { throw }
            $report.staleProviderRejected = $true
        }
        Set-Content -LiteralPath ([System.IO.Path]::ChangeExtension($MetadataPath, 'check')) -Value 'check'
        $checked = [System.IO.Path]::ChangeExtension($MetadataPath, 'checked')
        $deadline = [DateTime]::UtcNow.AddSeconds(5)
        while (-not (Test-Path -LiteralPath $checked) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 20 }
        if (-not (Test-Path -LiteralPath $checked)) { throw 'Host did not drain actions from the retained provider' }
        if (-not [ProbeNative]::IsWindow([IntPtr]$meta.first) -or -not [ProbeNative]::IsWindow([IntPtr]$meta.second)) { throw 'Stale provider closed a probe window after host recreation' }
        $report.staleProviderLeftBothWindowsAlive = $true
        if (-not [ProbeNative]::IsWindow([IntPtr]$replacement.menu)) { throw 'Stale provider dismissed the replacement menu' }
        $report.staleProviderLeftMenuAlive = $true
        $freshRoot = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$replacement.menu)
        $condition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, $meta.closeName)
        $freshClose = $freshRoot.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
        if ($null -eq $freshClose) { throw 'Recreated host did not expose probe B close Button' }
        $freshInvoke = $freshClose.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
        $freshInvoke.Invoke()
        $deadline = [DateTime]::UtcNow.AddSeconds(5)
        while ([ProbeNative]::IsWindow([IntPtr]$meta.second) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 20 }
        if ([ProbeNative]::IsWindow([IntPtr]$meta.second)) { throw 'Fresh provider did not close probe B' }
        if (-not [ProbeNative]::IsWindow([IntPtr]$meta.first)) { throw 'Fresh provider also closed probe A' }
        $report.freshProviderClosedSecondWindow = $true
    }
    if (-not $ScrollOnly) {
        $bitmap = [System.Drawing.Bitmap]::new([int]$bounds.Width, [int]$bounds.Height)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.CopyFromScreen([int]$bounds.Left, [int]$bounds.Top, 0, 0, $bitmap.Size)
            $bitmap.Save((Join-Path (Split-Path $ResultPath) 'dock-menu-uia.png'))
        } finally { $graphics.Dispose(); $bitmap.Dispose() }
        $close = @($buttons | Where-Object { $_.Current.Name -ceq $meta.closeName })
        if ($close.Count -ne 1) { throw 'Expected the uniquely named close Button for probe B' }
        if (-not [ProbeNative]::IsWindow([IntPtr]$meta.first) -or -not [ProbeNative]::IsWindow([IntPtr]$meta.second)) { throw 'Both owned windows must exist before Invoke' }
        if (-not $close[0].Current.IsKeyboardFocusable) { throw 'Close Button must be keyboard focusable' }
        $close[0].SetFocus()
        $deadline = [DateTime]::UtcNow.AddSeconds(5)
        while (-not $close[0].Current.HasKeyboardFocus -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 50 }
        if (-not $close[0].Current.HasKeyboardFocus) { throw 'Close Button did not acquire keyboard focus' }
        $report.closeFocused = $true
        $invoke = $close[0].GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
        $invoke.Invoke()
        $deadline = [DateTime]::UtcNow.AddSeconds(5)
        while ([ProbeNative]::IsWindow([IntPtr]$meta.second) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 50 }
        if ([ProbeNative]::IsWindow([IntPtr]$meta.second)) { throw 'Invoke did not close probe B' }
        if (-not [ProbeNative]::IsWindow([IntPtr]$meta.first)) { throw 'Invoke also closed probe A' }
    }
    $report.passed = $true
} catch {
    $report.error = $_.Exception.ToString()
    try {
        if ($null -ne $meta) {
            $foreground = [ProbeNative]::GetForegroundWindow()
            $report.failureState = @{
                menuIsWindow = [ProbeNative]::IsWindow([IntPtr]$meta.menu)
                foregroundHwnd = $foreground.ToInt64()
                menuIsForeground = $foreground -eq [IntPtr]$meta.menu
            }
        }
    } catch { $report.nativeSnapshotError = $_.Exception.Message }
    try {
        if ($null -ne $rows -and $rows.Count -gt 1) {
            $rect = $rows[1].Current.BoundingRectangle
            $report.failureRowBounds = @{ isEmpty = $rect.IsEmpty; left = $rect.Left; top = $rect.Top; width = $rect.Width; height = $rect.Height }
        }
    } catch { $report.rowSnapshotError = $_.Exception.Message }
}
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $ResultPath
if (-not $report.passed) { exit 1 }
