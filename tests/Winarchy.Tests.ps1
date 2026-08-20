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
