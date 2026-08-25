#Requires -Modules Pester

BeforeAll {
    $script:Root = Split-Path $PSScriptRoot -Parent
    Import-Module (Join-Path $script:Root 'module\Winarchy\Winarchy.psd1') -Force
}

Describe 'ConvertFrom-WinarchyToml' {
    It 'parses sections, quoted keys and scalar types' {
        InModuleScope Winarchy {
            $toml = @'
# comentario
name = "gruvbox"
enabled = true
size = 12
ratio = 0.5

[colors]
background = "#282828"
"LGUG2Z.komorebi" = "0.1.38"
'@
            $d = ConvertFrom-WinarchyToml -Content $toml
            $d['name']              | Should -Be 'gruvbox'
            $d['enabled']           | Should -BeTrue
            $d['size']              | Should -Be 12
            $d['ratio']             | Should -Be 0.5
            $d['colors']['background'] | Should -Be '#282828'
            $d['colors']['LGUG2Z.komorebi'] | Should -Be '0.1.38'
        }
    }

    It 'accumulates array tables in order' {
        InModuleScope Winarchy {
            $d = ConvertFrom-WinarchyToml -Content "[[games]]`nexe = `"a.exe`"`n`n[[games]]`nexe = `"b.exe`""
            $d['games'].Count      | Should -Be 2
            $d['games'][0]['exe']  | Should -Be 'a.exe'
            $d['games'][1]['exe']  | Should -Be 'b.exe'
        }
    }

    It 'strips trailing comments from unquoted values' {
        InModuleScope Winarchy {
            (ConvertFrom-WinarchyToml -Content 'mode = dark  # el modo')['mode'] | Should -Be 'dark'
        }
    }

    It 'throws on a line it cannot represent instead of dropping it' {
        InModuleScope Winarchy {
            { ConvertFrom-WinarchyToml -Content 'no es una linea toml' } | Should -Throw '*Unsupported TOML line*'
        }
    }

    It 'throws on unsupported value forms instead of storing garbage' {
        InModuleScope Winarchy {
            { ConvertFrom-WinarchyToml -Content 'authors = ["x"]' } | Should -Throw '*Unsupported TOML value*'
            { ConvertFrom-WinarchyToml -Content 'owner = { name = "x" }' } | Should -Throw '*Unsupported TOML value*'
        }
    }
}

Describe 'Invoke-WinarchyTemplate' {
    BeforeAll {
        $script:Tpl = Join-Path $TestDrive 'sample.tpl'
    }

    It 'substitutes flat and sectioned tokens' {
        Set-Content $script:Tpl -Value 'bg={{colors.background}} name={{ name }}' -NoNewline
        InModuleScope Winarchy -Parameters @{ Tpl = $script:Tpl } {
            param($Tpl)
            Invoke-WinarchyTemplate -TemplatePath $Tpl -Context @{
                'colors.background' = '#282828'; 'name' = 'gruvbox'
            } | Should -Be 'bg=#282828 name=gruvbox'
        }
    }

    It 'throws on an unresolved token rather than emitting it literally' {
        Set-Content $script:Tpl -Value '{{colors.nope}}' -NoNewline
        InModuleScope Winarchy -Parameters @{ Tpl = $script:Tpl } {
            param($Tpl)
            { Invoke-WinarchyTemplate -TemplatePath $Tpl -Context @{} } |
                Should -Throw '*Unresolved token*colors.nope*'
        }
    }
}

Describe 'ConvertTo-WinarchyFlatContext' {
    It 'flattens nested tables to section.key' {
        InModuleScope Winarchy {
            $flat = ConvertTo-WinarchyFlatContext -Data @{ name = 'x'; colors = @{ accent = '#fff' } }
            $flat['name']          | Should -Be 'x'
            $flat['colors.accent'] | Should -Be '#fff'
        }
    }
}

Describe 'Test-WinarchyThemeData' {
    It 'reports every missing required key' {
        InModuleScope Winarchy {
            $missing = Test-WinarchyThemeData -Theme @{ name = 'x'; mode = 'purple' }
            $missing | Should -Contain '[colors]'
            $missing | Should -Contain '[borders]'
            $missing | Should -Contain "mode (must be 'dark' or 'light')"
        }
    }

    It 'rejects a bad colour and an unknown key in the optional [ui] section' {
        InModuleScope Winarchy {
            $missing = Test-WinarchyThemeData -Theme @{ name = 'x'; mode = 'dark'; ui = @{ system = 'red'; nope = '#ffffff' } }
            $missing | Should -Contain 'ui.system (must be #rrggbb)'
            $missing | Should -Contain 'ui.nope (unknown key)'
        }
    }

    It 'rejects non-integer measures in the optional [bar] section' {
        InModuleScope Winarchy {
            $missing = Test-WinarchyThemeData -Theme @{ name = 'x'; mode = 'dark'; bar = @{ height = '34px' } }
            $missing | Should -Contain 'bar.height (must be an integer)'
        }
    }

    It 'accepts a theme without [bar] (inherits the defaults)' {
        InModuleScope Winarchy {
            $missing = Test-WinarchyThemeData -Theme @{ name = 'x'; mode = 'dark' }
            $missing | Where-Object { $_ -like 'bar.*' } | Should -BeNullOrEmpty
        }
    }

    # Los valores llegan por reemplazo textual a JSON/CSS/YAML/XAML/INI/TOML/Lua generados:
    # un valor que no es un color escribe fuera de su lugar en el archivo generado.
    It 'rejects a <Section> value that is not #rrggbb' -ForEach @(
        @{ Section = 'colors'; Key = 'background'; Value = '#000000", "injected": "yes' }
        @{ Section = 'borders'; Key = 'focused'; Value = 'red' }
    ) {
        InModuleScope Winarchy -Parameters @{ Section = $Section; Key = $Key; Value = $Value } {
            param($Section, $Key, $Value)
            $theme = Import-WinarchyToml -Path (Join-Path (Get-WinarchyThemesDir) 'catppuccin\theme.toml')
            $theme[$Section][$Key] = $Value
            Test-WinarchyThemeData -Theme $theme | Should -Contain "$Section.$Key (must be #rrggbb)"
        }
    }

    It 'rejects a bar.font_family that is not a font name' {
        InModuleScope Winarchy {
            $missing = Test-WinarchyThemeData -Theme @{ name = 'x'; mode = 'dark'; bar = @{ font_family = "Foo`n  bad: yes" } }
            $missing | Should -Contain 'bar.font_family (must be a font name)'
        }
    }

    It 'accepts the font family shipped as the default' {
        InModuleScope Winarchy {
            $missing = Test-WinarchyThemeData -Theme @{ name = 'x'; mode = 'dark'; bar = @{ font_family = 'JetBrainsMono NF' } }
            $missing | Where-Object { $_ -like 'bar.font_family*' } | Should -BeNullOrEmpty
        }
    }
}

Describe 'Expand-WinarchyThemePalette' {
    It 'derives the ANSI slots from the semantic names' {
        InModuleScope Winarchy {
            $t = Expand-WinarchyThemePalette -Theme @{
                name = 'x'; mode = 'dark'
                colors = @{
                    background = '#111111'; foreground = '#eeeeee'; accent = '#3264eb'
                    selection = '#d0d0d0'; muted = '#9e9e9e'; darker_background = '#dedede'
                    light_foreground = '#424242'; bright_foreground = '#000000'
                    red = '#c900c4'; green = '#4a2fd0'; yellow = '#026fde'
                    blue = '#3264eb'; magenta = '#8a4ad7'; cyan = '#0c67de'
                    bright_red = '#f930fb'; bright_green = '#9f85e0'; bright_yellow = '#358fff'
                    bright_blue = '#5482ff'; bright_magenta = '#b363ff'; bright_cyan = '#3986ff'
                }
            }
            $t['colors']['color0']  | Should -Be '#dedede'
            $t['colors']['color1']  | Should -Be '#c900c4'
            $t['colors']['color7']  | Should -Be '#424242'
            $t['colors']['color8']  | Should -Be '#9e9e9e'
            $t['colors']['color15'] | Should -Be '#000000'
            $t['colors']['cursor']  | Should -Be '#eeeeee'
            $t['colors']['selection_background'] | Should -Be '#d0d0d0'
            $t['colors']['selection_foreground'] | Should -Be '#111111'
            $t['borders']['focused'] | Should -Be '#3264eb'
            $t['borders']['urgent']  | Should -Be '#c900c4'
            (Test-WinarchyThemeData -Theme $t) -join ', ' | Should -BeNullOrEmpty
        }
    }

    It 'never overwrites what the theme declared itself' {
        InModuleScope Winarchy {
            $t = Expand-WinarchyThemePalette -Theme @{
                colors = @{ red = '#ff0000'; color1 = '#00ff00'; foreground = '#ffffff'; cursor = '#123456' }
            }
            $t['colors']['color1'] | Should -Be '#00ff00'
            $t['colors']['cursor'] | Should -Be '#123456'
        }
    }

    It 'leaves an ANSI-only theme untouched' {
        InModuleScope Winarchy {
            $before = Import-WinarchyToml -Path (Join-Path (Get-WinarchyThemesDir) 'catppuccin\theme.toml')
            $after = Expand-WinarchyThemePalette -Theme (Import-WinarchyToml -Path (Join-Path (Get-WinarchyThemesDir) 'catppuccin\theme.toml'))
            $after['colors'].Count | Should -Be $before['colors'].Count
            $after['borders']['focused'] | Should -Be $before['borders']['focused']
        }
    }
}

Describe 'Theme VS Code drop-in' {
    BeforeAll {
        $script:VSCodeThemeDir = Join-Path ([IO.Path]::GetTempPath()) "winarchy-vscode-$([guid]::NewGuid())"
        New-Item -ItemType Directory -Path $script:VSCodeThemeDir -Force | Out-Null
    }
    AfterAll { Remove-Item $script:VSCodeThemeDir -Recurse -Force -ErrorAction SilentlyContinue }

    It 'refuses to install an extension that is not publisher.name' {
        $dir = $script:VSCodeThemeDir
        '{ "name": "X", "extension": "evil; rm -rf /" }' | Set-Content (Join-Path $dir 'vscode.json') -Encoding UTF8
        InModuleScope Winarchy -Parameters @{ Dir = $dir } {
            param($Dir)
            Mock Write-WinarchyWarn {}
            Mock Get-WinarchyVSCodeEditors { @() }
            Sync-WinarchyVSCode -ThemeDir $Dir
            Should -Invoke Write-WinarchyWarn -Times 1 -ParameterFilter { $Message -like '*publisher.name*' }
        }
    }

    It 'accepts the extension ids shipped by the themes' {
        InModuleScope Winarchy {
            Get-ChildItem (Get-WinarchyThemesDir) -Directory -Filter * |
                ForEach-Object { Join-Path $_.FullName 'vscode.json' } |
                Where-Object { Test-Path $_ } |
                ForEach-Object { (Get-Content $_ -Raw | ConvertFrom-Json -AsHashtable)['extension'] } |
                Where-Object { $_ } |
                ForEach-Object { $_ | Should -Match $script:VSCodeExtensionPattern }
        }
    }
}

Describe 'Command palette' {
    It 'every command maps to a subcommand the dispatcher knows' {
        InModuleScope Winarchy {
            $psm1 = Get-Content (Join-Path (Get-WinarchyRoot) 'module\Winarchy\Winarchy.psm1') -Raw
            $verbs = [regex]::Matches($psm1, "(?m)^\s{8}'([a-z-]+)'\s*\{") | ForEach-Object { $_.Groups[1].Value }
            $verbs | Should -Not -BeNullOrEmpty
            foreach ($cmd in $script:PaletteCommands) {
                ($cmd.Args -split ' ')[0] | Should -BeIn $verbs
            }
        }
    }

    It 'points the shortcuts at a shim that exists' {
        InModuleScope Winarchy {
            Test-Path (Join-Path (Get-WinarchyRoot) 'bin\winarchy.ps1') | Should -BeTrue
        }
    }

    It 'drops shortcuts that no longer match a command' {
        InModuleScope Winarchy {
            $dir = Get-WinarchyPaletteDir
            $stale = Join-Path $dir 'Winarchy Retired command.lnk'
            Set-Content $stale -Value '' -Encoding UTF8
            Mock Write-WinarchyOk {}
            Sync-WinarchyPalette
            Test-Path $stale | Should -BeFalse
            (Get-ChildItem $dir -Filter '*.lnk').Count | Should -Be $script:PaletteCommands.Count
        }
    }
}

Describe 'Bar callbacks' {
    It 'wires only callbacks the pinned YASB registers' {
        $known = @(
            'do_nothing', 'toggle_label', 'update_label', 'toggle_menu',
            'toggle_calendar', 'next_timezone', 'context_menu', 'toggle_timer', 'toggle_alarm',
            'toggle_mute', 'toggle_volume_menu', 'toggle_mic_menu',
            'toggle_status', 'cycle_status', 'refresh',
            'toggle_brightness_menu', 'toggle_level_next', 'toggle_level_prev',
            'toggle_play_pause', 'open_media_source',
            'toggle_notification', 'clear_notifications'
        )
        $tpl = Get-Content (Join-Path (Split-Path $PSScriptRoot -Parent) 'templates\yasb-config.yaml.tpl') -Raw
        $used = [regex]::Matches($tpl, '(?m)^\s+on_(?:left|middle|right):\s*"([^"]+)"') |
            ForEach-Object { $_.Groups[1].Value } |
            Where-Object { $_ -notlike 'exec *' } |
            Sort-Object -Unique
        $used | Should -Not -BeNullOrEmpty
        $used | Where-Object { $_ -notin $known } | Should -BeNullOrEmpty
    }
}

Describe 'Set-WinarchyJsonSetting' {
    BeforeAll {
        $script:JsonFile = Join-Path ([IO.Path]::GetTempPath()) "winarchy-json-$([guid]::NewGuid()).json"
    }
    AfterAll { Remove-Item $script:JsonFile -Force -ErrorAction SilentlyContinue }

    It 'replaces an existing value and leaves the rest of the file alone' {
        @'
{
  // un comentario que un re-serialize borraria
  "model": "opus",
  "theme": "light"
}
'@ | Set-Content $script:JsonFile -Encoding UTF8
        InModuleScope Winarchy -Parameters @{ Path = $script:JsonFile } {
            param($Path)
            Set-WinarchyJsonSetting -Path $Path -Key 'theme' -Value 'dark'
        }
        $raw = Get-Content $script:JsonFile -Raw
        $raw | Should -Match '"theme":\s*"dark"'
        $raw | Should -Match 'un comentario'
        $raw | Should -Match '"model":\s*"opus"'
    }

    It 'inserts the key when it is not there yet' {
        '{ "model": "opus" }' | Set-Content $script:JsonFile -Encoding UTF8
        InModuleScope Winarchy -Parameters @{ Path = $script:JsonFile } {
            param($Path)
            Set-WinarchyJsonSetting -Path $Path -Key 'workbench.colorTheme' -Value 'Winarchy'
        }
        (Get-Content $script:JsonFile -Raw | ConvertFrom-Json).'workbench.colorTheme' | Should -Be 'Winarchy'
    }

    It 'writes a whole file when there is none' {
        Remove-Item $script:JsonFile -Force -ErrorAction SilentlyContinue
        InModuleScope Winarchy -Parameters @{ Path = $script:JsonFile } {
            param($Path)
            Set-WinarchyJsonSetting -Path $Path -Key 'theme' -Value 'dark'
        }
        (Get-Content $script:JsonFile -Raw | ConvertFrom-Json).theme | Should -Be 'dark'
    }
}

Describe 'Coding agent' {
    BeforeAll {
        $script:AgentPref = InModuleScope Winarchy { Get-WinarchyCodingAgentPath }
        $script:AgentBackup = if (Test-Path $script:AgentPref) { Get-Content $script:AgentPref -Raw } else { $null }
    }
    AfterAll {
        if ($script:AgentBackup) { Set-Content $script:AgentPref -Value $script:AgentBackup -NoNewline -Encoding UTF8 }
        elseif (Test-Path $script:AgentPref) { Remove-Item $script:AgentPref -Force }
    }

    It 'refuses an agent it does not know' {
        { Set-WinarchyCodingAgent -Id 'nope' } | Should -Throw '*Unknown agent*'
    }

    It 'prefers the chosen agent when it is installed' {
        InModuleScope Winarchy {
            Mock Get-WinarchyCodingAgents {
                @([pscustomobject]@{ Id = 'codex'; Label = 'C'; Command = 'codex'; Installed = $true }
                  [pscustomobject]@{ Id = 'claude'; Label = 'CC'; Command = 'claude'; Installed = $true })
            }
            Set-Content (Get-WinarchyCodingAgentPath) -Value 'claude' -NoNewline -Encoding UTF8
            (Get-WinarchyCodingAgent).Id | Should -Be 'claude'
        }
    }

    # La eleccion sobrevive a desinstalar ese agente: el hotkey tiene que seguir sirviendo.
    It 'falls back to the first installed one when the chosen agent is gone' {
        InModuleScope Winarchy {
            Mock Get-WinarchyCodingAgents {
                @([pscustomobject]@{ Id = 'codex'; Label = 'C'; Command = 'codex'; Installed = $true }
                  [pscustomobject]@{ Id = 'claude'; Label = 'CC'; Command = 'claude'; Installed = $false })
            }
            Set-Content (Get-WinarchyCodingAgentPath) -Value 'claude' -NoNewline -Encoding UTF8
            (Get-WinarchyCodingAgent).Id | Should -Be 'codex'
        }
    }

    It 'launches nothing when no agent is installed' {
        InModuleScope Winarchy {
            Mock Get-WinarchyCodingAgents { @() }
            Mock Start-Process {}
            Mock Write-WinarchyWarn {}
            Start-WinarchyCodingAgent
            Should -Not -Invoke Start-Process
            Should -Invoke Write-WinarchyWarn -Times 1
        }
    }

    It 'launches the agent command through Windows Terminal' {
        InModuleScope Winarchy {
            Mock Get-WinarchyCodingAgents { @([pscustomobject]@{ Id = 'codex'; Label = 'C'; Command = 'codex'; Installed = $true }) }
            Mock Start-Process {}
            Start-WinarchyCodingAgent
            Should -Invoke Start-Process -Times 1 -ParameterFilter {
                $FilePath -eq 'wt.exe' -and $ArgumentList -contains 'codex'
            }
        }
    }
}

Describe 'Backgrounds' {
    BeforeAll {
        $script:BgTheme = Get-WinarchyThemes | Where-Object { (Get-WinarchyBackgrounds -Name $_).Count -gt 1 } | Select-Object -First 1
        $script:BgPrefs = InModuleScope Winarchy { Get-WinarchyBackgroundPrefsPath }
        $script:BgPrefsBackup = if (Test-Path $script:BgPrefs) { Get-Content $script:BgPrefs -Raw } else { $null }
    }
    AfterAll {
        if ($script:BgPrefsBackup) { Set-Content $script:BgPrefs -Value $script:BgPrefsBackup -NoNewline -Encoding UTF8 }
        elseif (Test-Path $script:BgPrefs) { Remove-Item $script:BgPrefs -Force }
    }

    It 'falls back to the first background when nothing was chosen' {
        if (Test-Path $script:BgPrefs) { Remove-Item $script:BgPrefs -Force }
        Get-WinarchyBackground -Name $script:BgTheme | Should -Be (Get-WinarchyBackgrounds -Name $script:BgTheme)[0]
    }

    It 'round-trips a choice through the repo TOML parser' {
        InModuleScope Winarchy -Parameters @{ Theme = $script:BgTheme } {
            param($Theme)
            $second = (Get-WinarchyBackgrounds -Name $Theme)[1]
            Export-WinarchyBackgroundPrefs -Pref @{ $Theme = $second }
            Get-WinarchyBackground -Name $Theme | Should -Be $second
        }
    }

    # Un theme puede perder una imagen en un update y dejar la elección apuntando a la nada.
    It 'ignores a choice whose file is gone' {
        InModuleScope Winarchy -Parameters @{ Theme = $script:BgTheme } {
            param($Theme)
            Export-WinarchyBackgroundPrefs -Pref @{ $Theme = 'deleted-upstream.png' }
            Get-WinarchyBackground -Name $Theme | Should -Be (Get-WinarchyBackgrounds -Name $Theme)[0]
        }
    }

    It 'returns nothing for a theme that ships no backgrounds' {
        $bare = Get-WinarchyThemes | Where-Object { (Get-WinarchyBackgrounds -Name $_).Count -eq 0 } | Select-Object -First 1
        Get-WinarchyBackground -Name $bare | Should -BeNullOrEmpty
    }

    It 'cycles and wraps around' {
        InModuleScope Winarchy -Parameters @{ Theme = $script:BgTheme } {
            param($Theme)
            Mock Get-WinarchyCurrentTheme { $Theme }
            Mock Set-WinarchyWallpaper {}
            Mock Write-WinarchyOk {}
            $all = Get-WinarchyBackgrounds -Name $Theme
            Export-WinarchyBackgroundPrefs -Pref @{ $Theme = $all[-1] }
            Set-WinarchyBackground -Next
            Get-WinarchyBackground -Name $Theme | Should -Be $all[0]
        }
    }

    It 'refuses a background the theme does not ship' {
        InModuleScope Winarchy -Parameters @{ Theme = $script:BgTheme } {
            param($Theme)
            Mock Get-WinarchyCurrentTheme { $Theme }
            { Set-WinarchyBackground -File 'nope.png' } | Should -Throw '*has no background*'
        }
    }
}

Describe 'Theme folder format' {
    BeforeDiscovery {
        $themesDir = Join-Path (Split-Path $PSScriptRoot -Parent) 'themes'
        $script:FormatCases = @(
            Get-ChildItem $themesDir -Directory |
                Where-Object { Test-Path (Join-Path $_.FullName 'theme.toml') } |
                ForEach-Object { @{ Name = $_.Name } }
        )
    }

    It '<Name> contains only files the engine reads' -ForEach $script:FormatCases {
        $allowed = @('theme.toml', 'preview.png', 'vscode.json', 'jetbrains.icls', 'wallpaper-engine.txt', 'backgrounds')
        Get-ChildItem (Join-Path (Split-Path $PSScriptRoot -Parent) "themes\$Name") |
            Where-Object { $_.Name -notin $allowed -and $_.Name -notmatch '^wallpaper\.(jpg|jpeg|png|bmp)$' } |
            ForEach-Object { $_.Name } | Should -BeNullOrEmpty
    }
}

Describe 'Theme defaults' {
    It 'fills the [bar] section when the theme omits it' {
        InModuleScope Winarchy {
            $theme = @{ name = 'x'; mode = 'dark'; colors = @{ accent = '#7aa2f7'; background = '#1a1b26' } }
            $ctx = Build-WinarchyThemeContext -Theme $theme -ThemeDir (Join-Path (Get-WinarchyThemesDir) 'tokyo-night')
            $ctx['bar.height'] | Should -Be 34
            $ctx['bar.font_family'] | Should -Be 'JetBrainsMono NF'
        }
    }

    It 'falls the [ui] colours back to the ANSI slots the bar used to read' {
        InModuleScope Winarchy {
            $theme = @{ name = 'x'; mode = 'dark'
                colors = @{ accent = '#7aa2f7'; background = '#1a1b26'; color4 = '#111111'; color5 = '#222222'; color6 = '#333333' } }
            $ctx = Build-WinarchyThemeContext -Theme $theme -ThemeDir (Join-Path (Get-WinarchyThemesDir) 'tokyo-night')
            $ctx['ui.system'] | Should -Be '#333333'
            $ctx['ui.media']  | Should -Be '#222222'
            $ctx['ui.net']    | Should -Be '#111111'
        }
    }

    It 'lets a theme decouple the bar colours from its ANSI palette' {
        InModuleScope Winarchy {
            $theme = @{ name = 'x'; mode = 'dark'; ui = @{ system = '#abcdef' }
                colors = @{ accent = '#7aa2f7'; background = '#1a1b26'; color4 = '#111111'; color5 = '#222222'; color6 = '#333333' } }
            $ctx = Build-WinarchyThemeContext -Theme $theme -ThemeDir (Join-Path (Get-WinarchyThemesDir) 'tokyo-night')
            $ctx['ui.system'] | Should -Be '#abcdef'
            $ctx['ui.net']    | Should -Be '#111111'
        }
    }

    It 'lets the theme override a default' {
        InModuleScope Winarchy {
            $theme = @{ name = 'x'; mode = 'dark'; bar = @{ height = 40 }
                colors = @{ accent = '#7aa2f7'; background = '#1a1b26' } }
            $ctx = Build-WinarchyThemeContext -Theme $theme -ThemeDir (Join-Path (Get-WinarchyThemesDir) 'tokyo-night')
            $ctx['bar.height'] | Should -Be 40
            $ctx['bar.font_size'] | Should -Be 13
        }
    }
}

Describe 'Get-WinarchyGames' {
    It 'includes [[launchers]], normalizes .exe and de-duplicates' {
        InModuleScope Winarchy {
            $toml = Join-Path $TestDrive 'games.toml'
            Set-Content $toml -Value @'
[[launchers]]
exe = "steam.exe"

[[games]]
exe = "Hades"

[[games]]
exe = "steam.exe"

[[games]]
exe = "Placeholder*.exe"
'@
            Mock Get-WinarchyRoot { $TestDrive }
            $games = @(Get-WinarchyGames)
            $games | Should -Contain 'steam.exe'
            $games | Should -Contain 'Hades.exe'
            $games | Should -Not -Contain 'Placeholder*.exe'
            @($games | Where-Object { $_ -eq 'steam.exe' }).Count | Should -Be 1
        }
    }

    It 'returns an empty array when games.toml is absent' {
        InModuleScope Winarchy {
            Mock Get-WinarchyRoot { Join-Path $TestDrive 'nowhere' }
            @(Get-WinarchyGames).Count | Should -Be 0
        }
    }

    # Regresión: con `return , @($list)` la función emitía el array como UN objeto al
    # pipear, así que @(...).Count daba 1 y Where-Object recibía todo junto. Andaba solo
    # por accidente de cómo PowerShell compara un array contra un string.
    It 'survives being piped and counted' {
        InModuleScope Winarchy {
            $direct = @(Get-WinarchyGames)
            $direct.Count | Should -BeGreaterThan 10
            @(Get-WinarchyGames | Where-Object { $_ -like '*.exe' }).Count | Should -Be $direct.Count
            @(foreach ($g in (Get-WinarchyGames)) { $g }).Count           | Should -Be $direct.Count
        }
    }
}

Describe 'Window placement' {
    BeforeAll {
        $script:State = Get-Content (Join-Path $PSScriptRoot 'fixtures\komorebi-state.json') -Raw | ConvertFrom-Json
    }

    It 'flattens the komorebi state to exe + monitor + workspace' {
        InModuleScope Winarchy -Parameters @{ State = $script:State } {
            param($State)
            $p = @(Get-WinarchyWindowPlacements -State $State)
            $p.Count | Should -Be 5
            $discord = $p | Where-Object { $_.Exe -eq 'Discord.exe' }
            $discord.Monitor       | Should -Be 'BBB2222-5&bbbbbbb&1&UID4353'
            $discord.Workspace     | Should -Be 0
            $discord.WorkspaceName | Should -Be 'A'

            $whatsapp = $p | Where-Object { $_.Exe -eq 'WhatsApp.Root.exe' }
            $whatsapp.Workspace | Should -Be 1
            $whatsapp.Monitor   | Should -Be 'AAA1111-5&aaaaaaa&1&UID4355'
        }
    }

    It 'keeps the first workspace when an app spans several' {
        InModuleScope Winarchy -Parameters @{ State = $script:State } {
            param($State)
            # Code.exe aparece en el monitor 0/ws 0 y en el monitor 1/ws 0
            $code = @(Get-WinarchyWindowPlacements -State $State | Where-Object { $_.Exe -eq 'Code.exe' })
            $code.Count      | Should -Be 1
            $code[0].Monitor | Should -Be 'AAA1111-5&aaaaaaa&1&UID4355'
        }
    }

    It 'maps device_id to monitor index' {
        InModuleScope Winarchy -Parameters @{ State = $script:State } {
            param($State)
            $map = Get-WinarchyMonitorIndexMap -State $State
            $map['AAA1111-5&aaaaaaa&1&UID4355'] | Should -Be 0
            $map['BBB2222-5&bbbbbbb&1&UID4353'] | Should -Be 1
            $map.ContainsKey('NOPE')             | Should -BeFalse
        }
    }

    It 'round-trips preferences through the repo TOML parser' {
        InModuleScope Winarchy {
            Mock Get-WinarchyRoot { $TestDrive }
            New-Item -ItemType Directory -Path (Join-Path $TestDrive 'config') -Force | Out-Null
            $prefs = @(
                [pscustomobject]@{ Exe = 'Discord.exe'; Monitor = 'BBB2222-5&b&1&UID1'; Workspace = 2; WorkspaceName = 'A'; Slot = 0; Pin = $false }
                [pscustomobject]@{ Exe = 'Code.exe';    Monitor = 'AAA1111-5&a&1&UID2'; Workspace = 0; WorkspaceName = '1'; Slot = $null; Pin = $true }
            )
            Export-WinarchyWindowPrefs -Pref $prefs
            $back = @(Import-WinarchyWindowPrefs)
            $back.Count | Should -Be 2
            $discord = $back | Where-Object { $_.Exe -eq 'Discord.exe' }
            $discord.Workspace | Should -Be 2
            $discord.Monitor   | Should -Be 'BBB2222-5&b&1&UID1'
            $discord.Pin       | Should -BeFalse
            $discord.Slot      | Should -Be 0
            ($back | Where-Object { $_.Exe -eq 'Code.exe' }).Slot | Should -BeNullOrEmpty
            ($back | Where-Object { $_.Exe -eq 'Code.exe' }).Pin | Should -BeTrue
        }
    }

    It 'returns an empty array when no preferences file exists' {
        InModuleScope Winarchy {
            Mock Get-WinarchyRoot { Join-Path $TestDrive 'nowhere' }
            @(Import-WinarchyWindowPrefs).Count | Should -Be 0
        }
    }

    It 'forgetting an entry leaves the others intact' {
        InModuleScope Winarchy {
            Mock Get-WinarchyRoot { $TestDrive }
            New-Item -ItemType Directory -Path (Join-Path $TestDrive 'config') -Force | Out-Null
            Export-WinarchyWindowPrefs -Pref @(
                [pscustomobject]@{ Exe = 'Discord.exe'; Monitor = 'M1'; Workspace = 0; WorkspaceName = 'A'; Slot = $null; Pin = $false }
                [pscustomobject]@{ Exe = 'Code.exe';    Monitor = 'M2'; Workspace = 1; WorkspaceName = '1'; Slot = $null; Pin = $false }
            )
            Remove-WinarchyWindowPref -Exe 'Discord'
            $back = @(Import-WinarchyWindowPrefs)
            $back.Count   | Should -Be 1
            $back[0].Exe  | Should -Be 'Code.exe'
        }
    }

    It 'injects the rules into the generated komorebi.json' {
        InModuleScope Winarchy -Parameters @{ State = $script:State } {
            param($State)
            Mock Get-WinarchyRoot { $TestDrive }
            Mock Test-WinarchyProcess { $true }
            Mock Get-WinarchyKomorebiState { $State }
            New-Item -ItemType Directory -Path (Join-Path $TestDrive 'config') -Force | Out-Null
            Export-WinarchyWindowPrefs -Pref @(
                [pscustomobject]@{ Exe = 'Discord.exe'; Monitor = 'BBB2222-5&bbbbbbb&1&UID4353'; Workspace = 0; WorkspaceName = 'A'; Slot = 0;     Pin = $false }
                [pscustomobject]@{ Exe = 'Code.exe';    Monitor = 'AAA1111-5&aaaaaaa&1&UID4355'; Workspace = 1; WorkspaceName = '2'; Slot = $null; Pin = $true }
                [pscustomobject]@{ Exe = 'Ghost.exe';   Monitor = 'NOT-CONNECTED';               Workspace = 0; WorkspaceName = 'X'; Slot = $null; Pin = $false }
            )
            $base = '{"monitors":[{"workspaces":[{"name":"1"},{"name":"2"}]},{"workspaces":[{"name":"A"}]}]}'
            $out = Add-WinarchyWindowRulesToKomorebiJson -Json $base | ConvertFrom-Json

            # initial por default, en el monitor/workspace correctos
            $out.monitors[1].workspaces[0].initial_workspace_rules.id | Should -Be 'Discord.exe'
            # pin = true usa la regla permanente
            $out.monitors[0].workspaces[1].workspace_rules.id | Should -Be 'Code.exe'
            # un monitor ausente no debe caer en otra pantalla
            ($out | ConvertTo-Json -Depth 20) | Should -Not -Match 'Ghost.exe'
        }
    }

    It 'leaves the json untouched when komorebi is not running' {
        InModuleScope Winarchy {
            Mock Test-WinarchyProcess { $false }
            $base = '{"monitors":[{"workspaces":[{"name":"1"}]}]}'
            Add-WinarchyWindowRulesToKomorebiJson -Json $base | Should -Be $base
        }
    }

    # Hermético a propósito: config/komorebi/komorebi.json es generado y gitignored, así
    # que en un checkout limpio no existe. Apoyarse en él hacía pasar el test solo en una
    # máquina con el stack ya instalado.
    It 'excludes both games.toml entries and komorebi ignore-rule exes' {
        InModuleScope Winarchy {
            Mock Get-WinarchyRoot { $TestDrive }
            New-Item -ItemType Directory -Path (Join-Path $TestDrive 'config\komorebi') -Force | Out-Null
            Set-Content (Join-Path $TestDrive 'games.toml') -Value @'
[[games]]
exe = "Hearthstone.exe"

[[launchers]]
exe = "Steam.exe"
'@
            Set-Content (Join-Path $TestDrive 'config\komorebi\komorebi.json') -Value @'
{ "ignore_rules": [
    { "kind": "Exe", "id": "Flow.Launcher.exe", "matching_strategy": "Equals" },
    { "kind": "Class", "id": "#32770", "matching_strategy": "Equals" }
] }
'@
            $skip = @(Get-WinarchyUnplaceableExes)
            $skip | Should -Contain 'Hearthstone.exe'      # [[games]]
            $skip | Should -Contain 'Steam.exe'            # [[launchers]]
            $skip | Should -Contain 'Flow.Launcher.exe'    # ignore_rules
            $skip | Should -Not -Contain '#32770'          # solo las reglas de tipo Exe
        }
    }

    It 'survives a missing komorebi.json (clean checkout)' {
        InModuleScope Winarchy {
            Mock Get-WinarchyRoot { $TestDrive }
            New-Item -ItemType Directory -Path $TestDrive -Force | Out-Null
            Set-Content (Join-Path $TestDrive 'games.toml') -Value "[[games]]`nexe = `"Hearthstone.exe`""
            { Get-WinarchyUnplaceableExes } | Should -Not -Throw
            @(Get-WinarchyUnplaceableExes) | Should -Contain 'Hearthstone.exe'
        }
    }
}

Describe 'games.toml (shipped)' {
    It 'parses and every entry carries an exe' {
        InModuleScope Winarchy {
            $data = Import-WinarchyToml -Path (Join-Path (Get-WinarchyRoot) 'games.toml')
            foreach ($section in @('games', 'launchers')) {
                foreach ($entry in $data[$section]) { $entry['exe'] | Should -Not -BeNullOrEmpty }
            }
        }
    }

    It 'has no exe listed in both sections' {
        InModuleScope Winarchy {
            $data = Import-WinarchyToml -Path (Join-Path (Get-WinarchyRoot) 'games.toml')
            $games = @($data['games'] | ForEach-Object { $_['exe'] })
            $dupes = @($data['launchers'] | ForEach-Object { $_['exe'] } | Where-Object { $games -contains $_ })
            $dupes -join ', ' | Should -BeNullOrEmpty
        }
    }

    It 'uses one line ending throughout' {
        $bytes = [System.IO.File]::ReadAllBytes((Join-Path $script:Root 'games.toml'))
        $bare = 0
        for ($i = 0; $i -lt $bytes.Length; $i++) {
            if ($bytes[$i] -eq 0x0A -and ($i -eq 0 -or $bytes[$i - 1] -ne 0x0D)) { $bare++ }
        }
        ($bare -eq 0 -or $bare -eq ($bytes | Where-Object { $_ -eq 0x0A }).Count) | Should -BeTrue
    }
}

Describe 'Game registration round-trip' {
    It 'add then remove leaves games.toml byte for byte identical' {
        InModuleScope Winarchy {
            $toml = Join-Path $TestDrive 'games.toml'
            Set-Content -Path $toml -Value "[[games]]`r`nexe = `"Existing.exe`"`r`n" -Encoding UTF8 -NoNewline
            $before = (Get-FileHash $toml -Algorithm SHA256).Hash

            Mock Get-WinarchyRoot { Split-Path $toml -Parent }
            Mock Update-WinarchyKomorebiRules { }
            Mock Update-WinarchyQuickAccentExclusion { }

            Add-WinarchyGame -Exe 'RoundTrip' | Out-Null
            (Get-Content $toml -Raw) | Should -Match 'RoundTrip\.exe'
            Remove-WinarchyGame -Exe 'RoundTrip' | Out-Null
            (Get-FileHash $toml -Algorithm SHA256).Hash | Should -Be $before
        }
    }
}

Describe 'Shipped themes' {
    BeforeDiscovery {
        $themesDir = Join-Path (Split-Path $PSScriptRoot -Parent) 'themes'
        $script:ThemeCases = @(
            Get-ChildItem $themesDir -Directory |
                Where-Object { Test-Path (Join-Path $_.FullName 'theme.toml') } |
                ForEach-Object { @{ Name = $_.Name } }
        )
    }

    It 'there is at least one theme to test' {
        (Get-WinarchyThemes).Count | Should -BeGreaterThan 0
    }

    It '<Name> has a valid theme.toml' -ForEach $script:ThemeCases {
        InModuleScope Winarchy -Parameters @{ Name = $Name } {
            param($Name)
            $data = Expand-WinarchyThemePalette -Theme (Import-WinarchyToml -Path (Join-Path (Get-WinarchyThemesDir) "$Name\theme.toml"))
            (Test-WinarchyThemeData -Theme $data) -join ', ' | Should -BeNullOrEmpty
        }
    }

    # Cubre de punta a punta lo que antes solo se sabía al aplicar un theme: los 11
    # templates renderizan sin tokens sin resolver y el JSON/XML generado parsea.
    It '<Name> renders every template (theme set -StageOnly)' -ForEach $script:ThemeCases {
        { Set-WinarchyTheme -Name $Name -StageOnly } | Should -Not -Throw
        # config.yaml de YASB no tiene validador de sintaxis en el pipeline (no hay parser
        # YAML en PowerShell): al menos la geometria inyectada tiene que quedar numerica,
        # que es lo unico que un theme puede romper ahi.
        $staged = Join-Path (Get-WinarchyStateDir) 'staging\config.yaml'
        (Get-Content $staged -Raw) | Should -Match '(?m)^\s+height: \d+\s*$'
    }
}

Describe 'Set-WinarchyTheme' {
    It 'rejects an unknown theme before touching anything' {
        { Set-WinarchyTheme -Name 'does-not-exist' -StageOnly } | Should -Throw "*does not exist*"
    }

    It 'rejects -LightRefresh on a static theme' {
        InModuleScope Winarchy {
            $static = Get-WinarchyThemes | Where-Object { -not (Test-WinarchyDynamicTheme -Name $_) } | Select-Object -First 1
            { Set-WinarchyTheme -Name $static -LightRefresh -StageOnly } | Should -Throw '*only applies to dynamic themes*'
        }
    }
}

Describe 'versions.lock.toml' {
    It 'pins the core components' {
        InModuleScope Winarchy {
            $lock = Import-WinarchyToml -Path (Join-Path (Get-WinarchyRoot) 'versions.lock.toml')
            $lock['core'].Keys | Should -Contain 'komorebi'
        }
    }
}

Describe 'App rules (community ASC + user layer)' {
    BeforeAll {
        # Capas sinteticas: el merge se prueba solo, sin depender de las 226 entradas reales.
        $script:MakeLayer = {
            param($Entries)
            [object[]]@(foreach ($e in $Entries) {
                    @{ Category = $e[0]
                        Rule     = [pscustomobject]@{ kind = 'Exe'; id = $e[1]; matching_strategy = 'Equals' }
                        Source   = $e[2]
                    }
                })
        }
    }

    It 'keeps the layers separate instead of flattening them' {
        InModuleScope Winarchy {
            $config = [pscustomobject]@{ ignore_rules = @() }
            $layers = Get-WinarchyAppRuleLayers -Config $config
            # Con [object[]] las capas vacias se desenrollan y la precedencia se pierde en
            # silencio: el conteo es lo unico que lo delata.
            $layers.Count | Should -Be 3
        }
    }

    It 'lets a higher layer move a window to another placement category' {
        InModuleScope Winarchy -Parameters @{ Make = $script:MakeLayer } {
            param($Make)
            $merged = Merge-WinarchyAppRules -Layers @(
                (& $Make @(, @('manage_rules', 'GalaxyClient.exe', 'asc:GOG Galaxy'))),
                (& $Make @(, @('ignore_rules', 'GalaxyClient.exe', 'winarchy')))
            )
            @($merged['ignore_rules']).Count | Should -Be 1
            # No alcanza con sumar el ignore: las listas de komorebi son aditivas, asi que
            # el manage de la capa de abajo TIENE que desaparecer.
            @($merged['manage_rules']).Count | Should -Be 0
        }
    }

    It 'unions attribute categories instead of overriding them' {
        InModuleScope Winarchy -Parameters @{ Make = $script:MakeLayer } {
            param($Make)
            $merged = Merge-WinarchyAppRules -Layers @(
                (& $Make @(, @('layered_applications', 'Discord.exe', 'asc:Discord'))),
                (& $Make @(, @('ignore_rules', 'Discord.exe', 'user:rules.toml')))
            )
            @($merged['ignore_rules']).Count | Should -Be 1
            @($merged['layered_applications']).Count | Should -Be 1
        }
    }

    It 'deduplicates identical rules across layers' {
        InModuleScope Winarchy -Parameters @{ Make = $script:MakeLayer } {
            param($Make)
            $merged = Merge-WinarchyAppRules -Layers @(
                (& $Make @(, @('ignore_rules', 'Steam.exe', 'asc:Steam'))),
                (& $Make @(, @('ignore_rules', 'steam.exe', 'winarchy')))
            )
            @($merged['ignore_rules']).Count | Should -Be 1
        }
    }

    It 'drops a whole ASC app listed in [[disable]]' {
        InModuleScope Winarchy {
            $rules = ConvertFrom-WinarchyToml -Content "[[disable]]`nasc = `"GOG Galaxy`""
            $disabled = @(Get-WinarchyDisabledAscApps -UserRules $rules)
            $disabled | Should -Contain 'GOG Galaxy'
            $layer = @(Get-WinarchyAscLayer -Disabled $disabled)
            @($layer | Where-Object { $_.Source -eq 'asc:GOG Galaxy' }).Count | Should -Be 0
            # el resto de ASC sigue entero
            @($layer | Where-Object { $_.Source -eq 'asc:Zen Browser' }).Count | Should -BeGreaterThan 0
        }
    }

    It 'rejects a rules.toml entry without exe/class/title' {
        InModuleScope Winarchy {
            $rules = ConvertFrom-WinarchyToml -Content "[[ignore]]`nnope = `"x`""
            { Get-WinarchyUserRuleLayer -UserRules $rules } | Should -Throw '*exe/class/title*'
        }
    }

    It 'vendors the ASC revision pinned in versions.lock.toml' {
        InModuleScope Winarchy {
            $lock = Import-WinarchyToml -Path (Join-Path (Get-WinarchyRoot) 'versions.lock.toml')
            $lock.ContainsKey('asc') | Should -BeTrue
            (Get-FileHash (Get-WinarchyAscPath) -Algorithm SHA256).Hash.ToLower() | Should -Be $lock['asc']['sha256']
        }
    }

    It 'compiles the community rules into the generated komorebi.json' {
        Set-WinarchyTheme -Name (Get-WinarchyThemes | Select-Object -First 1) -StageOnly
        $json = Get-Content (Join-Path (Get-WinarchyStateDir) 'staging\komorebi.json') -Raw -Encoding UTF8
        # MozillaDialogClass solo puede venir de ASC.
        $json | Should -Match 'MozillaDialogClass'
        ($json | ConvertFrom-Json).layered_applications.id | Should -Contain 'Discord.exe'
    }
}

Describe 'Keybindings documentation' {
    # docs/keybindings.md se desincronizó del .ahk: SUPER+Shift+V y +G (grabación de
    # pantalla) vivieron sin documentar. Este test compara la fuente con la doc.
    BeforeAll {
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk')
        $script:Doc = Get-Content (Join-Path $script:Root 'docs\keybindings.md') -Raw
        # Las teclas de dígito y las flechas están colapsadas a propósito en la doc
        # (SUPER+1..9, SUPER+←/→/↑/↓), así que se verifican por su forma colapsada.
        $collapsed = '^([1-9]|Left|Right|Up|Down)$'
        $script:Hotkeys = foreach ($line in $ahk) {
            if ($line -notmatch '^#(?<mods>[!+^]*)(?<key>[^:\s]+)::') { continue }
            if ($Matches['key'] -match $collapsed) { continue }
            $label = 'SUPER'
            if ($Matches['mods'] -like '*!*') { $label += '+Alt' }
            if ($Matches['mods'] -like '*^*') { $label += '+Ctrl' }
            if ($Matches['mods'] -like '*+*') { $label += '+Shift' }
            # AHK acepta Enter y Return como la misma tecla; la doc usa Return (convención
            # heredada de Omarchy/Hyprland).
            $key = $Matches['key'] -replace '^Enter$', 'Return'
            "$label+$key"
        }
    }

    It 'finds hotkeys to check' {
        @($script:Hotkeys).Count | Should -BeGreaterThan 15
    }

    It 'documents every SUPER hotkey defined in winarchy.ahk' {
        $undocumented = @($script:Hotkeys | Where-Object { $script:Doc -notmatch [regex]::Escape($_) })
        $undocumented -join ', ' | Should -BeNullOrEmpty
    }

    It 'documents the collapsed workspace and direction ranges' {
        foreach ($range in 'SUPER+1..9', 'SUPER+Shift+1..9') {
            $script:Doc | Should -Match ([regex]::Escape($range))
        }
    }
}

Describe 'Module manifest' {
    It 'exports every function the CLI dispatcher calls' {
        $manifest = Test-ModuleManifest (Join-Path $script:Root 'module\Winarchy\Winarchy.psd1')
        $manifest.ExportedFunctions.Keys | Should -Contain 'Invoke-Winarchy'
    }

    It 'has a ModuleVersion matching the release tag format' {
        $manifest = Test-ModuleManifest (Join-Path $script:Root 'module\Winarchy\Winarchy.psd1')
        $manifest.Version.ToString() | Should -Match '^\d+\.\d+\.\d+$'
    }
}

Describe 'Window slots' {
    BeforeAll {
        $script:State = Get-Content (Join-Path $PSScriptRoot 'fixtures\komorebi-state.json') -Raw | ConvertFrom-Json
        # El monitor BBB (índice 1), workspace 0: [Discord] [Hearthstone+Code].
        $script:Monitor2 = 'BBB2222-5&bbbbbbb&1&UID4353'
        $script:Swapped = Get-Content (Join-Path $PSScriptRoot 'fixtures\komorebi-state.json') -Raw | ConvertFrom-Json
        $ws = $script:Swapped.monitors.elements[1].workspaces.elements[0]
        $ws.containers.elements = @($ws.containers.elements[1], $ws.containers.elements[0])
    }

    It 'captures the container index as the slot' {
        InModuleScope Winarchy -Parameters @{ State = $script:State } {
            param($State)
            $p = @(Get-WinarchyWindowPlacements -State $State)
            ($p | Where-Object { $_.Exe -eq 'Discord.exe' }).Slot     | Should -Be 0
            ($p | Where-Object { $_.Exe -eq 'Hearthstone.exe' }).Slot | Should -Be 1
        }
    }

    It 'reads a preferences file written before slots existed' {
        InModuleScope Winarchy {
            Mock Get-WinarchyRoot { $TestDrive }
            New-Item -ItemType Directory -Path (Join-Path $TestDrive 'config') -Force | Out-Null
            $legacy = "[[windows]]`nexe = `"Discord.exe`"`nmonitor = `"M1`"`nworkspace = 0"
            Set-Content -Path (Join-Path $TestDrive 'config\windows.toml') -Encoding UTF8 -Value $legacy
            $back = @(Import-WinarchyWindowPrefs)
            $back.Count   | Should -Be 1
            $back[0].Slot | Should -BeNullOrEmpty
        }
    }

    It 'emits no moves when every window already sits in its slot' {
        InModuleScope Winarchy -Parameters @{ State = $script:State; Monitor = $script:Monitor2 } {
            param($State, $Monitor)
            $prefs = @([pscustomobject]@{ Exe = 'Discord.exe'; Monitor = $Monitor; Workspace = 0; WorkspaceName = 'A'; Slot = 0; Pin = $false })
            @(Get-WinarchySlotMoves -State $State -Pref $prefs).Count | Should -Be 0
        }
    }

    It 'moves the anchor back to its slot' {
        InModuleScope Winarchy -Parameters @{ State = $script:Swapped; Monitor = $script:Monitor2 } {
            param($State, $Monitor)
            $prefs = @([pscustomobject]@{ Exe = 'Discord.exe'; Monitor = $Monitor; Workspace = 0; WorkspaceName = 'A'; Slot = 0; Pin = $false })
            $moves = @(Get-WinarchySlotMoves -State $State -Pref $prefs)
            $moves.Count      | Should -Be 1
            $moves[0].From    | Should -Be 1
            $moves[0].To      | Should -Be 0
            $moves[0].Monitor | Should -Be 1
        }
    }

    It 'survives a focused index that points past the last element' {
        InModuleScope Winarchy -Parameters @{ State = $script:State } {
            param($State)
            $State.monitors | Add-Member -NotePropertyName focused -NotePropertyValue @($State.monitors.elements).Count -Force
            { Get-WinarchyFocusedHwnd -State $State } | Should -Not -Throw
            Get-WinarchyFocusedHwnd -State $State | Should -BeNullOrEmpty
        }
    }

    It 'moves the app that is not open off a slot another one already holds' {
        InModuleScope Winarchy -Parameters @{ Monitor = $script:Monitor2 } {
            param($Monitor)
            $prefs = @(
                [pscustomobject]@{ Exe = 'Absent.exe'; Monitor = $Monitor; Workspace = 0; WorkspaceName = 'A'; Slot = 0; Pin = $false }
                [pscustomobject]@{ Exe = 'Discord.exe'; Monitor = $Monitor; Workspace = 0; WorkspaceName = 'A'; Slot = 0; Pin = $false }
            )
            $live = @{ "Discord.exe|$Monitor|0" = 0 }
            Resolve-WinarchySlotConflicts -Pref $prefs -Live $live | Should -BeTrue
            ($prefs | Where-Object { $_.Exe -eq 'Discord.exe' }).Slot | Should -Be 0
            ($prefs | Where-Object { $_.Exe -eq 'Absent.exe' }).Slot  | Should -Be 1
            Resolve-WinarchySlotConflicts -Pref $prefs -Live $live | Should -BeFalse
        }
    }

    It 'does not fight itself when two apps claim the same slot' {
        InModuleScope Winarchy -Parameters @{ State = $script:Swapped; Monitor = $script:Monitor2 } {
            param($State, $Monitor)
            $prefs = @(
                [pscustomobject]@{ Exe = 'Discord.exe'; Monitor = $Monitor; Workspace = 0; WorkspaceName = 'A'; Slot = 0; Pin = $false }
                [pscustomobject]@{ Exe = 'Hearthstone.exe'; Monitor = $Monitor; Workspace = 0; WorkspaceName = 'A'; Slot = 0; Pin = $false }
            )
            $moves = @(Get-WinarchySlotMoves -State $State -Pref $prefs)
            $moves.Count   | Should -Be 1
            $moves[0].Exe  | Should -Be 'Discord.exe'
        }
    }

    It 'ignores preferences without a slot and monitors that are not connected' {
        InModuleScope Winarchy -Parameters @{ State = $script:Swapped; Monitor = $script:Monitor2 } {
            param($State, $Monitor)
            $noSlot = @([pscustomobject]@{ Exe = 'Discord.exe'; Monitor = $Monitor; Workspace = 0; WorkspaceName = 'A'; Slot = $null; Pin = $false })
            @(Get-WinarchySlotMoves -State $State -Pref $noSlot).Count | Should -Be 0

            $absent = @([pscustomobject]@{ Exe = 'Discord.exe'; Monitor = 'NOT-CONNECTED'; Workspace = 0; WorkspaceName = 'A'; Slot = 0; Pin = $false })
            @(Get-WinarchySlotMoves -State $State -Pref $absent).Count | Should -Be 0
        }
    }

    It 'navigates by index, never by exe, to execute a move' {
        InModuleScope Winarchy {
            # foco en el container 0, hay que mover el container 2 hasta el 0
            $commands = @(Get-WinarchySlotCommands -Move ([pscustomobject]@{
                        Exe = 'Discord.exe'; Monitor = 1; Workspace = 0; From = 2; To = 0; Focused = 0; Count = 3 }))
            $commands[0] | Should -Be @('focus-monitor-workspace', '1', '0')
            # dos cycle-focus para pararse en el container 2, dos cycle-move para bajarlo a 0
            @($commands | Where-Object { $_[0] -eq 'cycle-focus' }).Count | Should -Be 2
            @($commands | Where-Object { $_[0] -eq 'cycle-move' }).Count | Should -Be 2
            $commands[-1] | Should -Be @('cycle-move', 'previous')
            # nunca por exe: eager-focus agarraría la ventana de otro monitor
            @($commands | Where-Object { $_[0] -eq 'eager-focus' }).Count | Should -Be 0
        }
    }

    It 'walks the shortest way around when the focus is past the target' {
        InModuleScope Winarchy {
            # foco en el container 2 de 3, hay que tomar el 0: un solo cycle-focus da la vuelta
            $commands = @(Get-WinarchySlotCommands -Move ([pscustomobject]@{
                        Exe = 'Discord.exe'; Monitor = 0; Workspace = 1; From = 0; To = 1; Focused = 2; Count = 3 }))
            @($commands | Where-Object { $_[0] -eq 'cycle-focus' }).Count | Should -Be 1
            @($commands | Where-Object { $_[0] -eq 'cycle-move' }).Count | Should -Be 1
            $commands[-1] | Should -Be @('cycle-move', 'next')
        }
    }

    It 'restores the focus to the window it was on, by location' {
        InModuleScope Winarchy -Parameters @{ State = $script:State } {
            param($State)
            # hwnd 4 = Discord, container 0 del monitor 1 / workspace 0
            $location = Get-WinarchyWindowLocation -State $State -Hwnd 4
            $location.Monitor   | Should -Be 1
            $location.Workspace | Should -Be 0
            $location.Index     | Should -Be 0
            $location.Count     | Should -Be 2

            Get-WinarchyWindowLocation -State $State -Hwnd 999999 | Should -BeNullOrEmpty

            $commands = @(Get-WinarchyFocusCommands -Location $location)
            $commands[0] | Should -Be @('focus-monitor-workspace', '1', '0')
        }
    }

    It 'learns the window of the workspace the preference names, not the first with that exe' {
        InModuleScope Winarchy -Parameters @{ State = $script:State } {
            param($State)
            # Code.exe está en m0/ws0 (container 0) y también en m1/ws0 (container 1).
            # La preferencia apunta al segundo: se aprende ESE, no el primero que matchea.
            $prefs = @([pscustomobject]@{
                    Exe = 'Code.exe'; Monitor = 'BBB2222-5&bbbbbbb&1&UID4353'; Workspace = 0
                    WorkspaceName = 'A'; Slot = 0; Pin = $false })
            $learned = Get-WinarchyLearnedSlots -State $State -Pref $prefs
            @($learned)[0].Slot | Should -Be 1
        }
    }

    It 'wakes up on the events that can change the layout, and only those' {
        InModuleScope Winarchy {
            foreach ($type in 'Show', 'ManageWindow', 'Uncloak', 'UnmanageWindow',
                              'MoveWindow', 'CycleMoveWindow', 'PromoteSwap',
                              'MouseCapture', 'MoveResizeEnd') {
                Test-WinarchySlotEvent -Type $type | Should -BeTrue -Because "$type cambia el layout"
            }
            foreach ($type in 'FocusChange', 'TitleUpdate', 'MoveResizeStart', 'SomethingNew') {
                Test-WinarchySlotEvent -Type $type | Should -BeFalse -Because "$type es ruido"
            }
        }
    }

    It 'tells a window appearing apart from a window being reordered' {
        InModuleScope Winarchy -Parameters @{ State = $script:State; Swapped = $script:Swapped } {
            param($State, $Swapped)
            $before = Get-WinarchyWorkspaceSignature -State $State
            $reordered = Get-WinarchyWorkspaceSignature -State $Swapped

            # mismas ventanas en otro orden: lo movió el usuario
            Test-WinarchyWindowsAppeared -Previous $before -Current $reordered | Should -BeFalse

            # una ventana menos: apareció o se fue algo
            $fewer = Get-WinarchyWorkspaceSignature -State $State
            $fewer['1,0'] = 'Discord.exe'
            Test-WinarchyWindowsAppeared -Previous $before -Current $fewer | Should -BeTrue

            # sin referencia previa no se concluye nada
            Test-WinarchyWindowsAppeared -Previous $null -Current $before | Should -BeFalse
        }
    }

    It 'learns the new position without touching anything else' {
        InModuleScope Winarchy -Parameters @{ State = $script:Swapped; Monitor = $script:Monitor2 } {
            param($State, $Monitor)
            $prefs = @(
                [pscustomobject]@{ Exe = 'Discord.exe'; Monitor = $Monitor; Workspace = 0; WorkspaceName = 'A'; Slot = 0; Pin = $true }
                [pscustomobject]@{ Exe = 'Zen.exe';     Monitor = $Monitor; Workspace = 0; WorkspaceName = 'A'; Slot = 3; Pin = $false }
            )
            $learned = Get-WinarchyLearnedSlots -State $State -Pref $prefs
            $learned | Should -Not -BeNullOrEmpty
            $discord = $learned | Where-Object { $_.Exe -eq 'Discord.exe' }
            $discord.Slot      | Should -Be 1
            $discord.Workspace | Should -Be 0
            $discord.Pin       | Should -BeTrue
            # Zen no está en el estado vivo: su preferencia queda intacta
            ($learned | Where-Object { $_.Exe -eq 'Zen.exe' }).Slot | Should -Be 3
        }
    }

    It 'does not learn when nothing changed' {
        InModuleScope Winarchy -Parameters @{ State = $script:State; Monitor = $script:Monitor2 } {
            param($State, $Monitor)
            $prefs = @([pscustomobject]@{ Exe = 'Discord.exe'; Monitor = $Monitor; Workspace = 0; WorkspaceName = 'A'; Slot = 0; Pin = $false })
            Get-WinarchyLearnedSlots -State $State -Pref $prefs | Should -BeNullOrEmpty
        }
    }

    It 'does not give a slot to an app without a preference' {
        InModuleScope Winarchy -Parameters @{ State = $script:Swapped; Monitor = $script:Monitor2 } {
            param($State, $Monitor)
            $prefs = @([pscustomobject]@{ Exe = 'Discord.exe'; Monitor = $Monitor; Workspace = 0; WorkspaceName = 'A'; Slot = 0; Pin = $false })
            $learned = @(Get-WinarchyLearnedSlots -State $State -Pref $prefs)
            $learned.Count | Should -Be 1
            $learned[0].Exe | Should -Be 'Discord.exe'
        }
    }

    It 'reports how many move events the reconciliation emitted' {
        InModuleScope Winarchy -Parameters @{ State = $script:Swapped; Monitor = $script:Monitor2 } {
            param($State, $Monitor)
            # nombres propios: dentro del mock, $State resolvería al parámetro homónimo de
            # la función bajo prueba (scoping dinámico), no al de la prueba
            $fixture = $State
            $device = $Monitor
            Mock Test-WinarchyProcess { $true }
            Mock Get-WinarchyKomorebiState { $fixture }
            Mock Import-WinarchyWindowPrefs {
                [pscustomobject]@{ Exe = 'Discord.exe'; Monitor = $device; Workspace = 0; WorkspaceName = 'A'; Slot = 0; Pin = $false }
            }
            Mock Invoke-WinarchyKomorebic { }
            Invoke-WinarchySlotReconcile -Quiet | Should -Be 1
            # y con el estado inyectado no hace falta leerlo de komorebi
            Invoke-WinarchySlotReconcile -Quiet -State $fixture | Should -Be 1
            # los comandos salen por el canal sin consola: una consola nueva sería una
            # ventana más que komorebi tilea, y el daemon la leería como "apareció algo"
            Should -Invoke Invoke-WinarchyKomorebic -Scope It
        }
    }
}

Describe 'Autostart' {
    BeforeAll {
        $script:MockPwsh = 'C:\Program Files\PowerShell\7\pwsh.exe'
    }

    It 'launches the slots daemon through Start-Process, not as the task action itself' {
        InModuleScope Winarchy -Parameters @{ Pwsh = $script:MockPwsh } {
            param($Pwsh)
            Mock Get-WinarchyKomorebiExe { 'C:\Program Files\komorebi\bin\komorebi.exe' }
            Mock Get-Command { [pscustomobject]@{ Source = $null } }
            Mock Get-Command { [pscustomobject]@{ Source = $Pwsh } } -ParameterFilter { $Name -eq 'pwsh' }

            $slots = Get-WinarchyAutostartComponents | Where-Object Key -eq 'window-slots'
            $slots           | Should -Not -BeNullOrEmpty
            $slots.Exe       | Should -BeLike '*\WindowsPowerShell\v1.0\powershell.exe'
            $slots.Arguments | Should -BeLike "*Start-Process -FilePath '$Pwsh' -WindowStyle Hidden*"
            $slots.Arguments | Should -BeLike '*Start-WindowSlots.ps1*'
        }
    }

    It 'renders task XML that parses, for every component' {
        InModuleScope Winarchy -Parameters @{ Pwsh = $script:MockPwsh } {
            param($Pwsh)
            Mock Get-WinarchyKomorebiExe { 'C:\Program Files\komorebi\bin\komorebi.exe' }
            Mock Get-Command { [pscustomobject]@{ Source = $null } }
            Mock Get-Command { [pscustomobject]@{ Source = $Pwsh } } -ParameterFilter { $Name -eq 'pwsh' }

            $components = @(Get-WinarchyAutostartComponents)
            $components.Count | Should -BeGreaterThan 0
            foreach ($c in $components) {
                { [xml](New-WinarchyTaskXml -Component $c -User 'DOMAIN\user') } | Should -Not -Throw
            }
        }
    }
}
