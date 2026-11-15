# Bar.ps1 — geometría de la barra que el usuario decide, no el theme

function Get-WinarchyBarPositionPath { Join-Path (Get-WinarchyStateDir) 'bar-position' }

function Get-WinarchyBarPosition {
    $path = Get-WinarchyBarPositionPath
    if (Test-Path $path) {
        $value = (Get-Content $path -Raw).Trim()
        if ($value -in @('top', 'bottom')) { return $value }
    }
    'top'
}

function Get-WinarchyBarTransparentPath { Join-Path (Get-WinarchyStateDir) 'bar-transparent' }

function Test-WinarchyBarTransparent { Test-Path (Get-WinarchyBarTransparentPath) }

function Get-WinarchyBarBackground {
    param([Parameter(Mandatory)][string]$Hex)
    if (-not (Test-WinarchyBarTransparent)) { return $Hex }
    $h = $Hex.TrimStart('#')
    $parts = 0, 2, 4 | ForEach-Object { [Convert]::ToInt32($h.Substring($_, 2), 16) }
    "rgba($($parts -join ', '), 0.72)"
}

function Set-WinarchyBarTransparent {
    param([Parameter(Mandatory)][ValidateSet('on', 'off', 'toggle')][string]$Mode)
    $path = Get-WinarchyBarTransparentPath
    $wanted = switch ($Mode) {
        'on' { $true }
        'off' { $false }
        'toggle' { -not (Test-WinarchyBarTransparent) }
    }
    if ($wanted) { Set-Content -Path $path -Value '' -NoNewline }
    elseif (Test-Path $path) { Remove-Item $path -Force }

    $theme = Get-WinarchyCurrentTheme
    if ($theme) { Set-WinarchyTheme -Name $theme }
    else { Write-WinarchyOk "Bar transparency: $(if ($wanted) { 'on' } else { 'off' })" }
}

function Set-WinarchyBarPosition {
    param([Parameter(Mandatory)][ValidateSet('top', 'bottom')][string]$Position)
    Set-Content -Path (Get-WinarchyBarPositionPath) -Value $Position -Encoding UTF8 -NoNewline
    $theme = Get-WinarchyCurrentTheme
    if ($theme) { Set-WinarchyTheme -Name $theme }
    else { Write-WinarchyOk "Bar position: $Position (applies on the next `winarchy theme set`)." }
}

function Get-WinarchyBarDockPath { Join-Path (Get-WinarchyStateDir) 'bar-dock' }

function Test-WinarchyBarDock {
    <# El dock es de la shell nativa: con YASB no hay dock aunque el interruptor esté prendido. #>
    (Get-WinarchyActiveBar) -eq 'native' -and (Test-Path (Get-WinarchyBarDockPath))
}

function Get-WinarchyDockWorkAreaOffset {
    <# Lo que komorebi reserva para el dock, en px. Espeja BAR_DIP + MARGIN_DIP (+2 de la franja de la taskbar nativa, solo abajo) de shell/src/dock.rs. #>
    $dpi = (Get-ItemProperty 'HKCU:\Control Panel\Desktop\WindowMetrics' -ErrorAction SilentlyContinue).AppliedDPI
    if (-not $dpi) { $dpi = 96 }
    $dockEdge = if ((Get-WinarchyBarPosition) -eq 'top') { 'bottom' } else { 'top' }
    [pscustomobject]@{
        Top    = if ($dockEdge -eq 'top') { [int][math]::Round(54 * $dpi / 96) } else { 0 }
        Bottom = if ($dockEdge -eq 'bottom') { [int][math]::Round(56 * $dpi / 96) } else { 2 }
    }
}

function Set-WinarchyBarDock {
    param([Parameter(Mandatory)][ValidateSet('on', 'off')][string]$Mode)
    $path = Get-WinarchyBarDockPath
    if ($Mode -eq 'on') { Set-WinarchyFileAtomic -Path $path -Value 'on' }
    elseif (Test-Path $path) { Remove-Item $path -Force }
    if ($Mode -eq 'on' -and (Get-WinarchyActiveBar) -ne 'native') { Write-WinarchyWarn 'The dock needs the native shell: run `winarchy bar use native`.' }
    $theme = Get-WinarchyCurrentTheme
    if ($theme) { Set-WinarchyTheme -Name $theme }
    else { Write-WinarchyOk "Dock: $Mode (applies on the next `winarchy theme set`)." }
}

function Set-WinarchyDockMonitorOffset {
    <# Con varios monitores el dock vive solo en el primario: el área reservada pasa del global a ese monitor. Sin komorebi o con un monitor queda el global del token. #>
    param([Parameter(Mandatory)]$Config, [Parameter(Mandatory)]$State)
    $elements = @($State.monitors.elements)
    if ($elements.Count -lt 2) { return }
    $primary = 0..($elements.Count - 1) | Where-Object { $elements[$_].size.left -eq 0 -and $elements[$_].size.top -eq 0 } | Select-Object -First 1
    if ($null -eq $primary -or -not $Config.monitors -or $primary -ge @($Config.monitors).Count) { return }
    $o = Get-WinarchyDockWorkAreaOffset
    $Config.global_work_area_offset = [pscustomobject]@{ left = 0; top = 0; right = 0; bottom = 2 }
    $Config.monitors[$primary] | Add-Member -NotePropertyName work_area_offset -NotePropertyValue ([pscustomobject]@{ left = 0; top = $o.Top; right = 0; bottom = $o.Bottom }) -Force
}
