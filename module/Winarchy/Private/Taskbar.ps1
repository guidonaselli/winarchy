# Taskbar.ps1 — auto-hide de la taskbar nativa de Windows (StuckRects3).

function Test-WinarchyTaskbarAutoHide {
    $val = (Get-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StuckRects3' -Name Settings -ErrorAction SilentlyContinue).Settings
    [bool]($val -and ($val[8] -band 0x01))
}

function Set-WinarchyTaskbarAutoHide {
    <# Devuelve $true si cambió el estado; reinicia Explorer solo en ese caso. #>
    param([Parameter(Mandatory)][bool]$Enabled)
    $stuck = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StuckRects3'
    $val = (Get-ItemProperty -Path $stuck -Name Settings).Settings
    $flags = if ($Enabled) { $val[8] -bor 0x01 } else { $val[8] -band (-bnot 0x01) }
    if ($flags -eq $val[8]) { return $false }    $null = Backup-WinarchyRegistryKey -Key 'HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StuckRects3' -Label 'taskbar'
    $val[8] = $flags
    Set-ItemProperty -Path $stuck -Name Settings -Value $val
    Stop-Process -Name explorer -Force
    $true
}

function Restore-WinarchyNativeTaskbar {
    <# Una shell matada a la fuerza no devuelve la taskbar que ocultó: se la muestra desde acá. #>
    if (-not ('Winarchy.TaskbarNative' -as [type])) {
        Add-Type -Namespace Winarchy -Name TaskbarNative -MemberDefinition '[DllImport("user32.dll")] public static extern IntPtr FindWindowEx(IntPtr p, IntPtr a, string c, string t); [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h); [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);'
    }
    foreach ($class in 'Shell_TrayWnd', 'Shell_SecondaryTrayWnd') {
        $h = [IntPtr]::Zero
        while (($h = [Winarchy.TaskbarNative]::FindWindowEx([IntPtr]::Zero, $h, $class, $null)) -ne [IntPtr]::Zero) {
            if (-not [Winarchy.TaskbarNative]::IsWindowVisible($h)) { $null = [Winarchy.TaskbarNative]::ShowWindow($h, 8) }
        }
    }
}

function Set-WinarchyTaskbarHidden {
    <# hide = auto-hide + la shell oculta la ventana (sin bandeja de íconos); autohide y show devuelven la nativa. #>
    param([Parameter(Mandatory)][ValidateSet('hide', 'autohide', 'show')][string]$Mode)
    $path = Get-WinarchyTaskbarHiddenPath
    if ($Mode -eq 'hide') { Set-WinarchyFileAtomic -Path $path -Value 'on' }
    elseif (Test-Path $path) { Remove-Item $path -Force }
    if ($Mode -ne 'hide') { Restore-WinarchyNativeTaskbar }
    if ($Mode -eq 'hide' -and -not (Test-WinarchyBarDock)) { Write-WinarchyWarn 'Hidden taskbar takes effect with the dock on: `winarchy bar dock on`.' }
    $null = Set-WinarchyTaskbarAutoHide -Enabled ($Mode -ne 'show')
    if (Test-WinarchyBarDock) {
        $theme = Get-WinarchyCurrentTheme
        if ($theme) { Set-WinarchyTheme -Name $theme }
    }
}
