# End-to-end checks of the release build through UI Automation and real input.
# Usage: pwsh -File tools/uitest.ps1 [-Only views,instance,bin,commands,thumbs,handler]
param([string[]]$Only = @('views', 'instance', 'bin', 'commands', 'thumbs', 'handler'))
$ErrorActionPreference = 'Stop'
$Only = $Only -split ','
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing, System.Windows.Forms
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class Win { [DllImport("user32.dll")] public static extern bool SetProcessDPIAware(); [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h); [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r); [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y); [DllImport("user32.dll")] public static extern void mouse_event(int f, int x, int y, int d, int e); public struct R { public int L, T, Rt, B; } }
"@
[void][Win]::SetProcessDPIAware()
$AE = [Windows.Automation.AutomationElement]
$Exe = Join-Path $PSScriptRoot '..\target\release\winarchy-files.exe'
$Work = Join-Path $env:TEMP "winarchy-uitest-$PID"
$script:Results = @()

function Check($name, [bool]$ok, $detail = '') {
    $script:Results += [pscustomobject]@{ Check = $name; Result = $(if ($ok) { 'PASS' } else { 'FAIL' }); Detail = $detail }
}
function Find($name, $timeout = 4) {
    $end = (Get-Date).AddSeconds($timeout)
    do {
        $found = $script:Root.FindFirst('Descendants', (New-Object Windows.Automation.PropertyCondition($AE::NameProperty, $name)))
        if ($found) { return $found }
        Start-Sleep -Milliseconds 200
    } while ((Get-Date) -lt $end)
}
function Press($element) {
    $pattern = $null
    if ($element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) { $pattern.Invoke() }
    elseif ($element.TryGetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern, [ref]$pattern)) { $pattern.Select() }
    else { throw "cannot press '$($element.Current.Name)'" }
    Start-Sleep -Milliseconds 700
}
function Click($element) {
    $rect = $element.Current.BoundingRectangle
    [void][Win]::SetCursorPos([int]($rect.X + $rect.Width / 2), [int]($rect.Y + $rect.Height / 2))
    Start-Sleep -Milliseconds 300
    [Win]::mouse_event(2, 0, 0, 0, 0); Start-Sleep -Milliseconds 80; [Win]::mouse_event(4, 0, 0, 0, 0)
    Start-Sleep -Milliseconds 700
}
function Menu($label) { Press (Find 'Más'); Press (Find $label) }
function Start-App($folder, $settings) {
    $env:WINARCHY_FILES_THEME = (Resolve-Path (Join-Path $PSScriptRoot '..\..\config\files\theme.json')).Path
    $env:WINARCHY_FILES_LANG = 'es'
    $env:WINARCHY_FILES_SETTINGS = $settings
    $script:App = Start-Process (Resolve-Path $Exe) -ArgumentList $folder -PassThru
    Start-Sleep 5
    $script:Root = $AE::FromHandle($script:App.MainWindowHandle)
    [void][Win]::SetForegroundWindow($script:App.MainWindowHandle)
}
function Stop-App { Get-Process winarchy-files -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep -Milliseconds 500 }
function Shot($name) {
    $r = New-Object Win+R; [void][Win]::GetWindowRect($script:App.MainWindowHandle, [ref]$r)
    $bitmap = New-Object Drawing.Bitmap ($r.Rt - $r.L), ($r.B - $r.T)
    [Drawing.Graphics]::FromImage($bitmap).CopyFromScreen($r.L, $r.T, 0, 0, $bitmap.Size)
    $bitmap.Save((Join-Path $env:TEMP $name))
}
function Status {
    $texts = $script:Root.FindAll('Descendants', (New-Object Windows.Automation.PropertyCondition($AE::ControlTypeProperty, [Windows.Automation.ControlType]::Text)))
    ($texts | ForEach-Object { $_.Current.Name } | Where-Object { $_ -match 'elementos|archivos|SHA-256' } | Select-Object -Last 1)
}
function New-Settings($layout = 'details') {
    $file = Join-Path $Work "settings-$layout.json"
    "{`"layout`":`"$layout`",`"hide_pane`":true,`"tabs`":[],`"pinned`":[],`"favorites`":[],`"tags`":{},`"sidebar_width`":220.0}" | Set-Content $file
    $file
}

New-Item -ItemType Directory -Force $Work | Out-Null
$komorebi = Get-Command komorebic -ErrorAction SilentlyContinue
try {
    if ($komorebi) { & komorebic border disable 2>$null }
    Stop-App

    if ('views' -in $Only) {
        Start-App 'C:\Windows' (New-Settings)
        Press (Find 'Este equipo')
        Check 'This PC lists the drives' ($null -ne (Find 'Disco local (C:)')) 'row "Disco local (C:)"'
        $target = Join-Path $Work "recent-$PID.txt"; Set-Content $target 'x'
        $link = Join-Path "$env:APPDATA\Microsoft\Windows\Recent" "recent-$PID.lnk"
        $shortcut = (New-Object -ComObject WScript.Shell).CreateShortcut($link); $shortcut.TargetPath = $target; $shortcut.Save()
        try {
            Press (Find 'Este equipo')
            Press (Find 'Recientes')
            Check 'Recent lists the target of a recent shortcut' ($null -ne (Find "recent-$PID.txt" 4)) "recent-$PID.txt"
        } finally { Remove-Item $link -ErrorAction SilentlyContinue }
        Press (Find 'Favoritos')
        Check 'Favorites view still opens' ((Status) -match 'elementos') (Status)
        Stop-App
    }

    if ('instance' -in $Only) {
        $a = Join-Path $Work 'alpha'; $b = Join-Path $Work 'beta'
        New-Item -ItemType Directory -Force $a, $b | Out-Null
        Start-App $a (New-Settings)
        $second = Start-Process (Resolve-Path $Exe) -ArgumentList $b -PassThru
        Start-Sleep 3
        Check 'second launch exits' $second.HasExited ''
        Check 'one window process' (@(Get-Process winarchy-files).Count -eq 1) ''
        Check 'second folder is a tab' ($null -ne (Find 'beta' 3)) 'tab "beta"'
        Stop-App
    }

    if ('bin' -in $Only) {
        $name = "winarchy-bin-$PID.txt"
        $file = Join-Path $Work $name
        Set-Content $file 'bin test'
        Add-Type -AssemblyName Microsoft.VisualBasic
        [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile($file, 'OnlyErrorDialogs', 'SendToRecycleBin')
        Check 'file went to the bin' (-not (Test-Path $file)) ''
        Start-App $Work (New-Settings)
        Press (Find 'Papelera de reciclaje')
        $row = Find $name 4
        Check 'bin lists the deleted file under its original name' ($null -ne $row) $name
        if ($row) {
            Click $row
            Menu 'Restaurar'
            Start-Sleep 1
            Check 'restore puts it back' (Test-Path $file) $file
            Remove-Item $file -ErrorAction SilentlyContinue
        }
        Stop-App
    }

    if ('commands' -in $Only) {
        $data = Join-Path $Work 'data'; New-Item -ItemType Directory -Force $data | Out-Null
        $sample = Join-Path $data 'sample.txt'; Set-Content $sample 'hello' -NoNewline
        Start-App $data (New-Settings)
        $row = Find 'sample.txt'
        Click $row
        Menu 'Copiar SHA-256'
        Start-Sleep 2
        $expected = (Get-FileHash $sample -Algorithm SHA256).Hash.ToLower()
        Check 'SHA-256 reaches the clipboard' ((Get-Clipboard) -eq $expected) $expected
        [Windows.Forms.SendKeys]::SendWait('{ESC}')
        Menu 'Tamaño de la carpeta'
        Start-Sleep 2
        Check 'folder size reports files' ((Status) -match 'archivos') (Status)
        Stop-App
    }

    if ('thumbs' -in $Only) {
        Start-App 'C:\Windows\Web\Wallpaper\Windows' (New-Settings 'grid')
        Start-Sleep 5
        Shot 'uitest-grid.png'
        Check 'thumbnails are cached' ((Get-ChildItem "$env:LOCALAPPDATA\winarchy\thumbs" -Filter *.png -ErrorAction SilentlyContinue).Count -gt 0) 'see %TEMP%\uitest-grid.png'
        Stop-App
    }

    if ('handler' -in $Only) {
        $keys = 'Folder', 'Directory', 'Drive' | ForEach-Object { "HKCU:\Software\Classes\$_\shell" }
        $before = $keys | ForEach-Object { Test-Path $_ }
        try {
            & $Exe --register-handler
            Start-Sleep 1
            & $Exe --handler-status
            $started = Start-Process 'cmd.exe' -ArgumentList '/c', 'start', '', $Work -PassThru -WindowStyle Hidden
            Start-Sleep 5
            Check 'start <folder> opens Files' (@(Get-Process winarchy-files -ErrorAction SilentlyContinue).Count -ge 1) ''
        } finally {
            Stop-App
            & $Exe --unregister-handler
        }
        $after = $keys | ForEach-Object { Test-Path $_ }
        Check 'unregister restores the registry' (($before -join ',') -eq ($after -join ',')) "$($before -join ',') -> $($after -join ',')"
    }
} finally {
    Stop-App
    if ($komorebi) { & komorebic border enable 2>$null }
    Remove-Item (Join-Path $Work '*') -Recurse -ErrorAction SilentlyContinue
}
$script:Results | Format-Table -AutoSize
if ($script:Results.Result -contains 'FAIL') { exit 1 }
