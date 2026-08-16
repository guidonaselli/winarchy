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
                [pscustomobject]@{ Exe = 'Discord.exe'; Monitor = 'BBB2222-5&b&1&UID1'; Workspace = 2; WorkspaceName = 'A'; Pin = $false }
                [pscustomobject]@{ Exe = 'Code.exe';    Monitor = 'AAA1111-5&a&1&UID2'; Workspace = 0; WorkspaceName = '1'; Pin = $true }
            )
            Export-WinarchyWindowPrefs -Pref $prefs
            $back = @(Import-WinarchyWindowPrefs)
            $back.Count | Should -Be 2
            $discord = $back | Where-Object { $_.Exe -eq 'Discord.exe' }
            $discord.Workspace | Should -Be 2
            $discord.Monitor   | Should -Be 'BBB2222-5&b&1&UID1'
            $discord.Pin       | Should -BeFalse
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
                [pscustomobject]@{ Exe = 'Discord.exe'; Monitor = 'M1'; Workspace = 0; WorkspaceName = 'A'; Pin = $false }
                [pscustomobject]@{ Exe = 'Code.exe';    Monitor = 'M2'; Workspace = 1; WorkspaceName = '1'; Pin = $false }
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
                [pscustomobject]@{ Exe = 'Discord.exe'; Monitor = 'BBB2222-5&bbbbbbb&1&UID4353'; Workspace = 0; WorkspaceName = 'A'; Pin = $false }
                [pscustomobject]@{ Exe = 'Code.exe';    Monitor = 'AAA1111-5&aaaaaaa&1&UID4355'; Workspace = 1; WorkspaceName = '2'; Pin = $true }
                [pscustomobject]@{ Exe = 'Ghost.exe';   Monitor = 'NOT-CONNECTED';               Workspace = 0; WorkspaceName = 'X'; Pin = $false }
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

    It 'excludes games and ignore-rule exes from capture' {
        InModuleScope Winarchy {
            $skip = Get-WinarchyUnplaceableExes
            $skip | Should -Contain 'Hearthstone.exe'
            $skip | Should -Contain 'Flow.Launcher.exe'
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
            $data = Import-WinarchyToml -Path (Join-Path (Get-WinarchyThemesDir) "$Name\theme.toml")
            (Test-WinarchyThemeData -Theme $data) -join ', ' | Should -BeNullOrEmpty
        }
    }

    # Cubre de punta a punta lo que antes solo se sabía al aplicar un theme: los 10
    # templates renderizan sin tokens sin resolver y el JSON/XML generado parsea.
    It '<Name> renders every template (theme set -StageOnly)' -ForEach $script:ThemeCases {
        { Set-WinarchyTheme -Name $Name -StageOnly } | Should -Not -Throw
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
