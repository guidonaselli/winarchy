# Runs inside Windows Sandbox (Windows PowerShell 5.1) as the default user. Changes the live view state of the real
# komorebi and records what survives a rewrite of the watched config: unchanged, atomic replace, and with the live values in it.
$ErrorActionPreference = 'Stop'
$out = 'C:\sandbox'
$log = Join-Path $out 'reload-state.log'
function Log($m) { "{0:HH:mm:ss} {1}" -f (Get-Date), $m | Tee-Object -FilePath $log -Append }
trap { Log "ERROR: $_ at $($_.InvocationInfo.PositionMessage)"; exit 1 }
$bin = 'C:\komorebi-bin'
$komorebic = "$bin\komorebic.exe"
$env:KOMOREBI_CONFIG_HOME = 'C:\komorebi-cfg'
New-Item -ItemType Directory -Path $env:KOMOREBI_CONFIG_HOME -Force | Out-Null
$configPath = "$env:KOMOREBI_CONFIG_HOME\komorebi.json"

function Publish-Config([int]$padding, $ws, [switch]$Atomic, [string]$Behaviour) {
    if (-not $ws) { $ws = @{ name = '1'; layout = 'BSP' }, @{ name = '2'; layout = 'BSP' } }
    $config = @{ default_container_padding = $padding; monitors = @(@{ workspaces = @($ws) }) }
    if ($Behaviour) { $config.window_container_behaviour = $Behaviour }
    $json = $config | ConvertTo-Json -Depth 6
    $target = if ($Atomic) { "$configPath.tmp" } else { $configPath }
    [IO.File]::WriteAllText($target, $json, [Text.UTF8Encoding]::new($false))
    if ($Atomic) { Move-Item -LiteralPath $target -Destination $configPath -Force }
}

function Get-LiveWorkspaces {
    foreach ($w in (Get-View).workspaces | Select-Object -First 2) {
        $c = @{ name = $w.name; layout = $w.layout.Default }
        if ($w.tile -eq $false) { $c.tile = $false }
        foreach ($k in 'layout_flip', 'container_padding', 'workspace_padding') { if ($null -ne $w[$k]) { $c[$k] = $w[$k] } }
        $c
    }
}

function Get-View {
    $s = (& $komorebic state) -join "`n" | ConvertFrom-Json
    [ordered]@{
        is_paused  = $s.is_paused
        new_window = $s.new_window_behaviour
        workspaces = @($s.monitors.elements[0].workspaces.elements | ForEach-Object {
                [ordered]@{
                    name = $_.name; tile = $_.tile; layout = $_.layout; layout_flip = $_.layout_flip
                    layout_options = $_.layout_options; container_padding = $_.container_padding
                    workspace_padding = $_.workspace_padding; layer = $_.layer; resize = $_.resize_dimensions
                    containers = @($_.containers.elements | ForEach-Object { @($_.windows.elements.hwnd) -join '+' })
                    floating = @($_.floating_windows.elements | ForEach-Object { $_.hwnd })
                }
            })
    }
}

function New-TestWindow([string]$title) {
    $p = Start-Process powershell.exe -PassThru -ArgumentList '-NoProfile', '-Command', "Add-Type -AssemblyName System.Windows.Forms; `$f = New-Object Windows.Forms.Form; `$f.Text = '$title'; [Windows.Forms.Application]::Run(`$f)"
    foreach ($i in 1..50) { $p.Refresh(); if ($p.MainWindowHandle -ne 0) { return }; Start-Sleep -Milliseconds 200 }
    throw "window $title did not appear"
}

function Set-LiveState {
    # workspace 0: tiled, layout/flip/padding/resize changed by hand. workspace 1: floating.
    foreach ($c in @('focus-workspace 0', 'change-layout columns', 'flip-layout horizontal', 'workspace-padding 0 0 31', 'container-padding 0 0 17',
            'focus left', 'resize-axis horizontal increase', 'workspace-tiling 0 1 disable', 'toggle-window-container-behaviour')) {
        $a = $c -split ' '
        & $komorebic @a | Out-Null
        if ($LASTEXITCODE -ne 0) { Log "command failed: $c ($LASTEXITCODE)" }
        Start-Sleep -Milliseconds 400
    }
}

function Set-ExtraState {
    # BSP sin tocar el layout (Columns no admite resize); la capa flotante pide una ventana flotante
    foreach ($c in 'focus-workspace 0', 'focus left', 'resize-axis horizontal increase', 'resize-axis vertical increase', 'focus right', 'toggle-float', 'toggle-workspace-layer') {
        $a = $c -split ' '
        & $komorebic @a | Out-Null
        if ($LASTEXITCODE -ne 0) { Log "command failed: $c ($LASTEXITCODE)" }
        Start-Sleep -Milliseconds 500
    }
}

$results = [ordered]@{}
foreach ($path in 'unchanged', 'atomic', 'preserve', 'paused', 'extras') {
    Publish-Config 10
    $komorebi = Start-Process "$bin\komorebi.exe" -WindowStyle Hidden -PassThru -ArgumentList '--config', $configPath -RedirectStandardOutput "$out\komorebi-$path.out.txt" -RedirectStandardError "$out\komorebi-$path.err.txt"
    foreach ($i in 1..40) { if ((& $komorebic state 2>$null) -match '"monitors"') { break }; Start-Sleep -Milliseconds 500 }
    Start-Sleep 3
    # una flotante en el workspace 2, que queda oculto
    New-TestWindow 'reload-hidden'; Start-Sleep 1
    & $komorebic move-to-workspace 1 | Out-Null; Start-Sleep 1
    foreach ($t in 'reload-a', 'reload-b', 'reload-c') { New-TestWindow $t; Start-Sleep 1 }
    if ($path -eq 'extras') { Set-ExtraState } else {
        Set-LiveState
        & $komorebic session-float-rule | Out-Null
    }
    if ($path -eq 'paused') { & $komorebic toggle-pause | Out-Null }
    # salvo en paused, se mide sin pausa: pausado, komorebi descarta la recarga
    Start-Sleep 2
    $before = Get-View
    $globalBefore = (& $komorebic global-state) -join "`n"
    switch ($path) {
        'unchanged' { Publish-Config 10 }
        'atomic' { Publish-Config 12 -Atomic }
        'preserve' { Publish-Config 12 @(Get-LiveWorkspaces) -Behaviour (Get-View).new_window }
        'paused' { Publish-Config 12 -Atomic }
        'extras' { Publish-Config 12 @(Get-LiveWorkspaces) }
    }
    Start-Sleep 6
    $results[$path] = [ordered]@{ before = $before; after = Get-View; global_before = $globalBefore; global_after = (& $komorebic global-state) -join "`n" }
    if ($path -eq 'atomic') {
        # el watcher tiene que seguir vivo después de que le reemplacen el archivo
        Set-LiveState; Start-Sleep 2
        $second = Get-View
        Publish-Config 14 -Atomic; Start-Sleep 6
        $results['atomic_again'] = [ordered]@{ before = $second; after = Get-View }
    }
    Log "$path captured"
    & $komorebic stop | Out-Null
    Get-Process komorebi -ErrorAction SilentlyContinue | Wait-Process -Timeout 20 -ErrorAction SilentlyContinue
    Get-Process powershell | Where-Object { $_.MainWindowTitle -like 'reload-*' } | Stop-Process -Force
}
$results | ConvertTo-Json -Depth 12 | Set-Content (Join-Path $out 'reload-state.json')
Log 'DONE'
