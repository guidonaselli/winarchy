# GameMode.ps1 — modo juego: flota solo la ventana del juego (NO pausa el tiling).
# El game-mode.flag lo vigila AHK para el estado/indicador; el accent-watcher queda inerte.

function Get-WinarchyGameModeFlag { Join-Path (Get-WinarchyStateDir) 'game-mode.flag' }
function Get-WinarchyFloatedMarker { Join-Path (Get-WinarchyStateDir) 'game-mode-floated' }

function Enable-WinarchyGameMode {
    $flag = Get-WinarchyGameModeFlag
    if (Test-Path $flag) { Write-WinarchyInfo 'Game-mode was already active.'; return }
    Set-Content -Path $flag -Value (Get-Date -Format 'o') -Encoding UTF8 -NoNewline

    # NO se pausa komorebi: se flota SOLO la ventana en primer plano con la primitiva
    # `session-float-rule` de komorebi (scope de sesión, sin tocar komorebi.json ni
    # des-manejar globalmente el exe). El resto del tiling y los hotkeys SUPER
    # (incl. SUPER+1/2) siguen activos.
    if (Test-WinarchyProcess 'komorebi') {
        komorebic session-float-rule 2>$null | Out-Null
        Set-Content -Path (Get-WinarchyFloatedMarker) -Value (Get-Date -Format 'o') -Encoding UTF8 -NoNewline
    }
    Write-WinarchyOk 'Game-mode ON: la ventana en primer plano flota fuera del tiling; el resto del tiling y los hotkeys SUPER siguen activos.'
}

function Disable-WinarchyGameMode {
    $flag = Get-WinarchyGameModeFlag
    if (-not (Test-Path $flag)) { Write-WinarchyInfo 'Game-mode was already inactive.'; return }
    Remove-Item $flag -Force

    # Limpiar la(s) regla(s) de floteo de sesión y re-tilear para que la ventana
    # vuelva al layout. komorebi nunca estuvo pausado.
    $marker = Get-WinarchyFloatedMarker
    if (Test-Path $marker) {
        if (Test-WinarchyProcess 'komorebi') {
            komorebic clear-session-float-rules 2>$null | Out-Null
            komorebic retile 2>$null | Out-Null
        }
        Remove-Item $marker -Force
    }
    Write-WinarchyOk 'Game-mode OFF: floteo de sesión limpiado y re-tilado; el tiling nunca se pausó.'
}

function Get-WinarchyGameModeStatus {
    $flag = Test-Path (Get-WinarchyGameModeFlag)
    $games = Get-WinarchyGames
    [pscustomobject]@{
        Active     = $flag
        GamesCount = $games.Count
        Games      = $games
    } | Format-List
    Write-WinarchyInfo 'Note: AHK also suspends hotkeys automatically for games.toml entries and fullscreen-exclusive apps.'
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
        Write-WinarchyWarn 'No active theme yet; the rules will apply with the first `winarchy theme set`.'
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

function Restart-WinarchyPowerToys {
    <# Reinicia PowerToys (si está corriendo) para que recargue settings.json del disco.
       PowerToys NO relee excluded_apps en caliente: mantiene la lista en memoria y
       pisa el archivo en su próximo guardado. Reiniciarlo es la única forma de que
       tome los cambios que escribimos por disco sin perderlos. No-op si no corre. #>
    $procs = @(Get-Process PowerToys -ErrorAction SilentlyContinue)
    if ($procs.Count -eq 0) { return }
    $exe = ($procs | Where-Object { $_.Path } | Select-Object -First 1).Path
    if (-not $exe) {
        $exe = @(
            "$env:LOCALAPPDATA\PowerToys\PowerToys.exe",
            "$env:ProgramFiles\PowerToys\PowerToys.exe"
        ) | Where-Object { Test-Path $_ } | Select-Object -First 1
    }
    Get-Process PowerToys* -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    if ($exe -and (Test-Path $exe)) {
        Start-Sleep -Milliseconds 500
        Start-Process $exe
    }
    else {
        Write-WinarchyWarn 'No encontré PowerToys.exe para reiniciarlo; reinicialo a mano para aplicar las exclusiones.'
    }
}

function Update-WinarchyQuickAccentExclusion {
    <# Merge quirúrgico de excluded_apps en el settings.json de PowerToys Quick Accent.
       Si PowerToys/Quick Accent no está instalado (no existe el archivo), no hace nada. #>
    param(
        [Parameter(Mandatory)][string]$Exe,
        [switch]$Remove
    )
    $path = Join-Path $env:LOCALAPPDATA 'Microsoft\PowerToys\QuickAccent\settings.json'
    if (-not (Test-Path $path)) { return }
    try {
        $json = Get-Content $path -Raw -Encoding UTF8 | ConvertFrom-Json
        if (-not $json.properties.PSObject.Properties['excluded_apps']) {
            $json.properties | Add-Member -NotePropertyName excluded_apps -NotePropertyValue ([pscustomobject]@{ value = '' })
        }
        $apps = @($json.properties.excluded_apps.value -split '[\r\n]+' | Where-Object { $_ })
        if ($Remove) {
            if ($apps -notcontains $Exe) { return }
            $apps = @($apps | Where-Object { $_ -ne $Exe })
        }
        else {
            if ($apps -contains $Exe) { return }
            $apps += $Exe
        }
        $json.properties.excluded_apps.value = if ($apps.Count) { ($apps -join "`r") + "`r" } else { '' }
        Set-Content -Path $path -Value ($json | ConvertTo-Json -Depth 10 -Compress) -Encoding UTF8 -NoNewline
        # PowerToys mantiene excluded_apps en memoria y pisaría el archivo en su próximo
        # guardado: hay que reiniciarlo para que recargue del disco lo que acabamos de escribir.
        Restart-WinarchyPowerToys
        $verb = if ($Remove) { 'removed from' } else { 'added to' }
        Write-WinarchyInfo "Quick Accent: '$Exe' $verb exclusions (PowerToys reiniciado para recargar)."
    }
    catch {
        Write-WinarchyWarn "Could not update Quick Accent exclusions: $_"
    }
}

function Add-WinarchyGame {
    param([Parameter(Mandatory)][string]$Exe)
    if ($Exe -notlike '*.exe') { $Exe = "$Exe.exe" }
    $gamesToml = Join-Path (Get-WinarchyRoot) 'games.toml'
    $existing = Get-WinarchyGames
    if ($existing -contains $Exe) {
        Write-WinarchyInfo "'$Exe' was already in games.toml."
        Update-WinarchyQuickAccentExclusion -Exe $Exe
        return
    }
    Add-Content -Path $gamesToml -Value "`n[[games]]`nexe = `"$Exe`"" -Encoding UTF8
    Update-WinarchyKomorebiRules
    # reload-configuration no des-maneja instancias ya corriendo; la regla runtime sí
    if (Test-WinarchyProcess 'komorebi') { komorebic ignore-rule exe $Exe 2>$null | Out-Null }
    Update-WinarchyQuickAccentExclusion -Exe $Exe
    Write-WinarchyOk "'$Exe' added: excluded from tiling, hotkeys suspended while in the foreground."
}

function Remove-WinarchyGame {
    param([Parameter(Mandatory)][string]$Exe)
    if ($Exe -notlike '*.exe') { $Exe = "$Exe.exe" }
    $gamesToml = Join-Path (Get-WinarchyRoot) 'games.toml'
    if (-not (Test-Path $gamesToml)) { Write-WinarchyWarn 'games.toml does not exist.'; return }
    $content = Get-Content $gamesToml -Raw -Encoding UTF8
    $pattern = "(?ms)^\[\[games\]\]\s*\r?\nexe\s*=\s*`"$([regex]::Escape($Exe))`"\s*(\r?\n)?"
    $updated = [regex]::Replace($content, $pattern, '')
    if ($updated -eq $content) { Write-WinarchyWarn "'$Exe' was not in games.toml."; return }
    Set-Content -Path $gamesToml -Value $updated -Encoding UTF8 -NoNewline
    Update-WinarchyKomorebiRules
    Update-WinarchyQuickAccentExclusion -Exe $Exe -Remove
    Write-WinarchyOk "'$Exe' removed from games.toml."
}
