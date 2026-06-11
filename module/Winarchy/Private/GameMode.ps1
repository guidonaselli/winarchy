# GameMode.ps1 — modo juego: pausa tiling + suspende hotkeys (vía flag que vigila AHK)

function Get-WinarchyGameModeFlag { Join-Path (Get-WinarchyStateDir) 'game-mode.flag' }
function Get-WinarchyPauseMarker  { Join-Path (Get-WinarchyStateDir) 'komorebi-paused-by-gamemode' }

function Enable-WinarchyGameMode {
    $flag = Get-WinarchyGameModeFlag
    if (Test-Path $flag) { Write-WinarchyInfo 'Game-mode ya estaba activo.'; return }
    Set-Content -Path $flag -Value (Get-Date -Format 'o') -Encoding UTF8 -NoNewline
    if ((Test-WinarchyProcess 'komorebi') -and -not (Test-Path (Get-WinarchyPauseMarker))) {
        komorebic toggle-pause 2>$null | Out-Null
        New-Item -ItemType File -Path (Get-WinarchyPauseMarker) -Force | Out-Null
    }
    Write-WinarchyOk 'Game-mode ON: tiling pausado, hotkeys SUPER suspendidos (AHK detecta el flag en <=1 s).'
}

function Disable-WinarchyGameMode {
    $flag = Get-WinarchyGameModeFlag
    if (-not (Test-Path $flag)) { Write-WinarchyInfo 'Game-mode ya estaba inactivo.'; return }
    Remove-Item $flag -Force
    if (Test-Path (Get-WinarchyPauseMarker)) {
        if (Test-WinarchyProcess 'komorebi') { komorebic toggle-pause 2>$null | Out-Null }
        Remove-Item (Get-WinarchyPauseMarker) -Force
    }
    Write-WinarchyOk 'Game-mode OFF: tiling y hotkeys restaurados.'
}

function Get-WinarchyGameModeStatus {
    $flag = Test-Path (Get-WinarchyGameModeFlag)
    $games = Get-WinarchyGames
    [pscustomobject]@{
        Active     = $flag
        GamesCount = $games.Count
        Games      = $games
    } | Format-List
    Write-WinarchyInfo 'Nota: AHK además suspende hotkeys automáticamente con juegos de games.toml o fullscreen-exclusive.'
}

function Get-WinarchyGames {
    $gamesToml = Join-Path (Get-WinarchyRoot) 'games.toml'
    if (-not (Test-Path $gamesToml)) { return @() }
    $data = Import-WinarchyToml -Path $gamesToml
    $list = @()
    if ($data.ContainsKey('games')) {
        $list = @($data['games'] | ForEach-Object { $_['exe'] } | Where-Object { $_ -and $_ -notlike '*`**' })
    }
    , $list
}

function Update-WinarchyKomorebiRules {
    <# Re-renderiza komorebi.json (con las ignore-rules nuevas) y recarga komorebi. #>
    $current = Get-WinarchyCurrentTheme
    if (-not $current) {
        Write-WinarchyWarn 'No hay theme activo todavía; las reglas se aplicarán con el primer `winarchy theme set`.'
        return
    }
    $root = Get-WinarchyRoot
    $themeDir = Join-Path (Get-WinarchyThemesDir) $current
    $theme = Import-WinarchyToml -Path (Join-Path $themeDir 'theme.toml')
    $ctx = Build-WinarchyThemeContext -Theme $theme -ThemeDir $themeDir
    $rendered = Invoke-WinarchyTemplate -TemplatePath (Join-Path $root 'templates\komorebi.json.tpl') -Context $ctx
    $rendered | ConvertFrom-Json | Out-Null
    Set-Content -Path (Join-Path $root 'config\komorebi\komorebi.json') -Value $rendered -Encoding UTF8 -NoNewline
    if (Test-WinarchyProcess 'komorebi') { komorebic reload-configuration 2>$null | Out-Null }
}

function Add-WinarchyGame {
    param([Parameter(Mandatory)][string]$Exe)
    if ($Exe -notlike '*.exe') { $Exe = "$Exe.exe" }
    $gamesToml = Join-Path (Get-WinarchyRoot) 'games.toml'
    $existing = Get-WinarchyGames
    if ($existing -contains $Exe) { Write-WinarchyInfo "'$Exe' ya estaba en games.toml."; return }
    Add-Content -Path $gamesToml -Value "`n[[games]]`nexe = `"$Exe`"" -Encoding UTF8
    Update-WinarchyKomorebiRules
    Write-WinarchyOk "'$Exe' agregado: excluido del tiling y con hotkeys suspendidos en foreground."
}

function Remove-WinarchyGame {
    param([Parameter(Mandatory)][string]$Exe)
    if ($Exe -notlike '*.exe') { $Exe = "$Exe.exe" }
    $gamesToml = Join-Path (Get-WinarchyRoot) 'games.toml'
    if (-not (Test-Path $gamesToml)) { Write-WinarchyWarn 'games.toml no existe.'; return }
    $content = Get-Content $gamesToml -Raw -Encoding UTF8
    $pattern = "(?ms)^\[\[games\]\]\s*\r?\nexe\s*=\s*`"$([regex]::Escape($Exe))`"\s*(\r?\n)?"
    $updated = [regex]::Replace($content, $pattern, '')
    if ($updated -eq $content) { Write-WinarchyWarn "'$Exe' no estaba en games.toml."; return }
    Set-Content -Path $gamesToml -Value $updated -Encoding UTF8 -NoNewline
    Update-WinarchyKomorebiRules
    Write-WinarchyOk "'$Exe' eliminado de games.toml."
}
