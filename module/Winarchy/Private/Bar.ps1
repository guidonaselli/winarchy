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

function Set-WinarchyBarPosition {
    param([Parameter(Mandatory)][ValidateSet('top', 'bottom')][string]$Position)
    Set-Content -Path (Get-WinarchyBarPositionPath) -Value $Position -Encoding UTF8 -NoNewline
    $theme = Get-WinarchyCurrentTheme
    if ($theme) { Set-WinarchyTheme -Name $theme }
    else { Write-WinarchyOk "Bar position: $Position (applies on the next `winarchy theme set`)." }
}
