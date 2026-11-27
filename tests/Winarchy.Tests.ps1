#Requires -Modules Pester

BeforeAll {
    $script:Root = Split-Path $PSScriptRoot -Parent
    Import-Module (Join-Path $script:Root 'module\Winarchy\Winarchy.psd1') -Force
}

Describe 'Write-WinarchyLog' {
    It 'writes one line per event and rotates past 512 KB' {
        InModuleScope Winarchy {
            Mock Get-WinarchyStateDir { "$TestDrive" }
            $log = Join-Path $TestDrive 'winarchy.log'
            Set-Content $log ('x' * 600KB)
            Write-WinarchyLog "a`r`nb"
            Test-Path "$log.1" | Should -BeTrue
            @(Get-Content $log).Count | Should -Be 1
            Get-Content $log | Should -Match 'cli\[\d+\] a \| b$'
        }
    }
}

Describe 'ConvertFrom-WinarchyToml' {
    It 'accepts comments after headers, single-quoted strings and empty content' {
        InModuleScope Winarchy {
            $d = ConvertFrom-WinarchyToml -Content "[colors] # paleta`nbg = 'x'`n[[games]] # g`nexe = 'Foo.exe'"
            $d['colors']['bg'] | Should -Be 'x'
            $d['games'][0]['exe'] | Should -Be 'Foo.exe'
            (ConvertFrom-WinarchyToml -Content $null).Count | Should -Be 0
        }
    }

    It 'round-trips a background pref whose theme name has spaces' {
        InModuleScope Winarchy {
            $f = Join-Path $TestDrive 'bg.toml'
            Mock Get-WinarchyBackgroundPrefsPath { $f }
            Export-WinarchyBackgroundPrefs -Pref @{ 'my theme' = 'a.png' }
            (Import-WinarchyToml -Path $f)['my theme'] | Should -Be 'a.png'
        }
    }

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

Describe 'Get-WinarchyStaleIgnores' {
    It 'reports live ignores the config dropped, composite ones included, regardless of strategy' {
        InModuleScope Winarchy {
            $live = '[{"kind":"Exe","id":"aLogin.exe","matching_strategy":"Equals"},{"kind":"Exe","id":"main.exe","matching_strategy":"Equals"},[{"kind":"Exe","id":"a.exe"},{"kind":"Class","id":"C"}]]' | ConvertFrom-Json -NoEnumerate
            $config = '[{"kind":"Exe","id":"MAIN.exe","matching_strategy":"Legacy"}]' | ConvertFrom-Json -NoEnumerate
            @(Get-WinarchyStaleIgnores -Live $live -Config $config) | Should -Be @('exe=alogin.exe', 'class=c&exe=a.exe')
            @(Get-WinarchyStaleIgnores -Live $live -Config $live).Count | Should -Be 0
            @(Get-WinarchyStaleIgnores -Live $null -Config $config).Count | Should -Be 0
        }
    }
}

Describe 'Get-WinarchyMonitorIndexMap' {
    It 'resolves a saved id by model when Windows reassigned the UID suffix, and leaves ambiguous ones alone' {
        InModuleScope Winarchy {
            $file = Join-Path $TestDrive 'windows.toml'
            Set-Content $file "[[windows]]`nexe = 'a.exe'`nmonitor = 'MSI3BA4-5&1&UID1'`nworkspace = 0`n[[windows]]`nexe = 'b.exe'`nmonitor = 'GONE000-5&1&UID9'`nworkspace = 0"
            Mock Get-WinarchyWindowPrefsPath { $file }
            $state = '{"monitors":{"elements":[{"device_id":"SAC2763-5&1&UID2"},{"device_id":"MSI3BA4-5&1&UID3"}]}}' | ConvertFrom-Json
            $map = Get-WinarchyMonitorIndexMap -State $state
            $map['MSI3BA4-5&1&UID1'] | Should -Be 1
            $map.ContainsKey('GONE000-5&1&UID9') | Should -BeFalse
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

Describe 'Bar layout' {
    BeforeAll { $script:BarTpl = Get-Content (Join-Path $script:Root 'templates\yasb-config.yaml.tpl') -Raw }

    It 'keeps the centre section to the clock alone' {
        $script:BarTpl | Should -Match '(?m)^\s*center: \["clock"\]\s*$'
    }

    It 'puts the tools drawer outside the centre section' {
        [regex]::Match($script:BarTpl, '(?m)^\s*left: \[(.+)\]\s*$').Groups[1].Value | Should -Match '"extras"'
    }

    It 'hides the flag widgets instead of leaving them padding an empty label' {
        foreach ($w in 'game-mode-widget', 'stay-awake-widget', 'winarchy-update-widget') {
            $script:BarTpl | Should -Match "class_name: `"$w`"\r?\n\s*exec_options:\r?\n\s*hide_empty: true"
        }
    }

    It 'gives every tool button a tooltip' {
        $tools = [regex]::Matches($script:BarTpl, '(?ms)^  (tool_\w+):\r?\n(.+?)(?=^  \w)')
        $tools.Count | Should -BeGreaterThan 0
        foreach ($t in $tools) {
            $t.Groups[2].Value | Should -Match 'tooltip: true' -Because "$($t.Groups[1].Value) no tiene tooltip"
            $t.Groups[2].Value | Should -Match 'tooltip_label: "[^"]+"'
        }
    }
}

Describe 'Shell prompt' {
    It 'keeps a 100 ms Starship scan budget in the managed template' {
        $template = Get-Content (Join-Path $script:Root 'templates\starship.toml.tpl') -Raw
        $template | Should -Match '(?m)^scan_timeout = 100\s*$'
    }
}

Describe 'CI workflow' {
    It 'uses the supported Node 24 checkout action' {
        $workflow = Get-Content (Join-Path $script:Root '.github\workflows\ci.yml') -Raw
        $workflow | Should -Match '(?m)^\s*- uses: actions/checkout@v7\s*$'
    }
}

Describe 'CLI errors' {
    It 'reports a bad argument without a PowerShell stack trace' {
        $shim = Join-Path $script:Root 'bin\winarchy.ps1'
        $out = & pwsh -NoProfile -ExecutionPolicy Bypass -File $shim screenshot nope 2>&1 | Out-String
        $out | Should -Match '\[XX\] Unknown capture'
        $out | Should -Not -Match 'at <ScriptBlock>'
    }
}

Describe 'Capture actions' {
    It 'every capture the menu offers is a kind the CLI knows' {
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw
        $kinds = [regex]::Matches($ahk, "Winarchy\('screenshot ([a-z-]+)'\)") | ForEach-Object { $_.Groups[1].Value }
        $kinds | Should -Not -BeNullOrEmpty
        InModuleScope Winarchy -Parameters @{ Kinds = $kinds } {
            param($Kinds)
            foreach ($k in $Kinds) { $script:CaptureActions.Contains($k) | Should -BeTrue -Because "'$k' no existe en la CLI" }
        }
    }
}

Describe 'Command palette' {
    It 'provides a Spanish label for every command' {
        InModuleScope Winarchy {
            foreach ($cmd in $script:PaletteCommands) {
                $cmd.NameEs | Should -Not -BeNullOrEmpty -Because "'$($cmd.Args)' needs a Spanish label"
            }
        }
    }

    It 'offers a localized wallpaper gallery command' {
        InModuleScope Winarchy {
            $gallery = @($script:PaletteCommands | Where-Object Args -EQ 'background gallery')
            $gallery.Count | Should -Be 1
            $gallery[0].Name | Should -Be 'Wallpaper gallery'
            $gallery[0].NameEs | Should -Be 'Galería de fondos'
        }
    }

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

Describe 'Installing a theme from a repository' {
    BeforeAll {
        $script:RepoDir = Join-Path ([IO.Path]::GetTempPath()) "winarchy-themerepo-$([guid]::NewGuid())"
        New-Item -ItemType Directory -Path (Join-Path $script:RepoDir 'backgrounds') -Force | Out-Null
        Copy-Item (Join-Path (Split-Path $PSScriptRoot -Parent) 'themes\lupine\theme.toml') $script:RepoDir
        'x' | Set-Content (Join-Path $script:RepoDir 'backgrounds\a.png')
        'payload' | Set-Content (Join-Path $script:RepoDir 'evil.exe')
        Push-Location $script:RepoDir
        git init -q; git add -A; git -c user.email=t@t -c user.name=t commit -qm init
        Pop-Location
        $script:Installed = Join-Path (Split-Path $PSScriptRoot -Parent) 'themes\pester-probe'
    }
    AfterAll {
        Remove-Item $script:RepoDir -Recurse -Force -ErrorAction SilentlyContinue
        Remove-Item $script:Installed -Recurse -Force -ErrorAction SilentlyContinue
    }

    It 'copies only the files the theme format documents' {
        Add-WinarchyTheme -Url $script:RepoDir -Name 'pester-probe' | Out-Null
        Test-Path (Join-Path $script:Installed 'theme.toml') | Should -BeTrue
        Test-Path (Join-Path $script:Installed 'backgrounds\a.png') | Should -BeTrue
        Test-Path (Join-Path $script:Installed 'evil.exe') | Should -BeFalse
        Test-Path (Join-Path $script:Installed '.git') | Should -BeFalse
    }

    It 'refuses to overwrite a theme that is already there' {
        { Add-WinarchyTheme -Url $script:RepoDir -Name 'pester-probe' } | Should -Throw '*already exists*'
    }

    It 'refuses a name that is not a folder-safe slug' {
        { Add-WinarchyTheme -Url $script:RepoDir -Name 'Bad Name' } | Should -Throw '*lowercase letters*'
    }

    It 'rejects a repository whose theme.toml does not validate, leaving nothing behind' {
        $bad = Join-Path ([IO.Path]::GetTempPath()) "winarchy-badrepo-$([guid]::NewGuid())"
        New-Item -ItemType Directory -Path $bad -Force | Out-Null
        "name = `"Bad`"`nmode = `"dark`"" | Set-Content (Join-Path $bad 'theme.toml')
        Push-Location $bad; git init -q; git add -A; git -c user.email=t@t -c user.name=t commit -qm init; Pop-Location
        $dest = Join-Path (Split-Path $PSScriptRoot -Parent) 'themes\pester-bad'
        { Add-WinarchyTheme -Url $bad -Name 'pester-bad' } | Should -Throw '*Invalid theme.toml*'
        Test-Path $dest | Should -BeFalse
        Remove-Item $bad -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe 'Generated JetBrains scheme' {
    It 'mixes two colours by weight' {
        InModuleScope Winarchy {
            Get-WinarchyMixedHex -A '#000000' -B '#ffffff' -Weight 0 | Should -Be '#000000'
            Get-WinarchyMixedHex -A '#000000' -B '#ffffff' -Weight 1 | Should -Be '#ffffff'
            Get-WinarchyMixedHex -A '#000000' -B '#ffffff' -Weight 0.5 | Should -Be '#808080'
        }
    }

    It 'renders a scheme every theme without a drop-in can use' {
        InModuleScope Winarchy {
            $bare = Get-WinarchyThemes | Where-Object { -not (Test-Path (Join-Path (Get-WinarchyThemesDir) "$_\jetbrains.icls")) }
            $bare | Should -Not -BeNullOrEmpty
            foreach ($name in $bare) {
                Set-WinarchyTheme -Name $name -StageOnly | Out-Null
                $xml = [xml](Get-Content (Join-Path (Get-WinarchyStateDir) 'staging\config\jetbrains\winarchy.icls') -Raw)
                $xml.scheme.name | Should -Be "Winarchy $((Import-WinarchyToml -Path (Join-Path (Get-WinarchyThemesDir) "$name\theme.toml"))['name'])"
                $shipped = [xml](Get-Content (Join-Path (Get-WinarchyThemesDir) 'catppuccin\jetbrains.icls') -Raw)
                $xml.SelectNodes('//option[@value]').Count | Should -Be $shipped.SelectNodes('//option[@value]').Count
            }
        }
    }

    It 'lets a theme override the generated scheme with its own' {
        InModuleScope Winarchy {
            Mock Write-WinarchyWarn {}
            $themed = Join-Path (Get-WinarchyThemesDir) 'catppuccin\jetbrains.icls'
            Test-Path $themed | Should -BeTrue
            ([xml](Get-Content $themed -Raw)).scheme.name | Should -Be 'Winarchy Catppuccin'
        }
    }
}

Describe 'Bar position' {
    BeforeAll {
        $script:BarStateBackup = InModuleScope Winarchy {
            @{
                Position    = if (Test-Path (Get-WinarchyBarPositionPath)) { Get-Content (Get-WinarchyBarPositionPath) -Raw } else { $null }
                Transparent = Test-Path (Get-WinarchyBarTransparentPath)
            }
        }
    }

    AfterAll {
        InModuleScope Winarchy -Parameters @{ Saved = $script:BarStateBackup } {
            param($Saved)
            Remove-Item (Get-WinarchyBarPositionPath) -Force -ErrorAction SilentlyContinue
            Remove-Item (Get-WinarchyBarTransparentPath) -Force -ErrorAction SilentlyContinue
            if ($null -ne $Saved.Position) { Set-Content (Get-WinarchyBarPositionPath) -Value $Saved.Position -NoNewline -Encoding UTF8 }
            if ($Saved.Transparent) { Set-Content (Get-WinarchyBarTransparentPath) -Value '' -NoNewline }
        }
    }

    It 'defaults to top' {
        InModuleScope Winarchy {
            Remove-Item (Get-WinarchyBarPositionPath) -Force -ErrorAction SilentlyContinue
            Get-WinarchyBarPosition | Should -Be 'top'
        }
    }

    It 'falls back to top when the stored value is not a position' {
        InModuleScope Winarchy {
            Set-Content (Get-WinarchyBarPositionPath) -Value 'sideways' -NoNewline -Encoding UTF8
            Get-WinarchyBarPosition | Should -Be 'top'
        }
    }

    It 'refuses a position YASB does not support' {
        { Set-WinarchyBarPosition -Position 'left' } | Should -Throw
    }

    It 'turns the bar background into rgba only while transparent' {
        InModuleScope Winarchy {
            Remove-Item (Get-WinarchyBarTransparentPath) -Force -ErrorAction SilentlyContinue
            Get-WinarchyBarBackground -Hex '#1a1b26' | Should -Be '#1a1b26'
            Set-Content (Get-WinarchyBarTransparentPath) -Value '' -NoNewline
            Get-WinarchyBarBackground -Hex '#1a1b26' | Should -Be 'rgba(26, 27, 38, 0.72)'
            Remove-Item (Get-WinarchyBarTransparentPath) -Force
        }
    }

    It 'reaches the generated bar config' {
        InModuleScope Winarchy {
            Set-Content (Get-WinarchyBarPositionPath) -Value 'bottom' -NoNewline -Encoding UTF8
            Set-WinarchyTheme -Name 'catppuccin' -StageOnly | Out-Null
            (Get-Content (Join-Path (Get-WinarchyStateDir) 'staging\config\yasb\config.yaml') -Raw) | Should -Match 'position: "bottom"'
        }
    }
}

Describe 'Update indicator flag' {
    BeforeAll {
        $script:UpdateFlagBackup = InModuleScope Winarchy {
            $f = Join-Path (Get-WinarchyStateDir) 'update-available.flag'
            if (Test-Path $f) { Get-Content $f -Raw } else { $null }
        }
    }

    AfterAll {
        InModuleScope Winarchy -Parameters @{ Saved = $script:UpdateFlagBackup } {
            param($Saved)
            $f = Join-Path (Get-WinarchyStateDir) 'update-available.flag'
            Remove-Item $f -Force -ErrorAction SilentlyContinue
            if ($null -ne $Saved) { Set-Content $f -Value $Saved -NoNewline }
        }
    }

    It 'raises the flag when a release is newer' {
        InModuleScope Winarchy {
            Mock Test-WinarchyUpdateAvailable { [pscustomobject]@{ Available = $true; Local = '0.1.0'; Latest = 'v0.2.0' } }
            Mock Write-WinarchyWarn {}
            Write-WinarchyUpdateNotice | Out-Null
            $flag = Join-Path (Get-WinarchyStateDir) 'update-available.flag'
            Test-Path $flag | Should -BeTrue
            (Get-Content $flag -Raw) | Should -Be 'v0.2.0'
        }
    }

    It 'clears the flag once there is nothing newer' {
        InModuleScope Winarchy {
            Mock Test-WinarchyUpdateAvailable { [pscustomobject]@{ Available = $false; Local = '0.2.0'; Latest = 'v0.2.0' } }
            Write-WinarchyUpdateNotice | Out-Null
            Test-Path (Join-Path (Get-WinarchyStateDir) 'update-available.flag') | Should -BeFalse
        }
    }
}

Describe 'Bar shell commands' {
    It 'never quotes a path inside a run_cmd or an exec callback' {
        $tpl = Get-Content (Join-Path (Split-Path $PSScriptRoot -Parent) 'templates\yasb-config.yaml.tpl') -Raw
        $cmds = [regex]::Matches($tpl, '(?m)^\s+(?:run_cmd|on_\w+):\s*(.+)$') |
            ForEach-Object { $_.Groups[1].Value.Trim() } |
            Where-Object { $_ -match 'cmd\.exe|powershell|pwsh' }
        $cmds | Should -Not -BeNullOrEmpty
        $cmds | Where-Object { $_ -match '\\"|''.*"' } | Should -BeNullOrEmpty
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
            'toggle_card',
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

    It 'escapes quotes, backslashes and $ in the value so the file stays valid' {
        '{ "theme": "light" }' | Set-Content $script:JsonFile -Encoding UTF8
        InModuleScope Winarchy -Parameters @{ Path = $script:JsonFile } {
            param($Path)
            Set-WinarchyJsonSetting -Path $Path -Key 'theme' -Value 'C:\a "b" $1'
        }
        (Get-Content $script:JsonFile -Raw | ConvertFrom-Json).theme | Should -Be 'C:\a "b" $1'
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

    It 'launches the agent command through WezTerm' {
        InModuleScope Winarchy {
            Mock Get-WinarchyCodingAgents { @([pscustomobject]@{ Id = 'codex'; Label = 'C'; Command = 'codex'; Installed = $true }) }
            Mock Start-Process {}
            Start-WinarchyCodingAgent
            Should -Invoke Start-Process -Times 1 -ParameterFilter {
                $FilePath -like '*wezterm-gui.exe' -and $ArgumentList -contains 'codex'
            }
        }
    }
}

Describe 'Background gallery' {
    It 'has a distinct English and Spanish message for every gallery state, formatting the dynamic ones' {
        InModuleScope Winarchy {
            foreach ($key in 'title', 'empty', 'dynamic', 'current', 'no-theme', 'apply-error') {
                $en = Get-WinarchyBackgroundGalleryText -Key $key -Theme 'nord' -File 'a.png' -Reason 'boom' -Language en
                $es = Get-WinarchyBackgroundGalleryText -Key $key -Theme 'nord' -File 'a.png' -Reason 'boom' -Language es
                $en | Should -Not -BeNullOrEmpty
                $es | Should -Not -BeNullOrEmpty
                $es | Should -Not -Be $en
            }
            Get-WinarchyBackgroundGalleryText -Key title -Theme 'nord' -Language en | Should -Match 'nord'
            $err = Get-WinarchyBackgroundGalleryText -Key apply-error -File 'a.png' -Reason 'boom' -Language es
            $err | Should -Match 'a\.png'
            $err | Should -Match 'boom'
        }
    }

    It 'fits three wallpaper cards in one row' {
        InModuleScope Winarchy {
            $layout = Get-WinarchyBackgroundGalleryLayout -Count 3 -WorkWidth 1920 -WorkHeight 1080
            $layout.Width | Should -Be 1120
            $layout.CardWidth | Should -Be 336
            $layout.Columns | Should -Be 3
            $layout.Rows | Should -Be 1
            $layout.Height | Should -Be 366
        }
    }

    It 'adds a second row for the fourth wallpaper' {
        InModuleScope Winarchy {
            $layout = Get-WinarchyBackgroundGalleryLayout -Count 4 -WorkWidth 1920 -WorkHeight 1080
            $layout.Rows | Should -Be 2
            $layout.Height | Should -Be 616
        }
    }

    It 'caps a long wallpaper gallery below the work area height' {
        InModuleScope Winarchy {
            $layout = Get-WinarchyBackgroundGalleryLayout -Count 30 -WorkWidth 1920 -WorkHeight 1080
            $layout.Rows | Should -Be 10
            $layout.Height | Should -Be 1032
        }
    }

    It 'keeps an empty wallpaper gallery at its minimum height' {
        InModuleScope Winarchy {
            $layout = Get-WinarchyBackgroundGalleryLayout -Count 0 -WorkWidth 1920 -WorkHeight 1080
            $layout.Rows | Should -Be 1
            $layout.Height | Should -Be 280
        }
    }

    It 'advertises the wallpaper picker in CLI help' {
        (Invoke-Winarchy 'background' 'gallery' '--help' 6>&1 | Out-String) |
            Should -Match 'background gallery'
    }

    It 'exposes the picker through the Winarchy menu in both languages' {
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw
        $ahk | Should -Match "text:'Wallpaper gallery'.*Winarchy\('background gallery'\)"
        $ahk | Should -Match "'Wallpaper gallery', 'Galería de fondos'"
    }

    It 'groups both galleries under the localized Appearance submenu' {
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw
        $menu = [regex]::Match($ahk, '(?ms)^WinarchyMenuItems\(\) \{.*?^\}').Value
        $appearance = [regex]::Match($ahk, '(?ms)^WinarchyAppearanceItems\(\) \{.*?^\}').Value
        $menu | Should -Match "text:'Appearance',\s+sub: WinarchyAppearanceItems\(\)"
        $ahk | Should -Match "'Appearance', 'Apariencia'"
        $appearance | Should -Match "text:'Theme gallery'.*Winarchy\('theme gallery'\)"
        $appearance | Should -Match "text:'Wallpaper gallery'.*Winarchy\('background gallery'\)"
    }

    It 'offers the Win+X tools and the accessibility apps with Spanish labels' {
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw
        $tools = [regex]::Match($ahk, '(?ms)^WinarchyToolItems\(\) \{.*?^\}').Value
        foreach ($target in 'devmgmt.msc', 'diskmgmt.msc', 'taskmgr.exe', 'ncpa.cpl', 'powercfg.cpl', 'eventvwr.msc', 'AdminTerminal()', 'magnify.exe', 'osk.exe') {
            $tools | Should -Match ([regex]::Escape($target))
        }
        foreach ($label in [regex]::Matches($tools, "text:'([^']+)'") | ForEach-Object { $_.Groups[1].Value }) {
            $ahk | Should -Match ([regex]::Escape("'$label', '")) -Because "'$label' needs a Spanish label"
        }
        $ahk | Should -Match 'sub: WinarchyToolItems\(\)'
        $ahk | Should -Match "(?m)^#NumpadAdd::RunTool\('magnify\.exe'\)"
    }

    It 'switches the projection topology without the Explorer panel' {
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw
        $ahk | Should -Not -Match 'DisplaySwitch\.exe''\)'
        $items = [regex]::Match($ahk, '(?ms)^ProjectItems\(\) \{.*?^\}').Value
        ([regex]::Matches($items, 'ProjectDisplay\((\d)\)') | ForEach-Object { $_.Groups[1].Value }) -join ',' | Should -Be '1,2,4,8'
        foreach ($label in [regex]::Matches($items, "text:'([^']+)'") | ForEach-Object { $_.Groups[1].Value }) {
            $ahk | Should -Match ([regex]::Escape("'$label', '")) -Because "'$label' needs a Spanish label"
        }
        $ahk | Should -Match "text:'Project',\s+sub: ProjectItems\(\)"
    }

    It 'replaces settings links without Explorer with the same applets as the shell' {
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw
        $rust = Get-Content (Join-Path $script:Root 'shell\src\dock_menu.rs') -Raw
        $ahk | Should -Not -Match "Run\('ms-settings:"
        $map = [regex]::Match($ahk, '(?s)SettingsFallback := Map\((.*?)\)\r?\n').Groups[1].Value
        $pairs = [regex]::Matches($map, "'(ms-settings:[^']*)', '(\S+)(?: ([^']+))?'")
        $pairs.Count | Should -BeGreaterThan 0
        foreach ($pair in $pairs) {
            $row = '("{0}", "{1}", "{2}")' -f $pair.Groups[1].Value, $pair.Groups[2].Value, $pair.Groups[3].Value
            $rust.Contains($row) | Should -BeTrue -Because "the shell needs the row $row"
        }
    }

    It 'keeps both localized gallery windows floating' {
        $template = Get-Content (Join-Path $script:Root 'templates\komorebi.json.tpl') -Raw
        $template | Should -Match '\^Winarchy — \(Themes\|Temas\|Wallpapers — \.\+\|Fondos — \.\+\)\$'
    }

    It 'returns the <Key> text in <Language>' -ForEach @(
        @{ Key = 'title'; Language = 'en'; Expected = 'Wallpapers — catppuccin' }
        @{ Key = 'title'; Language = 'es'; Expected = 'Fondos — catppuccin' }
        @{ Key = 'empty'; Language = 'en'; Expected = 'This theme has no bundled wallpapers.' }
        @{ Key = 'empty'; Language = 'es'; Expected = 'Este theme no incluye fondos de pantalla.' }
        @{ Key = 'dynamic'; Language = 'en'; Expected = 'Auto Accent follows Windows colors. Choose a wallpaper in Helium or Wallpaper Engine.' }
        @{ Key = 'dynamic'; Language = 'es'; Expected = 'Auto Accent sigue los colores de Windows. Elegí el fondo en Helium o Wallpaper Engine.' }
        @{ Key = 'current'; Language = 'en'; Expected = 'Current wallpaper' }
        @{ Key = 'current'; Language = 'es'; Expected = 'Fondo actual' }
        @{ Key = 'no-theme'; Language = 'en'; Expected = 'No theme applied yet; run winarchy theme set <name> first.' }
        @{ Key = 'no-theme'; Language = 'es'; Expected = 'Todavía no hay un theme aplicado; primero ejecutá winarchy theme set <nombre>.' }
        @{ Key = 'apply-error'; Language = 'en'; Expected = "Wallpaper 'wall.jpg' was not applied: denied" }
        @{ Key = 'apply-error'; Language = 'es'; Expected = "No se pudo aplicar el fondo 'wall.jpg': denied" }
    ) {
        InModuleScope Winarchy -Parameters @{ Key = $Key; Language = $Language; Expected = $Expected } {
            param($Key, $Language, $Expected)
            Get-WinarchyBackgroundGalleryText -Key $Key -Theme 'catppuccin' -Language $Language -File 'wall.jpg' -Reason 'denied' |
                Should -Be $Expected
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
            Mock Set-WinarchyLockScreen {}
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

    It 'darkens the [ui] colours that vanish against a light background' {
        InModuleScope Winarchy {
            $theme = @{ name = 'x'; mode = 'light'
                colors = @{ accent = '#7aa2f7'; background = '#ffffff'; color4 = '#dddddd'; color5 = '#dddddd'; color6 = '#dddddd'; color8 = '#dddddd' } }
            $ctx = Build-WinarchyThemeContext -Theme $theme -ThemeDir (Join-Path (Get-WinarchyThemesDir) 'tokyo-night')
            foreach ($k in 'ui.system', 'ui.media', 'ui.net', 'ui.muted', 'ui.accent') {
                (Get-WinarchyYiqLuminance -Hex $ctx[$k]) | Should -BeLessThan 126
            }
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

    It 'persists non-default workspace layouts and keeps them when only windows are saved' {
        InModuleScope Winarchy {
            Mock Get-WinarchyRoot { $TestDrive }
            New-Item -ItemType Directory -Path (Join-Path $TestDrive 'config') -Force | Out-Null
            $state = [pscustomobject]@{ monitors = [pscustomobject]@{ elements = @(
                [pscustomobject]@{ device_id = 'AAA1'; workspaces = [pscustomobject]@{ elements = @(
                    [pscustomobject]@{ name = '1'; layout = [pscustomobject]@{ Default = 'Grid' } }
                    [pscustomobject]@{ name = '2'; layout = [pscustomobject]@{ Default = 'BSP' } }
                    [pscustomobject]@{ name = '3'; layout = [pscustomobject]@{ Custom = @() } }) } }) } }
            $live = @(Get-WinarchyLiveWorkspaceLayouts -State $state)
            $live.Count | Should -Be 1
            $live[0].Layout | Should -Be 'Grid'
            Export-WinarchyWindowPrefs -Pref @() -Layout (@($live) + [pscustomobject]@{ Monitor = 'GONE'; Workspace = 4; Layout = 'Columns'; Mode = 'tiling' })
            Export-WinarchyWindowPrefs -Pref @([pscustomobject]@{ Exe = 'a.exe'; Monitor = 'AAA1'; Workspace = 0; WorkspaceName = '1'; Slot = $null; Pin = $false })
            $back = @(Import-WinarchyWorkspaceLayouts)
            $back.Count | Should -Be 2
            # un monitor desconectado conserva el suyo; el conectado se reemplaza por lo vivo
            $merged = @(Save-WinarchyWorkspaceLayouts -State $state)
            ($merged | Where-Object Monitor -eq 'GONE').Layout | Should -Be 'Columns'
            ($merged | Where-Object Monitor -eq 'AAA1').Layout | Should -Be 'Grid'
        }
    }

    It 'tells tiling, scrolling and floating apart from the komorebi workspace state' {
        InModuleScope Winarchy {
            $ws = { param($layout, $tile) $o = [pscustomobject]@{ layout = [pscustomobject]@{ Default = $layout } }; if ($null -ne $tile) { $o | Add-Member tile $tile }; $o }
            Get-WinarchyWorkspaceMode -Workspace (& $ws 'BSP' $true)       | Should -Be 'tiling'
            Get-WinarchyWorkspaceMode -Workspace (& $ws 'Grid' $null)      | Should -Be 'tiling'
            Get-WinarchyWorkspaceMode -Workspace (& $ws 'Scrolling' $true) | Should -Be 'scrolling'
            Get-WinarchyWorkspaceMode -Workspace (& $ws 'BSP' $false)      | Should -Be 'floating'
            # tile apagado gana sobre el layout: así lo resuelve komorebi
            Get-WinarchyWorkspaceMode -Workspace (& $ws 'Scrolling' $false) | Should -Be 'floating'
        }
    }

    It 'captures the modes that differ from the default and nothing else' {
        InModuleScope Winarchy {
            $mk = { param($layout, $tile) [pscustomobject]@{ layout = [pscustomobject]@{ Default = $layout }; tile = $tile } }
            $state = [pscustomobject]@{ monitors = [pscustomobject]@{ elements = @(
                [pscustomobject]@{ device_id = 'AAA1'; workspaces = [pscustomobject]@{ elements = @(
                    (& $mk 'BSP' $true), (& $mk 'Scrolling' $true), (& $mk 'BSP' $false)) } }) } }
            $live = @(Get-WinarchyLiveWorkspaceLayouts -State $state)
            $live.Count | Should -Be 2
            ($live | Where-Object Workspace -eq 1).Mode | Should -Be 'scrolling'
            ($live | Where-Object Workspace -eq 2).Mode | Should -Be 'floating'
            ($live | Where-Object Workspace -eq 2).Layout | Should -Be 'BSP'
            # con default floating, lo que se aparta es el tiling y el scrolling, no el floating
            $viaFloating = @(Get-WinarchyLiveWorkspaceLayouts -State $state -DefaultMode 'floating')
            $viaFloating.Workspace | Should -Be @(0, 1)
        }
    }

    It 'round-trips the mode and default_mode through windows.toml' {
        InModuleScope Winarchy {
            Mock Get-WinarchyRoot { $TestDrive }
            New-Item -ItemType Directory -Path (Join-Path $TestDrive 'config') -Force | Out-Null
            Export-WinarchyWindowPrefs -Pref @() -DefaultMode 'floating' -Layout @(
                [pscustomobject]@{ Monitor = 'AAA1'; Workspace = 0; Layout = 'BSP'; Mode = 'tiling' }
                [pscustomobject]@{ Monitor = 'AAA1'; Workspace = 2; Layout = 'Scrolling'; Mode = 'scrolling' })
            Import-WinarchyDefaultMode | Should -Be 'floating'
            $back = @(Import-WinarchyWorkspaceLayouts)
            ($back | Where-Object Workspace -eq 0).Mode | Should -Be 'tiling'
            ($back | Where-Object Workspace -eq 2).Mode | Should -Be 'scrolling'
            # guardar solo ventanas no pisa el default ni los modos
            Export-WinarchyWindowPrefs -Pref @([pscustomobject]@{ Exe = 'a.exe'; Monitor = 'AAA1'; Workspace = 0; WorkspaceName = '1'; Slot = $null; Pin = $false })
            Import-WinarchyDefaultMode | Should -Be 'floating'
            @(Import-WinarchyWorkspaceLayouts).Count | Should -Be 2
        }
    }

    It 'reads an old windows.toml that only has layout, and falls back to tiling without default_mode' {
        InModuleScope Winarchy {
            Mock Get-WinarchyRoot { $TestDrive }
            New-Item -ItemType Directory -Path (Join-Path $TestDrive 'config') -Force | Out-Null
            Set-Content (Join-Path $TestDrive 'config\windows.toml') "[[workspaces]]`nmonitor = `"AAA1`"`nworkspace = 1`nlayout = `"Scrolling`"`n`n[[workspaces]]`nmonitor = `"AAA1`"`nworkspace = 2`nlayout = `"Grid`""
            $back = @(Import-WinarchyWorkspaceLayouts)
            ($back | Where-Object Workspace -eq 1).Mode | Should -Be 'scrolling'
            ($back | Where-Object Workspace -eq 2).Mode | Should -Be 'tiling'
            Import-WinarchyDefaultMode | Should -Be 'tiling'
        }
    }

    It 'saving the modes refreshes komorebi.json on disk without reloading komorebi' {
        InModuleScope Winarchy {
            Mock Get-WinarchyRoot { $TestDrive }
            New-Item -ItemType Directory -Path (Join-Path $TestDrive 'config') -Force | Out-Null
            Mock Get-WinarchyKomorebiState { [pscustomobject]@{ monitors = [pscustomobject]@{ elements = @(
                [pscustomobject]@{ device_id = 'AAA1'; workspaces = [pscustomobject]@{ elements = @(
                    [pscustomobject]@{ name = '1'; layout = [pscustomobject]@{ Default = 'BSP' }; tile = $false }) } }) } } }
            Mock Update-WinarchyKomorebiRules { }
            Save-WinarchyWorkspaceModes
            Should -Invoke Update-WinarchyKomorebiRules -Times 1 -ParameterFilter { $NoReload }
            (@(Import-WinarchyWorkspaceLayouts))[0].Mode | Should -Be 'floating'
        }
    }

    It 'writes floating as tile=false and scrolling as the Scrolling layout into komorebi.json' {
        InModuleScope Winarchy {
            Mock Set-WinarchyConfigLiveView { }
            Mock Test-WinarchyProcess { $true }
            Mock Get-WinarchyKomorebiState { [pscustomobject]@{ monitors = [pscustomobject]@{ elements = @([pscustomobject]@{ device_id = 'AAA1' }) } } }
            Mock Import-WinarchyWindowPrefs { }
            Mock Import-WinarchyDefaultMode { 'tiling' }
            Mock Import-WinarchyWorkspaceLayouts {
                [pscustomobject]@{ Monitor = 'AAA1'; Workspace = 1; Layout = 'BSP'; Mode = 'floating' }
                [pscustomobject]@{ Monitor = 'AAA1'; Workspace = 2; Layout = 'BSP'; Mode = 'scrolling' }
            }
            $json = '{"monitors":[{"workspaces":[{"name":"1","layout":"BSP"},{"name":"2","layout":"BSP"},{"name":"3","layout":"BSP"}]}]}'
            $out = Add-WinarchyWindowRulesToKomorebiJson -Json $json | ConvertFrom-Json
            $out.monitors[0].workspaces[0].PSObject.Properties['tile']  | Should -BeNullOrEmpty
            $out.monitors[0].workspaces[1].tile   | Should -BeFalse
            $out.monitors[0].workspaces[1].layout | Should -Be 'BSP'
            $out.monitors[0].workspaces[2].layout | Should -Be 'Scrolling'
            $out.monitors[0].workspaces[2].PSObject.Properties['tile'] | Should -BeNullOrEmpty
        }
    }

    It 'applies default_mode only to workspaces without a saved mode' {
        InModuleScope Winarchy {
            Mock Set-WinarchyConfigLiveView { }
            Mock Test-WinarchyProcess { $true }
            Mock Get-WinarchyKomorebiState { [pscustomobject]@{ monitors = [pscustomobject]@{ elements = @([pscustomobject]@{ device_id = 'AAA1' }) } } }
            Mock Import-WinarchyWindowPrefs { }
            Mock Import-WinarchyDefaultMode { 'floating' }
            Mock Import-WinarchyWorkspaceLayouts { [pscustomobject]@{ Monitor = 'AAA1'; Workspace = 0; Layout = 'BSP'; Mode = 'tiling' } }
            $json = '{"monitors":[{"workspaces":[{"name":"1","layout":"BSP"},{"name":"2","layout":"BSP"}]}]}'
            $out = Add-WinarchyWindowRulesToKomorebiJson -Json $json | ConvertFrom-Json
            $out.monitors[0].workspaces[0].PSObject.Properties['tile'] | Should -BeNullOrEmpty
            $out.monitors[0].workspaces[1].tile | Should -BeFalse
        }
    }

    It 'writes saved layouts into the generated komorebi.json only for connected monitors' {
        InModuleScope Winarchy {
            Mock Set-WinarchyConfigLiveView { }
            Mock Test-WinarchyProcess { $true }
            Mock Get-WinarchyKomorebiState { [pscustomobject]@{ monitors = [pscustomobject]@{ elements = @([pscustomobject]@{ device_id = 'AAA1' }) } } }
            Mock Import-WinarchyWindowPrefs { }
            Mock Import-WinarchyWorkspaceLayouts {
                [pscustomobject]@{ Monitor = 'AAA1'; Workspace = 1; Layout = 'Columns'; Mode = 'tiling' }
                [pscustomobject]@{ Monitor = 'GONE'; Workspace = 0; Layout = 'Rows'; Mode = 'tiling' }
            }
            $json = '{"monitors":[{"workspaces":[{"name":"1","layout":"BSP"},{"name":"2","layout":"BSP"}]}]}'
            $out = Add-WinarchyWindowRulesToKomorebiJson -Json $json | ConvertFrom-Json
            $out.monitors[0].workspaces[0].layout | Should -Be 'BSP'
            $out.monitors[0].workspaces[1].layout | Should -Be 'Columns'
        }
    }

    It 'lets the live view win over saved modes unless the live view is skipped' {
        InModuleScope Winarchy {
            Mock Test-WinarchyProcess { $true }
            Mock Get-WinarchyKomorebiState { '{"new_window_behaviour":"Append","monitors":{"elements":[{"device_id":"AAA1","workspaces":{"elements":[
                {"layout":{"Default":"Columns"},"tile":true,"layout_flip":"Horizontal","container_padding":17,"workspace_padding":31},
                {"layout":{"Default":"BSP"},"tile":false,"layout_flip":null,"container_padding":null,"workspace_padding":null}]}}]}}' | ConvertFrom-Json }
            Mock Import-WinarchyWindowPrefs { }
            Mock Import-WinarchyDefaultMode { 'tiling' }
            Mock Import-WinarchyWorkspaceLayouts { [pscustomobject]@{ Monitor = 'AAA1'; Workspace = 0; Layout = 'BSP'; Mode = 'floating' } }
            $json = '{"monitors":[{"workspaces":[{"name":"1","layout":"BSP"},{"name":"2","layout":"BSP","layout_flip":"Vertical"}]}]}'

            $live = (Add-WinarchyWindowRulesToKomorebiJson -Json $json | ConvertFrom-Json).monitors[0].workspaces
            $live[0].layout | Should -Be 'Columns'
            $live[0].layout_flip | Should -Be 'Horizontal'
            $live[0].container_padding | Should -Be 17
            $live[0].workspace_padding | Should -Be 31
            $live[0].PSObject.Properties['tile'] | Should -BeNullOrEmpty
            $live[1].tile | Should -BeFalse
            foreach ($key in 'layout_flip', 'container_padding', 'workspace_padding') { $live[1].PSObject.Properties[$key] | Should -BeNullOrEmpty }

            $applied = Add-WinarchyWindowRulesToKomorebiJson -Json $json -SkipLiveView | ConvertFrom-Json
            $applied.window_container_behaviour | Should -Be 'Append'
            $saved = $applied.monitors[0].workspaces
            $saved[0].tile | Should -BeFalse
            $saved[0].layout | Should -Be 'BSP'
        }
    }

    It 'layout apply renders without the live view' {
        InModuleScope Winarchy {
            Mock Test-WinarchyProcess { $true }
            Mock Import-WinarchyWindowPrefs { }
            Mock Import-WinarchyWorkspaceLayouts { [pscustomobject]@{ Monitor = 'AAA1'; Workspace = 0; Layout = 'BSP'; Mode = 'floating' } }
            Mock Get-WinarchyKomorebiState { [pscustomobject]@{ monitors = [pscustomobject]@{ elements = @() } } }
            Mock Update-WinarchyKomorebiRules { }
            Invoke-WinarchyLayoutApply -Quiet
            Should -Invoke Update-WinarchyKomorebiRules -Times 1 -ParameterFilter { $SkipLiveView }
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
            Mock Set-WinarchyConfigLiveView { }
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

    It 'writes the live view into the config and skips monitors it cannot identify' {
        InModuleScope Winarchy {
            $live = [pscustomobject]@{ layout = [pscustomobject]@{ Default = 'Columns' }; tile = $false; container_padding = 7 }
            $state = [pscustomobject]@{ new_window_behaviour = 'Append'; monitors = [pscustomobject]@{ elements = @(
                [pscustomobject]@{ device_id = 'AAA1'; workspaces = [pscustomobject]@{ elements = @($live) } }
                [pscustomobject]@{ device_id = 'GONE'; workspaces = [pscustomobject]@{ elements = @($live) } }) } }
            $config = '{"monitors":[{"workspaces":[{"name":"1"}]},{"workspaces":[{"name":"A"}]}]}' | ConvertFrom-Json
            Set-WinarchyConfigLiveView -Config $config -State $state -Map @{ AAA1 = 0 }

            $config.window_container_behaviour | Should -Be 'Append'
            $config.monitors[0].workspaces[0].layout | Should -Be 'Columns'
            $config.monitors[0].workspaces[0].tile | Should -BeFalse
            $config.monitors[0].workspaces[0].container_padding | Should -Be 7
            $config.monitors[1].workspaces[0].PSObject.Properties['layout'] | Should -BeNullOrEmpty
        }
    }

    It 'does not publish komorebi.json when a running komorebi gives no state' {
        InModuleScope Winarchy {
            Mock Get-WinarchyRoot { $TestDrive }
            Mock Test-WinarchyProcess { $true }
            Mock Get-WinarchyKomorebiState { throw 'komorebic state returned nothing' }
            Mock Write-WinarchyWarn { }
            $base = '{"monitors":[]}'
            $path = Join-Path $TestDrive 'config\komorebi\komorebi.json'
            Remove-Item $path -ErrorAction SilentlyContinue
            Add-WinarchyWindowRulesToKomorebiJson -Json $base | Should -Be $base
            New-Item -ItemType File -Path $path -Force | Out-Null
            Add-WinarchyWindowRulesToKomorebiJson -Json $base | Should -BeNullOrEmpty
            Should -Invoke Write-WinarchyWarn -Times 1
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
    BeforeEach { Mock -ModuleName Winarchy Test-WinarchyProcess { $false } }   # nunca tocar el komorebi real

    It 'add writes games.user.toml, never the shipped games.toml, and remove undoes it' {
        InModuleScope Winarchy {
            $toml = Join-Path $TestDrive 'games.toml'
            $user = Join-Path $TestDrive 'games.user.toml'
            Remove-Item $user -ErrorAction SilentlyContinue
            Set-Content -Path $toml -Value "[[games]]`r`nexe = `"Existing.exe`"`r`n" -Encoding UTF8 -NoNewline
            $before = (Get-FileHash $toml -Algorithm SHA256).Hash

            Mock Get-WinarchyRoot { Split-Path $toml -Parent }
            Mock Update-WinarchyKomorebiRules { }
            Mock Update-WinarchyQuickAccentExclusion { }

            Add-WinarchyGame -Exe 'RoundTrip' | Out-Null
            (Get-Content $user -Raw) | Should -Match 'RoundTrip\.exe'
            @(Get-WinarchyGames) | Should -Be @('Existing.exe', 'RoundTrip.exe')
            (Get-FileHash $toml -Algorithm SHA256).Hash | Should -Be $before

            Remove-WinarchyGame -Exe 'RoundTrip' | Out-Null
            @(Get-WinarchyGames) | Should -Be @('Existing.exe')
            (Get-FileHash $toml -Algorithm SHA256).Hash | Should -Be $before
        }
    }

    It 'keeps multi-word webapp names and rule values from the unquoted command line' {
        InModuleScope Winarchy {
            Mock Install-WinarchyWebapp { $script:got = "$Name|$Url" }
            Mock Add-WinarchyUserRule { $script:rule = "$Value|$MatchingStrategy" }

            Invoke-Winarchy 'webapp' 'install' 'My' 'App' 'https://x.test'
            $script:got | Should -Be 'My App|https://x.test'

            Invoke-Winarchy 'rules' 'add' 'ignore' 'title' 'Visual' 'Studio' 'Code'
            $script:rule | Should -Be 'Visual Studio Code|'
            Invoke-Winarchy 'rules' 'add' 'ignore' 'title' 'Visual' 'Studio' 'Contains'
            $script:rule | Should -Be 'Visual Studio|Contains'
        }
    }

    It 'rejects webapp names and URLs that would break the shortcut' {
        InModuleScope Winarchy {
            { Install-WinarchyWebapp -Name 'a/b' -Url 'https://x.test' } | Should -Throw '*Invalid webapp name*'
            { Install-WinarchyWebapp -Name 'ok' -Url 'javascript:alert(1)' } | Should -Throw '*Invalid webapp URL*'
            { Install-WinarchyWebapp -Name 'ok' -Url 'https://x.test --evil' } | Should -Throw '*Invalid webapp URL*'
        }
    }

    It 'passes game-mode on --delay to Enable and rejects a bad value' {
        InModuleScope Winarchy {
            Mock Enable-WinarchyGameMode { $script:delay = $Delay }
            Invoke-Winarchy 'game-mode' 'on' '--delay' '7'
            $script:delay | Should -Be 7
            Invoke-Winarchy 'game-mode' 'on'
            $script:delay | Should -Be 0
            { Invoke-Winarchy 'game-mode' 'on' '--delay' 'x' } | Should -Throw '*Usage*'
            { Invoke-Winarchy 'game-mode' 'on' '--delay' } | Should -Throw '*Usage*'
        }
    }

    It 'joins an unquoted multi-word exe name and rejects names that would break the toml' {
        InModuleScope Winarchy {
            $toml = Join-Path $TestDrive 'games.toml'
            $user = Join-Path $TestDrive 'games.user.toml'
            Remove-Item $user -ErrorAction SilentlyContinue
            Set-Content -Path $toml -Value '' -Encoding UTF8 -NoNewline
            Mock Get-WinarchyRoot { Split-Path $toml -Parent }
            Mock Update-WinarchyKomorebiRules { }
            Mock Update-WinarchyQuickAccentExclusion { }
            Mock Test-WinarchyProcess { $false }

            Invoke-Winarchy 'game-mode' 'add' 'The' 'iTero' 'Coach.exe' | Out-Null
            (Get-Content $user -Raw) | Should -Match 'exe = "The iTero Coach\.exe"'
            Remove-Item $user
            { Add-WinarchyGame -Exe 'a"b.exe' } | Should -Throw
        }
    }

    It 'removes launcher entries, commented entries and other capitalizations' {
        InModuleScope Winarchy {
            $toml = Join-Path $TestDrive 'games.toml'
            Remove-Item (Join-Path $TestDrive 'games.user.toml') -ErrorAction SilentlyContinue
            Set-Content -Path $toml -Value "[[games]]`nexe = `"Keep.exe`"`n`n[[launchers]]`nexe = `"Steam.exe`" # store`n`n[[games]]`nexe = `"Eso64.exe`"`n" -Encoding UTF8 -NoNewline
            Mock Get-WinarchyRoot { Split-Path $toml -Parent }
            Mock Update-WinarchyKomorebiRules { }
            Mock Update-WinarchyQuickAccentExclusion { }

            Remove-WinarchyGame -Exe 'steam.exe' | Out-Null
            Remove-WinarchyGame -Exe 'ESO64' | Out-Null
            @(Get-WinarchyGames) | Should -Be @('Keep.exe')
        }
    }

    It 'reapplies rules when the game is already registered' {
        InModuleScope Winarchy {
            $toml = Join-Path $TestDrive 'games.toml'
            Set-Content -Path $toml -Value "[[games]]`r`nexe = `"Existing.exe`"`r`n" -Encoding UTF8 -NoNewline
            $before = (Get-FileHash $toml -Algorithm SHA256).Hash

            $marker = Join-Path $TestDrive 'rules-updated'
            Mock Get-WinarchyRoot { Split-Path $toml -Parent }
            Mock Update-WinarchyKomorebiRules { Set-Content -Path $marker -Value 'updated' }
            Mock Update-WinarchyQuickAccentExclusion { }
            Mock Test-WinarchyProcess { $false }

            Add-WinarchyGame -Exe 'Existing.exe' | Out-Null

            Test-Path $marker | Should -BeTrue
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

    It 'doctor valida el theme expandido, igual que el render' {
        $src = Get-Content (Join-Path $script:Root 'module\Winarchy\Private\Doctor.ps1') -Raw
        $src | Should -Match 'Expand-WinarchyThemePalette -Theme \(Import-WinarchyToml'
    }

    # Cubre de punta a punta lo que antes solo se sabía al aplicar un theme: los 11
    # templates renderizan sin tokens sin resolver y el JSON/XML generado parsea.
    It '<Name> renders every template (theme set -StageOnly)' -ForEach $script:ThemeCases {
        { Set-WinarchyTheme -Name $Name -StageOnly } | Should -Not -Throw
        # config.yaml de YASB no tiene validador de sintaxis en el pipeline (no hay parser
        # YAML en PowerShell): al menos la geometria inyectada tiene que quedar numerica,
        # que es lo unico que un theme puede romper ahi.
        $staged = Join-Path (Get-WinarchyStateDir) 'staging\config\yasb\config.yaml'
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

Describe 'WezTerm terminal' {
    It 'registers the generated config as a render target validated as Lua' {
        InModuleScope Winarchy {
            $target = Get-WinarchyRenderTargets | Where-Object { $_.Template -eq 'wezterm.lua.tpl' }
            $target | Should -Not -BeNullOrEmpty
            $target.Output | Should -Be (Join-Path (Get-WinarchyRoot) 'config\wezterm\wezterm.lua')
            $target.Validate | Should -Be 'lua'
        }
    }

    It 'renders the palette with no leftover tokens' {
        InModuleScope Winarchy {
            $themeDir = Join-Path (Get-WinarchyThemesDir) 'tokyo-night'
            $theme = Expand-WinarchyThemePalette -Theme (Import-WinarchyToml -Path (Join-Path $themeDir 'theme.toml'))
            $ctx = Build-WinarchyThemeContext -Theme $theme -ThemeDir $themeDir
            $rendered = Invoke-WinarchyTemplate -TemplatePath (Join-Path (Get-WinarchyRoot) 'templates\wezterm.lua.tpl') -Context $ctx

            $rendered | Should -Not -Match '\{\{'
            $rendered | Should -Match 'managed by winarchy'
            $rendered | Should -Match ([regex]::Escape($theme['colors']['background']))
            $rendered | Should -Match ([regex]::Escape($theme['colors']['color15']))
            $rendered | Should -Match ([regex]::Escape($theme['borders']['focused']))
        }
    }

    # La traslucidez sobre fondo claro degrada la legibilidad: solo los themes oscuros.
    It 'derives the window opacity from the theme mode' {
        InModuleScope Winarchy {
            $themeDir = Join-Path (Get-WinarchyThemesDir) 'tokyo-night'
            $dark = @{ name = 'x'; mode = 'dark'; colors = @{ accent = '#7aa2f7'; background = '#1a1b26' } }
            $light = @{ name = 'x'; mode = 'light'; colors = @{ accent = '#7aa2f7'; background = '#1a1b26' } }
            (Build-WinarchyThemeContext -Theme $dark -ThemeDir $themeDir)['computed.terminal_opacity'] | Should -Be '0.95'
            (Build-WinarchyThemeContext -Theme $light -ThemeDir $themeDir)['computed.terminal_opacity'] | Should -Be '1.0'
        }
    }

    # El override es del usuario: ningun render lo escribe, lo borra ni lo pisa.
    It 'loads the user override without generating it' {
        InModuleScope Winarchy {
            $userLua = Join-Path (Get-WinarchyRoot) 'config\wezterm\user.lua'
            $backup = if (Test-Path $userLua) { Get-Content $userLua -Raw } else { $null }
            try {
                Set-Content -Path $userLua -Value 'return { font_size = 42.0 }' -Encoding UTF8 -NoNewline
                Set-WinarchyTheme -Name 'tokyo-night' -StageOnly
                $staged = Get-Content (Join-Path (Get-WinarchyStateDir) 'staging\config\wezterm\wezterm.lua') -Raw

                $staged | Should -Match 'config/wezterm/user\.lua'
                $staged | Should -Match 'pcall\(dofile'
                (Get-Content $userLua -Raw) | Should -Be 'return { font_size = 42.0 }'
            }
            finally {
                if ($null -ne $backup -and $backup -ne 'return { font_size = 42.0 }') { Set-Content -Path $userLua -Value $backup -NoNewline -Encoding UTF8 }
                elseif (Test-Path $userLua) { Remove-Item $userLua -Force }
            }
        }
    }

    It 'embeds the install folder in a Lua long string, safe for apostrophes' {
        $tpl = Get-Content (Join-Path $script:Root 'templates\wezterm.lua.tpl') -Raw
        $tpl | Should -Match ([regex]::Escape('[==[{{computed.root}}/config/wezterm/user.lua]==]'))
    }

    It 'prefers a WezTerm dropped in state\bin over the installed one' {
        InModuleScope Winarchy {
            $state = Join-Path $TestDrive 'state'
            New-Item -ItemType File -Path (Join-Path $state 'bin\wezterm\wezterm-gui.exe') -Force | Out-Null
            Mock Get-WinarchyStateDir { $state }
            Get-WinarchyWeztermExe | Should -Be (Join-Path $state 'bin\wezterm\wezterm-gui.exe')
        }
    }

    # wezterm#7705: recorrer el árbol de procesos se desborda al reusarse un PID y cierra la terminal.
    It 'never walks the process tree from the terminal config' {
        $code = Get-Content (Join-Path $script:Root 'templates\wezterm.lua.tpl') | Where-Object { $_ -notmatch '^\s*--' }
        $code -join "`n" | Should -Not -Match 'foreground_process|current_working_dir|procinfo'
    }

    # Sin default_prog WezTerm levanta cmd.exe y todo el terminal profile del stack
    # (starship, fzf, zoxide, eza, bat) no carga.
    It 'defaults to PowerShell 7 and keeps its own updater off' {
        $tpl = Get-Content (Join-Path $script:Root 'templates\wezterm.lua.tpl') -Raw
        $tpl | Should -Match "config\.default_prog = \{ 'pwsh\.exe'"
        # Winarchy es el unico canal de updates, como con YASB y Flow.
        $tpl | Should -Match 'config\.check_for_updates = false'
    }

    # Un proceso del stack arrancado antes de instalar WezTerm no hereda
    # WEZTERM_CONFIG_FILE y abriria ventanas con los defaults de fabrica.
    It 'points every Winarchy launch at the managed config explicitly' {
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw
        $ahk | Should -Match '--config-file'
        @($ahk -split "`n" | Where-Object { $_ -match 'WeztermExe' -and $_ -match 'Run\(' -and $_ -notmatch '--config-file' }) |
            Should -BeNullOrEmpty

        InModuleScope Winarchy {
            Mock Get-WinarchyCodingAgents { @([pscustomobject]@{ Id = 'codex'; Label = 'C'; Command = 'codex'; Installed = $true }) }
            Mock Start-Process {}
            Start-WinarchyCodingAgent
            Should -Invoke Start-Process -Times 1 -ParameterFilter {
                $ArgumentList -contains '--config-file' -and
                $ArgumentList -contains "`"$(Join-Path (Get-WinarchyRoot) 'config\wezterm\wezterm.lua')`""
            }
        }
    }

    # AHK es el dueno unico de los hotkeys globales: los keybinds del terminal viven
    # detras del leader y no pueden tomar un combo suelto que pise a uno global.
    It 'keeps its keybinds out of the global AHK hotkey space' {
        $tpl = Get-Content (Join-Path $script:Root 'templates\wezterm.lua.tpl') -Raw
        $tpl | Should -Match "config\.leader = \{ key = 'Space', mods = 'CTRL'"

        # Alt+1..9 (ir a la tab N) es lo único suelto que se permite; ningún hotkey de AHK lo toma.
        $unprefixed = [regex]::Matches($tpl, "\{ key = '(?<key>[^']+)', mods = '(?<mods>[^']+)'") |
            Where-Object { $_.Groups['mods'].Value -notmatch 'LEADER' } |
            ForEach-Object { "$($_.Groups['mods'].Value)+$($_.Groups['key'].Value)" } |
            Where-Object { $_ -ne 'CTRL+Space' }   # el leader mismo, y su passthrough
        $unprefixed -join ', ' | Should -BeNullOrEmpty -Because 'Alt+N se agrega en un bucle, no como literal'
        $tpl | Should -Match "mods = 'ALT', action = act\.ActivateTab"

        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk')
        @($ahk | Where-Object { $_ -match '^\^Space::' }) | Should -BeNullOrEmpty
        @($ahk | Where-Object { $_ -match '^![0-9]::' }) | Should -BeNullOrEmpty
    }

    It 'cleans inherited agent session variables on startup' {
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw
        $ahk | Should -Match "EnvSet\('CLAUDE_CODE_CHILD_SESSION'\)"
        $ahk | Should -Match "EnvSet\('CLAUDECODE'\)"
    }

    It 'activates and brings the terminal window to foreground on launch' {
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw
        $ahk | Should -Match 'AllowSetForegroundWindow'
        $ahk | Should -Match "WinActivate\('ahk_id ' hwnd\)"
    }
}

Describe 'versions.lock.toml' {
    It 'pins the core components' {
        InModuleScope Winarchy {
            $lock = Import-WinarchyToml -Path (Join-Path (Get-WinarchyRoot) 'versions.lock.toml')
            $lock['core'].Keys | Should -Contain 'komorebi'
            # WezTerm es core: `winarchy update` no debe moverlo, solo `update --core`.
            $lock['core'].Keys | Should -Contain 'wezterm'
            $lock['winget'].Keys | Should -Contain 'wez.wezterm'
        }
    }
}

Describe 'App rules (community ASC + user layer)' {
    BeforeEach { Mock -ModuleName Winarchy Test-WinarchyProcess { $false } }   # nunca tocar el komorebi real

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

    It 'creates rules.toml from the example and appends the new rule' {
        InModuleScope Winarchy {
            Mock Get-WinarchyRoot { $TestDrive }
            $exampleDir = Join-Path $TestDrive 'config\komorebi'
            New-Item -ItemType Directory -Path $exampleDir -Force | Out-Null
            Set-Content -Path (Join-Path $exampleDir 'rules.toml.example') -Value "# example header`n" -Encoding UTF8

            Add-WinarchyUserRule -Category 'floating' -Field 'exe' -Value 'PicView.exe'

            $path = Join-Path $exampleDir 'rules.toml'
            Test-Path $path | Should -BeTrue
            $rules = Import-WinarchyToml -Path $path
            $rules['floating'][0]['exe'] | Should -Be 'PicView.exe'
        }
    }

    It 'does not duplicate an identical rule on a second add' {
        InModuleScope Winarchy {
            Mock Get-WinarchyRoot { $TestDrive }
            $dir = Join-Path $TestDrive 'config\komorebi'
            New-Item -ItemType Directory -Path $dir -Force | Out-Null
            Remove-Item (Join-Path $dir 'rules.toml') -Force -ErrorAction SilentlyContinue

            Add-WinarchyUserRule -Category 'floating' -Field 'exe' -Value 'PicView.exe'
            Add-WinarchyUserRule -Category 'floating' -Field 'exe' -Value 'PicView.exe'

            $rules = Import-WinarchyToml -Path (Join-Path $TestDrive 'config\komorebi\rules.toml')
            @($rules['floating']).Count | Should -Be 1
        }
    }

    It 'keeps pre-existing hand-written entries untouched (append-only)' {
        InModuleScope Winarchy {
            Mock Get-WinarchyRoot { $TestDrive }
            $dir = Join-Path $TestDrive 'config\komorebi'
            New-Item -ItemType Directory -Path $dir -Force | Out-Null
            Set-Content -Path (Join-Path $dir 'rules.toml') -Value "[[ignore]]`nexe = `"GalaxyClient.exe`"`n" -Encoding UTF8

            Add-WinarchyUserRule -Category 'floating' -Field 'exe' -Value 'PicView.exe'

            $rules = Import-WinarchyToml -Path (Join-Path $dir 'rules.toml')
            $rules['ignore'][0]['exe'] | Should -Be 'GalaxyClient.exe'
            $rules['floating'][0]['exe'] | Should -Be 'PicView.exe'
        }
    }

    It 'writes an optional matching_strategy' {
        InModuleScope Winarchy {
            Mock Get-WinarchyRoot { $TestDrive }
            $dir = Join-Path $TestDrive 'config\komorebi'
            New-Item -ItemType Directory -Path $dir -Force | Out-Null
            Remove-Item (Join-Path $dir 'rules.toml') -Force -ErrorAction SilentlyContinue

            Add-WinarchyUserRule -Category 'ignore' -Field 'title' -Value 'Picture-in-Picture' -MatchingStrategy 'Contains'

            $rules = Import-WinarchyToml -Path (Join-Path $TestDrive 'config\komorebi\rules.toml')
            $rules['ignore'][0]['matching_strategy'] | Should -Be 'Contains'
        }
    }

    It 'applies the rule immediately instead of requiring a theme set' {
        InModuleScope Winarchy {
            Mock Get-WinarchyRoot { $TestDrive }
            $dir = Join-Path $TestDrive 'config\komorebi'
            New-Item -ItemType Directory -Path $dir -Force | Out-Null
            Mock Update-WinarchyKomorebiRules { }
            Mock Test-WinarchyProcess { $true }
            if (-not (Get-Command komorebic -ErrorAction SilentlyContinue)) { function komorebic { } }
            Mock komorebic { }

            Add-WinarchyUserRule -Category 'floating' -Field 'exe' -Value 'PicView.exe'

            Should -Invoke Update-WinarchyKomorebiRules -Times 1
            Should -Invoke komorebic -Times 1 -ParameterFilter { $args[0] -eq 'retile' }
        }
    }

    It 'rejects an unknown category' {
        InModuleScope Winarchy {
            Mock Get-WinarchyRoot { $TestDrive }
            { Add-WinarchyUserRule -Category 'nope' -Field 'exe' -Value 'x.exe' } | Should -Throw '*Unknown rule category*'
        }
    }

    It 'requires the asc field for disable and rejects it elsewhere' {
        InModuleScope Winarchy {
            Mock Get-WinarchyRoot { $TestDrive }
            { Add-WinarchyUserRule -Category 'disable' -Field 'exe' -Value 'x.exe' } | Should -Throw "*needs the 'asc' field*"
            { Add-WinarchyUserRule -Category 'ignore' -Field 'asc' -Value 'GOG Galaxy' } | Should -Throw "*only valid with*"
        }
    }

    It 'accepts a disable/asc entry' {
        InModuleScope Winarchy {
            Mock Get-WinarchyRoot { $TestDrive }
            New-Item -ItemType Directory -Path (Join-Path $TestDrive 'config\komorebi') -Force | Out-Null

            Add-WinarchyUserRule -Category 'disable' -Field 'asc' -Value 'GOG Galaxy'

            $rules = Import-WinarchyToml -Path (Join-Path $TestDrive 'config\komorebi\rules.toml')
            $rules['disable'][0]['asc'] | Should -Be 'GOG Galaxy'
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
        $json = Get-Content (Join-Path (Get-WinarchyStateDir) 'staging\config\komorebi\komorebi.json') -Raw -Encoding UTF8
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
        $collapsed = '^([0-9]|Left|Right|Up|Down)$'
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
        foreach ($range in 'SUPER+1..0', 'SUPER+Shift+1..0') {
            $script:Doc | Should -Match ([regex]::Escape($range))
        }
    }
}

Describe 'Module manifest' {
    It 'exports every function the CLI dispatcher calls' {
        $manifest = Test-ModuleManifest (Join-Path $script:Root 'module\Winarchy\Winarchy.psd1')
        $manifest.ExportedFunctions.Keys | Should -Contain 'Invoke-Winarchy'
    }

    It 'exports helpers used by installer entry points' {
        $manifest = Test-ModuleManifest (Join-Path $script:Root 'module\Winarchy\Winarchy.psd1')
        foreach ($name in 'Install-WinarchySkill', 'Uninstall-WinarchySkill') {
            $manifest.ExportedFunctions.Keys | Should -Contain $name
        }
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

    It 'skips workspaces outside the requested filter, even if out of slot' {
        InModuleScope Winarchy -Parameters @{ State = $script:Swapped; Monitor = $script:Monitor2 } {
            param($State, $Monitor)
            $prefs = @([pscustomobject]@{ Exe = 'Discord.exe'; Monitor = $Monitor; Workspace = 0; WorkspaceName = 'A'; Slot = 0; Pin = $false })
            $none = [System.Collections.Generic.HashSet[string]]::new()
            @(Get-WinarchySlotMoves -State $State -Pref $prefs -Workspaces $none).Count | Should -Be 0

            $withIt = [System.Collections.Generic.HashSet[string]]::new()
            $null = $withIt.Add('1,0')
            @(Get-WinarchySlotMoves -State $State -Pref $prefs -Workspaces $withIt).Count | Should -Be 1
        }
    }

    It 'flags every key as changed on the first read, with nothing to compare against' {
        InModuleScope Winarchy -Parameters @{ State = $script:State } {
            param($State)
            $current = Get-WinarchyWorkspaceSignature -State $State
            $changed = Get-WinarchyChangedWorkspaceKeys -Previous $null -Current $current
            $changed.Count | Should -Be $current.Keys.Count
        }
    }

    It 'flags only the workspace whose window set actually changed' {
        InModuleScope Winarchy -Parameters @{ State = $script:State } {
            param($State)
            $before = Get-WinarchyWorkspaceSignature -State $State
            $after = Get-WinarchyWorkspaceSignature -State $State
            $after['1,0'] = 'Discord.exe'
            $changed = Get-WinarchyChangedWorkspaceKeys -Previous $before -Current $after
            $changed.Count | Should -Be 1
            $changed.Contains('1,0') | Should -BeTrue
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

    It 'does not learn a slot for an app that has none' {
        InModuleScope Winarchy -Parameters @{ State = $script:State; Monitor = $script:Monitor2 } {
            param($State, $Monitor)
            $prefs = @([pscustomobject]@{ Exe = 'Discord.exe'; Monitor = $Monitor; Workspace = 0; WorkspaceName = 'A'; Slot = $null; Pin = $false })
            Get-WinarchyLearnedSlots -State $State -Pref $prefs | Should -BeNullOrEmpty
        }
    }

    It 'reverting hardening restores what the user had, and removes what did not exist' {
        InModuleScope Winarchy {
            $key = 'HKCU:\Software\WinarchyPesterHardening'
            Mock Get-WinarchyStateDir { $TestDrive }
            Mock Backup-WinarchyRegistryKey { }
            Mock Stop-Process { }
            Mock Get-WinarchyHardeningSettings {
                , @('HKCU:\Software\WinarchyPesterHardening', 'Had', 0)
                , @('HKCU:\Software\WinarchyPesterHardening', 'Absent', 0)
                , @('HKCU:\Software\WinarchyPesterHardening', 'Delay', '100', 'String')
            }
            try {
                $null = New-Item -Path $key -Force
                Set-ItemProperty -Path $key -Name 'Had' -Value 1 -Type DWord
                Set-ItemProperty -Path $key -Name 'Delay' -Value '400' -Type String

                Set-WinarchyWindowsHardening | Should -Be 3
                (Get-ItemProperty $key).Had | Should -Be 0
                (Get-Item $key).GetValueKind('Delay') | Should -Be 'String'
                (Get-ItemProperty $key).Delay | Should -Be '100'

                Set-WinarchyWindowsHardening -Revert | Should -Be 3
                (Get-ItemProperty $key).Had | Should -Be 1
                (Get-ItemProperty $key).Delay | Should -Be '400'
                (Get-ItemProperty $key).PSObject.Properties['Absent'] | Should -BeNullOrEmpty
            }
            finally { Remove-Item $key -Recurse -Force -ErrorAction SilentlyContinue }
        }
    }

    It 'declares every hardening setting as (path, name, value[, type])' {
        InModuleScope Winarchy {
            $settings = @(Get-WinarchyHardeningSettings)
            $settings.Count | Should -BeGreaterThan 20
            foreach ($s in $settings) {
                @($s).Count | Should -BeIn 3, 4
                $s[0] | Should -Match '^HKCU:\\'
            }
        }
    }

    It 'never captures explorer or terminals as placements' {
        InModuleScope Winarchy {
            Mock Get-WinarchyGames { @() }
            $skip = @(Get-WinarchyUnplaceableExes)
            $skip | Should -Contain 'explorer.exe'
            $skip | Should -Contain 'WindowsTerminal.exe'
            $skip | Should -Contain 'wezterm-gui.exe'
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

    It 'keeps the slots command valid when the install folder has an apostrophe' {
        InModuleScope Winarchy -Parameters @{ Pwsh = $script:MockPwsh } {
            param($Pwsh)
            Mock Get-WinarchyRoot { "C:\Users\O'Brien\winarchy" }
            Mock Get-WinarchyKomorebiExe { 'C:\Program Files\komorebi\bin\komorebi.exe' }
            Mock Get-Command { [pscustomobject]@{ Source = $null } }
            Mock Get-Command { [pscustomobject]@{ Source = $Pwsh } } -ParameterFilter { $Name -eq 'pwsh' }

            $slots = Get-WinarchyAutostartComponents | Where-Object Key -eq 'window-slots'
            $inner = [regex]::Match($slots.Arguments, '-Command "(.*)"$').Groups[1].Value
            $errors = $null
            $ast = [Management.Automation.Language.Parser]::ParseInput($inner, [ref]$null, [ref]$errors)
            $errors | Should -BeNullOrEmpty
            $ast.FindAll({ $args[0] -is [Management.Automation.Language.StringConstantExpressionAst] }, $true).Value |
                Should -Contain "\`"C:\Users\O'Brien\winarchy\scripts\Start-WindowSlots.ps1\`""   # entre \" para que un path con espacios llegue entero a -File
        }
    }

    It 'only autostarts the network glyph watcher for the YASB bar' {
        InModuleScope Winarchy {
            Mock Get-Command { [pscustomobject]@{ Source = $null } }
            Mock Get-WinarchyActiveBar { 'native' }
            Get-WinarchyAutostartComponents | Where-Object Key -eq 'net-icon' | Should -BeNullOrEmpty
            Mock Get-WinarchyActiveBar { 'yasb' }
            Get-WinarchyAutostartComponents | Where-Object Key -eq 'net-icon' | Should -Not -BeNullOrEmpty
        }
    }

    It 'keeps session-shell mode out of scheduled shell task arguments' {
        InModuleScope Winarchy -Parameters @{ Root = (Join-Path $TestDrive 'autostart-session-shell') } {
            param($Root)
            Mock Resolve-Path { [pscustomobject]@{ Path = $Root } } -ParameterFilter { $Path -like '*\..\..\..' }
            Mock Get-Command { [pscustomobject]@{ Source = $null } }
            $state = Get-WinarchyStateDir
            New-Item -ItemType Directory -Path (Join-Path $state 'bin') -Force | Out-Null
            Set-Content (Join-Path $state 'bin\winarchy-shell.exe') -Value 'fixture' -NoNewline
            Set-Content (Join-Path $state 'bar') -Value 'native' -NoNewline
            Set-Content (Join-Path $state "session-shell-$([Diagnostics.Process]::GetCurrentProcess().SessionId)") -Value '' -NoNewline

            $shell = Get-WinarchyAutostartComponents | Where-Object Key -eq 'shell'

            $shell.Arguments | Should -Be "--root `"$Root`""
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

    It 'derives the komorebi config path inside its launcher' {
        $launcher = Get-Content (Join-Path $script:Root 'scripts\Start-Komorebi.ps1') -Raw
        $launcher | Should -Match ([regex]::Escape("`$env:KOMOREBI_CONFIG_HOME = Join-Path `$root 'config\komorebi'"))
    }

    It 'bootstraps the managed YASB path even when the repo path contains spaces' {
        InModuleScope Winarchy {
            $root = 'C:\Repo With Space\Winarchy'
            $yasbc = 'C:\Program Files\YASB\yasbc.exe'
            Mock Get-WinarchyRoot { $root }
            Mock Get-WinarchyKomorebiExe { $null }
            Mock Get-Command { [pscustomobject]@{ Source = $null } }
            Mock Get-Command { [pscustomobject]@{ Source = $yasbc } } -ParameterFilter { $Name -eq 'yasbc' }

            $yasb = Get-WinarchyAutostartComponents | Where-Object Key -eq 'yasb'
            $yasb.Exe | Should -BeLike '*\WindowsPowerShell\v1.0\powershell.exe'
            $yasb.Arguments | Should -Match ([regex]::Escape('scripts\Start-Yasb.ps1'))
            $yasb.Arguments | Should -Match ([regex]::Escape('-YasbExe "C:\Program Files\YASB\yasbc.exe"'))
            $yasb.Arguments | Should -Match ([regex]::Escape('-YasbConfigHome "C:\Repo With Space\Winarchy\config\yasb"'))
            ([xml](New-WinarchyTaskXml -Component $yasb -User 'DOMAIN\user')).Task.Actions.Exec.Arguments |
                Should -Match 'YasbConfigHome'
        }
    }
}

Describe 'Runtime configuration environment' {
    It 'repairs missing and stale config paths before starting YASB' {
        InModuleScope Winarchy -Parameters @{ Root = $script:Root } {
            param($Root)
            $oldKomorebi = $env:KOMOREBI_CONFIG_HOME
            $oldYasb = $env:YASB_CONFIG_HOME
            try {
                Mock Get-WinarchyRoot { $Root }
                Mock Test-WinarchyProcess { $false }
                Mock Get-WinarchyActiveBar { 'yasb' }
                Mock Start-WinarchyKomorebi {}
                Mock Restore-WinarchyFloatingApps {}
                Mock Get-Command { [pscustomobject]@{ Source = 'C:\Program Files\YASB\yasbc.exe' } } -ParameterFilter { $Name -eq 'yasbc' }
                Mock Start-Process {}
                Mock Stop-Process {}
                Mock Stop-WinarchyShell { $true }

                foreach ($initial in @($null, 'C:\stale')) {
                    $env:KOMOREBI_CONFIG_HOME = $initial
                    $env:YASB_CONFIG_HOME = $initial
                    Invoke-WinarchyReload -Light
                    $env:KOMOREBI_CONFIG_HOME | Should -Be (Join-Path $Root 'config\komorebi')
                    $env:YASB_CONFIG_HOME | Should -Be (Join-Path $Root 'config\yasb')
                }
                Should -Invoke Start-Process -Times 2 -ParameterFilter { $FilePath -eq 'yasbc' }
            }
            finally {
                $env:KOMOREBI_CONFIG_HOME = $oldKomorebi
                $env:YASB_CONFIG_HOME = $oldYasb
            }
        }
    }

    It 'removes Claude child session variables before reloading AHK' {
        InModuleScope Winarchy -Parameters @{ Root = $script:Root } {
            param($Root)
            $env:CLAUDE_CODE_CHILD_SESSION = '1'
            $env:CLAUDECODE = '1'
            try {
                Mock Get-WinarchyRoot { $Root }
                Mock Test-WinarchyProcess { $false }
                Mock Start-WinarchyKomorebi {}
                Mock Restore-WinarchyFloatingApps {}
                Mock Start-WinarchyWindowSlots {}
                Mock Get-WinarchyAhkExe { 'AutoHotkey64.exe' }
                Mock Get-Command { [pscustomobject]@{ Source = 'C:\Program Files\YASB\yasbc.exe' } } -ParameterFilter { $Name -eq 'yasbc' }
                Mock Start-Process {}
                Mock Stop-Process {}
                Mock Stop-WinarchyShell { $true }

                Invoke-WinarchyReload
                $env:CLAUDE_CODE_CHILD_SESSION | Should -BeNullOrEmpty
                $env:CLAUDECODE | Should -BeNullOrEmpty
            }
            finally {
                Remove-Item Env:CLAUDE_CODE_CHILD_SESSION, Env:CLAUDECODE -ErrorAction SilentlyContinue
            }
        }
    }
}

Describe 'Config environment diagnostics' {
    It 'reports only the current process as stale when user registration is correct' {
        InModuleScope Winarchy {
            $root = 'C:\Repo With Space\Winarchy'
            $expectedKomorebi = Join-Path $root 'config\komorebi'
            $expectedYasb = Join-Path $root 'config\yasb'
            $expectedWezterm = Join-Path $root 'config\wezterm\wezterm.lua'
            $checks = @(Get-WinarchyConfigEnvironmentChecks -Root $root `
                -ProcessKomorebi '' -ProcessYasb 'C:\stale' -ProcessWezterm '' `
                -UserKomorebi $expectedKomorebi -UserYasb $expectedYasb -UserWezterm $expectedWezterm)

            ($checks | Where-Object Name -eq 'config env vars in current process').Ok | Should -BeFalse
            ($checks | Where-Object Name -eq 'config env vars in current process').Fix | Should -Match 'reopen'
            ($checks | Where-Object Name -eq 'config env vars registered for user').Ok | Should -BeTrue
        }
    }

    It 'reports user registration drift with the installer remediation' {
        InModuleScope Winarchy {
            $root = 'C:\Repo With Space\Winarchy'
            $expectedKomorebi = Join-Path $root 'config\komorebi'
            $expectedYasb = Join-Path $root 'config\yasb'
            $expectedWezterm = Join-Path $root 'config\wezterm\wezterm.lua'
            $checks = @(Get-WinarchyConfigEnvironmentChecks -Root $root `
                -ProcessKomorebi $expectedKomorebi -ProcessYasb $expectedYasb -ProcessWezterm $expectedWezterm `
                -UserKomorebi 'C:\old\komorebi' -UserYasb 'C:\old\yasb' -UserWezterm 'C:\old\wezterm.lua')

            ($checks | Where-Object Name -eq 'config env vars in current process').Ok | Should -BeTrue
            ($checks | Where-Object Name -eq 'config env vars registered for user').Ok | Should -BeFalse
            ($checks | Where-Object Name -eq 'config env vars registered for user').Fix | Should -Match 'install\.ps1'
        }
    }
}

Describe 'WezTerm context menu' {
    BeforeAll {
        InModuleScope Winarchy {
            $fakeExeDir = Join-Path $env:TEMP 'winarchy-tests-wezterm'
            New-Item -ItemType Directory -Path $fakeExeDir -Force | Out-Null
            $script:FakeWeztermGui = Join-Path $fakeExeDir 'wezterm-gui.exe'
            $script:FakeWeztermCli = Join-Path $fakeExeDir 'wezterm.exe'
            Set-Content -Path $script:FakeWeztermGui -Value 'fake' -Encoding UTF8
            Set-Content -Path $script:FakeWeztermCli -Value 'fake' -Encoding UTF8

            # Namespace de test dedicado: nunca tocar la subclave real de WinarchyWezTerm.
            $script:WinarchyWeztermContextMenuKeys = @(
                'Registry::HKEY_CURRENT_USER\Software\Winarchy\Tests\ContextMenuA',
                'Registry::HKEY_CURRENT_USER\Software\Winarchy\Tests\ContextMenuB'
            )
        }
    }

    AfterAll {
        InModuleScope Winarchy {
            foreach ($k in $script:WinarchyWeztermContextMenuKeys) {
                if (Test-Path $k) { Remove-Item -Path $k -Recurse -Force -ErrorAction SilentlyContinue }
            }
            Remove-Item -Path (Split-Path $script:FakeWeztermGui -Parent) -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    AfterEach {
        InModuleScope Winarchy {
            foreach ($k in $script:WinarchyWeztermContextMenuKeys) {
                if (Test-Path $k) { Remove-Item -Path $k -Recurse -Force -ErrorAction SilentlyContinue }
            }
        }
    }

    It 'install is idempotent: running it twice does not fail or duplicate keys' {
        InModuleScope Winarchy {
            Mock Get-WinarchyWeztermExe { $script:FakeWeztermGui }
            Mock Test-WinarchyWeztermNativeContextMenu { $false }
            { Install-WinarchyWeztermContextMenu } | Should -Not -Throw
            { Install-WinarchyWeztermContextMenu } | Should -Not -Throw

            foreach ($k in $script:WinarchyWeztermContextMenuKeys) {
                Test-Path $k | Should -BeTrue
                $cmd = (Get-ItemProperty -Path (Join-Path $k 'command') -Name '(default)').'(default)'
                $cmd | Should -Match ([regex]::Escape($script:FakeWeztermCli))
                $cmd | Should -Match 'start --cwd'
            }
        }
    }

    It 'status reports installed with the resolved target after install' {
        InModuleScope Winarchy {
            Mock Get-WinarchyWeztermExe { $script:FakeWeztermGui }
            Mock Test-WinarchyWeztermNativeContextMenu { $false }
            Install-WinarchyWeztermContextMenu
            $status = Test-WinarchyWeztermContextMenuInstalled
            $status.Installed | Should -BeTrue
            $status.Source | Should -Be 'winarchy'
            $status.TargetExists | Should -BeTrue
            $status.TargetPath | Should -Be $script:FakeWeztermCli
        }
    }

    It 'remove on a not-installed state is a no-op without error' {
        InModuleScope Winarchy {
            Mock Test-WinarchyWeztermNativeContextMenu { $false }
            (Test-WinarchyWeztermContextMenuInstalled).Installed | Should -BeFalse
            { Remove-WinarchyWeztermContextMenu } | Should -Not -Throw
            (Test-WinarchyWeztermContextMenuInstalled).Installed | Should -BeFalse
        }
    }

    It 'remove after install clears the keys' {
        InModuleScope Winarchy {
            Mock Get-WinarchyWeztermExe { $script:FakeWeztermGui }
            Mock Test-WinarchyWeztermNativeContextMenu { $false }
            Install-WinarchyWeztermContextMenu
            Remove-WinarchyWeztermContextMenu
            foreach ($k in $script:WinarchyWeztermContextMenuKeys) { Test-Path $k | Should -BeFalse }
        }
    }

    It 'install skips without creating keys when WezTerm''s own installer already added the verb' {
        InModuleScope Winarchy {
            Mock Get-WinarchyWeztermExe { $script:FakeWeztermGui }
            Mock Test-WinarchyWeztermNativeContextMenu { $true }
            Install-WinarchyWeztermContextMenu
            foreach ($k in $script:WinarchyWeztermContextMenuKeys) { Test-Path $k | Should -BeFalse }
        }
    }

    It 'status reports the native WezTerm installer entry without claiming ownership' {
        InModuleScope Winarchy {
            Mock Test-WinarchyWeztermNativeContextMenu { $true }
            $status = Test-WinarchyWeztermContextMenuInstalled
            $status.Installed | Should -BeTrue
            $status.Source | Should -Be 'wezterm-installer'
        }
    }
}

Describe 'Add-WinarchyFlowKeyword' {
    It 'appends to a single-element array instead of concatenating it as a string' {
        InModuleScope Winarchy {
            # ConvertFrom-Json -AsHashtable turns ["*"] into a one-element array, not a string.
            $plugins = @{ 'p1' = @{ ActionKeywords = @('*') } }
            $changed = Add-WinarchyFlowKeyword $plugins 'p1' 'app'
            $changed | Should -BeTrue
            $plugins['p1']['ActionKeywords'] | Should -Be @('*', 'app')
        }
    }

    It 'is idempotent when the keyword is already present' {
        InModuleScope Winarchy {
            $plugins = @{ 'p1' = @{ ActionKeywords = @('*', 'app') } }
            Add-WinarchyFlowKeyword $plugins 'p1' 'app' | Should -BeFalse
            $plugins['p1']['ActionKeywords'] | Should -Be @('*', 'app')
        }
    }

    It 'returns false when the plugin id is not present' {
        InModuleScope Winarchy {
            $plugins = @{ 'p1' = @{ ActionKeywords = @('*') } }
            Add-WinarchyFlowKeyword $plugins 'nope' 'app' | Should -BeFalse
        }
    }
}

Describe 'Set-WinarchyFlowKeywords' {
    BeforeAll {
        $script:FlowProgramId = '791FC278BA414111B8D1886DFE447410'
        $script:FlowExplorerId = '572be03c74c642baae319fc283e561a8'
        $script:FlowLegacyEverythingId = 'D2D2C23B084D411DB66FE0C79D6C2A6E'
    }

    It 'sets scoped keywords, drops the legacy Everything plugin, and is idempotent' {
        InModuleScope Winarchy -Parameters @{
            ProgramId  = $script:FlowProgramId
            ExplorerId = $script:FlowExplorerId
            LegacyId   = $script:FlowLegacyEverythingId
        } {
            param($ProgramId, $ExplorerId, $LegacyId)

            $oldAppData = $env:APPDATA
            $appData = Join-Path $TestDrive 'appdata'
            $env:APPDATA = $appData
            try {
                $settingsDir = Join-Path $appData 'FlowLauncher\Settings'
                $explorerDir = Join-Path $settingsDir 'Plugins\Flow.Launcher.Plugin.Explorer'
                $pluginsDir = Join-Path $appData 'FlowLauncher\Plugins'
                $legacyDir = Join-Path $pluginsDir 'Everything-1.7.7'
                New-Item -ItemType Directory -Path $settingsDir, $explorerDir, $legacyDir -Force | Out-Null

                $settings = @{
                    PluginSettings = @{
                        Plugins = @{
                            $ProgramId  = @{ ActionKeywords = @('*') }
                            $ExplorerId = @{ ActionKeywords = @('*', 'doc:') }
                            $LegacyId   = @{ ActionKeywords = @('*') }
                        }
                    }
                }
                $settingsPath = Join-Path $settingsDir 'Settings.json'
                $settings | ConvertTo-Json -Depth 10 | Set-Content -Path $settingsPath -Encoding UTF8

                $explorerSettings = @{ FileSearchActionKeyword = '*'; FileSearchKeywordEnabled = $false; IndexSearchEngine = 0 }
                $explorerPath = Join-Path $explorerDir 'Settings.json'
                $explorerSettings | ConvertTo-Json | Set-Content -Path $explorerPath -Encoding UTF8

                $legacyManifestPath = Join-Path $legacyDir 'plugin.json'
                @{ ID = $LegacyId; Name = 'Everything'; Version = '1.7.7' } | ConvertTo-Json |
                    Set-Content -Path $legacyManifestPath -Encoding UTF8

                Mock Get-Process { }
                Mock Start-Process { }
                $snapshotDir = Join-Path $TestDrive 'snapshot'
                New-Item -ItemType Directory -Path $snapshotDir -Force | Out-Null
                Mock New-WinarchySnapshot { $snapshotDir }

                Set-WinarchyFlowKeywords

                $result = Get-Content $settingsPath -Raw | ConvertFrom-Json -AsHashtable
                $plugins = $result['PluginSettings']['Plugins']
                @($plugins[$ProgramId]['ActionKeywords']) | Should -Be @('*', 'app')
                @($plugins[$ExplorerId]['ActionKeywords']) | Should -Contain 'f'
                $plugins.ContainsKey($LegacyId) | Should -BeFalse

                Test-Path $legacyDir | Should -BeFalse
                Test-Path (Join-Path $snapshotDir 'Everything-1.7.7') | Should -BeTrue

                $resultExplorer = Get-Content $explorerPath -Raw | ConvertFrom-Json -AsHashtable
                $resultExplorer['FileSearchActionKeyword'] | Should -Be 'f'
                $resultExplorer['FileSearchKeywordEnabled'] | Should -BeTrue

                Should -Invoke New-WinarchySnapshot -Times 1

                Set-WinarchyFlowKeywords

                Should -Invoke New-WinarchySnapshot -Times 1
            }
            finally {
                $env:APPDATA = $oldAppData
            }
        }
    }
}

Describe 'Set-WinarchyShareXIdentity' {
    It 'keeps the tray icon (silent start needs it), turns off the update check, keeps the rest, and is idempotent' {
        InModuleScope Winarchy {
            $configPath = Join-Path $TestDrive 'ShareX\ApplicationConfig.json'
            New-Item -ItemType Directory -Path (Split-Path $configPath) -Force | Out-Null
            @{ ShowTray = $false; AutoCheckUpdate = $true; TrayLeftClickAction = 'RectangleRegion' } |
                ConvertTo-Json | Set-Content -Path $configPath -Encoding UTF8
            Mock Get-WinarchyShareXExe { 'ShareX.exe' }
            Mock Get-Process { }
            Mock Start-Process { }
            Mock New-WinarchySnapshot { }
            Mock Join-Path { $configPath } -ParameterFilter { $ChildPath -eq 'ShareX\ApplicationConfig.json' }

            Set-WinarchyShareXIdentity

            $result = Get-Content $configPath -Raw | ConvertFrom-Json -AsHashtable
            $result['ShowTray'] | Should -BeTrue
            $result['AutoCheckUpdate'] | Should -BeFalse
            $result['TrayLeftClickAction'] | Should -Be 'RectangleRegion'

            Set-WinarchyShareXIdentity
            Should -Invoke New-WinarchySnapshot -Times 1
        }
    }
}

Describe 'Set-WinarchyEverythingIdentity' {
    It 'turns off the tray icon and the update check, keeps the rest, and is idempotent' {
        InModuleScope Winarchy {
            $oldAppData = $env:APPDATA
            $oldProgramFiles = $env:ProgramFiles
            $env:APPDATA = Join-Path $TestDrive 'appdata'
            $env:ProgramFiles = Join-Path $TestDrive 'programfiles'
            try {
                New-Item -ItemType File -Path (Join-Path $env:ProgramFiles 'Everything\Everything.exe') -Force | Out-Null
                $iniPath = Join-Path $env:APPDATA 'Everything\Everything.ini'
                New-Item -ItemType Directory -Path (Split-Path $iniPath) -Force | Out-Null
                "; comment`r`n[Everything]`r`nwindow_x=10`r`nshow_tray_icon=1`r`n" |
                    Set-Content -Path $iniPath -NoNewline -Encoding UTF8
                Mock Get-Process { [pscustomobject]@{ SessionId = 1 } }
                Mock Get-Process { } -ParameterFilter { $Name -eq 'Everything' }
                Mock Start-Process { }
                Mock New-WinarchySnapshot { }

                Set-WinarchyEverythingIdentity

                $result = Get-Content $iniPath -Raw
                $result | Should -Match '(?m)^show_tray_icon=0\r$'
                $result | Should -Match '(?m)^check_for_updates_on_startup=0\r$'
                $result | Should -Match '(?m)^window_x=10\r$'
                $result | Should -Match '^; comment'

                Set-WinarchyEverythingIdentity
                Should -Invoke New-WinarchySnapshot -Times 1
            }
            finally {
                $env:APPDATA = $oldAppData
                $env:ProgramFiles = $oldProgramFiles
            }
        }
    }
}


Describe 'Menu font from the theme' {
    It 'emits bar.font_family into theme.ini and the palette reads it with a JetBrainsMono fallback' {
        (Get-Content (Join-Path $script:Root 'templates\ahk-theme.ini.tpl') -Raw) | Should -Match '(?s)\[font\]\s+family=\{\{bar\.font_family\}\}'
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw
        $ahk | Should -Match "IniRead\(ini, 'font', 'family', c\.font\)"
        $ahk | Should -Match 'g\.SetFont\(opts, font\)'
    }
}

Describe 'Assistant doctor check' {
    It 'checks the assistant process only when it is enabled' {
        $src = Get-Content (Join-Path $script:Root 'module\Winarchy\Private\Doctor.ps1') -Raw
        $src | Should -Match "(?s)assistant\.enabled'\)\) \{\s+Add-Check 'voice assistant running' \(Test-WinarchyAssistantRunning\)"
    }
}


Describe 'Get-WinarchyFlowPluginUpdates' {
    It 'returns only plugins with a strictly newer manifest version' {
        InModuleScope Winarchy {
            $installed = @(
                [pscustomobject]@{ ID = 'a'; Name = 'Newer'; Version = '1.0.0'; Dir = 'C:\a' }
                [pscustomobject]@{ ID = 'b'; Name = 'Same'; Version = '2.0.0'; Dir = 'C:\b' }
                [pscustomobject]@{ ID = 'c'; Name = 'Older'; Version = '3.0.0'; Dir = 'C:\c' }
                [pscustomobject]@{ ID = 'd'; Name = 'Unknown'; Version = '1.0.0'; Dir = 'C:\d' }
                [pscustomobject]@{ ID = 'e'; Name = 'Bad'; Version = 'not-a-version'; Dir = 'C:\e' }
            )
            $manifest = @(
                [pscustomobject]@{ ID = 'a'; Version = '1.1.0'; UrlDownload = 'https://example.test/a.zip' }
                [pscustomobject]@{ ID = 'b'; Version = '2.0.0'; UrlDownload = 'https://example.test/b.zip' }
                [pscustomobject]@{ ID = 'c'; Version = '2.9.0'; UrlDownload = 'https://example.test/c.zip' }
                [pscustomobject]@{ ID = 'e'; Version = '1.0.0'; UrlDownload = 'https://example.test/e.zip' }
            )
            $updates = @(Get-WinarchyFlowPluginUpdates -Installed $installed -Manifest $manifest)
            $updates.Count | Should -Be 1
            $updates[0].ID | Should -Be 'a'
            $updates[0].From | Should -Be '1.0.0'
            $updates[0].To | Should -Be '1.1.0'
        }
    }

    It 'accepts a plain array manifest (as parsed from plugins.json)' {
        InModuleScope Winarchy {
            $installed = @([pscustomobject]@{ ID = 'x'; Name = 'X'; Version = '0.1.0'; Dir = 'C:\x' })
            $manifest = ,([pscustomobject]@{ ID = 'x'; Version = '0.2.0'; UrlDownload = 'https://example.test/x.zip' })
            $updates = @(Get-WinarchyFlowPluginUpdates -Installed $installed -Manifest $manifest)
            $updates.Count | Should -Be 1
            $updates[0].To | Should -Be '0.2.0'
        }
    }

    It 'returns nothing when nothing is installed' {
        InModuleScope Winarchy {
            @(Get-WinarchyFlowPluginUpdates -Installed @() -Manifest @([pscustomobject]@{ ID = 'a'; Version = '1.0.0' })) |
                Should -BeNullOrEmpty
        }
    }
}

Describe 'First install on a clean machine' {
    It 'takes a snapshot when none of the paths exist yet' {
        InModuleScope Winarchy {
            Mock Get-WinarchyBackupsDir { $TestDrive }
            $dir = New-WinarchySnapshot -Path @() -Label 'empty'
            Test-Path $dir | Should -BeTrue
            @(Get-ChildItem $dir).Count | Should -Be 0
        }
    }

    It 'falls back when a component is not on PATH yet' {
        InModuleScope Winarchy {
            Mock Get-Command { }
            Mock Test-Path { $false }
            Get-WinarchyKomorebiExe | Should -BeNullOrEmpty
        }
    }

    It 'never reads .Source off a Get-Command that may find nothing' {
        $files = git -C $script:Root ls-files '*.ps1' '*.psm1' ':!tests' | ForEach-Object { Join-Path $script:Root $_ }
        $hits = Select-String -Path $files -Pattern '\(Get-Command[^)]*SilentlyContinue\)\.Source'
        $hits | ForEach-Object { "$($_.Path):$($_.LineNumber)" } | Should -BeNullOrEmpty
    }

    Context 'Flow profile' {
        BeforeEach {
            $script:SavedEnv = @{ APPDATA = $env:APPDATA; LOCALAPPDATA = $env:LOCALAPPDATA }
            $env:APPDATA = Join-Path $TestDrive "appdata-$([guid]::NewGuid())"
            $env:LOCALAPPDATA = Join-Path $TestDrive "local-$([guid]::NewGuid())"
            New-Item -ItemType Directory -Force "$env:LOCALAPPDATA\FlowLauncher" | Out-Null
            Set-Content "$env:LOCALAPPDATA\FlowLauncher\Flow.Launcher.exe" ''
        }
        AfterEach {
            $env:APPDATA = $script:SavedEnv.APPDATA
            $env:LOCALAPPDATA = $script:SavedEnv.LOCALAPPDATA
        }

        It 'starts Flow once when its profile does not exist yet, then stops it' {
            InModuleScope Winarchy {
                Mock Start-Process {
                    foreach ($p in 'Settings\Settings.json', 'Settings\Plugins\Flow.Launcher.Plugin.Explorer\Settings.json') {
                        New-Item -ItemType File -Force -Path (Join-Path "$env:APPDATA\FlowLauncher" $p) | Out-Null
                    }
                    [pscustomobject]@{}
                }
                Mock Start-Sleep { }
                Mock Get-Process { [pscustomobject]@{ Name = 'Flow.Launcher' } } -ParameterFilter { $Name -eq 'Flow.Launcher' }
                Mock Stop-Process { }
                Mock Wait-Process { }
                Initialize-WinarchyFlow
                Should -Invoke Start-Process -Times 1
                Should -Invoke Stop-Process -Times 1
            }
        }

        It 'leaves an existing profile alone' {
            InModuleScope Winarchy {
                foreach ($p in 'Settings\Settings.json', 'Settings\Plugins\Flow.Launcher.Plugin.Explorer\Settings.json') {
                    New-Item -ItemType File -Force -Path (Join-Path "$env:APPDATA\FlowLauncher" $p) | Out-Null
                }
                Mock Start-Process { }
                Initialize-WinarchyFlow
                Should -Invoke Start-Process -Times 0
            }
        }

        It 'does nothing when Flow is not installed' {
            InModuleScope Winarchy {
                Remove-Item "$env:LOCALAPPDATA\FlowLauncher\Flow.Launcher.exe"
                Mock Start-Process { }
                Initialize-WinarchyFlow
                Should -Invoke Start-Process -Times 0
            }
        }
    }

    It 'writes a profile hook that survives an apostrophe in the install path' {
        InModuleScope Winarchy {
            $root = Join-Path $TestDrive "d'Test"
            $profilePath = Join-Path $TestDrive 'profile.ps1'
            Mock Get-WinarchyRoot { $root }
            Mock Get-WinarchyShellProfilePath { $profilePath }
            Install-WinarchyShellProfile
            $errors = $null
            $ast = [System.Management.Automation.Language.Parser]::ParseFile($profilePath, [ref]$null, [ref]$errors)
            $errors | Should -BeNullOrEmpty
            $ast.FindAll({ $args[0] -is [System.Management.Automation.Language.StringConstantExpressionAst] }, $true).Value |
                Should -Contain (Join-Path $root 'config\pwsh\profile.ps1')
            Remove-WinarchyShellProfile
            Test-Path $profilePath | Should -BeFalse
        }
    }

    It 'reads the winget upgrade result from its exit code, whatever the Windows language' {
        InModuleScope Winarchy {
            foreach ($case in @(
                    @{ Code = 0; Status = 'updated' },
                    @{ Code = -1978334967; Status = 'updated' },
                    @{ Code = -1978335189; Status = 'current' },
                    @{ Code = -1978335212; Status = 'missing' },
                    @{ Code = -1978335226; Status = 'failed' })) {
                $script:WingetCode = $case.Code
                function winget { 'Keine anwendbaren Upgrades gefunden.'; $global:LASTEXITCODE = $script:WingetCode }
                (Update-WinarchyWingetPackage -Id 'x.y').Status | Should -Be $case.Status
            }
        }
    }

    It 'reads the installed version of a winget package' {
        InModuleScope Winarchy {
            function winget {
                'Name              Id                    Version  Available Source'
                '------------------------------------------------------------------'
                'AutoHotkey 1.1.37 AutoHotkey.AutoHotkey 1.1.37.02 2.0.28   winget'
            }
            Get-WinarchyInstalledVersion -Id 'AutoHotkey.AutoHotkey' | Should -Be '1.1.37.02'
            function winget { 'No installed package found matching input criteria.' }
            Get-WinarchyInstalledVersion -Id 'AutoHotkey.AutoHotkey' | Should -BeNullOrEmpty
        }
    }

    It 'compares an installed version with the pin' {
        Test-WinarchyVersionBelow -Installed '1.1.37.02' -Pinned '2.0.28' | Should -BeTrue
        Test-WinarchyVersionBelow -Installed '2.0.26' -Pinned '2.0.28' | Should -BeTrue
        Test-WinarchyVersionBelow -Installed '2.0.28' -Pinned '2.0.28' | Should -BeFalse
        Test-WinarchyVersionBelow -Installed '2.0.30' -Pinned '2.0.28' | Should -BeFalse
        Test-WinarchyVersionBelow -Installed '20240203-110809-5046fc22' -Pinned '20240203-110809-5046fc22' | Should -BeFalse
        Test-WinarchyVersionBelow -Installed 'Unknown' -Pinned '2.0.28' | Should -BeFalse
    }

    It 'exports every module function the scripts call' {
        $exported = @((Get-Module Winarchy).ExportedFunctions.Keys)
        $scripts = git -C $script:Root ls-files '*.ps1' ':!tests' ':!module' |
            ForEach-Object { Join-Path $script:Root $_ } |
            Where-Object { Select-String -Path $_ -SimpleMatch 'Winarchy.psd1' -Quiet }
        $missing = foreach ($file in $scripts) {
            $ast = [System.Management.Automation.Language.Parser]::ParseFile($file, [ref]$null, [ref]$null)
            $defined = @($ast.FindAll({ $args[0] -is [System.Management.Automation.Language.FunctionDefinitionAst] }, $true).Name)
            $ast.FindAll({ $args[0] -is [System.Management.Automation.Language.CommandAst] }, $true) |
                ForEach-Object { $_.GetCommandName() } |
                Where-Object { $_ -match '^\w+-Winarchy' -and $_ -notin $exported -and $_ -notin $defined } |
                ForEach-Object { "$(Split-Path $file -Leaf): $_" }
        }
        $missing | Sort-Object -Unique | Should -BeNullOrEmpty
    }

    It 'keeps the line endings of games.user.toml when a game is added' {
        InModuleScope Winarchy {
            $root = Join-Path $TestDrive 'games-root'
            New-Item -ItemType Directory -Force $root | Out-Null
            [IO.File]::WriteAllText((Join-Path $root 'games.user.toml'), "[[games]]`r`nexe = `"a.exe`"`r`n")
            Mock Get-WinarchyRoot { $root }
            Mock Update-WinarchyKomorebiRules { }
            Mock Test-WinarchyProcess { $false }
            Mock Update-WinarchyQuickAccentExclusion { }
            Add-WinarchyGame -Exe 'b.exe'
            $raw = [IO.File]::ReadAllText((Join-Path $root 'games.user.toml'))
            $raw | Should -Not -Match "(?<!`r)`n"
            $raw | Should -Match "exe = `"b.exe`"`r`n$"
        }
    }

    It 'talks to the user in English' {
        $files = git -C $script:Root ls-files '*.ps1' '*.psm1' ':!tests' ':!scripts/*seelen*' | ForEach-Object { Join-Path $script:Root $_ }
        $hits = Select-String -Path $files -Pattern '(Write-Winarchy(Ok|Info|Warn|Err)|Invoke-Step|throw)\b.*[áéíóúñ¿¡]'
        $hits | ForEach-Object { "$($_.Path):$($_.LineNumber)" } | Should -BeNullOrEmpty
    }

    It 'queries winget only through its own source' {
        $files = git -C $script:Root ls-files '*.ps1' '*.psm1' ':!tests' | ForEach-Object { Join-Path $script:Root $_ }
        $hits = Select-String -Path $files -Pattern '^[^''"#]*\bwinget (install|upgrade|uninstall|show|list|search|pin add|pin remove)\b' |
            Where-Object { $_.Line -notmatch '--source winget' }
        $hits | ForEach-Object { "$($_.Path):$($_.LineNumber)" } | Should -BeNullOrEmpty
        Get-Content (Join-Path $script:Root 'install.ps1') -Raw | Should -Match "'install', '--id', \`$id, '--exact', '--source', 'winget'"
    }

    It 'installs every [core] version pinned' {
        $lock = Import-WinarchyToml -Path (Join-Path $script:Root 'versions.lock.toml')
        $pinned = Get-WinarchyCoreVersions -Lock $lock
        @($pinned.Values | Sort-Object) | Should -Be @($lock['core'].Values | Sort-Object)
        foreach ($id in $pinned.Keys) { ($lock['winget'][$id] -or $lock['winget-optional'][$id]) | Should -BeTrue -Because $id }
    }
}

Describe 'boot.ps1' {
    BeforeDiscovery {
        $elevated = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
            [Security.Principal.WindowsBuiltInRole]::Administrator)
    }

    BeforeAll {
        $script:Boot = Join-Path $script:Root 'boot.ps1'
        $script:Harness = Join-Path $TestDrive 'harness.ps1'
        Set-Content -Path $script:Harness -Encoding ASCII -Value @'
param($Boot, $Dir, $Log, [string]$Strip, $WingetExit, $Activate, $Branch, $SchtasksExit)
$global:Log = $Log; $global:WingetExit = [int]$WingetExit; $global:Branch = $Branch; $global:SchtasksExit = [int]$SchtasksExit
$env:WINARCHY_DIR = $Dir
$env:WINARCHY_ACTIVATE = $Activate
$env:Path = ($env:Path -split ';' | Where-Object { $_ -and -not ($Strip -and (Test-Path (Join-Path $_ "$Strip.exe"))) }) -join ';'
function global:winget { Add-Content $global:Log "winget $args"; $global:LASTEXITCODE = 0; if ($args[0] -eq 'install') { $global:LASTEXITCODE = $global:WingetExit } }
function global:git {
    Add-Content $global:Log "git $args"
    if ($args[0] -eq 'clone') { New-Item -ItemType Directory -Force -Path (Join-Path $args[-1] '.git') | Out-Null }
    if ($args -contains 'rev-parse') { $global:Branch }
    $global:LASTEXITCODE = 0
}
function global:pwsh { Add-Content $global:Log "pwsh $args"; $global:LASTEXITCODE = 0 }
function global:schtasks { $global:LASTEXITCODE = $global:SchtasksExit }
try { Invoke-Expression (Get-Content $Boot -Raw) } catch { Add-Content $global:Log "ERROR $($_.Exception.Message)" }
'@

        function Invoke-Boot {
            param([string]$Strip = '', [int]$WingetExit = 0, [string]$Activate = '', [string]$Branch = 'release',
                [int]$SchtasksExit = 1, [scriptblock]$Arrange)
            $dir = Join-Path $TestDrive ([guid]::NewGuid())
            $log = "$dir.log"
            if ($Arrange) { & $Arrange $dir }
            $null = & $script:Shell -NoProfile -ExecutionPolicy Bypass -File $script:Harness -Boot $script:Boot -Dir $dir `
                -Log $log -Strip $Strip -WingetExit $WingetExit -Activate $Activate -Branch $Branch -SchtasksExit $SchtasksExit
            [pscustomobject]@{ Dir = $dir; Log = @(if (Test-Path $log) { Get-Content $log }) }
        }
    }

    It 'is plain ASCII without a BOM' {
        [IO.File]::ReadAllBytes($script:Boot) | Where-Object { $_ -ge 0x80 } | Should -BeNullOrEmpty
    }

    It 'parses in Windows PowerShell 5.1' {
        $errors = powershell.exe -NoProfile -Command "`$e = `$null; [void][System.Management.Automation.Language.Parser]::ParseFile('$script:Boot', [ref]`$null, [ref]`$e); `$e | ForEach-Object Message"
        $errors | Should -BeNullOrEmpty
    }

    Context 'run in <_> with fake winget, git and pwsh' -ForEach 'powershell.exe', 'pwsh' -Skip:$elevated {
        BeforeAll { $script:Shell = $_ }

        It 'installs git when missing, then clones release' {
            $r = Invoke-Boot -Strip git
            ($r.Log -match '^winget install --id Git\.Git --exact --source winget').Count | Should -Be 1
            $r.Log | Should -Contain "git clone --branch release https://github.com/guidonaselli/winarchy.git $($r.Dir)"
            $r.Log | Should -Contain "pwsh -NoProfile -ExecutionPolicy Bypass -File $($r.Dir)\install.ps1"
            $r.Log -match '^ERROR' | Should -BeNullOrEmpty
        }

        It 'goes on when winget asks for a restart to finish the install' {
            $r = Invoke-Boot -Strip git -WingetExit -1978334967
            $r.Log -match '^ERROR' | Should -BeNullOrEmpty
            $r.Log -match '^git clone' | Should -Not -BeNullOrEmpty
        }

        It 'installs only pwsh when git is present' {
            $r = Invoke-Boot -Strip pwsh
            ($r.Log -match '^winget install').Count | Should -Be 1
            ($r.Log -match '^winget install --id Microsoft\.PowerShell --exact').Count | Should -Be 1
        }

        It 'stops without cloning when winget fails' {
            $r = Invoke-Boot -Strip git -WingetExit 1603
            $r.Log -match '^ERROR .*Git\.Git.*1603' | Should -Not -BeNullOrEmpty
            $r.Log -match '^(git|pwsh) ' | Should -BeNullOrEmpty
        }

        It 'pulls when the folder is already a release checkout' {
            $r = Invoke-Boot -Arrange { param($d) New-Item -ItemType Directory -Force -Path "$d\.git" | Out-Null }
            $r.Log | Should -Contain "git -C $($r.Dir) pull --ff-only origin release"
            $r.Log -match '^git clone' | Should -BeNullOrEmpty
            $r.Log -match '^pwsh ' | Should -Not -BeNullOrEmpty
        }

        It 'refuses a checkout on another branch' {
            $r = Invoke-Boot -Branch main -Arrange { param($d) New-Item -ItemType Directory -Force -Path "$d\.git" | Out-Null }
            $r.Log -match "^ERROR .*on 'main'" | Should -Not -BeNullOrEmpty
            $r.Log -match '^(git -C \S+ pull|pwsh )' | Should -BeNullOrEmpty
        }

        It 'passes -Activate with WINARCHY_ACTIVATE=1' {
            $r = Invoke-Boot -Activate '1' -SchtasksExit 0
            $r.Log | Should -Contain "pwsh -NoProfile -ExecutionPolicy Bypass -File $($r.Dir)\install.ps1 -Activate"
        }

        It 'leaves an occupied folder untouched' {
            $r = Invoke-Boot -Arrange { param($d) New-Item -ItemType Directory -Force -Path $d | Out-Null; Set-Content "$d\keep.txt" 'x' }
            $r.Log -match '^ERROR .*not a Winarchy checkout' | Should -Not -BeNullOrEmpty
            $r.Log -match '^(git|pwsh) ' | Should -BeNullOrEmpty
            @(Get-ChildItem $r.Dir).Name | Should -Be @('keep.txt')
        }
    }
}

Describe 'Get-WinarchyLockScreenImage' {
    BeforeEach {
        $script:Dir = Join-Path $TestDrive ([guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $script:Dir -Force | Out-Null
    }
    It 'prefers the theme lockscreen over everything else, even when dynamic' {
        New-Item (Join-Path $script:Dir 'lockscreen.jpg') -ItemType File | Out-Null
        InModuleScope Winarchy -Parameters @{ Dir = $script:Dir } {
            Get-WinarchyLockScreenImage -ThemeDir $Dir | Should -Be (Join-Path $Dir 'lockscreen.jpg')
            Get-WinarchyLockScreenImage -ThemeDir $Dir -Dynamic | Should -Be (Join-Path $Dir 'lockscreen.jpg')
        }
    }
    It 'falls back to the active background' {
        InModuleScope Winarchy -Parameters @{ Dir = $script:Dir } {
            Mock Get-WinarchyBackground { 'wall.png' }
            Get-WinarchyLockScreenImage -ThemeDir $Dir | Should -Be (Join-Path $Dir 'backgrounds\wall.png')
        }
    }
    It 'returns nothing for dynamic themes, Wallpaper Engine themes or themes without backgrounds' {
        InModuleScope Winarchy -Parameters @{ Dir = $script:Dir } {
            Mock Get-WinarchyBackground { 'wall.png' }
            Get-WinarchyLockScreenImage -ThemeDir $Dir -Dynamic | Should -BeNullOrEmpty
            New-Item (Join-Path $Dir 'wallpaper-engine.txt') -ItemType File | Out-Null
            Get-WinarchyLockScreenImage -ThemeDir $Dir | Should -BeNullOrEmpty
            Remove-Item (Join-Path $Dir 'wallpaper-engine.txt')
            Mock Get-WinarchyBackground { $null }
            Get-WinarchyLockScreenImage -ThemeDir $Dir | Should -BeNullOrEmpty
        }
    }
}

Describe 'Accent lock screen' {
    It 'scales the mark to fit the screen with a margin' {
        InModuleScope Winarchy {
            Get-WinarchyLockScreenScale -Width 1920 -Height 1080 | Should -BeGreaterThan 17
            Get-WinarchyLockScreenScale -Width 1920 -Height 1080 | Should -BeLessThan 18
            Get-WinarchyLockScreenScale -Width 3440 -Height 1440 | Should -Be (1440 * 0.9 / 56)
        }
    }
    It 'renders the requested size with the accent on the last bar and the background in the corner' {
        $out = Join-Path $TestDrive 'lock.png'
        InModuleScope Winarchy -Parameters @{ Out = $out } {
            New-WinarchyAccentLockScreen -Accent '#e5484d' -Background '#1a1b26' -Foreground '#c8c8c8' -OutPath $Out -Width 800 -Height 450
        }
        Add-Type -AssemblyName System.Drawing
        $bmp = [System.Drawing.Bitmap]::new($out)
        try {
            $bmp.Width | Should -Be 800
            $bmp.Height | Should -Be 450
            $bar = $bmp.GetPixel(680, 250)
            $bar.R | Should -BeGreaterThan ($bar.B + 40)
            $corner = $bmp.GetPixel(2, 2)
            [Math]::Abs($corner.R - 0x1a) | Should -BeLessThan 12
            [Math]::Abs($corner.B - 0x26) | Should -BeLessThan 12
        }
        finally { $bmp.Dispose() }
    }
    It 'generates and applies the image for a dynamic theme without lockscreen' {
        $dir = Join-Path $TestDrive 'dyn'
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
        InModuleScope Winarchy -Parameters @{ Dir = $dir; State = $TestDrive } {
            Mock Get-WinarchyStateDir { $State }
            Mock Get-WinarchyPrimaryScreenSize { @{ Width = 400; Height = 225 } }
            Mock Set-WinarchyLockScreen {}
            $colors = @{ accent = '#0078d4'; background = '#1a1b26'; foreground = '#c8c8c8' }
            Update-WinarchyLockScreen -ThemeDir $Dir -Colors $colors -Dynamic
            Should -Invoke Set-WinarchyLockScreen -Times 1 -ParameterFilter { $ImagePath -eq (Join-Path $State 'lockscreen-accent.png') }
            Test-Path (Join-Path $State 'lockscreen-accent.png') | Should -BeTrue
        }
    }
    It 'leaves the lock screen alone for a static theme without images' {
        $dir = Join-Path $TestDrive 'static'
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
        InModuleScope Winarchy -Parameters @{ Dir = $dir } {
            Mock Get-WinarchyBackground { $null }
            Mock Set-WinarchyLockScreen {}
            Update-WinarchyLockScreen -ThemeDir $Dir -Colors @{ accent = '#0078d4'; background = '#1a1b26'; foreground = '#c8c8c8' }
            Should -Invoke Set-WinarchyLockScreen -Times 0
        }
    }
}

Describe 'Get-WinarchyDefenderExclusionPaths' {
    It 'includes the profile tools and the target of their scoop shims' {
        InModuleScope Winarchy {
            $real = Join-Path $TestDrive 'apps\eza\eza.exe'
            $shim = Join-Path $TestDrive 'shims\eza.exe'
            New-Item -ItemType Directory -Path (Split-Path $real), (Split-Path $shim) -Force | Out-Null
            Set-Content $real 'x'; Set-Content $shim 'x'
            Set-Content ([IO.Path]::ChangeExtension($shim, '.shim')) "path = `"$real`""
            Mock Get-Command { }
            Mock Get-Command { [pscustomobject]@{ Source = $shim } } -ParameterFilter { $All -and $Name -eq 'eza' }

            $paths = @(Get-WinarchyDefenderExclusionPaths)

            $paths | Should -Contain $shim
            $paths | Should -Contain $real
        }
    }
}

Describe 'Get-WinarchyActiveBar' {
    It 'honors state/bar; absent or unknown is native, or yasb on machines that already had it' {
        InModuleScope Winarchy {
            $root = Join-Path $TestDrive 'repo'
            New-Item -ItemType Directory -Path (Join-Path $root 'state') -Force | Out-Null
            Mock Get-WinarchyRoot { $root }
            Mock Get-Command { $null } -ParameterFilter { $Name -eq 'yasbc' }
            Get-WinarchyActiveBar | Should -Be 'native'
            Mock Get-Command { [pscustomobject]@{ Source = 'yasbc.exe' } } -ParameterFilter { $Name -eq 'yasbc' }
            Get-WinarchyActiveBar | Should -Be 'yasb'
            Set-Content (Join-Path $root 'state\bar') 'native'
            Get-WinarchyActiveBar | Should -Be 'native'
            Set-Content (Join-Path $root 'state\bar') "yasb`r`n"
            Mock Get-Command { $null } -ParameterFilter { $Name -eq 'yasbc' }
            Get-WinarchyActiveBar | Should -Be 'yasb'
        }
    }

    It 'keeps YASB out of the reload, autostart, doctor and the AHK watchdog when native' {
        $root = Split-Path $PSScriptRoot -Parent
        foreach ($f in 'module\Winarchy\Private\ThemeEngine.ps1', 'module\Winarchy\Private\Autostart.ps1', 'module\Winarchy\Private\Doctor.ps1', 'install.ps1') {
            (Get-Content (Join-Path $root $f) -Raw) | Should -Match 'Get-WinarchyActiveBar'
        }
        (Get-Content (Join-Path $root 'config\ahk\winarchy.ahk') -Raw) | Should -Match "(?s)YasbWatch\(\) \{.*?'native'"
    }
}

Describe 'shell theme tokens' {
    It 'renders valid shell theme.json for every theme' {
        InModuleScope Winarchy {
            $target = Get-WinarchyRenderTargets | Where-Object { $_.Template -eq 'shell-theme.json.tpl' }
            $target | Should -Not -BeNullOrEmpty
            foreach ($name in (Get-WinarchyThemes)) {
                $dir = Join-Path (Get-WinarchyThemesDir) $name
                $theme = Expand-WinarchyThemePalette -Theme (Import-WinarchyToml -Path (Join-Path $dir 'theme.toml'))
                $ctx = Build-WinarchyThemeContext -Theme $theme -ThemeDir $dir
                $json = Invoke-WinarchyTemplate -TemplatePath (Join-Path (Get-WinarchyRoot) 'templates\shell-theme.json.tpl') -Context $ctx | ConvertFrom-Json
                $json.v | Should -Be 1
                $json.background | Should -Match '^#[0-9a-fA-F]{6}$'
                $json.accent | Should -Match '^#[0-9a-fA-F]{6}$'
                $json.position | Should -BeIn 'top', 'bottom'
            }
        }
    }
}

Describe 'native shell guardrails' {
    It 'never registers global hotkeys or keyboard hooks (AHK owns them)' {
        $src = Get-ChildItem (Join-Path (Split-Path $PSScriptRoot -Parent) 'shell\src') -Filter *.rs -Recurse
        $src | Should -Not -BeNullOrEmpty
        foreach ($f in $src) {
            (Get-Content $f.FullName -Raw) | Should -Not -Match 'RegisterHotKey|WH_KEYBOARD|SHAppBarMessage'
        }
    }

    It 'keeps the shell exe out of tiling, window close and game detection' {
        $root = Split-Path $PSScriptRoot -Parent
        (Get-Content (Join-Path $root 'templates\komorebi.json.tpl') -Raw) | Should -Match 'winarchy-shell\.exe'
        (Get-Content (Join-Path $root 'config\ahk\winarchy.ahk') -Raw) | Should -Match '\(yasb\|winarchy-shell\)'
    }

    It 'bounds AHK shell pipe connection attempts and opens the pipe write-only' {
        $root = Split-Path $PSScriptRoot -Parent
        $ahk = Get-Content (Join-Path $root 'config\ahk\winarchy.ahk') -Raw
        $island = [regex]::Match($ahk, '(?ms)^ShellIsland\(.*?^\}').Value
        $island | Should -Match "(?s)if !DllCall\('WaitNamedPipeW', 'str', pipeName, 'uint', 250, 'int'\)\s+return false\s+pipe := DllCall\('CreateFileW', 'str', pipeName, 'uint', 0x40000000, 'uint', 0, 'ptr', 0, 'uint', 3, 'uint', 0, 'ptr', 0, 'ptr'\)\s+if pipe = -1\s+return false"
        $island | Should -Not -Match 'FileOpen\('
    }

    It 'writes an AHK shell request as newline-delimited UTF-8 and always closes its handle' {
        $root = Split-Path $PSScriptRoot -Parent
        $ahk = Get-Content (Join-Path $root 'config\ahk\winarchy.ahk') -Raw
        $island = [regex]::Match($ahk, '(?ms)^ShellIsland\(.*?^\}').Value
        $island | Should -Match ([regex]::Escape('payload := ''{"v":1,"id":1,"cmd":"'' cmd ''"'' extra ''}'' Chr(10)'))
        $island | Should -Match "(?s)try \{\s+payload := [^\r\n]+\s+byteCount := StrPut\(payload, 'UTF-8'\) - 1\s+data := Buffer\(byteCount \+ 1\)\s+StrPut\(payload, data, 'UTF-8'\)\s+written := 0\s+return DllCall\('WriteFile', 'ptr', pipe, 'ptr', data, 'uint', byteCount, 'uint\*', &written, 'ptr', 0, 'int'\) && written = byteCount\s+\} finally \{\s+DllCall\('CloseHandle', 'ptr', pipe\)\s+\}"
    }

    It 'recovers a hung native shell with a bounded UI probe and restores taskbars before relaunch' {
        $root = Split-Path $PSScriptRoot -Parent
        $ahk = Get-Content (Join-Path $root 'config\ahk\winarchy.ahk') -Raw
        $watchdog = [regex]::Match($ahk, '(?s); --- Native shell watchdog.*?(?=; --- Monitor watchdog)').Value
        $watchdog | Should -Not -BeNullOrEmpty
        $watchdog | Should -Not -Match "if !native \|\| ProcessExist\('winarchy-shell\.exe'\)"
        $watchdog | Should -Match 'SendMessageTimeoutW'
        $watchdog | Should -Match "(?s)SendMessageTimeoutW[^\r\n]*'UInt',\s*(?:0x0+|0)\s*,"
        $watchdog | Should -Match "(?s)SendMessageTimeoutW.*?'UInt',\s*[1-9][0-9]{0,3}\s*,\s*'(?:U?Ptr)\*'"
        $watchdog | Should -Match 'ShellResponsive\(pid\)'
        $watchdog | Should -Match '\? 12 : 2\s+if \(\+\+ShellMisses < limit\)'
        $watchdog | Should -Match "(?s)(?:ProcessClose\(|taskkill[^\r\n]*/F).*?ShowTaskbars\(\).*?Run\("
    }

    It 'points the YASB watchdog at state\bar so it does not relaunch YASB under the native bar' {
        $root = Split-Path $PSScriptRoot -Parent
        (Get-Content (Join-Path $root 'config\ahk\winarchy.ahk') -Raw) | Should -Match ([regex]::Escape('BarFile := StateDir "\bar"'))
    }
}

Describe 'Bar dock' {
    BeforeAll {
        $script:DockBackup = InModuleScope Winarchy {
            $path = Get-WinarchyBarDockPath
            @{ Exists = Test-Path $path; Content = if (Test-Path $path) { ,([System.IO.File]::ReadAllBytes($path)) } else { $null } }
        }
    }

    AfterAll {
        InModuleScope Winarchy -Parameters @{ Backup = $script:DockBackup } {
            param($Backup)
            if ($Backup.Exists) { [System.IO.File]::WriteAllBytes((Get-WinarchyBarDockPath), [byte[]]$Backup.Content) } else { Remove-Item (Get-WinarchyBarDockPath) -Force -ErrorAction SilentlyContinue }
        }
    }

    It 'reserves what dock.rs draws: bar + margin, plus 2 px of native strip only at the bottom' {
        $rs = Get-Content (Join-Path (Split-Path $PSScriptRoot -Parent) 'shell\src\dock.rs') -Raw
        $bar = [int][regex]::Match($rs, 'BAR_DIP: i32 = (\d+)').Groups[1].Value
        $margin = [int][regex]::Match($rs, 'const MARGIN_DIP: i32 = (\d+)').Groups[1].Value
        InModuleScope Winarchy -Parameters @{ Bar = $bar; Margin = $margin } {
            param($Bar, $Margin)
            Mock Get-WinarchyDockMode { 'reserve' }
            Mock Get-ItemProperty { [pscustomobject]@{ AppliedDPI = 96 } }
            Mock Get-WinarchyBarPosition { 'top' }
            (Get-WinarchyDockWorkAreaOffset).Bottom | Should -Be ($Bar + $Margin + 2)
            Mock Get-WinarchyBarPosition { 'bottom' }
            (Get-WinarchyDockWorkAreaOffset).Top | Should -Be ($Bar + $Margin)
        }
    }

    It 'scales the reserve with the system DPI' {
        InModuleScope Winarchy {
            Mock Get-WinarchyDockMode { 'reserve' }
            Mock Get-ItemProperty { [pscustomobject]@{ AppliedDPI = 144 } }
            Mock Get-WinarchyBarPosition { 'top' }
            (Get-WinarchyDockWorkAreaOffset).Bottom | Should -Be 84
        }
    }

    It 'has no dock under YASB even with the switch on' {
        InModuleScope Winarchy {
            Set-Content (Get-WinarchyBarDockPath) -Value '' -NoNewline
            Mock Get-WinarchyActiveBar { 'yasb' }
            Test-WinarchyBarDock | Should -BeFalse
            Mock Get-WinarchyActiveBar { 'native' }
            Test-WinarchyBarDock | Should -BeTrue
        }
    }

    It 'float mode reserves nothing; the mode is read from the switch file' {
        InModuleScope Winarchy {
            Mock Test-WinarchyTaskbarHidden { $false }
            Mock Get-WinarchyActiveBar { 'native' }
            Mock Get-ItemProperty { [pscustomobject]@{ AppliedDPI = 96 } }
            Mock Get-WinarchyBarPosition { 'top' }
            Set-Content (Get-WinarchyBarDockPath) -Value 'float' -NoNewline
            Get-WinarchyDockMode | Should -Be 'float'
            $o = Get-WinarchyDockWorkAreaOffset
            $o.Top | Should -Be 0
            $o.Bottom | Should -Be 2
            Set-Content (Get-WinarchyBarDockPath) -Value 'hide' -NoNewline
            Get-WinarchyDockMode | Should -Be 'hide'
            (Get-WinarchyDockWorkAreaOffset).Bottom | Should -Be 2
            Set-Content (Get-WinarchyBarDockPath) -Value 'auto' -NoNewline
            Get-WinarchyDockMode | Should -Be 'auto'
            Set-Content (Get-WinarchyBarDockPath) -Value 'on' -NoNewline
            Get-WinarchyDockMode | Should -Be 'reserve'
            Remove-Item (Get-WinarchyBarDockPath) -Force
            Get-WinarchyDockMode | Should -Be 'off'
        }
    }

    It 'the cluster monitor defaults to primary and reads primary, off or a position from its file' {
        InModuleScope Winarchy {
            $path = Get-WinarchyClusterMonitorPath
            $saved = if (Test-Path $path) { [System.IO.File]::ReadAllBytes($path) }
            try {
                Remove-Item $path -Force -ErrorAction SilentlyContinue
                Get-WinarchyClusterMonitor | Should -Be 'primary'
                foreach ($v in 'off', '2', 'primary') { Set-Content $path -Value $v -NoNewline; Get-WinarchyClusterMonitor | Should -Be $v }
                Set-Content $path -Value 'Off' -NoNewline
                Get-WinarchyClusterMonitor | Should -Be 'off'
                Set-Content $path -Value 'banana' -NoNewline
                Get-WinarchyClusterMonitor | Should -Be 'primary'
            }
            finally {
                if ($saved) { [System.IO.File]::WriteAllBytes($path, $saved) } else { Remove-Item $path -Force -ErrorAction SilentlyContinue }
            }
        }
    }

    It '<Mode> mode reserves <Bottom> bottom pixels when taskbar-hidden is <Hidden>' -ForEach @(
        @{ Mode = 'hide'; Hidden = $true; Bottom = 0 }
        @{ Mode = 'float'; Hidden = $true; Bottom = 0 }
        @{ Mode = 'hide'; Hidden = $false; Bottom = 2 }
        @{ Mode = 'float'; Hidden = $false; Bottom = 2 }
        @{ Mode = 'auto'; Hidden = $true; Bottom = 0 }
        @{ Mode = 'auto'; Hidden = $false; Bottom = 2 }
    ) {
        InModuleScope Winarchy -Parameters @{ Root = $TestDrive; Mode = $Mode; Hidden = $Hidden; Bottom = $Bottom } {
            param($Root, $Mode, $Hidden, $Bottom)
            Mock Resolve-Path { [pscustomobject]@{ Path = $Root } } -ParameterFilter { $Path -like '*\..\..\..' }
            $state = Join-Path $Root 'state'
            New-Item -ItemType Directory -Path $state -Force | Out-Null
            Set-Content (Join-Path $state 'bar') -Value 'native' -NoNewline
            Set-Content (Join-Path $state 'bar-dock') -Value $Mode -NoNewline
            $flag = Join-Path $state 'taskbar-hidden'
            if ($Hidden) { Set-Content $flag -Value '' -NoNewline }
            elseif (Test-Path $flag) { Remove-Item $flag -Force }

            $offset = Get-WinarchyDockWorkAreaOffset

            $offset.Top | Should -Be 0
            $offset.Bottom | Should -Be $Bottom
        }
    }

    It 'hiding the native taskbar sets auto-hide, flags the shell and is undone by autohide' {
        InModuleScope Winarchy {
            Mock Set-WinarchyTaskbarAutoHide { $true }
            Mock Restore-WinarchyNativeTaskbar {}
            Mock Test-WinarchyBarDock { $false }
            $flag = Get-WinarchyTaskbarHiddenPath
            $had = Test-Path $flag
            try {
                Set-WinarchyTaskbarHidden -Mode hide
                Test-WinarchyTaskbarHidden | Should -BeTrue
                Should -Invoke Set-WinarchyTaskbarAutoHide -ParameterFilter { $Enabled } -Times 1
                Set-WinarchyTaskbarHidden -Mode autohide
                Test-WinarchyTaskbarHidden | Should -BeFalse
                Set-WinarchyTaskbarHidden -Mode show
                Should -Invoke Set-WinarchyTaskbarAutoHide -ParameterFilter { -not $Enabled } -Times 1
            }
            finally { if ($had) { Set-Content $flag -Value 'on' -NoNewline } else { Remove-Item $flag -Force -ErrorAction SilentlyContinue } }
        }
    }

    It 'renders the offset and the shell flag from the tokens' {
        InModuleScope Winarchy {
            $root = Get-WinarchyRoot
            $tpl = Join-Path $root 'templates\komorebi.json.tpl'
            $ctx = @{}
            foreach ($m in [regex]::Matches((Get-Content $tpl -Raw), '\{\{([^}]+)\}\}')) { $ctx[$m.Groups[1].Value] = 'x' }
            $ctx['computed.work_top'] = 0
            $ctx['computed.work_bottom'] = 56
            $komorebi = Invoke-WinarchyTemplate -TemplatePath $tpl -Context $ctx
            $komorebi | Should -Match '"bottom": 56 \}'
            $shellTpl = Join-Path $root 'templates\shell-theme.json.tpl'
            $shellCtx = @{}
            foreach ($m in [regex]::Matches((Get-Content $shellTpl -Raw), '\{\{([^}]+)\}\}')) { $shellCtx[$m.Groups[1].Value] = '1' }
            $shellCtx['computed.dock'] = 'true'
            $shellCtx['computed.dock_float'] = 'true'
            $shellCtx['computed.hide_native'] = 'false'
            $shell = Invoke-WinarchyTemplate -TemplatePath $shellTpl -Context $shellCtx
            ($shell | ConvertFrom-Json).dock | Should -BeTrue
            ($shell | ConvertFrom-Json).dock_float | Should -BeTrue
            ($shell | ConvertFrom-Json).hide_native_taskbar | Should -BeFalse
        }
    }

    It 'bar dock on|off flips the switch and re-renders the theme' {
        InModuleScope Winarchy {
            Mock Get-WinarchyCurrentTheme { 'tokyo-night' }
            Mock Set-WinarchyTheme {}
            Mock Get-WinarchyActiveBar { 'native' }
            Set-WinarchyBarDock -Mode on
            Test-Path (Get-WinarchyBarDockPath) | Should -BeTrue
            Set-WinarchyBarDock -Mode float
            (Get-Content (Get-WinarchyBarDockPath) -Raw) | Should -Be 'float'
            Set-WinarchyBarDock -Mode off
            Test-Path (Get-WinarchyBarDockPath) | Should -BeFalse
            Should -Invoke Set-WinarchyTheme -Times 3
        }
    }

    It 'rejects YASB while the current session uses the Winarchy shell with ProcessAlive=<ProcessAlive>' -ForEach @(
        @{ ProcessAlive = $true }
        @{ ProcessAlive = $false }
    ) {
        InModuleScope Winarchy -Parameters @{ Root = (Join-Path $TestDrive "session-shell-$ProcessAlive"); ProcessAlive = $ProcessAlive } {
            param($Root, $ProcessAlive)
            Mock Resolve-Path { [pscustomobject]@{ Path = $Root } } -ParameterFilter { $Path -like '*\..\..\..' }
            $state = Get-WinarchyStateDir
            Set-Content (Join-Path $state 'bar') -Value 'native' -NoNewline
            $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
            Mock Get-WinarchyCurrentLogonIdentity { '00000000000000ab-01dc000000000001' }
            Set-Content (Join-Path $state "session-shell-$sessionId") -Value '00000000000000ab-01dc000000000001' -NoNewline
            Set-Content (Join-Path $state "shell-session-$sessionId") -Value '4242|1|00000000000000ab-01dc000000000001' -NoNewline
            Mock Get-Process {
                if ($ProcessAlive) { [pscustomobject]@{ ProcessName = 'winarchy-shell'; SessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId } }
            } -ParameterFilter { $Id -eq 4242 }
            Mock Get-Command { throw 'Unexpected YASB lookup after session shell guard' } -ParameterFilter { $Name -eq 'yasbc' }

            { Set-WinarchyBar -Name yasb } | Should -Throw 'YASB cannot replace the Winarchy session shell. Exit the custom shell session before changing bars.'
            (Get-Content (Join-Path $state 'bar') -Raw) | Should -Be 'native'
            if (-not $ProcessAlive) {
                Get-WinarchyShellArguments | Should -Be "--root `"$Root`""
            }
        }
    }

    It 'switching bars re-renders komorebi only while the dock switch is on with <SessionState>' -ForEach @(
        @{ SessionState = 'no-marker' }
        @{ SessionState = 'stale-marker' }
        @{ SessionState = 'missing-process' }
        @{ SessionState = 'wrong-session' }
    ) {
        InModuleScope Winarchy -Parameters @{ Root = (Join-Path $TestDrive "bar-switch-$SessionState"); SessionState = $SessionState } {
            param($Root, $SessionState)
            Mock Resolve-Path { [pscustomobject]@{ Path = $Root } } -ParameterFilter { $Path -like '*\..\..\..' }
            Mock Install-WinarchyYasb {}
            Mock Test-WinarchyTask { $false }
            Mock Invoke-WinarchyReload {}
            Mock Update-WinarchyKomorebiRules {}
            Mock Get-Command { [pscustomobject]@{ Name = 'yasbc' } } -ParameterFilter { $Name -eq 'yasbc' }
            $state = Get-WinarchyStateDir
            $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
            if ($SessionState -ne 'no-marker') {
                Set-Content (Join-Path $state "session-shell-$sessionId") -Value '00000000000000ab-01dc000000000000' -NoNewline
            }
            if ($SessionState -in 'missing-process', 'wrong-session') {
                Set-Content (Join-Path $state "shell-session-$sessionId") -Value '4242|1|00000000000000ab-01dc000000000001' -NoNewline
                Mock Get-Process { $null } -ParameterFilter { $Id -eq 4242 }
                if ($SessionState -eq 'wrong-session') {
                    Mock Get-Process { [pscustomobject]@{ ProcessName = 'winarchy-shell'; SessionId = ([Diagnostics.Process]::GetCurrentProcess().SessionId + 1) } } -ParameterFilter { $Id -eq 4242 }
                }
            }
            Remove-Item (Get-WinarchyBarDockPath) -Force -ErrorAction SilentlyContinue
            Set-WinarchyBar -Name yasb
            (Get-Content (Join-Path (Get-WinarchyStateDir) 'bar') -Raw) | Should -Be 'yasb'
            Should -Invoke Update-WinarchyKomorebiRules -Times 0
            Set-Content (Get-WinarchyBarDockPath) -Value '' -NoNewline
            Set-WinarchyBar -Name yasb
            Should -Invoke Update-WinarchyKomorebiRules -Times 1
        }
    }
}

Describe 'Install-WinarchyShellBinary' {
    BeforeEach {
        $script:repo = Join-Path $TestDrive 'repo'
        New-Item -ItemType Directory -Path $script:repo -Force | Out-Null
        $script:exe = Join-Path $TestDrive 'winarchy-shell.exe'
        Set-Content $script:exe 'fake binary' -NoNewline
    }

    It 'refuses to install without a pinned hash or with a mismatching one' {
        InModuleScope Winarchy -Parameters @{ Repo = $script:repo; Exe = $script:exe } {
            param($Repo, $Exe)
            Mock Get-WinarchyRoot { $Repo }
            Mock Assert-WinarchyCustomShellPolicySafeForMaintenance {}
            Set-Content (Join-Path $Repo 'versions.lock.toml') "[core]`nkomorebi = `"1`""
            { Install-WinarchyShellBinary -Source $Exe } | Should -Throw '*no [[]shell[]] sha256*'
            Set-Content (Join-Path $Repo 'versions.lock.toml') "[shell]`nsha256 = `"$('0' * 64)`""
            { Install-WinarchyShellBinary -Source $Exe } | Should -Throw '*mismatch*'
            Test-Path (Join-Path $Repo 'state\bin\winarchy-shell.exe') | Should -BeFalse
        }
    }

    It 'installs it into state\bin when the SHA256 matches' {
        InModuleScope Winarchy -Parameters @{ Repo = $script:repo; Exe = $script:exe } {
            param($Repo, $Exe)
            Mock Get-WinarchyRoot { $Repo }
            Mock Assert-WinarchyCustomShellPolicySafeForMaintenance {}
            $hash = (Get-FileHash $Exe -Algorithm SHA256).Hash.ToLowerInvariant()
            Set-Content (Join-Path $Repo 'versions.lock.toml') "[shell]`nsha256 = `"$hash`""
            $dest = Install-WinarchyShellBinary -Source $Exe
            Test-Path $dest | Should -BeTrue
            Get-WinarchyShellExe | Should -Be $dest
        }
    }
}

Describe 'Install-WinarchyAssistantBinary' {
    It 'installs only a binary whose SHA256 matches the pin' {
        $repo = Join-Path $TestDrive 'repo'
        New-Item -ItemType Directory -Path $repo -Force | Out-Null
        $exe = Join-Path $TestDrive 'winarchy-assistant.exe'
        Set-Content $exe 'fake binary' -NoNewline
        InModuleScope Winarchy -Parameters @{ Repo = $repo; Exe = $exe } {
            param($Repo, $Exe)
            Mock Get-WinarchyRoot { $Repo }
            $dest = Join-Path $Repo 'state\bin\winarchy-assistant.exe'
            Set-Content (Join-Path $Repo 'versions.lock.toml') "[core]`nkomorebi = `"1`""
            { Install-WinarchyAssistantBinary -Source $Exe } | Should -Throw '*no [[]assistant[]] sha256*'
            Set-Content (Join-Path $Repo 'versions.lock.toml') "[assistant]`nsha256 = `"$('0' * 64)`""
            { Install-WinarchyAssistantBinary -Source $Exe } | Should -Throw '*mismatch*'
            Test-Path $dest | Should -BeFalse
            $hash = (Get-FileHash $Exe -Algorithm SHA256).Hash.ToLowerInvariant()
            Set-Content (Join-Path $Repo 'versions.lock.toml') "[assistant]`nsha256 = `"$hash`""
            Install-WinarchyAssistantBinary -Source $Exe
            Test-Path $dest | Should -BeTrue
        }
    }
}

Describe 'Install-WinarchyFilesBinary' {
    It 'installs only a binary whose SHA256 matches the pin and keeps the previous one' {
        $repo = Join-Path $TestDrive 'repo'
        New-Item -ItemType Directory -Path $repo -Force | Out-Null
        $exe = Join-Path $TestDrive 'winarchy-files.exe'
        Set-Content $exe 'fake binary' -NoNewline
        InModuleScope Winarchy -Parameters @{ Repo = $repo; Exe = $exe } {
            param($Repo, $Exe)
            Mock Get-WinarchyRoot { $Repo }
            $dest = Join-Path $Repo 'state\bin\winarchy-files.exe'
            Set-Content (Join-Path $Repo 'versions.lock.toml') "[core]`nkomorebi = `"1`""
            { Install-WinarchyFilesBinary -Source $Exe } | Should -Throw '*no [[]files[]] sha256*'
            Set-Content (Join-Path $Repo 'versions.lock.toml') "[files]`nsha256 = `"$('0' * 64)`""
            { Install-WinarchyFilesBinary -Source $Exe } | Should -Throw '*mismatch*'
            Test-Path $dest | Should -BeFalse
            $hash = (Get-FileHash $Exe -Algorithm SHA256).Hash.ToLowerInvariant()
            Set-Content (Join-Path $Repo 'versions.lock.toml') "[files]`nsha256 = `"$hash`""
            Install-WinarchyFilesBinary -Source $Exe
            Test-Path $dest | Should -BeTrue
            Test-Path "$dest.prev" | Should -BeFalse
            Install-WinarchyFilesBinary -Source $Exe
            Test-Path "$dest.prev" | Should -BeTrue
        }
    }
}

Describe 'Restore-WinarchyFilesPrevious' {
    It 'swaps the current binary with the previous one and refuses without a previous one' {
        $repo = Join-Path $TestDrive 'repo-rollback'
        $bin = Join-Path (Join-Path $repo 'state') 'bin'
        New-Item -ItemType Directory -Path $bin -Force | Out-Null
        InModuleScope Winarchy -Parameters @{ Bin = $bin } {
            param($Bin)
            Mock Get-WinarchyStateDir { Split-Path (Split-Path $Bin -Parent) -Parent | Join-Path -ChildPath 'state' }
            Mock Get-Process { $null }
            { Restore-WinarchyFilesPrevious } | Should -Throw '*No previous*'
            Set-Content (Join-Path $Bin 'winarchy-files.exe') 'new' -NoNewline
            Set-Content (Join-Path $Bin 'winarchy-files.exe.prev') 'old' -NoNewline
            Restore-WinarchyFilesPrevious
            Get-Content (Join-Path $Bin 'winarchy-files.exe') -Raw | Should -Be 'old'
            Get-Content (Join-Path $Bin 'winarchy-files.exe.prev') -Raw | Should -Be 'new'
        }
    }
}

Describe 'Start-WinarchyFiles' {
    It 'tries the release when no binary exists and explains how to build when that fails' {
        $repo = Join-Path $TestDrive 'repo-files'
        New-Item -ItemType Directory -Path $repo -Force | Out-Null
        InModuleScope Winarchy -Parameters @{ Repo = $repo } {
            param($Repo)
            Mock Get-WinarchyRoot { $Repo }
            Mock Install-WinarchyFilesRelease { throw 'offline' }
            { Start-WinarchyFiles } | Should -Throw '*release download failed (offline)*cargo build*'
            Should -Invoke Install-WinarchyFilesRelease -Times 1 -Exactly
        }
    }
}

Describe 'Invoke-WinarchyAssistant' {
    It 'rejects unknown subcommands and reports status without a binary' {
        InModuleScope Winarchy {
            Mock Get-WinarchyAssistantExe { $null }
            { Invoke-WinarchyAssistant -Sub 'dance' } | Should -Throw '*Unknown subcommand*'
            $s = Get-WinarchyAssistantStatus
            $s.Binary | Should -BeFalse
            $s.Login | Should -Be 'n/a'
        }
    }

    It 'only enables push-to-talk while the assistant runs and game mode is off' {
        $ahk = Get-Content (Join-Path (Split-Path $PSScriptRoot -Parent) 'config\ahk\winarchy.ahk') -Raw
        $ready = [regex]::Match($ahk, '(?ms)^AssistantReady\(\) \{.*?^\}').Value
        $ready | Should -Match 'return !FileExist\(GameFlag\) && GetAssistantPid\(\)'
        $pidLookup = [regex]::Match($ahk, '(?ms)^GetAssistantPid\(\) \{.*?^\}').Value
        $pidLookup | Should -Match 'candidateSession != sessionId'
        $pidLookup | Should -Match "StrLower\(ProcessGetName\(pid\)\) = 'winarchy-assistant\.exe'"
        $ahk | Should -Match '(?s)#HotIf AssistantReady\(\)\s*\r?\n#CapsLock::AssistantPtt\(\)'
    }
}

Describe 'Monitor health' {
    BeforeAll {
        $script:HealthyJson = '[{"id":2296521,"name":"DISPLAY1","device":"SAC2763","device_id":"SAC2763-5&51cf543&1&UID4355","serial_number_id":"0000000000000","size":{"left":0,"top":0,"right":2560,"bottom":1440}},{"id":131233,"name":"DISPLAY2","device":"MSI3BA4","device_id":"MSI3BA4-5&51cf543&1&UID4353","serial_number_id":"808470350","size":{"left":-1920,"top":0,"right":1920,"bottom":1080}}]'
        $script:CorruptJson = '[{"id":2296521,"name":"DISPLAY1","device":"SAC2763","device_id":"SAC2763-5&51cf543&1&UID4355","serial_number_id":null,"size":{"left":0,"top":0,"right":2560,"bottom":1440}},{"id":65697,"name":"DISPLAY1","device":"UNKNOWN","device_id":"UNKNOWN","serial_number_id":null,"size":{"left":0,"top":0,"right":2560,"bottom":1440}}]'
    }

    Context 'Test-WinarchyKomorebiMonitors' {
        It 'accepts one monitor per screen with distinct origins' {
            InModuleScope Winarchy -Parameters @{ Json = $script:HealthyJson } {
                param($Json)
                Test-WinarchyKomorebiMonitors -Monitors @($Json | ConvertFrom-Json) -ScreenCount 2 | Should -BeTrue
            }
        }
        It 'rejects two monitors sharing an origin' {
            InModuleScope Winarchy -Parameters @{ Json = $script:CorruptJson } {
                param($Json)
                Test-WinarchyKomorebiMonitors -Monitors @($Json | ConvertFrom-Json) -ScreenCount 2 | Should -BeFalse
            }
        }
        It 'rejects a monitor count that differs from the screen count' {
            InModuleScope Winarchy -Parameters @{ Json = $script:HealthyJson } {
                param($Json)
                Test-WinarchyKomorebiMonitors -Monitors @($Json | ConvertFrom-Json) -ScreenCount 3 | Should -BeFalse
            }
        }
        It 'accepts a single monitor on a single screen' {
            InModuleScope Winarchy -Parameters @{ Json = $script:HealthyJson } {
                param($Json)
                Test-WinarchyKomorebiMonitors -Monitors @(($Json | ConvertFrom-Json)[0]) -ScreenCount 1 | Should -BeTrue
            }
        }
    }

    Context 'Test-WinarchyMonitorHealth' {
        It 'reports healthy when the monitor query throws' {
            InModuleScope Winarchy {
                Mock Get-WinarchyKomorebiMonitors { throw 'komorebi is not running' }
                Mock Get-WinarchyScreenCount { 2 }
                Test-WinarchyMonitorHealth | Should -BeTrue
            }
        }
    }

    Context 'Repair-WinarchyKomorebiMonitors' {
        BeforeEach {
            InModuleScope Winarchy -Parameters @{ Healthy = $script:HealthyJson; Corrupt = $script:CorruptJson } {
                param($Healthy, $Corrupt)
                $script:stopped = $false
                $script:calls = 0
                $script:corrupt = @($Corrupt | ConvertFrom-Json)
                $script:healthy = @($Healthy | ConvertFrom-Json)
                $state = Join-Path $TestDrive ('state-' + [guid]::NewGuid())
                New-Item -ItemType Directory -Path $state | Out-Null
                $script:stateDir = $state
                Mock Get-WinarchyStateDir { $script:stateDir }
                Mock Get-WinarchyGameModeFlag { Join-Path $script:stateDir 'game-mode' }
                Mock Get-WinarchyScreenCount { 2 }
                $script:layout = '-1920,0,1920,1080;0,0,2560,1440'
                Mock Get-WinarchyScreenSignature { $script:layout }
                Mock Start-Sleep { }
                Mock Write-WinarchyOk { }
                Mock Write-WinarchyWarn { }
                Mock Test-WinarchyProcess { -not $script:stopped }
                Mock Start-WinarchyKomorebi { $script:stopped = $false }
                Mock Invoke-WinarchyKomorebic {
                    if ($Arguments[0] -eq 'stop') { $script:stopped = $true }
                    if ($Arguments[0] -eq 'state') { '{}' }
                }
                Mock Get-WinarchyKomorebiMonitors { $script:corrupt }
            }
        }

        It 'does nothing when the monitor map is healthy' {
            InModuleScope Winarchy {
                Mock Get-WinarchyKomorebiMonitors { $script:healthy }
                Repair-WinarchyKomorebiMonitors | Should -BeFalse
                Should -Invoke Invoke-WinarchyKomorebic -Times 0 -ParameterFilter { $Arguments[0] -eq 'stop' }
                Should -Invoke Start-WinarchyKomorebi -Times 0
            }
        }

        It 'does nothing when the map is only transiently unhealthy' {
            InModuleScope Winarchy {
                Mock Get-WinarchyKomorebiMonitors { $script:calls++; if ($script:calls -eq 1) { $script:corrupt } else { $script:healthy } }
                Repair-WinarchyKomorebiMonitors | Should -BeFalse
                Should -Invoke Invoke-WinarchyKomorebic -Times 0 -ParameterFilter { $Arguments[0] -eq 'stop' }
            }
        }

        It 'stops and starts komorebi when the map stays unhealthy' {
            InModuleScope Winarchy {
                Repair-WinarchyKomorebiMonitors | Should -BeTrue
                Should -Invoke Invoke-WinarchyKomorebic -Times 1 -ParameterFilter { $Arguments[0] -eq 'stop' }
                Should -Invoke Start-WinarchyKomorebi -Times 1 -ParameterFilter { $Resilient }
            }
        }

        It 'does not restart while game mode is on' {
            InModuleScope Winarchy {
                Set-Content -Path (Get-WinarchyGameModeFlag) -Value '1'
                Repair-WinarchyKomorebiMonitors | Should -BeFalse
                Should -Invoke Invoke-WinarchyKomorebic -Times 0 -ParameterFilter { $Arguments[0] -eq 'stop' }
            }
        }

        It 'throttles a second restart within five minutes only when asked' {
            InModuleScope Winarchy {
                Repair-WinarchyKomorebiMonitors -Throttled | Should -BeTrue
                Repair-WinarchyKomorebiMonitors -Throttled | Should -BeFalse
                Should -Invoke Start-WinarchyKomorebi -Times 1
                Repair-WinarchyKomorebiMonitors | Should -BeTrue
                Should -Invoke Start-WinarchyKomorebi -Times 2
            }
        }

        It 'does not throttle a restart when the screen layout changed since the last one' {
            InModuleScope Winarchy {
                Repair-WinarchyKomorebiMonitors -Throttled | Should -BeTrue
                $script:layout = '0,0,2560,1440'
                Repair-WinarchyKomorebiMonitors -Throttled | Should -BeTrue
                Should -Invoke Start-WinarchyKomorebi -Times 2
            }
        }

        It 'throws without starting komorebi when it does not exit after stop' {
            InModuleScope Winarchy {
                Mock Test-WinarchyProcess { $true }
                $script:now = [datetime]'2026-01-01'
                Mock Get-Date { $script:now = $script:now.AddSeconds(4); $script:now }
                { Repair-WinarchyKomorebiMonitors } | Should -Throw '*did not stop*'
                Should -Invoke Start-WinarchyKomorebi -Times 0
            }
        }
    }

    Context 'Invoke-WinarchyReload' {
        BeforeAll {
            $script:envBefore = @{ K = $env:KOMOREBI_CONFIG_HOME; Y = $env:YASB_CONFIG_HOME }
        }
        AfterAll {
            $env:KOMOREBI_CONFIG_HOME = $script:envBefore.K
            $env:YASB_CONFIG_HOME = $script:envBefore.Y
        }
        BeforeEach {
            InModuleScope Winarchy {
                Mock Get-WinarchyRoot { $TestDrive }
                Mock Test-WinarchyProcess { $true }
                Mock Get-WinarchyActiveBar { 'native' }
                Mock Get-WinarchyShellExe { $null }
                Mock Start-Process { }
                Mock Stop-Process { }
                Mock Stop-WinarchyShell { $true }
                Mock Start-WinarchyWindowSlots { }
                Mock Get-WinarchyAhkExe { $null }
                Mock Invoke-WinarchyKomorebiStaticReload { }
            }
        }

        It 'skips the configuration reload when komorebi was just restarted' {
            InModuleScope Winarchy {
                Mock Repair-WinarchyKomorebiMonitors { $true }
                Invoke-WinarchyReload
                Should -Invoke Invoke-WinarchyKomorebiStaticReload -Times 0
            }
        }

        It 'reloads the generated configuration when komorebi was not restarted' {
            InModuleScope Winarchy {
                Mock Repair-WinarchyKomorebiMonitors { $false }
                Invoke-WinarchyReload
                Should -Invoke Invoke-WinarchyKomorebiStaticReload -Times 1 -ParameterFilter { $ConfigPath -eq (Join-Path $TestDrive 'config\komorebi\komorebi.json') }
            }
        }

        It 'never repairs monitors on a light reload' {
            InModuleScope Winarchy {
                Mock Repair-WinarchyKomorebiMonitors { $true }
                Invoke-WinarchyReload -Light
                Should -Invoke Repair-WinarchyKomorebiMonitors -Times 0
                Should -Invoke Invoke-WinarchyKomorebiStaticReload -Times 1
            }
        }
    }
}

Describe 'Windows accent palette' {
    It 'builds the 8-color RGBA palette with the theme accent in the fourth slot' {
        InModuleScope Winarchy {
            $p = Get-WinarchyAccentPalette -Hex '#f38d70'
            $p.Length | Should -Be 32
            $p[12..15] | Should -Be @(0xf3, 0x8d, 0x70, 0x00)
            $p[28..31] | Should -Be @(0x88, 0x17, 0x98, 0x00)
            # claros crecen hacia blanco y oscuros hacia negro
            $p[1] | Should -BeGreaterThan $p[9]
            $p[9] | Should -BeGreaterThan 0x8d
            $p[17] | Should -BeLessThan 0x8d
        }
    }
}

Describe 'Flow theme copy' {
    BeforeAll {
        $script:OldAppData = $env:APPDATA
        $env:APPDATA = Join-Path $TestDrive 'appdata'
        New-Item -ItemType Directory -Path (Join-Path $env:APPDATA 'FlowLauncher') -Force | Out-Null
        $script:Xaml = Join-Path $TestDrive 'Winarchy.xaml'
        Set-Content $script:Xaml -Value '<a/>' -NoNewline
        $script:Target = Join-Path $env:APPDATA 'FlowLauncher\Themes\Winarchy.xaml'
    }
    AfterAll { $env:APPDATA = $script:OldAppData }

    It 'does not touch the target when the content is identical' {
        InModuleScope Winarchy -Parameters @{ Xaml = $script:Xaml; Target = $script:Target } {
            param($Xaml, $Target)
            Set-WinarchyFlowTheme -XamlPath $Xaml
            $stamp = (Get-Item $Target).LastWriteTimeUtc
            Start-Sleep -Milliseconds 50
            $lock = [System.IO.File]::Open($Target, 'Open', 'Read', 'Read')
            try { Set-WinarchyFlowTheme -XamlPath $Xaml } finally { $lock.Dispose() }
            (Get-Item $Target).LastWriteTimeUtc | Should -Be $stamp
        }
    }

    It 'retries a short lock when the content changed' {
        InModuleScope Winarchy -Parameters @{ Xaml = $script:Xaml; Target = $script:Target } {
            param($Xaml, $Target)
            Set-Content $Xaml -Value '<b/>' -NoNewline
            $lock = [System.IO.File]::Open($Target, 'Open', 'Read', 'Read')
            if (-not ('WinarchyTestRelease' -as [type])) {
                Add-Type -TypeDefinition 'public static class WinarchyTestRelease { public static void After(System.IDisposable d, int ms) { new System.Threading.Thread(() => { System.Threading.Thread.Sleep(ms); d.Dispose(); }).Start(); } }'
            }
            [WinarchyTestRelease]::After($lock, 500)
            Set-WinarchyFlowTheme -XamlPath $Xaml
            Get-Content $Target -Raw | Should -Be '<b/>'
        }
    }
}

Describe 'Tray hook lookup' {
    It 'finds the YASB tray hook under Program Files and is null without it' {
        InModuleScope Winarchy -Parameters @{ Root = $TestDrive } {
            param($Root)
            $saved = @($env:ProgramW6432, $env:ProgramFiles)
            try {
                $env:ProgramW6432 = Join-Path $Root 'w6432'
                $env:ProgramFiles = Join-Path $Root 'pf'
                Get-WinarchyTrayHookPath | Should -BeNullOrEmpty
                $lib = Join-Path $env:ProgramFiles 'YASB\lib'
                New-Item -ItemType Directory -Path $lib -Force | Out-Null
                Set-Content (Join-Path $lib 'YASBTrayHook.dll') 'x'
                Get-WinarchyTrayHookPath | Should -Be (Join-Path $lib 'YASBTrayHook.dll')
            }
            finally { $env:ProgramW6432, $env:ProgramFiles = $saved }
        }
    }
}

Describe 'Floating apps already open' {
    It 'lists only the exes with a floating rule that komorebi holds in a container' {
        InModuleScope Winarchy {
            $config = '{"floating_applications":[{"kind":"Exe","id":"aLogin.exe","matching_strategy":"Equals"},{"kind":"Exe","id":"Free.exe","matching_strategy":"Equals"},{"kind":"Class","id":"X","matching_strategy":"Equals"}]}' | ConvertFrom-Json
            $state = '{"monitors":{"elements":[{"workspaces":{"elements":[{"containers":{"elements":[{"windows":{"elements":[{"exe":"alogin.exe"}]}},{"windows":{"elements":[{"exe":"code.exe"}]}}]},"floating_windows":{"elements":[{"exe":"Free.exe"}]}}]}}]}}' | ConvertFrom-Json
            @(Get-WinarchyTiledFloatingExes -State $state -Config $config) | Should -Be @('alogin.exe')
        }
    }
}

Describe 'Session process isolation' {
    It '<Subject> resolves only its recorded process with <Case>' -ForEach @(
        foreach ($subject in 'shell', 'assistant') {
            foreach ($case in 'valid', 'missing-file', 'malformed', 'missing-process', 'wrong-name', 'wrong-session') {
                @{ Subject = $subject; Case = $case }
            }
        }
    ) {
        InModuleScope Winarchy -Parameters @{ Root = (Join-Path $TestDrive "$Subject-$Case"); Subject = $Subject; Case = $Case } {
            param($Root, $Subject, $Case)
            Mock Resolve-Path { [pscustomobject]@{ Path = $Root } } -ParameterFilter { $Path -like '*\..\..\..' }
            $state = Get-WinarchyStateDir
            $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
            Mock Get-WinarchyCurrentLogonIdentity { '00000000000000ab-01dc000000000001' }
            $path = Join-Path $state "$Subject-session-$sessionId"
            if ($Case -ne 'missing-file') {
                $content = if ($Subject -eq 'shell') { '4242|0|00000000000000ab-01dc000000000001' } else { '4242' }
                if ($Case -eq 'malformed') { $content = 'not a PID' }
                Set-Content $path -Value $content -NoNewline
            }
            Mock Get-Process {} -ParameterFilter { $Name -in 'winarchy-shell', 'winarchy-assistant' }
            Mock Get-Process {
                if ($Case -ne 'missing-process') {
                    $name = if ($Subject -eq 'shell') { 'winarchy-shell' } else { 'winarchy-assistant' }
                    if ($Case -eq 'wrong-name') { $name = 'unrelated' }
                    $processSession = [Diagnostics.Process]::GetCurrentProcess().SessionId
                    if ($Case -eq 'wrong-session') { $processSession++ }
                    [pscustomobject]@{ Id = 4242; ProcessName = $name; SessionId = $processSession }
                }
            } -ParameterFilter { $Id -eq 4242 }

            $process = if ($Subject -eq 'shell') { Get-WinarchyShellProcess } else { Get-WinarchyAssistantProcess }

            if ($Case -eq 'valid') { $process.Id | Should -Be 4242 }
            else { $process | Should -BeNullOrEmpty }
        }
    }

    It '<Subject> migration lookup accepts only matching repo paths in its session with <Case>' -ForEach @(
        foreach ($subject in 'shell', 'assistant') {
            foreach ($case in 'installed', 'development', 'wrong-session', 'wrong-path', 'missing-path') {
                @{ Subject = $subject; Case = $case }
            }
        }
    ) {
        InModuleScope Winarchy -Parameters @{ Root = (Join-Path $TestDrive "migration-$Subject-$Case"); Subject = $Subject; Case = $Case } {
            param($Root, $Subject, $Case)
            Mock Resolve-Path { [pscustomobject]@{ Path = $Root } } -ParameterFilter { $Path -like '*\..\..\..' }
            $null = Get-WinarchyStateDir
            Mock Get-CimInstance {
                $binary = Join-Path $Root 'state\bin\winarchy-assistant.exe'
                if ($Case -eq 'development') { $binary = Join-Path $Root 'assistant\target\release\winarchy-assistant.exe' }
                [pscustomobject]@{ CommandLine = '"{0}" serve' -f $binary }
            } -ParameterFilter { $ClassName -eq 'Win32_Process' -and $Filter -eq 'ProcessId = 4343' }
            Mock Get-Process {
                $processSession = [Diagnostics.Process]::GetCurrentProcess().SessionId
                if ($Case -eq 'wrong-session') { $processSession++ }
                $binary = Join-Path $Root "state\bin\winarchy-$Subject.exe"
                if ($Case -eq 'development') { $binary = Join-Path $Root "$Subject\target\release\winarchy-$Subject.exe" }
                if ($Case -eq 'wrong-path') { $binary = Join-Path $Root "other\winarchy-$Subject.exe" }
                if ($Case -eq 'missing-path') { $binary = $null }
                [pscustomobject]@{ Id = 4343; ProcessName = "winarchy-$Subject"; SessionId = $processSession; Path = $binary }
            } -ParameterFilter { $Name -in 'winarchy-shell', 'winarchy-assistant' }

            $process = if ($Subject -eq 'shell') { Get-WinarchyShellProcess } else { Get-WinarchyAssistantProcess }

            if ($Case -in 'installed', 'development') { $process.Id | Should -Be 4343 }
            else { $process | Should -BeNullOrEmpty }
        }
    }

    It 'assistant migration requires an initial serve command with <Case>' -ForEach @(
        @{ Case = 'serve'; Command = 'serve'; Expected = $true }
        @{ Case = 'serve-with-options'; Command = 'serve --verbose'; Expected = $true }
        @{ Case = 'login'; Command = 'login'; Expected = $false }
        @{ Case = 'ask'; Command = 'ask serve'; Expected = $false }
        @{ Case = 'ask-with-quoted-serve-text'; Command = 'ask --prompt "{exe}" serve'; Expected = $false }
        @{ Case = 'serve-prefix'; Command = 'server'; Expected = $false }
        @{ Case = 'missing-command-line'; Command = $null; Expected = $false }
        @{ Case = 'unavailable-cim'; Command = 'serve'; Expected = $false }
    ) {
        InModuleScope Winarchy -Parameters @{ Root = (Join-Path $TestDrive "assistant-command-$Case"); Case = $Case; Command = $Command; Expected = $Expected } {
            param($Root, $Case, $Command, $Expected)
            Mock Resolve-Path { [pscustomobject]@{ Path = $Root } } -ParameterFilter { $Path -like '*\..\..\..' }
            $null = Get-WinarchyStateDir
            Mock Get-Process {
                [pscustomobject]@{ Id = 4343; ProcessName = 'winarchy-assistant'; SessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId; Path = (Join-Path $Root 'state\bin\winarchy-assistant.exe') }
            } -ParameterFilter { $Name -eq 'winarchy-assistant' }
            Mock Get-CimInstance {
                if ($Case -eq 'unavailable-cim') { throw 'CIM unavailable' }
                $binary = Join-Path $Root 'state\bin\winarchy-assistant.exe'
                $line = if ($null -ne $Command) { '"{0}" {1}' -f $binary, $Command.Replace('{exe}', $binary) } else { $null }
                [pscustomobject]@{ CommandLine = $line }
            } -ParameterFilter { $ClassName -eq 'Win32_Process' -and $Filter -eq 'ProcessId = 4343' }

            $process = Get-WinarchyAssistantProcess

            if ($Expected) { $process.Id | Should -Be 4343 }
            else { $process | Should -BeNullOrEmpty }
        }
    }

    It 'assistant startup handles a legacy candidate classified as <Case>' -ForEach @(
        @{ Case = 'unavailable-cim'; Command = $null; Reject = $true; Launch = $false }
        @{ Case = 'null-command'; Command = $null; Reject = $true; Launch = $false }
        @{ Case = 'empty-command'; Command = ''; Reject = $true; Launch = $false }
        @{ Case = 'whitespace-command'; Command = '   '; Reject = $true; Launch = $false }
        @{ Case = 'serve-quoted'; Command = '"{exe}" serve'; Reject = $false; Launch = $false }
        @{ Case = 'ask-quoted'; Command = '"{exe}" ask'; Reject = $false; Launch = $true }
        @{ Case = 'login-quoted'; Command = '"{exe}" login'; Reject = $false; Launch = $true }
        @{ Case = 'logout-quoted'; Command = '"{exe}" logout'; Reject = $false; Launch = $true }
        @{ Case = 'status-quoted'; Command = '"{exe}" status'; Reject = $false; Launch = $true }
        @{ Case = 'levels-quoted'; Command = '"{exe}" levels'; Reject = $false; Launch = $true }
        @{ Case = 'listen-quoted'; Command = '"{exe}" listen'; Reject = $false; Launch = $true }
        @{ Case = 'serve-unquoted'; Command = '{exe} serve'; Reject = $false; Launch = $false }
        @{ Case = 'ask-unquoted'; Command = '{exe} ask'; Reject = $false; Launch = $true }
        @{ Case = 'login-unquoted'; Command = '{exe} login'; Reject = $false; Launch = $true }
        @{ Case = 'logout-unquoted'; Command = '{exe} logout'; Reject = $false; Launch = $true }
        @{ Case = 'status-unquoted'; Command = '{exe} status'; Reject = $false; Launch = $true }
        @{ Case = 'levels-unquoted'; Command = '{exe} levels'; Reject = $false; Launch = $true }
        @{ Case = 'listen-unquoted'; Command = '{exe} listen'; Reject = $false; Launch = $true }
        @{ Case = 'unknown-command'; Command = '"{exe}" dance'; Reject = $true; Launch = $false }
        @{ Case = 'malformed-path'; Command = '"{exe} serve'; Reject = $true; Launch = $false }
        @{ Case = 'path-suffix'; Command = '"{exe}extra" serve'; Reject = $true; Launch = $false }
        @{ Case = 'missing-argument'; Command = '"{exe}"'; Reject = $true; Launch = $false }
        @{ Case = 'serve-prefix'; Command = '"{exe}" server'; Reject = $true; Launch = $false }
    ) {
        InModuleScope Winarchy -Parameters @{ Root = (Join-Path $TestDrive "assistant-start-$Case"); Case = $Case; Command = $Command; Reject = $Reject; Launch = $Launch } {
            param($Root, $Case, $Command, $Reject, $Launch)
            Mock Resolve-Path { [pscustomobject]@{ Path = $Root } } -ParameterFilter { $Path -like '*\..\..\..' }
            $state = Get-WinarchyStateDir
            New-Item -ItemType Directory -Path (Join-Path $state 'bin') -Force | Out-Null
            $binary = Join-Path $state 'bin\winarchy-assistant.exe'
            Set-Content $binary -Value 'fixture' -NoNewline
            Mock Get-Process {
                [pscustomobject]@{ Id = 4343; ProcessName = 'winarchy-assistant'; SessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId; Path = $binary }
            } -ParameterFilter { $Name -eq 'winarchy-assistant' }
            Mock Get-CimInstance {
                if ($Case -eq 'unavailable-cim') { throw 'CIM unavailable' }
                $line = if ($null -eq $Command) { $null } else { $Command.Replace('{exe}', $binary) }
                [pscustomobject]@{ CommandLine = $line }
            } -ParameterFilter { $ClassName -eq 'Win32_Process' -and $Filter -eq 'ProcessId = 4343' }
            Mock Start-Process {}
            Mock Stop-Process {}
            $previousRoot = $env:WINARCHY_ROOT
            try {
                if ($Reject) {
                    { Start-WinarchyAssistant } | Should -Throw 'Cannot safely start the assistant while a same-session process cannot be classified; retry after Windows process query access is restored.'
                } else { Start-WinarchyAssistant }
                if ($Launch) {
                    Should -Invoke Start-Process -Exactly -Times 1 -ParameterFilter { $FilePath -eq $binary -and $ArgumentList -eq 'serve' -and $WindowStyle -eq 'Hidden' }
                } else { Should -Invoke Start-Process -Times 0 }
                Should -Invoke Stop-Process -Times 0
            } finally { $env:WINARCHY_ROOT = $previousRoot }
        }
    }

    It 'assistant does not use migration fallback when its PID file has <_>' -ForEach 'malformed', 'missing-process', 'wrong-name', 'wrong-session' {
        InModuleScope Winarchy -Parameters @{ Root = (Join-Path $TestDrive "assistant-no-fallback-$_"); Case = $_ } {
            param($Root, $Case)
            Mock Resolve-Path { [pscustomobject]@{ Path = $Root } } -ParameterFilter { $Path -like '*\..\..\..' }
            $state = Get-WinarchyStateDir
            $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
            $content = if ($Case -eq 'malformed') { 'invalid' } else { '4242' }
            Set-Content (Join-Path $state "assistant-session-$sessionId") -Value $content -NoNewline
            Mock Get-Process {
                if ($Case -ne 'missing-process') {
                    $name = if ($Case -eq 'wrong-name') { 'unrelated' } else { 'winarchy-assistant' }
                    $processSession = [Diagnostics.Process]::GetCurrentProcess().SessionId
                    if ($Case -eq 'wrong-session') { $processSession++ }
                    [pscustomobject]@{ Id = 4242; ProcessName = $name; SessionId = $processSession }
                }
            } -ParameterFilter { $Id -eq 4242 }
            Mock Get-Process {
                [pscustomobject]@{ Id = 4343; ProcessName = 'winarchy-assistant'; SessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId; Path = (Join-Path $Root 'state\bin\winarchy-assistant.exe') }
            } -ParameterFilter { $Name -eq 'winarchy-assistant' }

            Get-WinarchyAssistantProcess | Should -BeNullOrEmpty
            Should -Invoke Get-Process -ParameterFilter { $Name -eq 'winarchy-assistant' } -Times 0
        }
    }

    It 'AHK migration discovery enumerates PIDs and checks session name and executable path' {
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw
        $lookup = [regex]::Match($ahk, '(?ms)^FindSessionProcessPid\(exeName, paths, commandArg := ''''\) \{.*?^\}').Value
        $lookup | Should -Match ([regex]::Escape("DllCall('Psapi.dll\EnumProcesses'"))
        $lookup | Should -Match ([regex]::Escape("pid := NumGet(pids, (A_Index - 1) * 4, 'uint')"))
        $lookup | Should -Match "ProcessIdToSessionId[^\r\n]*candidateSession != sessionId"
        $lookup | Should -Match ([regex]::Escape('if StrLower(ProcessGetName(pid)) != exeName'))
        $lookup | Should -Match ([regex]::Escape('path := StrLower(ProcessGetPath(pid))'))
        $lookup | Should -Match '(?s)for candidatePath in paths\s+if path = StrLower\(candidatePath\) \{\s+matchesPath := true'
        $lookup | Should -Match '(?s)if !matchesPath\s+continue'
    }

    It 'AHK command filtering requires the initial command argument and its boundary' {
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw
        $lookup = [regex]::Match($ahk, '(?ms)^FindSessionProcessPid\(.*?^\}').Value
        $lookup | Should -Match ([regex]::Escape('if commandArg {'))
        $lookup | Should -Match ([regex]::Escape('commandKind := GetAssistantCommandKind(path, commandLine)'))
        $lookup | Should -Match '(?s)if \(commandKind = ''unknown''\)\s+return -1'
        $lookup | Should -Match '(?s)if \(commandKind != commandArg\)\s+continue'
        $classifier = [regex]::Match($ahk, '(?ms)^GetAssistantCommandKind\(path, commandLine\) \{.*?^\}').Value
        $classifier | Should -Match ([regex]::Escape('SubStr(commandLine, 1, StrLen(quotedPath)) = quotedPath'))
        $classifier | Should -Match ([regex]::Escape('SubStr(commandLine, 1, StrLen(path)) = path'))
        $classifier | Should -Match ([regex]::Escape("SubStr(commandLine, offset + 1, 1)"))
        $classifier | Should -Match '(?s)if \(argument = ''serve''\)\s+return ''serve'''
        foreach ($command in 'ask', 'login', 'logout', 'status', 'levels', 'listen') {
            $classifier | Should -Match ([regex]::Escape("argument = '$command'"))
        }
        $classifier | Should -Match "return 'cli'"
        $classifier | Should -Match "return 'unknown'"
    }

    It 'AHK leaves an unclassifiable assistant alive and suppresses daemon startup' {
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw
        $lookup = [regex]::Match($ahk, '(?ms)^FindSessionProcessPid\(.*?^\}').Value
        $lookup | Should -Match '(?s)catch \{\s+return -1'
        $lookup | Should -Match '(?s)if \(commandLine = ''''\)\s+return -1'
        $startup = [regex]::Match($ahk, '(?ms)^if FileExist[^\r\n]*assistant\.enabled[^\r\n]*\{.*?^\}').Value
        $startup | Should -Match '(?s)if legacyPid > 0 \{\s+try \{\s+ProcessClose\(legacyPid\)'
        $startup | Should -Match '(?s)if \(assistantCandidate = 0\) \{\s+for exe.*?Run\('
    }

    It 'AHK shell migration discovery is limited to an absent session status file' {
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw
        $lookup = [regex]::Match($ahk, '(?ms)^GetShellPid\(\) \{.*?^\}').Value
        $lookup | Should -Match '(?s)if !ShellStatusFile \|\| !FileExist\(ShellStatusFile\)\s+return FindSessionProcessPid'
        $lookup | Should -Match ([regex]::Escape("FindSessionProcessPid('winarchy-shell.exe', [RepoRoot '\state\bin\winarchy-shell.exe', RepoRoot '\shell\target\release\winarchy-shell.exe'])"))
        [regex]::Matches($lookup, 'FindSessionProcessPid\(').Count | Should -Be 1
    }

    It 'AHK assistant startup stops a repo legacy process before launching its replacement' {
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw
        $startup = [regex]::Match($ahk, '(?ms)^if FileExist[^\r\n]*assistant\.enabled[^\r\n]*\{.*?^\}').Value
        $startup | Should -Match ([regex]::Escape("legacyPid := !FileExist(AssistantPidFile) ? FindSessionProcessPid('winarchy-assistant.exe', [RepoRoot '\state\bin\winarchy-assistant.exe', RepoRoot '\assistant\target\release\winarchy-assistant.exe'], 'serve') : 0"))
        $startup | Should -Match '(?s)if legacyPid > 0 \{.*?ProcessClose\(legacyPid\).*?ProcessWaitClose\(legacyPid, 1\).*?assistantCandidate := !GetAssistantPid\(\) \? FindSessionProcessPid\(.*?if \(assistantCandidate = 0\) \{.*?Run\('
        [regex]::Matches($startup, "FindSessionProcessPid\('winarchy-assistant\.exe', [^\r\n]*?\], 'serve'\)").Count | Should -Be 2
        $startup | Should -Not -Match ([regex]::Escape('\\.\pipe\winarchy-assistant'))
    }

    It 'AHK never falls back to unsuffixed shell or assistant pipes' {
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw
        foreach ($name in 'shell', 'assistant') {
            $ahk | Should -Not -Match (([regex]::Escape("\\.\pipe\winarchy-$name")) + '[''"]')
        }
        $ahk | Should -Match ([regex]::Escape('if FileExist(StateDir "\assistant.enabled") && !GetAssistantPid()'))
    }

    It 'AHK routes both command pipes through the current session' {
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw
        $ahk | Should -Match ([regex]::Escape("DllCall('ProcessIdToSessionId', 'uint', DllCall('GetCurrentProcessId'), 'uint*', &sessionId"))
        $ahk | Should -Match ([regex]::Escape("pipeName := '\\.\pipe\winarchy-shell-' sessionId"))
        $ahk | Should -Match ([regex]::Escape("FileOpen('\\.\pipe\winarchy-assistant-' sessionId, 'w')"))
    }

    It 'AHK CustomShell bridge finishes before services stop' {
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw
        $quit = [regex]::Match($ahk, '(?ms)^QuitStack\(\) \{.*?^\}').Value
        $handoff = [regex]::Match($quit, '(?ms)^    if sessionShell \{.*?^    \}')
        $handoff.Value | Should -Match '(?s)SetTimer\(ShellWatch, 0\).*result := RunShellRecoveryBridge\(\).*if !result'
        foreach ($teardown in 'SetTimer(YasbWatch, 0)', "Komorebic('stop')", "RunWait('taskkill /IM yasb.exe /F'", "RunWait('taskkill /IM Flow.Launcher.exe /F'") {
            $quit.IndexOf($teardown) | Should -BeGreaterOrEqual ($handoff.Index + $handoff.Length)
        }
    }

    It 'AHK CustomShell restores its watchdog and aborts teardown when the bridge fails' {
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw
        $quit = [regex]::Match($ahk, '(?ms)^QuitStack\(\) \{.*?^\}').Value
        $failure = [regex]::Match($quit, '(?ms)^        if !result \{.*?^        \}').Value

        $quit | Should -Match '(?s)result := RunShellRecoveryBridge\(\).*if !result'
        $failure | Should -Match '(?s)ShellLifecycleStopping := false\s+SetTimer\(ShellWatch, 5000\)\s+TrayTip\([^\r\n]+\)\s+return'
        $failure | Should -Not -Match 'Komorebic|taskkill|ProcessClose|ExitApp'
        $quit.IndexOf($failure) | Should -BeLessThan $quit.IndexOf("Komorebic('stop')")
    }

    It 'the CustomShell bridge propagates failed PowerShell recovery as a nonzero exit' {
        $bridge = Get-Content (Join-Path $script:Root 'scripts\Stop-WinarchySessionShell.ps1') -Raw

        $bridge | Should -Match '(?s)\$module = Join-Path \$Root ''module/Winarchy/Winarchy.psd1''\s+Import-Module \$module -Force\s+if \(-not \(Stop-WinarchyShell\)\) \{ exit 1 \}\s+exit 0'
    }

    It 'PowerShell checks Explorer ownership in the current session before a bounded readiness message' {
        $shell = Get-Content (Join-Path $script:Root 'module\Winarchy\Private\Shell.ps1') -Raw
        $ready = [regex]::Match($shell, '(?ms)^function Test-WinarchyExplorerShellReady \{.*?(?=^function Start-WinarchyExplorer)').Value

        $ready | Should -Match 'process.SessionId == sessionId && String.Equals\(process.ProcessName, "explorer", StringComparison.OrdinalIgnoreCase\)'
        $ready | Should -Match 'SendMessageTimeout\(window, 0, IntPtr.Zero, IntPtr.Zero, 0x22, 500, out result\) != IntPtr.Zero'
        $ready | Should -Match '\[WinarchyExplorerShellReadiness\]::IsResponsive\(\[Diagnostics.Process\]::GetCurrentProcess\(\).SessionId\)'
    }

    It 'AHK accepts valid status mode zero before considering a stale CustomShell marker' {
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw
        $quit = [regex]::Match($ahk, '(?ms)^QuitStack\(\) \{.*?^\}').Value
        $quit | Should -Match ([regex]::Escape("if status.Length = 3 && RegExMatch(status[1], '^\d+$') && status[3] = ShellLogonIdentity && status[2] ~= '^[01]$' {"))
        $quit | Should -Match '(?s)statusValid := true\s+sessionShell := status\[2\] = ''1''\s+\}\s+\}\s+if !statusValid && ShellSessionMarker && FileExist\(ShellSessionMarker\) \{\s+try sessionShell := Trim\(FileRead\(ShellSessionMarker\)\) = ShellLogonIdentity'
    }

    It 'AHK QuitStack closes the recorded shell PID without killing by image name' {
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw
        $quit = [regex]::Match($ahk, '(?ms)^QuitStack\(\) \{.*?^\}').Value
        $quit | Should -Match 'shellPid := GetShellPid\(\)'
        $quit | Should -Match 'ProcessClose\(shellPid\)'
        $quit | Should -Not -Match 'taskkill[^\r\n]*/IM\s+winarchy-shell'
    }
}

Describe 'Stop-WinarchyShell' {
    BeforeEach {
        InModuleScope Winarchy -Parameters @{ Root = (Join-Path $TestDrive 'shell-stop') } {
            param($Root)
            $script:shellStopRoot = $Root
            Mock Resolve-Path { [pscustomobject]@{ Path = $script:shellStopRoot } } -ParameterFilter { $Path -like '*\..\..\..' }
            $state = Get-WinarchyStateDir
            $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
            Mock Get-WinarchyCurrentLogonIdentity { '00000000000000ab-01dc000000000001' }
            Remove-Item (Join-Path $state "session-shell-$sessionId") -Force -ErrorAction SilentlyContinue
            Remove-Item (Join-Path $state "shell-relaunch-inhibit-$sessionId") -Force -ErrorAction SilentlyContinue
            Set-Content (Join-Path $state "shell-session-$sessionId") -Value '4242|0|00000000000000ab-01dc000000000001' -NoNewline
            $script:savedShellQuitWaitMs = $script:ShellQuitWaitMs
            $script:savedShellReturnWaitMs = $script:ShellReturnWaitMs
            $script:log = [System.Collections.Generic.List[string]]::new()
            $script:alive = $true
            $script:written = $null
            $script:connectFails = $false
            $script:stopFails = $false
            $script:explorerReady = $false
            Mock Test-WinarchyExplorerShellReady { $script:explorerReady }
            Mock Start-WinarchyExplorer { }
            Mock Get-Process {
                if ($script:alive) {
                    [pscustomobject]@{ Id = 4242; ProcessName = 'winarchy-shell'; SessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId; Path = (Join-Path $script:shellStopRoot 'state\bin\winarchy-shell.exe') }
                }
            }
            Mock Stop-Process { if ($Name -ne 'yasb' -and -not $script:stopFails) { $script:alive = $false }; $script:log.Add('stop') }
            Mock Wait-Process { }
            Mock Restore-WinarchyNativeTaskbar { $script:log.Add('restore') }
            Mock New-Object {
                $pipe = [pscustomobject]@{}
                $pipe | Add-Member ScriptMethod Connect { param($ms) $script:log.Add("connect:$ms"); if ($script:connectFails) { throw 'timeout' } }
                $pipe | Add-Member ScriptMethod WriteAsync { param($b, $o, $n) $script:written = [Text.Encoding]::UTF8.GetString($b, $o, $n); [Threading.Tasks.Task]::CompletedTask }
                $pipe | Add-Member ScriptMethod Flush { }
                $pipe | Add-Member ScriptMethod Dispose { $script:log.Add('dispose') }
                $pipe
            }
            Mock Start-Sleep { $script:log.Add("wait:$Milliseconds"); if ($script:exitsOnWait) { $script:alive = $false } }
            $script:exitsOnWait = $false
        }
    }

    AfterEach {
        InModuleScope Winarchy {
            $script:ShellQuitWaitMs = $script:savedShellQuitWaitMs
            $script:ShellReturnWaitMs = $script:savedShellReturnWaitMs
        }
    }

    It 'returns without touching the pipe or killing anything when no shell runs' {
        InModuleScope Winarchy {
            $script:alive = $false
            $script:explorerReady = $true
            Stop-WinarchyShell
            Should -Invoke New-Object -Times 0
            Should -Invoke Stop-Process -Times 0
        }
    }

    It 'sends quit with a trailing LF, waits, and does not force a shell that exited' {
        InModuleScope Winarchy {
            $script:exitsOnWait = $true
            Stop-WinarchyShell
            $script:written | Should -Be "{`"v`":1,`"id`":1,`"cmd`":`"quit`"}`n"
            $script:log[0] | Should -Be 'connect:200'
            $script:log -contains 'wait:50' | Should -BeTrue
            Should -Invoke Get-Process -ParameterFilter { $null -ne $Id -and @($Id | Where-Object { $_ -ne 4242 }).Count -gt 0 } -Times 0
            Should -Invoke New-Object -ParameterFilter { $TypeName -eq 'System.IO.Pipes.NamedPipeClientStream' -and $ArgumentList[1] -eq "winarchy-shell-$([Diagnostics.Process]::GetCurrentProcess().SessionId)" } -Exactly -Times 1
            Should -Invoke Stop-Process -Times 0
            Should -Invoke Restore-WinarchyNativeTaskbar -Times 0
        }
    }

    It 'returns to Explorer only when stopping a custom session shell' {
        InModuleScope Winarchy {
            $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
            Set-Content (Join-Path (Get-WinarchyStateDir) "shell-session-$sessionId") -Value '4242|1|00000000000000ab-01dc000000000001' -NoNewline
            $script:exitsOnWait = $true
            $script:explorerReady = $true
            Stop-WinarchyShell | Should -BeTrue
            $script:written | Should -Be "{`"v`":1,`"id`":1,`"cmd`":`"return`"}`n"
            Should -Invoke Start-WinarchyExplorer -Times 0
            Should -Invoke Stop-Process -Times 0
        }
    }

    It 'reports failure when the shell exits after return but Explorer never becomes ready' {
        InModuleScope Winarchy {
            $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
            Set-Content (Join-Path (Get-WinarchyStateDir) "shell-session-$sessionId") -Value '4242|1|00000000000000ab-01dc000000000001' -NoNewline
            $script:exitsOnWait = $true
            $script:explorerReady = $false
            $script:ShellReturnWaitMs = 50

            Stop-WinarchyShell | Should -BeFalse

            $script:alive | Should -BeFalse
            $script:written | Should -Be "{`"v`":1,`"id`":1,`"cmd`":`"return`"}`n"
            Should -Invoke Start-WinarchyExplorer -Exactly -Times 1
            Should -Invoke Stop-Process -Times 0
            Should -Invoke Restore-WinarchyNativeTaskbar -Times 0
        }
    }

    It 'keeps the session shell alive when the Explorer return request does not exit' {
        InModuleScope Winarchy {
            $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
            Set-Content (Join-Path (Get-WinarchyStateDir) "shell-session-$sessionId") -Value '4242|1|00000000000000ab-01dc000000000001' -NoNewline
            $script:ShellReturnWaitMs = 10
            $script:explorerReady = $false
            Stop-WinarchyShell | Should -BeFalse
            $script:written | Should -Be "{`"v`":1,`"id`":1,`"cmd`":`"return`"}`n"
            Should -Invoke Start-WinarchyExplorer -Exactly -Times 1
            Should -Invoke Stop-Process -Times 0
            Should -Invoke Restore-WinarchyNativeTaskbar -Times 0
        }
    }

    It 'stops a custom session shell only after Explorer recovers from an unavailable return pipe' {
        InModuleScope Winarchy {
            $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
            Set-Content (Join-Path (Get-WinarchyStateDir) "shell-session-$sessionId") -Value '4242|1|00000000000000ab-01dc000000000001' -NoNewline
            $script:connectFails = $true
            $script:ShellQuitWaitMs = 0
            $script:ShellReturnWaitMs = 1000
            Mock Start-WinarchyExplorer { $script:explorerReady = $true; $script:log.Add('explorer-ready') }
            Mock Stop-Process { $script:alive = $false; $script:log.Add('stop') }
            Mock Wait-Process { }

            Stop-WinarchyShell | Should -BeTrue

            $script:log -join ',' | Should -Be 'connect:200,dispose,explorer-ready,wait:100,stop,restore'
            Should -Invoke Stop-Process -Exactly -Times 1 -ParameterFilter { $Force -and $Id.Count -eq 1 -and $Id[0] -eq 4242 }
        }
    }

    It 'uses quit when a valid normal-shell status conflicts with a stale session marker' {
        InModuleScope Winarchy {
            $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
            Set-Content (Join-Path (Get-WinarchyStateDir) "session-shell-$sessionId") -Value '00000000000000ab-01dc000000000000' -NoNewline
            $script:ShellQuitWaitMs = 0
            Stop-WinarchyShell
            $script:written | Should -Be "{`"v`":1,`"id`":1,`"cmd`":`"quit`"}`n"
            Should -Invoke Stop-Process -Times 1
            Should -Invoke Restore-WinarchyNativeTaskbar -Times 1
        }
    }

    It 'uses return when a live session shell has a marker but lost its status file' {
        InModuleScope Winarchy {
            $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
            Set-Content (Join-Path (Get-WinarchyStateDir) "session-shell-$sessionId") -Value '00000000000000ab-01dc000000000001' -NoNewline
            Remove-Item (Join-Path (Get-WinarchyStateDir) "shell-session-$sessionId") -Force
            $script:ShellReturnWaitMs = 10
            $script:explorerReady = $false
            Stop-WinarchyShell | Should -BeFalse
            $script:written | Should -Be "{`"v`":1,`"id`":1,`"cmd`":`"return`"}`n"
            Should -Invoke Start-WinarchyExplorer -Exactly -Times 1
            Should -Invoke Stop-Process -Times 0
        }
    }

    It 'sends restart instead of quit when the shell will be relaunched' {
        InModuleScope Winarchy {
            $script:exitsOnWait = $true
            Stop-WinarchyShell -Restart
            $script:written | Should -Be "{`"v`":1,`"id`":1,`"cmd`":`"restart`"}`n"
            Should -Invoke Stop-Process -Times 0
        }
    }

    It 'reloads a custom session shell in place without waiting for exit or forcing it' {
        InModuleScope Winarchy {
            $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
            Set-Content (Join-Path (Get-WinarchyStateDir) "shell-session-$sessionId") -Value '4242|1|00000000000000ab-01dc000000000001' -NoNewline
            Stop-WinarchyShell -Restart
            $script:written | Should -Be "{`"v`":1,`"id`":1,`"cmd`":`"reload`"}`n"
            Should -Invoke Start-Sleep -Times 0
            Should -Invoke Stop-Process -Times 0
            Should -Invoke Restore-WinarchyNativeTaskbar -Times 0
        }
    }

    It 'keeps a custom session shell running when its reload pipe is unavailable' {
        InModuleScope Winarchy {
            $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
            Set-Content (Join-Path (Get-WinarchyStateDir) "shell-session-$sessionId") -Value '4242|1|00000000000000ab-01dc000000000001' -NoNewline
            $script:connectFails = $true
            $script:ShellQuitWaitMs = 0
            Stop-WinarchyShell -Restart
            $script:log -join ',' | Should -Be 'connect:200,dispose'
            Should -Invoke Stop-Process -Times 0
            Should -Invoke Restore-WinarchyNativeTaskbar -Times 0
        }
    }

    It 'native reload <Mode> launches <Starts> replacement shell processes' -ForEach @(
        @{ Mode = 'CustomShell'; SessionShell = '1'; Starts = 0; Command = 'reload'; Light = $true; Inhibited = $false; RetainsInhibit = $false }
        @{ Mode = 'normal'; SessionShell = '0'; Starts = 1; Command = 'restart'; Light = $true; Inhibited = $false; RetainsInhibit = $false }
        @{ Mode = 'Light with current-logon inhibit'; SessionShell = '0'; Starts = 0; Command = 'restart'; Light = $true; Inhibited = $true; RetainsInhibit = $true }
        @{ Mode = 'full with current-logon inhibit'; SessionShell = '0'; Starts = 1; Command = 'restart'; Light = $false; Inhibited = $true; RetainsInhibit = $false }
        @{ Mode = 'full without inhibit'; SessionShell = '0'; Starts = 1; Command = 'restart'; Light = $false; Inhibited = $false; RetainsInhibit = $false }
    ) {
        InModuleScope Winarchy -Parameters @{ SessionShell = $SessionShell; Starts = $Starts; Command = $Command; Light = $Light; Inhibited = $Inhibited; RetainsInhibit = $RetainsInhibit } {
            param($SessionShell, $Starts, $Command, $Light, $Inhibited, $RetainsInhibit)
            $oldKomorebi = $env:KOMOREBI_CONFIG_HOME
            $oldYasb = $env:YASB_CONFIG_HOME
            try {
                $state = Get-WinarchyStateDir
                $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
                Set-Content (Join-Path $state "shell-session-$sessionId") -Value "4242|$SessionShell|00000000000000ab-01dc000000000001" -NoNewline
                Set-Content (Join-Path $state 'bar') -Value 'native' -NoNewline
                $bin = New-Item (Join-Path $state 'bin') -ItemType Directory -Force
                $exe = Join-Path $bin.FullName 'winarchy-shell.exe'
                Set-Content $exe -Value ''
                Mock Start-Process { }
                if (-not (Get-Command komorebic -ErrorAction SilentlyContinue)) { function komorebic { } }
                Mock komorebic { }
                Mock Invoke-WinarchyKomorebiStaticReload { }
                Mock komorebic { throw 'monitor information unavailable' } -ParameterFilter { $args[0] -eq 'monitor-information' }
                $script:ShellQuitWaitMs = 0
                $inhibitPath = Join-Path $state "shell-relaunch-inhibit-$sessionId"
                if ($Inhibited) {
                    Set-Content $inhibitPath -Value '00000000000000ab-01dc000000000001' -NoNewline
                }

                Invoke-WinarchyReload -Light:$Light

                $script:written | Should -Be "{`"v`":1,`"id`":1,`"cmd`":`"$Command`"}`n"
                Should -Invoke Start-Process -Exactly -Times $Starts -ParameterFilter { $FilePath -eq $exe -and $WindowStyle -eq 'Hidden' }
                if ($Light) { Should -Invoke Start-Process -Exactly -Times $Starts }
                (Test-Path -LiteralPath $inhibitPath) | Should -Be $RetainsInhibit
                if ($RetainsInhibit) {
                    Get-Content -LiteralPath $inhibitPath -Raw | Should -Be '00000000000000ab-01dc000000000001'
                }
            }
            finally {
                $env:KOMOREBI_CONFIG_HOME = $oldKomorebi
                $env:YASB_CONFIG_HOME = $oldYasb
            }
        }
    }

    It 'aborts <Bar> reload after a failed session-shell reload without starting later processes' -ForEach @(
        @{ Bar = 'native' }
        @{ Bar = 'yasb' }
    ) {
        InModuleScope Winarchy -Parameters @{ Bar = $Bar } {
            param($Bar)
            $oldKomorebi = $env:KOMOREBI_CONFIG_HOME
            $oldYasb = $env:YASB_CONFIG_HOME
            try {
                $state = Get-WinarchyStateDir
                $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
                Set-Content (Join-Path $state "shell-session-$sessionId") -Value '4242|1|00000000000000ab-01dc000000000001' -NoNewline
                Set-Content (Join-Path $state 'bar') -Value $Bar -NoNewline
                New-Item (Join-Path $script:shellStopRoot 'scripts') -ItemType Directory -Force | Out-Null
                Set-Content (Join-Path $script:shellStopRoot 'scripts\Start-WindowSlots.ps1') -Value ''
                $script:connectFails = $true
                $script:ShellQuitWaitMs = 0
                Mock Get-Process { } -ParameterFilter { $Name -eq 'yasb' }
                $script:reloadTestPath = Get-Command Test-Path
                $script:reloadGetCommand = Get-Command Get-Command
                Mock Test-Path { if ($LiteralPath) { & $script:reloadTestPath -LiteralPath $LiteralPath } else { & $script:reloadTestPath -Path $Path } }
                Mock Test-Path { $false } -ParameterFilter { $Path -eq '\\.\pipe\winarchy-window-slots' }
                Mock Get-Command { & $script:reloadGetCommand -Name $Name -ErrorAction SilentlyContinue }
                Mock Get-Command { $null } -ParameterFilter { $Name -in 'yasbc', 'yasb' }
                Mock Start-Process { }
                if (-not (Get-Command komorebic -ErrorAction SilentlyContinue)) { function komorebic { } }
                Mock komorebic { }
                Mock Invoke-WinarchyKomorebiStaticReload { }
                Mock komorebic { throw 'monitor information unavailable' } -ParameterFilter { $args[0] -eq 'monitor-information' }

                { Invoke-WinarchyReload } | Should -Throw '*shell could not reload*'

                Should -Invoke New-Object -Exactly -Times 1
                Should -Invoke Start-Process -Times 0
                Should -Invoke Restore-WinarchyNativeTaskbar -Times 0
            }
            finally {
                $env:KOMOREBI_CONFIG_HOME = $oldKomorebi
                $env:YASB_CONFIG_HOME = $oldYasb
            }
        }
    }

    It 'reports failure when normal-shell <Mode> cannot stop the process' -ForEach @(
        @{ Mode = 'quit'; Restart = $false }
        @{ Mode = 'restart'; Restart = $true }
    ) {
        InModuleScope Winarchy -Parameters @{ Restart = $Restart } {
            param($Restart)
            $script:ShellQuitWaitMs = 0
            $script:stopFails = $true
            Mock Wait-Process { }

            $result = Stop-WinarchyShell -Restart:$Restart

            Should -Invoke Stop-Process -Exactly -Times 1 -ParameterFilter { $Force -and $Id.Count -eq 1 -and $Id[0] -eq 4242 }
            Should -Invoke Restore-WinarchyNativeTaskbar -Exactly -Times 1
            $result | Should -BeFalse
        }
    }

    It 'forces only the recorded shell PID after its grace period expires' {
        InModuleScope Winarchy {
            $script:ShellQuitWaitMs = 0
            Stop-WinarchyShell
            ($script:log -join ',') | Should -Be 'connect:200,dispose,stop,restore'
            Should -Invoke Stop-Process -Times 1 -ParameterFilter { $Force -and $Id.Count -eq 1 -and $Id[0] -eq 4242 }
        }
    }

    It 'checks the recorded shell PID again after each polling interval' {
        InModuleScope Winarchy {
            $script:exitsOnWait = $true
            Stop-WinarchyShell
            $script:log -contains 'wait:50' | Should -BeTrue
            Should -Invoke Start-Sleep -ParameterFilter { $Milliseconds -eq 50 } -Exactly -Times 1
            Should -Invoke Stop-Process -Times 0
        }
    }

    It 'forces without waiting when Connect throws and the shell is alive' {
        InModuleScope Winarchy {
            $script:connectFails = $true
            $script:ShellQuitWaitMs = 0
            Stop-WinarchyShell
            ($script:log | Where-Object { $_ -like 'wait:*' }) | Should -BeNullOrEmpty
            Should -Invoke Stop-Process -Times 1 -ParameterFilter { $Force }
            Should -Invoke Restore-WinarchyNativeTaskbar -Times 1
        }
    }
}

Describe 'Session shell launcher readiness' {
    BeforeAll {
        function Invoke-LauncherReadinessCheck {
            param([string]$Launcher, [object[]]$Processes = @(), [int]$Foreground = 0, [switch]$RetryLoop)

            $path = Join-Path $script:Root "scripts\$Launcher"
            $ast = [System.Management.Automation.Language.Parser]::ParseFile($path, [ref]$null, [ref]$null)
            $functions = $ast.FindAll({
                param($node)
                $node -is [System.Management.Automation.Language.FunctionDefinitionAst]
            }, $false).Extent.Text -join "`n"
            $loop = $ast.EndBlock.Statements | Where-Object {
                $_ -is [System.Management.Automation.Language.WhileStatementAst] -and
                $_.Condition.Extent.Text -eq '(Get-Date) -lt $overallDeadline'
            } | Select-Object -Last 1
            $functions = $functions.Replace('[Win.FgAhk]', '[LauncherForeground]').Replace('[Win.Fg]', '[LauncherForeground]')
            $loopText = $loop.Extent.Text.Replace('[Win.Fg]', '[LauncherForeground]')
            $foregroundType = 'LauncherForeground' + [guid]::NewGuid().ToString('N')
            $functions = $functions.Replace('LauncherForeground', $foregroundType)
            $loopText = $loopText.Replace('LauncherForeground', $foregroundType)
            $runner = [powershell]::Create()
            try {
                $runnerScript = {
                    param($functions, $loopText, $processes, $foreground, $retryLoop, $logPath)
                    class LauncherForeground {
                        static [IntPtr]$Handle
                        static [IntPtr] GetForegroundWindow() { return [LauncherForeground]::Handle }
                        static [bool] SystemParametersInfo([uint32]$a, [uint32]$b, [IntPtr]$c, [uint32]$d) { return $true }
                        static [void] keybd_event([byte]$a, [byte]$b, [uint32]$c, [UIntPtr]$d) {}
                    }
                    [LauncherForeground]::Handle = [IntPtr]$foreground
                    $script:processes = $processes
                    $sessionId = 42
                    $script:now = [datetime]'2026-01-01T00:00:00'
                    $overallDeadline = $script:now.AddSeconds(1)
                    $script:launched = 0
                    $exe = 'unused.exe'
                    $script = 'unused.ahk'
                    $log = $logPath
                    $outLog = "$logPath.out"
                    $errLog = "$logPath.err"
                    function Get-Process {
                        param([string[]]$Name, $ErrorAction)
                        $script:processes | Where-Object { $_.ProcessName -in $Name }
                    }
                    function Get-CimInstance { param($ClassName, $Filter, $ErrorAction) }
                    function Get-Date {
                        param($Format)
                        if ($Format) { return '2026-01-01 00:00:00' }
                        $script:now
                    }
                    function Start-Sleep {
                        param($Milliseconds, $Seconds)
                        $script:now = $overallDeadline
                    }
                    function Start-Process {
                        param($FilePath, $ArgumentList, $WindowStyle, [switch]$PassThru, $RedirectStandardOutput, $RedirectStandardError)
                        $script:launched++
                        [pscustomobject]@{ HasExited = $true; ExitCode = 1 }
                    }
                    . ([scriptblock]::Create($functions))
                    if ($retryLoop) {
                        & ([scriptblock]::Create($loopText))
                        return $script:launched
                    }
                    Test-ShellReady
                }
                $null = $runner.AddScript($runnerScript.ToString().Replace('LauncherForeground', $foregroundType)).
                    AddArgument($functions).AddArgument($loopText).AddArgument($Processes).
                    AddArgument($Foreground).AddArgument([bool]$RetryLoop).AddArgument((Join-Path $TestDrive 'launcher.log'))
                $result = $runner.Invoke()
                if ($runner.HadErrors) { throw ($runner.Streams.Error -join "`n") }
                $result
            }
            finally { $runner.Dispose() }
        }
    }

    Context '<Launcher>' -ForEach @(
        @{ Launcher = 'Start-Ahk.ps1' }
        @{ Launcher = 'Start-Komorebi.ps1' }
    ) {
        It 'accepts the session shell without a foreground window' {
            $process = [pscustomobject]@{ ProcessName = 'winarchy-shell'; SessionId = 42 }
            Invoke-LauncherReadinessCheck $Launcher -Processes @($process) | Should -BeTrue
        }

        It 'requires a foreground window when only Explorer is running' {
            $process = [pscustomobject]@{ ProcessName = 'explorer'; SessionId = 42 }
            Invoke-LauncherReadinessCheck $Launcher -Processes @($process) | Should -BeFalse
        }

        It 'accepts Explorer with a foreground window' {
            $process = [pscustomobject]@{ ProcessName = 'explorer'; SessionId = 42 }
            Invoke-LauncherReadinessCheck $Launcher -Processes @($process) -Foreground 1 | Should -BeTrue
        }

        It 'rejects a shell from another session even with a foreground window' {
            $process = [pscustomobject]@{ ProcessName = 'winarchy-shell'; SessionId = 43 }
            Invoke-LauncherReadinessCheck $Launcher -Processes @($process) -Foreground 1 | Should -BeFalse
        }

        It 'rejects Explorer from another session even with a foreground window' {
            $process = [pscustomobject]@{ ProcessName = 'explorer'; SessionId = 43 }
            Invoke-LauncherReadinessCheck $Launcher -Processes @($process) -Foreground 1 | Should -BeFalse
        }

        It 'rejects a foreground window without a shell process' {
            Invoke-LauncherReadinessCheck $Launcher -Foreground 1 | Should -BeFalse
        }

        It 'does not launch a child when the readiness wait consumes the deadline' {
            Invoke-LauncherReadinessCheck $Launcher -RetryLoop | Should -Be 0
        }
    }
}

Describe 'Custom shell policy maintenance guard' -Tag 'CustomShellPolicyMaintenance' {
    It 'exports the guard to maintenance scripts' {
        (Get-Command Assert-WinarchyCustomShellPolicySafeForMaintenance).Name | Should -Be 'Assert-WinarchyCustomShellPolicySafeForMaintenance'
    }

    It 'allows unrelated or empty shell commands: <Command>' -TestCases @(
        @{ Command = $null }
        @{ Command = '' }
        @{ Command = '   ' }
        @{ Command = 'explorer.exe' }
        @{ Command = '"C:\Other Shell\shell.exe" --session-shell' }
        @{ Command = 'winarchy-shell.exe.backup' }
        @{ Command = 'other-winarchy-shell.exe' }
        @{ Command = 'explorer.exe winarchy-shell.exe' }
    ) {
        param($Command)
        InModuleScope Winarchy -Parameters @{ Command = $Command } {
            param($Command)
            Mock Get-WinarchyCustomShellPolicySnapshot {
                if ($null -eq $Command) { New-WinarchyCustomShellPolicySnapshot -Present $false }
                else { New-WinarchyCustomShellPolicySnapshot -Present $true -Kind String -RawBytes ([Text.Encoding]::Unicode.GetBytes("$Command`0")) }
            }

            { Assert-WinarchyCustomShellPolicySafeForMaintenance } | Should -Not -Throw
            Should -Invoke Get-WinarchyCustomShellPolicySnapshot -Times 1 -Exactly -ParameterFilter {
                $SubKey -eq 'Software\Microsoft\Windows\CurrentVersion\Policies\System'
            }
        }
    }

    It 'refuses maintenance for the exact Winarchy shell executable: <Command>' -TestCases @(
        @{ Command = 'winarchy-shell.exe' }
        @{ Command = 'winarchy-shell.exe --session-shell' }
        @{ Command = '"C:\Program Files\Winarchy\winarchy-shell.exe" --session-shell' }
        @{ Command = '"C:\Program Files\Winarchy\WINARCHY-SHELL.EXE"' }
        @{ Command = 'C:\Winarchy\winarchy-shell.exe' }
    ) {
        param($Command)
        InModuleScope Winarchy -Parameters @{ Command = $Command } {
            param($Command)
            Mock Get-WinarchyCustomShellPolicySnapshot {
                New-WinarchyCustomShellPolicySnapshot -Present $true -Kind String -RawBytes ([Text.Encoding]::Unicode.GetBytes("$Command`0"))
            }

            { Assert-WinarchyCustomShellPolicySafeForMaintenance } | Should -Throw '*winarchy-shell.exe*'
        }
    }

    It 'detects Winarchy in an environment-expanded policy command' {
        InModuleScope Winarchy {
            $env:WINARCHY_POLICY_TEST_ROOT = 'C:\Program Files\Winarchy'
            try {
                Mock Get-WinarchyCustomShellPolicySnapshot {
                    $value = '%WINARCHY_POLICY_TEST_ROOT%\winarchy-shell.exe'
                    New-WinarchyCustomShellPolicySnapshot -Present $true -Kind ExpandString -RawBytes ([Text.Encoding]::Unicode.GetBytes("$value`0"))
                }

                { Assert-WinarchyCustomShellPolicySafeForMaintenance } | Should -Throw '*winarchy-shell.exe*'
            }
            finally { Remove-Item Env:WINARCHY_POLICY_TEST_ROOT -ErrorAction SilentlyContinue }
        }
    }

    It 'allows a missing policy key' {
        InModuleScope Winarchy {
            Mock Get-WinarchyCustomShellPolicySnapshot { New-WinarchyCustomShellPolicySnapshot -Present $false }

            { Assert-WinarchyCustomShellPolicySafeForMaintenance } | Should -Not -Throw
            Should -Invoke Get-WinarchyCustomShellPolicySnapshot -Times 1 -Exactly
        }
    }

    It 'allows a missing Shell value' {
        InModuleScope Winarchy {
            Mock Get-WinarchyCustomShellPolicySnapshot { New-WinarchyCustomShellPolicySnapshot -Present $false }

            { Assert-WinarchyCustomShellPolicySafeForMaintenance } | Should -Not -Throw
            Should -Invoke Get-WinarchyCustomShellPolicySnapshot -Times 1 -Exactly
        }
    }

    It 'fails closed when registry inspection is denied' {
        InModuleScope Winarchy {
            Mock Get-WinarchyCustomShellPolicySnapshot { throw [System.UnauthorizedAccessException]::new('Policy inspection denied') }

            { Assert-WinarchyCustomShellPolicySafeForMaintenance } | Should -Throw '*Policy inspection denied*'
        }
    }

    It 'fails closed on an unexpected registry read error' {
        InModuleScope Winarchy {
            Mock Get-WinarchyCustomShellPolicySnapshot { throw [System.IO.IOException]::new('Policy read failed') }

            { Assert-WinarchyCustomShellPolicySafeForMaintenance } | Should -Throw '*Policy read failed*'
        }
    }

    It 'fails closed on unsupported value types and malformed command bytes' -ForEach @(
        @{ Kind = [Microsoft.Win32.RegistryValueKind]::Binary; Bytes = [byte[]]@(1, 2) }
        @{ Kind = [Microsoft.Win32.RegistryValueKind]::String; Bytes = [byte[]]@(65) }
        @{ Kind = [Microsoft.Win32.RegistryValueKind]::String; Bytes = [byte[]]@(65, 0, 0, 0, 66, 0, 0, 0) }
    ) {
        InModuleScope Winarchy -Parameters @{ Kind = $Kind; Bytes = $Bytes } {
            param($Kind, $Bytes)
            Mock Get-WinarchyCustomShellPolicySnapshot {
                New-WinarchyCustomShellPolicySnapshot -Present $true -Kind $Kind -RawBytes $Bytes
            }

            { Assert-WinarchyCustomShellPolicySafeForMaintenance } | Should -Throw '*CustomShell policy*'
        }
    }
}

Describe 'Custom shell policy maintenance caller order' -Tag 'CustomShellPolicyMaintenance' {
    It 'checks policy before destructive commands in <File>' -TestCases @(
        @{ File = 'install.ps1'; Function = $null; Mutations = @('New-WinarchySnapshot', 'Set-Content', 'winget', 'Initialize-WinarchyBar') }
        @{ File = 'uninstall.ps1'; Function = $null; Mutations = @('Stop-WinarchyShell', 'Stop-WinarchyWindowSlots', 'Stop-Process', 'Unregister-WinarchyAutostart') }
        @{ File = 'scripts\rollback-to-seelen.ps1'; Function = $null; Mutations = @('Stop-WinarchyShell', 'komorebic', 'Stop-Process', 'Remove-Item', 'Set-ItemProperty', 'Unregister-WinarchyAutostart') }
        @{ File = 'module\Winarchy\Private\SelfUpdate.ps1'; Function = 'Invoke-WinarchySelfUpdate'; Mutations = @('git', 'pwsh') }
        @{ File = 'module\Winarchy\Private\Shell.ps1'; Function = 'Install-WinarchyShellBinary'; Mutations = @('Copy-Item') }
    ) {
        param($File, $Function, $Mutations)
        $ast = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $script:Root $File), [ref]$null, [ref]$null)
        if ($Function) {
            $ast = $ast.Find({ param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $Function }, $true).Body
        }
        $commands = @($ast.FindAll({ $args[0] -is [System.Management.Automation.Language.CommandAst] }, $true))
        $guard = @($commands | Where-Object { $_.GetCommandName() -eq 'Assert-WinarchyCustomShellPolicySafeForMaintenance' })
        $guard.Count | Should -Be 1
        foreach ($name in $Mutations) {
            $sideEffects = @($commands | Where-Object { $_.GetCommandName() -eq $name })
            $sideEffects.Count | Should -BeGreaterThan 0
            foreach ($sideEffect in $sideEffects) {
                $guard[0].Extent.StartOffset | Should -BeLessThan $sideEffect.Extent.StartOffset
            }
        }
    }
}

Describe 'Shell stop callers' {
    BeforeAll {
        $script:Theme = Get-Content (Join-Path $script:Root 'module\Winarchy\Private\ThemeEngine.ps1') -Raw
        $script:Uninstall = Get-Content (Join-Path $script:Root 'uninstall.ps1') -Raw
        $script:Rollback = Get-Content (Join-Path $script:Root 'scripts\rollback-to-seelen.ps1') -Raw
    }

    It 'uninstall and rollback call Stop-WinarchyShell' {
        $script:Uninstall | Should -Match 'Stop-WinarchyShell'
        $script:Rollback | Should -Match 'Stop-WinarchyShell'
    }

    It 'uninstall and rollback refuse teardown before stopping services when Explorer cannot return' {
        $script:Uninstall.IndexOf('if (-not $DryRun -and -not (Stop-WinarchyShell))') | Should -BeGreaterThan -1
        $script:Uninstall.IndexOf('if (-not $DryRun -and -not (Stop-WinarchyShell))') | Should -BeLessThan $script:Uninstall.IndexOf("Invoke-Step 'Stop komorebi")
        $script:Rollback.IndexOf('if (-not (Stop-WinarchyShell))') | Should -BeGreaterThan -1
        $script:Rollback.IndexOf('if (-not (Stop-WinarchyShell))') | Should -BeLessThan $script:Rollback.IndexOf('komorebic stop')
        $script:Rollback.IndexOf('if (-not (Stop-WinarchyShell))') | Should -BeLessThan $script:Rollback.IndexOf('Stop-Process -Name yasb')
    }

    It 'theme reload uses the non-restoring restart command' {
        ([regex]::Matches($script:Theme, [regex]::Escape('if (-not (Stop-WinarchyShell -Restart))'))).Count | Should -Be 2
    }

    It 'no caller kills winarchy-shell by name with Stop-Process' {
        foreach ($text in $script:Theme, $script:Uninstall, $script:Rollback) {
            $text | Should -Not -Match 'Stop-Process[^\r\n]*winarchy-shell'
        }
    }

    It 'the AHK QuitStack normal-mode source wait is 1.5 seconds and matches ShellQuitWaitMs' {
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw
        $quit = [regex]::Match($ahk, '(?ms)^QuitStack\(\) \{.*?^\}').Value
        ($quit -match "(?s)if !sessionShell \{\s+ShellIsland\('quit'\)\s+if shellPid && ProcessWaitClose\(shellPid, ([\d.]+)\)") | Should -BeTrue
        $seconds = [double]$Matches[1]
        $seconds | Should -Be 1.5
        $shellMs = InModuleScope Winarchy { $script:ShellQuitWaitMs }
        ($seconds * 1000) | Should -Be $shellMs
    }

    It 'CustomShell delegates its 25 second return wait to PowerShell through the bridge' {
        $shell = Get-Content (Join-Path $script:Root 'module\Winarchy\Private\Shell.ps1') -Raw
        $bridge = Get-Content (Join-Path $script:Root 'scripts\Stop-WinarchySessionShell.ps1') -Raw
        $ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw
        $quit = [regex]::Match($ahk, '(?ms)^QuitStack\(\) \{.*?^\}').Value

        InModuleScope Winarchy { $script:ShellReturnWaitMs | Should -Be 25000 }
        $shell | Should -Match ([regex]::Escape('$waitMs = if ($command -eq ''return'' -and $sent) { $script:ShellReturnWaitMs } else { $script:ShellQuitWaitMs }'))
        $bridge | Should -Match 'if \(-not \(Stop-WinarchyShell\)\) \{ exit 1 \}'
        $quit | Should -Match 'RunShellRecoveryBridge\(\)'
        $quit | Should -Not -Match "ShellIsland\('return'\)|ProcessWaitClose\(shellPid, 25\)|WaitExplorerShellReady"
    }
}

Describe 'Super+drag in a floating workspace' {
    BeforeAll { $script:Ahk = Get-Content (Join-Path $script:Root 'config\ahk\winarchy.ahk') -Raw }

    It 'move and resize leave the window where it was dropped when nothing is tiled' {
        $fn = [regex]::Match($script:Ahk, '(?s)DragIsFree\(hwnd\) \{.*?\n\}')
        $fn.Success | Should -BeTrue
        $fn.Value | Should -Match '!ws\.tile'
        $fn.Value | Should -Match 'floating_windows'
        foreach ($name in 'DragMove', 'DragResize') {
            $body = [regex]::Match($script:Ahk, "(?s)$name\(hwnd, x0, y0, held\) \{.*?\n\}").Value
            $body | Should -Match '(?s)timeEndPeriod.*?if DragIsFree\(hwnd\)\s+return' -Because "$name no debe restaurar la posición en un workspace flotante"
        }
    }

    It 'only the tiled path snaps the window back before the swap' {
        $move = [regex]::Match($script:Ahk, '(?s)DragMove\(hwnd, x0, y0, held\) \{.*?\n\}').Value
        $move.IndexOf('DragIsFree') | Should -BeLessThan $move.IndexOf("'int', wx, 'int', wy, 'int', 0")
    }
}

Describe 'Layout apply report' {
    It 'does not claim success when komorebi.json was not written' {
        InModuleScope Winarchy {
            Mock Import-WinarchyWindowPrefs { [pscustomobject]@{ Exe = 'a.exe'; Monitor = 'm' } }
            Mock Import-WinarchyWorkspaceLayouts { }
            Mock Test-WinarchyProcess { $true }
            Mock Get-WinarchyKomorebiState { @{} }
            Mock Get-WinarchyMonitorIndexMap { @{ m = 0 } }
            Mock Update-WinarchyKomorebiRules { $false }
            Mock Write-WinarchyOk { }
            Invoke-WinarchyLayoutApply
            Should -Invoke Update-WinarchyKomorebiRules -Times 1 -Exactly
            Should -Invoke Write-WinarchyOk -Times 0 -Exactly
        }
    }
}

Describe 'Dynamic accent marker' {
    It 'stays pending while komorebi.json cannot be published' {
        InModuleScope Winarchy -Parameters @{ Root = $script:Root; Work = (Join-Path $TestDrive 'accent') } {
            param($Root, $Work)
            New-Item -ItemType Directory -Path (Join-Path $Work 'state') -Force | Out-Null
            foreach ($dir in 'themes', 'templates') { Copy-Item (Join-Path $Root $dir) (Join-Path $Work $dir) -Recurse }
            $oldProfile = $env:USERPROFILE
            try {
                $env:USERPROFILE = $Work
                Mock Get-WinarchyRoot { $Work }
                Mock Get-WinarchyStateDir { Join-Path $Work 'state' }
                Mock Get-WinarchyThemesDir { Join-Path $Work 'themes' }
                Mock Set-WinarchyThemeLiveAccent { '#123456' }
                Mock Add-WinarchyAppRulesToKomorebiJson { $Json }
                Mock Add-WinarchyWindowRulesToKomorebiJson {}
                Mock Assert-WinarchyWeztermConfig {}
                Mock Get-WinarchyTerminalSettingsPath {}
                Mock Merge-WinarchyTerminalScheme {}
                Mock Set-WinarchyFlowTheme {}
                Mock Invoke-WinarchyReload {}
                Mock Update-WinarchyLockScreen {}
                Set-WinarchyTheme -Name 'auto-accent' -LightRefresh
                Get-Content (Join-Path $Work 'state\dynamic-accent') -Raw | Should -Be 'pending'
                Should -Invoke Invoke-WinarchyReload -Times 1 -Exactly
            }
            finally { $env:USERPROFILE = $oldProfile }
        }
    }
}
