# ThemeEngine.ps1 — paleta TOML + templates → generación atómica con rollback
# Superficies: komorebi (bordes), YASB (css), Windows Terminal (scheme), Flow Launcher
# (xaml + Settings.json), neovim, btop, wallpaper, dark/light + accent de Windows.

$script:ThemeRequiredColorKeys = @(
    'background', 'foreground', 'accent', 'cursor',
    'selection_background', 'selection_foreground'
) + (0..15 | ForEach-Object { "color$_" })
$script:ThemeRequiredBorderKeys = @('focused', 'unfocused', 'urgent', 'monocle', 'stack')

function Get-WinarchyThemesDir { Join-Path (Get-WinarchyRoot) 'themes' }

function Get-WinarchyThemes {
    <# Lista los themes drop-in válidos (carpeta con theme.toml). #>
    Get-ChildItem -Path (Get-WinarchyThemesDir) -Directory -ErrorAction SilentlyContinue |
        Where-Object { Test-Path (Join-Path $_.FullName 'theme.toml') } |
        Sort-Object Name |
        ForEach-Object { $_.Name }
}

function Get-WinarchyCurrentTheme {
    $file = Join-Path (Get-WinarchyStateDir) 'current-theme'
    if (Test-Path $file) { (Get-Content $file -Raw).Trim() } else { $null }
}

function Test-WinarchyThemeData {
    <# Valida claves obligatorias. Devuelve lista de faltantes (vacía = válido). #>
    param([Parameter(Mandatory)][hashtable]$Theme)
    $missing = [System.Collections.Generic.List[string]]::new()
    foreach ($k in @('name', 'mode')) {
        if (-not $Theme.ContainsKey($k)) { $missing.Add($k) }
    }
    if ($Theme.ContainsKey('mode') -and $Theme['mode'] -notin @('dark', 'light')) {
        $missing.Add("mode (must be 'dark' or 'light')")
    }
    if (-not $Theme.ContainsKey('colors')) { $missing.Add('[colors]') }
    else {
        foreach ($k in $script:ThemeRequiredColorKeys) {
            if (-not $Theme['colors'].ContainsKey($k)) { $missing.Add("colors.$k") }
        }
    }
    if (-not $Theme.ContainsKey('borders')) { $missing.Add('[borders]') }
    else {
        foreach ($k in $script:ThemeRequiredBorderKeys) {
            if (-not $Theme['borders'].ContainsKey($k)) { $missing.Add("borders.$k") }
        }
    }
    , $missing
}

function Get-WinarchyGameIgnoreRulesJson {
    <# Fragmento JSON de ignore-rules de komorebi generado desde games.toml. #>
    $fragment = ''
    foreach ($exe in (Get-WinarchyGames)) {
        $fragment += ",`n    { `"kind`": `"Exe`", `"id`": `"$exe`", `"matching_strategy`": `"Equals`" }"
    }
    $fragment
}

function Build-WinarchyThemeContext {
    param([Parameter(Mandatory)][hashtable]$Theme, [Parameter(Mandatory)][string]$ThemeDir)
    $ctx = ConvertTo-WinarchyFlatContext -Data $Theme
    $ctx['computed.theme_dir'] = Split-Path $ThemeDir -Leaf
    # Raíz del repo con forward slashes: apto para rutas dentro de JSON generado
    $ctx['computed.root'] = (Get-WinarchyRoot) -replace '\\', '/'
    $ctx['computed.game_ignore_rules'] = Get-WinarchyGameIgnoreRulesJson
    # auto-accent puede traer un accent tan oscuro (o claro) como el background:
    # accent_ui es el accent corregido para usarse como texto/superficie dentro de
    # apps con fondo del theme, y on_accent el texto legible sobre ese accent_ui.
    $ctx['colors.accent_ui'] = Get-WinarchyReadableAccentHex -Accent $Theme['colors']['accent'] -Background $Theme['colors']['background']
    $ctx['colors.on_accent'] = Get-WinarchyOnAccentHex -Hex $ctx['colors.accent_ui']
    # Accent como "R;G;B" decimal, para escapes ANSI truecolor (fastfetch usa "38;2;R;G;B")
    $accentHex = $ctx['colors.accent_ui'].TrimStart('#')
    $ctx['colors.accent_ansi_rgb'] = (0, 2, 4 | ForEach-Object {
            [Convert]::ToInt32($accentHex.Substring($_, 2), 16) }) -join ';'
    $ctx
}

function Get-WinarchyRenderTargets {
    <# Mapa template → archivo generado dentro del repo. #>
    $root = Get-WinarchyRoot
    @(
        @{ Template = 'komorebi.json.tpl';                Output = Join-Path $root 'config\komorebi\komorebi.json';      Validate = 'json' }
        @{ Template = 'yasb-styles.css.tpl';              Output = Join-Path $root 'config\yasb\styles.css';             Validate = $null }
        @{ Template = 'windows-terminal-scheme.json.tpl'; Output = Join-Path $root 'config\terminal\winarchy-scheme.json'; Validate = 'json' }
        @{ Template = 'flow-theme.xaml.tpl';              Output = Join-Path $root 'config\flow\Winarchy.xaml';          Validate = 'xml' }
        @{ Template = 'btop.theme.tpl';                   Output = Join-Path $root 'config\btop\winarchy.theme';         Validate = $null }
        @{ Template = 'nvim-theme.lua.tpl';               Output = Join-Path $root 'config\nvim\winarchy-theme.lua';     Validate = $null }
        @{ Template = 'ahk-theme.ini.tpl';                Output = Join-Path $root 'config\ahk\theme.ini';               Validate = $null }
        @{ Template = 'obsidian.css.tpl';                 Output = Join-Path $root 'config\obsidian\winarchy-theme.css'; Validate = $null }
        @{ Template = 'starship.toml.tpl';                Output = Join-Path $root 'config\pwsh\starship.toml';          Validate = $null }
        @{ Template = 'fastfetch.jsonc.tpl';              Output = Join-Path $root 'config\fastfetch\config.jsonc';      Validate = 'json' }
    )
}

function Get-WinarchyTerminalSettingsPath {
    $candidates = @(
        "$env:LOCALAPPDATA\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json",
        "$env:LOCALAPPDATA\Microsoft\Windows Terminal\settings.json"
    )
    $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}

function Merge-WinarchyTerminalScheme {
    <# Merge quirúrgico: solo el scheme "Winarchy" + colorScheme del profile default. #>
    param([Parameter(Mandatory)][string]$SchemeJsonPath)
    $settingsPath = Get-WinarchyTerminalSettingsPath
    if (-not $settingsPath) {
        Write-WinarchyWarn 'Windows Terminal settings.json not found; scheme not applied.'
        return $null
    }
    $settings = Get-Content $settingsPath -Raw -Encoding UTF8 | ConvertFrom-Json -AsHashtable
    $scheme = Get-Content $SchemeJsonPath -Raw -Encoding UTF8 | ConvertFrom-Json -AsHashtable

    if (-not $settings.ContainsKey('schemes')) { $settings['schemes'] = @() }
    $settings['schemes'] = @($settings['schemes'] | Where-Object { $_['name'] -ne 'Winarchy' }) + @($scheme)

    if (-not $settings.ContainsKey('profiles')) { $settings['profiles'] = @{} }
    if ($settings['profiles'] -is [hashtable]) {
        if (-not $settings['profiles'].ContainsKey('defaults') -or $null -eq $settings['profiles']['defaults']) {
            $settings['profiles']['defaults'] = @{}
        }
        $settings['profiles']['defaults']['colorScheme'] = 'Winarchy'
    }
    $settings | ConvertTo-Json -Depth 50 | Set-Content -Path $settingsPath -Encoding UTF8
    $settingsPath
}

function Set-WinarchyFlowTheme {
    <# Copia el xaml al dir de themes de Flow y apunta Settings.json a "Winarchy". #>
    param([Parameter(Mandatory)][string]$XamlPath)
    $flowDir = "$env:APPDATA\FlowLauncher"
    if (-not (Test-Path $flowDir)) {
        Write-WinarchyWarn 'Flow Launcher not found; Flow theme not applied.'
        return
    }
    $themesDir = Join-Path $flowDir 'Themes'
    if (-not (Test-Path $themesDir)) { New-Item -ItemType Directory -Path $themesDir -Force | Out-Null }
    Copy-Item $XamlPath (Join-Path $themesDir 'Winarchy.xaml') -Force

    $settingsPath = Join-Path $flowDir 'Settings\Settings.json'
    if (Test-Path $settingsPath) {
        $settings = Get-Content $settingsPath -Raw -Encoding UTF8 | ConvertFrom-Json -AsHashtable
        $settings['Theme'] = 'Winarchy'
        $settings | ConvertTo-Json -Depth 50 | Set-Content -Path $settingsPath -Encoding UTF8
    }
}

function Get-WinarchyVSCodeEditors {
    <# Editores familia VS Code soportados: CLI para extensiones + path del settings.json. #>
    @(
        @{ Id = 'code';   Cli = 'code';   Settings = "$env:APPDATA\Code\User\settings.json" }
        @{ Id = 'cursor'; Cli = 'cursor'; Settings = "$env:APPDATA\Cursor\User\settings.json" }
    )
}

function Get-WinarchyVSCodeSettingsFiles {
    <# settings.json raíz + el de cada perfil de usuario existente (un perfil activo
       pisa al settings raíz, así que el theme debe escribirse en ambos niveles). #>
    param([Parameter(Mandatory)][hashtable]$Editor)
    $files = [System.Collections.Generic.List[string]]::new()
    $files.Add($Editor.Settings)
    $profiles = Join-Path (Split-Path $Editor.Settings -Parent) 'profiles'
    if (Test-Path $profiles) {
        foreach ($p in Get-ChildItem $profiles -Directory -ErrorAction SilentlyContinue) {
            $s = Join-Path $p.FullName 'settings.json'
            if (Test-Path $s) { $files.Add($s) }
        }
    }
    $files
}

function Get-WinarchyVSCodeProfileNames {
    <# Nombres de los perfiles de usuario del editor (registro en
       globalStorage\storage.json bajo userDataProfiles). Best-effort: vacío si no hay. #>
    param([Parameter(Mandatory)][hashtable]$Editor)
    $storage = Join-Path (Split-Path $Editor.Settings -Parent) 'globalStorage\storage.json'
    if (-not (Test-Path $storage)) { return @() }
    try {
        $data = Get-Content $storage -Raw -Encoding UTF8 | ConvertFrom-Json -AsHashtable
        @($data['userDataProfiles'] | Where-Object { $_ -and $_['name'] } | ForEach-Object { $_['name'] })
    }
    catch { @() }
}

function Set-WinarchyVSCodeColorTheme {
    <# Setea workbench.colorTheme editando solo ese valor en el texto: los settings de
       VS Code son JSONC (comentarios, trailing commas) y un parse/re-serialize los
       destruiría. Valida que el resultado siga parseando antes de escribir. #>
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][string]$Name)
    $raw = if (Test-Path $Path) { Get-Content $Path -Raw -Encoding UTF8 } else { '' }
    if (-not $raw -or -not $raw.Trim()) {
        "{`n  `"workbench.colorTheme`": `"$Name`"`n}" | Set-Content -Path $Path -Encoding UTF8
        return
    }
    $pattern = [regex]'("workbench\.colorTheme"\s*:\s*")(?:[^"\\]|\\.)*(")'
    if ($pattern.IsMatch($raw)) {
        $updated = $pattern.Replace($raw, "`${1}$Name`${2}", 1)
    }
    else {
        $brace = $raw.IndexOf('{')
        if ($brace -lt 0) { throw "no root JSON object: $Path" }
        $rest = $raw.Substring($brace + 1)
        $comma = if ($rest -match '^\s*\}') { '' } else { ',' }
        $updated = $raw.Substring(0, $brace + 1) + "`n  `"workbench.colorTheme`": `"$Name`"$comma" + $rest
    }
    $null = $updated | ConvertFrom-Json -AsHashtable   # sanity: sigue siendo JSON(C) válido
    Set-Content -Path $Path -Value $updated -Encoding UTF8
}

function Get-WinarchyObsidianVaults {
    <# Vaults registrados en el índice de Obsidian que tienen carpeta .obsidian\. #>
    $index = "$env:APPDATA\obsidian\obsidian.json"
    if (-not (Test-Path $index)) { return @() }
    try { $data = Get-Content $index -Raw -Encoding UTF8 | ConvertFrom-Json -AsHashtable }
    catch { return @() }
    if (-not $data -or -not $data.ContainsKey('vaults')) { return @() }
    @($data['vaults'].Values | ForEach-Object { $_['path'] } |
            Where-Object { $_ -and (Test-Path (Join-Path $_ '.obsidian')) })
}

function Sync-WinarchyJetBrains {
    <# Copia el jetbrains.icls del theme a cada IDE bajo %APPDATA%\JetBrains y apunta
       el scheme global. Sin drop-in: limpia el Winarchy.icls previo (colors.scheme.xml
       queda como esté, igual que omarchy). El IDE abierto recoge el scheme al reiniciar. #>
    param([Parameter(Mandatory)][string]$ThemeDir)
    $jbRoot = "$env:APPDATA\JetBrains"
    if (-not (Test-Path $jbRoot)) { return }

    $icls = Join-Path $ThemeDir 'jetbrains.icls'
    $schemeName = $null
    if (Test-Path $icls) {
        try { $schemeName = ([xml](Get-Content $icls -Raw -Encoding UTF8)).scheme.name }
        catch { Write-WinarchyWarn "Theme's jetbrains.icls does not parse as XML; JetBrains skipped." ; return }
    }

    foreach ($product in Get-ChildItem $jbRoot -Directory -ErrorAction SilentlyContinue) {
        if (-not (Test-Path (Join-Path $product.FullName 'options'))) { continue }
        $target = Join-Path $product.FullName 'colors\Winarchy.icls'

        if (-not $schemeName) {
            if (Test-Path $target) { Remove-Item $target -Force }
            continue
        }

        $colorsDir = Split-Path $target -Parent
        if (-not (Test-Path $colorsDir)) { New-Item -ItemType Directory -Path $colorsDir -Force | Out-Null }
        Copy-Item $icls $target -Force

        $schemeXmlPath = Join-Path $product.FullName 'options\colors.scheme.xml'
        if (Test-Path $schemeXmlPath) {
            try { $doc = [xml](Get-Content $schemeXmlPath -Raw -Encoding UTF8) }
            catch {
                Write-WinarchyWarn "colors.scheme.xml of $($product.Name) does not parse; global scheme not pointed."
                continue
            }
        }
        else {
            $doc = [xml]'<application />'
        }
        $component = $doc.SelectSingleNode("//component[@name='EditorColorsManagerImpl']")
        if (-not $component) {
            $component = $doc.CreateElement('component')
            $component.SetAttribute('name', 'EditorColorsManagerImpl')
            $doc.DocumentElement.AppendChild($component) | Out-Null
        }
        $node = $component.SelectSingleNode('global_color_scheme')
        if (-not $node) {
            $node = $doc.CreateElement('global_color_scheme')
            $component.AppendChild($node) | Out-Null
        }
        $node.SetAttribute('name', $schemeName)
        $doc.Save($schemeXmlPath)
    }
}

function Sync-WinarchyVSCode {
    <# Merge quirúrgico de workbench.colorTheme en VS Code/Cursor según vscode.json del
       theme; instala la extensión si falta. Sin drop-in: no toca settings. #>
    param([Parameter(Mandatory)][string]$ThemeDir)
    $dropin = Join-Path $ThemeDir 'vscode.json'
    if (-not (Test-Path $dropin)) { return }
    try { $spec = Get-Content $dropin -Raw -Encoding UTF8 | ConvertFrom-Json -AsHashtable }
    catch { Write-WinarchyWarn "Theme's vscode.json does not parse; VS Code skipped."; return }
    if (-not $spec -or -not $spec['name']) { return }

    foreach ($editor in Get-WinarchyVSCodeEditors) {
        $cli = Get-Command $editor.Cli -ErrorAction SilentlyContinue
        if (-not $cli) { continue }

        if ($spec['extension']) {
            # Las extensiones son por perfil: el default (sin flag) + cada perfil de usuario.
            $scopes = @(, @()) + @(Get-WinarchyVSCodeProfileNames $editor | ForEach-Object { , @('--profile', $_) })
            foreach ($scopeArgs in $scopes) {
                try {
                    $installed = @(& $cli.Source --list-extensions @scopeArgs 2>$null)
                    if ($installed -notcontains $spec['extension']) {
                        & $cli.Source --install-extension $spec['extension'] @scopeArgs 2>$null | Out-Null
                    }
                }
                catch { Write-WinarchyWarn "Extension $($spec['extension']) not installed in $($editor.Id); the theme will resolve once it shows up." }
            }
        }

        $dir = Split-Path $editor.Settings -Parent
        if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
        foreach ($settingsPath in Get-WinarchyVSCodeSettingsFiles $editor) {
            try { Set-WinarchyVSCodeColorTheme -Path $settingsPath -Name $spec['name'] }
            catch { Write-WinarchyWarn "$($editor.Id) settings not updatable ($settingsPath); colorTheme not applied there." }
        }
    }
}

function Sync-WinarchyObsidian {
    <# Copia el CSS renderizado a .obsidian\themes\Winarchy\ de cada vault registrado.
       manifest.json solo se crea si falta; appearance.json no se toca (activación manual). #>
    param([Parameter(Mandatory)][string]$CssPath)
    if (-not (Test-Path $CssPath)) { return }
    foreach ($vault in Get-WinarchyObsidianVaults) {
        try {
            $themeDir = Join-Path $vault '.obsidian\themes\Winarchy'
            if (-not (Test-Path $themeDir)) { New-Item -ItemType Directory -Path $themeDir -Force | Out-Null }
            Copy-Item $CssPath (Join-Path $themeDir 'theme.css') -Force
            $manifest = Join-Path $themeDir 'manifest.json'
            if (-not (Test-Path $manifest)) {
                @'
{
  "name": "Winarchy",
  "version": "1.0.0",
  "minAppVersion": "0.16.0",
  "description": "Synced with the active Winarchy theme",
  "author": "Winarchy"
}
'@ | Set-Content -Path $manifest -Encoding UTF8
            }
        }
        catch { Write-WinarchyWarn "Obsidian vault not writable, skipped: $vault" }
    }
}

function Get-WinarchyAdapterBackupTargets {
    <# Archivos externos que los adapters pueden tocar, con subruta relativa única dentro
       del snapshot (evita colisión entre settings.json / colors.scheme.xml homónimos). #>
    $targets = [System.Collections.Generic.List[hashtable]]::new()
    $jbRoot = "$env:APPDATA\JetBrains"
    if (Test-Path $jbRoot) {
        foreach ($product in Get-ChildItem $jbRoot -Directory -ErrorAction SilentlyContinue) {
            if (-not (Test-Path (Join-Path $product.FullName 'options'))) { continue }
            $targets.Add(@{ Path = Join-Path $product.FullName 'colors\Winarchy.icls';      Rel = "jetbrains\$($product.Name)\Winarchy.icls" })
            $targets.Add(@{ Path = Join-Path $product.FullName 'options\colors.scheme.xml'; Rel = "jetbrains\$($product.Name)\colors.scheme.xml" })
        }
    }
    foreach ($editor in Get-WinarchyVSCodeEditors) {
        $userDir = Split-Path $editor.Settings -Parent
        foreach ($file in Get-WinarchyVSCodeSettingsFiles $editor) {
            $targets.Add(@{ Path = $file; Rel = "vscode\$($editor.Id)\$($file.Substring($userDir.Length + 1))" })
        }
    }
    $i = 0
    foreach ($vault in Get-WinarchyObsidianVaults) {
        $targets.Add(@{ Path = Join-Path $vault '.obsidian\themes\Winarchy\theme.css'; Rel = "obsidian\$i\theme.css" })
        $i++
    }
    , $targets
}

function Get-WinarchyWindowsAccent {
    <# Accent vivo de Windows (DWORD ABGR del registry → '#rrggbb'); $null si no es legible. #>
    foreach ($probe in @(
            @{ Path = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Accent'; Name = 'AccentColorMenu' },
            @{ Path = 'HKCU:\SOFTWARE\Microsoft\Windows\DWM'; Name = 'AccentColor' }
        )) {
        $item = Get-ItemProperty -Path $probe.Path -Name $probe.Name -ErrorAction SilentlyContinue
        if ($item) {
            $v = $item.($probe.Name)
            return ('#{0:x2}{1:x2}{2:x2}' -f ($v -band 0xFF), (($v -shr 8) -band 0xFF), (($v -shr 16) -band 0xFF))
        }
    }
    $null
}

function Get-WinarchyShadedHex {
    <# Lerp de '#rrggbb' hacia blanco (Factor > 0) o hacia negro (Factor < 0). #>
    param([Parameter(Mandatory)][string]$Hex, [Parameter(Mandatory)][double]$Factor)
    $target = if ($Factor -ge 0) { 255 } else { 0 }
    $f = [Math]::Abs($Factor)
    $rgb = foreach ($i in @(1, 3, 5)) {
        $c = [Convert]::ToInt32($Hex.Substring($i, 2), 16)
        [int][Math]::Round($c + ($target - $c) * $f)
    }
    '#{0:x2}{1:x2}{2:x2}' -f $rgb[0], $rgb[1], $rgb[2]
}

function Get-WinarchyYiqLuminance {
    <# Luminancia YIQ (0-255) de un '#rrggbb'. #>
    param([Parameter(Mandatory)][string]$Hex)
    $r = [Convert]::ToInt32($Hex.Substring(1, 2), 16)
    $g = [Convert]::ToInt32($Hex.Substring(3, 2), 16)
    $b = [Convert]::ToInt32($Hex.Substring(5, 2), 16)
    (($r * 299) + ($g * 587) + ($b * 114)) / 1000
}

function Get-WinarchyOnAccentHex {
    <# Blanco o casi-negro según la luminancia YIQ del color dado, para que el
       texto sobre fondo accent sea legible con cualquier accent. #>
    param([Parameter(Mandatory)][string]$Hex)
    if ((Get-WinarchyYiqLuminance -Hex $Hex) -ge 128) { '#1a1a1a' } else { '#ffffff' }
}

function Get-WinarchyReadableAccentHex {
    <# Accent apto como texto/superficie sobre el background del theme: si el
       contraste YIQ es bajo, lo acerca a blanco (fondo oscuro) o negro (fondo
       claro) lo justo para que se distinga. Themes curados quedan intactos. #>
    param([Parameter(Mandatory)][string]$Accent, [Parameter(Mandatory)][string]$Background)
    $bgYiq = Get-WinarchyYiqLuminance -Hex $Background
    $direction = if ($bgYiq -lt 128) { 1.0 } else { -1.0 }
    $hex = $Accent
    for ($f = 0.0; $f -le 1.0; $f += 0.1) {
        $hex = Get-WinarchyShadedHex -Hex $Accent -Factor ($direction * $f)
        if ([Math]::Abs((Get-WinarchyYiqLuminance -Hex $hex) - $bgYiq) -ge 80) { break }
    }
    $hex
}

function Set-WinarchyWindowsAppearance {
    <# Dark/light + accent de Windows vía registry (best-effort, revertible). #>
    param(
        [Parameter(Mandatory)][string]$Mode,
        [Parameter(Mandatory)][string]$AccentHex,
        # themes dinámicos: Windows es el dueño del accent y no se le escribe encima
        [switch]$SkipAccent
    )
    $personalize = 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Themes\Personalize'
    $light = if ($Mode -eq 'light') { 1 } else { 0 }
    Set-ItemProperty -Path $personalize -Name 'AppsUseLightTheme' -Value $light -Type DWord
    Set-ItemProperty -Path $personalize -Name 'SystemUsesLightTheme' -Value $light -Type DWord
    # ColorPrevalence = accent sobre taskbar/Start/bordes de ventana. Se activa siempre
    # (incluso en themes dinámicos: el hue lo pone Windows, pero la barra igual reacciona).
    Set-ItemProperty -Path $personalize -Name 'ColorPrevalence' -Value 1 -Type DWord
    Set-ItemProperty -Path 'HKCU:\SOFTWARE\Microsoft\Windows\DWM' -Name 'ColorPrevalence' -Value 1 -Type DWord
    if ($SkipAccent) { return }
    try {
        $r = [Convert]::ToInt32($AccentHex.Substring(1, 2), 16)
        $g = [Convert]::ToInt32($AccentHex.Substring(3, 2), 16)
        $b = [Convert]::ToInt32($AccentHex.Substring(5, 2), 16)
        $abgr = (0xFF -shl 24) -bor ($b -shl 16) -bor ($g -shl 8) -bor $r
        # int32 negativo a propósito: DWord toma el patrón de bits; [uint32] valida rango y revienta
        Set-ItemProperty -Path 'HKCU:\SOFTWARE\Microsoft\Windows\DWM' -Name 'AccentColor' -Value $abgr -Type DWord
        Set-ItemProperty -Path 'HKCU:\SOFTWARE\Microsoft\Windows\DWM' -Name 'ColorizationColor' -Value $abgr -Type DWord
    }
    catch { Write-WinarchyWarn "Accent not applied: $($_.Exception.Message)" }
}

function Set-WinarchyWallpaper {
    <# Wallpaper Engine si el theme trae perfil; si no, wallpaper estático nativo. #>
    param([Parameter(Mandatory)][string]$ThemeDir)
    $weProfile = Join-Path $ThemeDir 'wallpaper-engine.txt'
    if (Test-Path $weProfile) {
        $we = @(
            "$env:ProgramFiles (x86)\Steam\steamapps\common\wallpaper_engine\wallpaper64.exe",
            "$env:ProgramFiles\Steam\steamapps\common\wallpaper_engine\wallpaper64.exe"
        ) | Where-Object { Test-Path $_ } | Select-Object -First 1
        if ($we) {
            $profileName = (Get-Content $weProfile -Raw).Trim()
            & $we -control openProfile -profile $profileName
            return
        }
    }
    $wallpaper = Get-ChildItem -Path $ThemeDir -File |
        Where-Object { $_.Name -match '^wallpaper\.(jpg|jpeg|png|bmp)$' } |
        Select-Object -First 1
    if ($wallpaper) {
        if (-not ('Winarchy.Native.Wallpaper' -as [type])) {
            Add-Type -Namespace Winarchy.Native -Name Wallpaper -MemberDefinition @'
[DllImport("user32.dll", CharSet = CharSet.Auto, SetLastError = true)]
public static extern int SystemParametersInfo(int uAction, int uParam, string lpvParam, int fuWinIni);
'@
        }
        [Winarchy.Native.Wallpaper]::SystemParametersInfo(20, 0, $wallpaper.FullName, 3) | Out-Null
    }
}

function Invoke-WinarchyReload {
    <# Recarga komorebi, YASB, AHK y Flow para reflejar configs nuevas.
       -Light: solo komorebi y YASB (sync de accent: no matar AHK/Flow por un color). #>
    param([switch]$Light)
    if (Test-WinarchyProcess 'komorebi') { komorebic reload-configuration 2>$null | Out-Null }
    else { Start-WinarchyKomorebi }
    # YASB: reinicio limpio (kill + start) en vez de `yasbc reload`. El hot-reload de
    # YASB re-suscribe su widget de komorebi al named pipe pero deja viva la suscripción
    # anterior; con >1 reactor vivo las barras pelean el foco y mandan FocusWorkspaceNumber(0),
    # que te tira al workspace 1 al minimizar/maximizar o abrir un juego. Un kill+start deja
    # un único suscriptor y elimina la pelea. (Investigación 2026-06-28; ver también
    # watch_config/watch_stylesheet apagados en config/yasb/config.yaml.)
    if (Test-WinarchyProcess 'yasb') {
        Stop-Process -Name 'yasb' -Force -ErrorAction SilentlyContinue
        Start-Sleep -Milliseconds 300
    }
    if (Get-Command yasbc -ErrorAction SilentlyContinue) { Start-Process 'yasbc' -ArgumentList 'start' -WindowStyle Hidden }
    else {
        $yasbExe = Get-Command yasb -ErrorAction SilentlyContinue
        if ($yasbExe) { Start-Process $yasbExe.Source }
    }
    if ($Light) { return }
    $ahk = Join-Path (Get-WinarchyRoot) 'config\ahk\winarchy.ahk'
    $ahkExe = Get-WinarchyAhkExe
    if ($ahkExe) { Start-Process $ahkExe -ArgumentList "/restart `"$ahk`"" }
    if (Test-WinarchyProcess 'Flow.Launcher') {
        Stop-Process -Name 'Flow.Launcher' -Force -ErrorAction SilentlyContinue
        $flow = "$env:LOCALAPPDATA\FlowLauncher\Flow.Launcher.exe"
        if (Test-Path $flow) { Start-Process $flow }
    }
}

function Set-WinarchyTheme {
    <#
      Aplica un theme de punta a punta: validación → staging → snapshot → apply
      atómico → superficies externas → recarga. Si algo falla, rollback completo.
    #>
    param(
        [Parameter(Mandatory)][string]$Name,
        [switch]$StageOnly,
        # sync de accent dinámico: sin snapshot timestampeado, sin registry/wallpaper,
        # recarga solo komorebi+YASB (ver Update-WinarchyAccent)
        [switch]$LightRefresh
    )
    $root = Get-WinarchyRoot
    $themeDir = Join-Path (Get-WinarchyThemesDir) $Name
    $themeToml = Join-Path $themeDir 'theme.toml'
    if (-not (Test-Path $themeToml)) {
        $available = (Get-WinarchyThemes) -join ', '
        throw "Theme '$Name' does not exist. Available: $available"
    }

    # 1. Validación — falla ANTES de modificar nada
    $theme = Import-WinarchyToml -Path $themeToml
    $missing = Test-WinarchyThemeData -Theme $theme
    if ($missing.Count -gt 0) {
        throw "Invalid theme.toml for '$Name'. Missing keys: $($missing -join ', ')"
    }

    # 1b. Theme dinámico: el accent vivo de Windows pisa el placeholder de la paleta
    $dynamicAccent = $theme.ContainsKey('dynamic_accent') -and $theme['dynamic_accent']
    if ($LightRefresh -and -not $dynamicAccent) {
        throw "-LightRefresh only applies to dynamic themes and '$Name' is not one."
    }
    $liveAccent = $null
    if ($dynamicAccent) {
        $liveAccent = Get-WinarchyWindowsAccent
        if ($liveAccent) {
            $theme['colors']['accent'] = $liveAccent
            $theme['borders']['focused'] = $liveAccent
            $theme['borders']['monocle'] = Get-WinarchyShadedHex -Hex $liveAccent -Factor 0.25
            $theme['borders']['stack'] = Get-WinarchyShadedHex -Hex $liveAccent -Factor -0.2
        }
        else { Write-WinarchyWarn 'Windows accent unreadable; using the theme base palette.' }
    }

    # 2. Render a staging
    $staging = Join-Path (Get-WinarchyStateDir) 'staging'
    if (Test-Path $staging) { Remove-Item $staging -Recurse -Force }
    New-Item -ItemType Directory -Path $staging -Force | Out-Null

    $ctx = Build-WinarchyThemeContext -Theme $theme -ThemeDir $themeDir
    $targets = Get-WinarchyRenderTargets
    foreach ($t in $targets) {
        $tpl = Join-Path $root "templates\$($t.Template)"
        $rendered = Invoke-WinarchyTemplate -TemplatePath $tpl -Context $ctx
        # Las preferencias de ubicación van dentro del config: por runtime no sobreviven al reload.
        if ($t.Template -eq 'komorebi.json.tpl') { $rendered = Add-WinarchyWindowRulesToKomorebiJson -Json $rendered }
        $stagedFile = Join-Path $staging (Split-Path $t.Output -Leaf)
        Set-Content -Path $stagedFile -Value $rendered -Encoding UTF8 -NoNewline
        switch ($t.Validate) {
            'json' { Get-Content $stagedFile -Raw | ConvertFrom-Json | Out-Null }
            'xml'  { [xml](Get-Content $stagedFile -Raw) | Out-Null }
        }
        $t['Staged'] = $stagedFile
    }
    if ($StageOnly) {
        Write-WinarchyOk "Theme '$Name' validated and rendered to staging: $staging"
        return
    }

    # 3. Snapshot para rollback (archivos generados + settings externos + registry)
    $terminalSettings = Get-WinarchyTerminalSettingsPath
    $flowSettings = "$env:APPDATA\FlowLauncher\Settings\Settings.json"
    $snapshotPaths = @($targets.Output) | Where-Object { $_ -and (Test-Path $_) }
    if ($LightRefresh) {
        # respaldo fijo y pisable: la rotación de wallpapers no debe engordar backups\
        $snapshot = Join-Path (Get-WinarchyStateDir) 'accent-prev'
        if (Test-Path $snapshot) { Remove-Item $snapshot -Recurse -Force }
        New-Item -ItemType Directory -Path $snapshot -Force | Out-Null
        foreach ($p in $snapshotPaths) { Copy-Item $p (Join-Path $snapshot (Split-Path $p -Leaf)) -Force }
    }
    else {
        $snapshot = New-WinarchySnapshot -Path $snapshotPaths -Label "theme-rollback"
    }
    # Settings externos: subruta relativa por superficie/adapter para que los homónimos
    # no se pisen dentro del snapshot (en NTFS case-insensitive, Settings.json de Flow
    # pisa a settings.json de Terminal si se aplanan por leaf name)
    $adapterBackups = @()
    if ($terminalSettings) { $adapterBackups += , @{ Path = $terminalSettings; Rel = 'terminal\settings.json' } }
    $adapterBackups += , @{ Path = $flowSettings; Rel = 'flow\Settings.json' }
    if (-not $LightRefresh) { $adapterBackups += Get-WinarchyAdapterBackupTargets }
    foreach ($b in $adapterBackups) {
        if (Test-Path $b.Path) {
            $dest = Join-Path $snapshot $b.Rel
            New-Item -ItemType Directory -Path (Split-Path $dest -Parent) -Force | Out-Null
            Copy-Item $b.Path $dest -Force
        }
    }
    $personalize = 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Themes\Personalize'
    $personalizeBackup = Get-ItemProperty -Path $personalize -ErrorAction SilentlyContinue
    $prevLight = $personalizeBackup.AppsUseLightTheme
    $prevPrevalence = $personalizeBackup.ColorPrevalence

    try {
        # 4. Apply atómico de los generados (move desde staging)
        foreach ($t in $targets) {
            $destDir = Split-Path $t.Output -Parent
            if (-not (Test-Path $destDir)) { New-Item -ItemType Directory -Path $destDir -Force | Out-Null }
            Move-Item -Path $t.Staged -Destination $t.Output -Force
        }

        # 5. Superficies externas
        Merge-WinarchyTerminalScheme -SchemeJsonPath (Join-Path $root 'config\terminal\winarchy-scheme.json') | Out-Null
        Set-WinarchyFlowTheme -XamlPath (Join-Path $root 'config\flow\Winarchy.xaml')
        $btopThemes = "$env:USERPROFILE\.config\btop\themes"
        if (Test-Path (Split-Path $btopThemes -Parent)) {
            if (-not (Test-Path $btopThemes)) { New-Item -ItemType Directory -Path $btopThemes -Force | Out-Null }
            Copy-Item (Join-Path $root 'config\btop\winarchy.theme') (Join-Path $btopThemes 'winarchy.theme') -Force
        }
        if (-not $LightRefresh) {
            Sync-WinarchyJetBrains -ThemeDir $themeDir
            Sync-WinarchyVSCode -ThemeDir $themeDir
            Sync-WinarchyObsidian -CssPath (Join-Path $root 'config\obsidian\winarchy-theme.css')
            Set-WinarchyWindowsAppearance -Mode $theme['mode'] -AccentHex $theme['colors']['accent'] -SkipAccent:$dynamicAccent
            Set-WinarchyWallpaper -ThemeDir $themeDir
        }

        # 6. Persistencia + recarga de servicios
        Set-Content -Path (Join-Path (Get-WinarchyStateDir) 'current-theme') -Value $Name -Encoding UTF8 -NoNewline
        # marker del watcher AccentWatch (AHK): existe = theme dinámico activo + hex aplicado
        $marker = Join-Path (Get-WinarchyStateDir) 'dynamic-accent'
        if ($dynamicAccent -and $liveAccent) {
            Set-Content -Path $marker -Value $liveAccent -Encoding UTF8 -NoNewline
        }
        elseif (-not $dynamicAccent -and (Test-Path $marker)) { Remove-Item $marker -Force }
        Invoke-WinarchyReload -Light:$LightRefresh

        if ($LightRefresh) { Write-WinarchyOk "Accent synced: $($theme['colors']['accent'])." }
        else { Write-WinarchyOk "Theme '$($theme['name'])' applied across all surfaces." }
    }
    catch {
        Write-WinarchyErr "Failed to apply theme: $($_.Exception.Message) — starting rollback."
        foreach ($t in $targets) {
            $bak = Join-Path $snapshot (Split-Path $t.Output -Leaf)
            if (Test-Path $bak) { Copy-Item $bak $t.Output -Force }
        }
        foreach ($b in $adapterBackups) {
            $bak = Join-Path $snapshot $b.Rel
            if (Test-Path $bak) { Copy-Item $bak $b.Path -Force }
        }
        if (-not $LightRefresh -and $null -ne $prevLight) {
            Set-ItemProperty -Path $personalize -Name 'AppsUseLightTheme' -Value $prevLight -Type DWord
            Set-ItemProperty -Path $personalize -Name 'SystemUsesLightTheme' -Value $prevLight -Type DWord
        }
        if (-not $LightRefresh -and $null -ne $prevPrevalence) {
            Set-ItemProperty -Path $personalize -Name 'ColorPrevalence' -Value $prevPrevalence -Type DWord
            Set-ItemProperty -Path 'HKCU:\SOFTWARE\Microsoft\Windows\DWM' -Name 'ColorPrevalence' -Value $prevPrevalence -Type DWord
        }
        Invoke-WinarchyReload -Light:$LightRefresh
        throw "Theme '$Name' NOT applied; previous state restored (snapshot: $snapshot)."
    }
}

function Set-WinarchyNextTheme {
    <# Cicla alfabéticamente al siguiente theme instalado. #>
    $themes = @(Get-WinarchyThemes)
    if ($themes.Count -eq 0) { throw 'No themes installed.' }
    $current = Get-WinarchyCurrentTheme
    $idx = [array]::IndexOf($themes, $current)
    $next = $themes[(($idx + 1) % $themes.Count)]
    Set-WinarchyTheme -Name $next
}

function Test-WinarchyDynamicTheme {
    <# ¿El theme dado (default: el activo) tiene dynamic_accent = true? #>
    param([string]$Name)
    if (-not $Name) { $Name = Get-WinarchyCurrentTheme }
    if (-not $Name) { return $false }
    $toml = Join-Path (Join-Path (Get-WinarchyThemesDir) $Name) 'theme.toml'
    if (-not (Test-Path $toml)) { return $false }
    $theme = Import-WinarchyToml -Path $toml
    [bool]($theme.ContainsKey('dynamic_accent') -and $theme['dynamic_accent'])
}

function Update-WinarchyAccent {
    <# Sync del accent dinámico (lo dispara AccentWatch desde AHK al cambiar el wallpaper).
       No-op si el theme activo no es dinámico o el accent no cambió. #>
    param([switch]$Force)
    $marker = Join-Path (Get-WinarchyStateDir) 'dynamic-accent'
    $current = Get-WinarchyCurrentTheme
    if (-not (Test-WinarchyDynamicTheme -Name $current)) {
        # marker huérfano (theme estático aplicado por fuera): limpiarlo apaga el watcher
        if (Test-Path $marker) { Remove-Item $marker -Force }
        Write-WinarchyInfo 'Active theme is not dynamic; nothing to sync.'
        return
    }
    $live = Get-WinarchyWindowsAccent
    if (-not $live) {
        Write-WinarchyWarn 'Windows accent unreadable; sync skipped.'
        return
    }
    $applied = if (Test-Path $marker) { (Get-Content $marker -Raw).Trim() } else { $null }
    if (-not $Force -and $live -eq $applied) {
        Write-WinarchyOk "Accent unchanged ($live)."
        return
    }
    Set-WinarchyTheme -Name $current -LightRefresh
}

function Get-WinarchyAccentStatus {
    <# Accent vivo vs aplicado y estado del modo dinámico. #>
    $current = Get-WinarchyCurrentTheme
    $live = Get-WinarchyWindowsAccent
    $marker = Join-Path (Get-WinarchyStateDir) 'dynamic-accent'
    $applied = if (Test-Path $marker) { (Get-Content $marker -Raw).Trim() } else { $null }
    $dynamic = Test-WinarchyDynamicTheme -Name $current
    Write-Host "  Active theme:    $(if ($current) { $current } else { '(none)' })$(if ($dynamic) { ' [dynamic]' })"
    Write-Host "  System accent:   $(if ($live) { $live } else { '(unreadable)' })"
    Write-Host "  Applied accent:  $(if ($applied) { $applied } else { '(n/a — watcher inactive)' })"
    if ($dynamic -and $live -and $applied -and $live -ne $applied) {
        Write-WinarchyInfo "Out of sync: run 'winarchy accent sync'."
    }
}
