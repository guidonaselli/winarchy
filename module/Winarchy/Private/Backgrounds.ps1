# Backgrounds.ps1 — elección del fondo dentro de los que trae el theme activo

$script:BackgroundExtensions = @('.jpg', '.jpeg', '.png', '.bmp')

function Get-WinarchyBackgroundPrefsPath { Join-Path (Get-WinarchyStateDir) 'backgrounds.toml' }

function Get-WinarchyBackgrounds {
    <# Fondos que trae un theme, en orden alfabético. Vacío = el theme no trae. #>
    param([Parameter(Mandatory)][string]$Name)
    $dir = Join-Path (Get-WinarchyThemesDir) "$Name\backgrounds"
    if (-not (Test-Path $dir)) { return @() }
    @(Get-ChildItem -Path $dir -File |
        Where-Object { $script:BackgroundExtensions -contains $_.Extension.ToLower() } |
        Sort-Object Name |
        ForEach-Object { $_.Name })
}

function Get-WinarchyBackground {
    <# El fondo elegido para el theme, o el primero. $null si el theme no trae fondos. #>
    param([Parameter(Mandatory)][string]$Name)
    $available = @(Get-WinarchyBackgrounds -Name $Name)
    if ($available.Count -eq 0) { return $null }

    $path = Get-WinarchyBackgroundPrefsPath
    if (Test-Path $path) {
        $chosen = (Import-WinarchyToml -Path $path)[$Name]
        if ($chosen -and $available -contains $chosen) { return $chosen }
    }
    $available[0]
}

function Set-WinarchyBackground {
    <# Fija el fondo del theme y lo aplica. Sin -File cicla al siguiente. #>
    param([string]$File, [switch]$Next)
    $theme = Get-WinarchyCurrentTheme
    if (-not $theme) { throw 'No theme applied yet; run `winarchy theme set <name>` first.' }

    $available = @(Get-WinarchyBackgrounds -Name $theme)
    if ($available.Count -eq 0) { Write-WinarchyWarn "Theme '$theme' ships no backgrounds."; return }

    if ($Next) {
        $current = Get-WinarchyBackground -Name $theme
        $File = $available[(([array]::IndexOf($available, $current) + 1) % $available.Count)]
    }
    elseif ($available -notcontains $File) {
        throw "Theme '$theme' has no background '$File'. Available: $($available -join ', ')"
    }

    $prefs = if (Test-Path (Get-WinarchyBackgroundPrefsPath)) { Import-WinarchyToml -Path (Get-WinarchyBackgroundPrefsPath) } else { @{} }
    $prefs[$theme] = $File
    Export-WinarchyBackgroundPrefs -Pref $prefs

    Set-WinarchyWallpaper -ThemeDir (Join-Path (Get-WinarchyThemesDir) $theme)
    Write-WinarchyOk "Background: $File"
}

function Export-WinarchyBackgroundPrefs {
    param([Parameter(Mandatory)][hashtable]$Pref)
    $lines = @('# Fondo elegido por theme. Lo escribe `winarchy background`.')
    $lines += ($Pref.Keys | Sort-Object | ForEach-Object { "$_ = `"$($Pref[$_])`"" })
    Set-Content -Path (Get-WinarchyBackgroundPrefsPath) -Value ($lines -join "`r`n") -Encoding UTF8 -NoNewline
}

function Get-WinarchyBackgroundStatus {
    <# Fondos del theme activo, marcando el aplicado. #>
    $theme = Get-WinarchyCurrentTheme
    if (-not $theme) { Write-WinarchyWarn 'No theme applied yet.'; return }
    $available = @(Get-WinarchyBackgrounds -Name $theme)
    if ($available.Count -eq 0) { Write-WinarchyWarn "Theme '$theme' ships no backgrounds."; return }
    $current = Get-WinarchyBackground -Name $theme
    foreach ($f in $available) { Write-Host "  $(if ($f -eq $current) { '*' } else { ' ' }) $f" }
}
