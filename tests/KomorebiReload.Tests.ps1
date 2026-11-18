#Requires -Modules Pester

BeforeAll {
    $script:SourceRoot = Split-Path $PSScriptRoot -Parent
    $script:FixtureRoot = Join-Path $TestDrive 'Winarchy with spaces'
    New-Item -ItemType Directory -Path "$script:FixtureRoot/module", "$script:FixtureRoot/templates", "$script:FixtureRoot/themes/tokyo-night", "$script:FixtureRoot/config/komorebi", "$script:FixtureRoot/state" -Force | Out-Null
    Copy-Item "$script:SourceRoot/module/Winarchy" "$script:FixtureRoot/module" -Recurse
    Copy-Item "$script:SourceRoot/templates/komorebi.json.tpl" "$script:FixtureRoot/templates"
    Copy-Item "$script:SourceRoot/themes/tokyo-night/theme.toml" "$script:FixtureRoot/themes/tokyo-night"
    Set-Content "$script:FixtureRoot/state/current-theme" 'tokyo-night'
    Set-Content "$script:FixtureRoot/state/bar" 'native'
    $script:EnvBefore = @{ K = $env:KOMOREBI_CONFIG_HOME; Y = $env:YASB_CONFIG_HOME; Local = $env:LOCALAPPDATA }
    $env:LOCALAPPDATA = $script:FixtureRoot
    Import-Module "$script:FixtureRoot/module/Winarchy/Winarchy.psd1" -Force
}

AfterAll {
    Remove-Module Winarchy -Force
    $env:KOMOREBI_CONFIG_HOME = $script:EnvBefore.K
    $env:YASB_CONFIG_HOME = $script:EnvBefore.Y
    $env:LOCALAPPDATA = $script:EnvBefore.Local
    Import-Module "$script:SourceRoot/module/Winarchy/Winarchy.psd1" -Force
}

Describe 'Komorebi configuration replacement' {
    BeforeEach {
        InModuleScope Winarchy {
            if (-not (Get-Command komorebic -ErrorAction SilentlyContinue)) { function komorebic { } }
            Mock komorebic { }
            Mock Get-Process { [pscustomobject]@{ ProcessName = 'komorebi' } } -ParameterFilter { $Name -eq 'komorebi' }
            Mock Stop-Process { }
            Mock Start-Process { }
        }
    }

    It 'light reload replaces the running configuration with the generated JSON path' {
        InModuleScope Winarchy {
            Invoke-WinarchyReload -Light
            Should -Invoke komorebic -Times 1 -Exactly -ParameterFilter {
                $args[0] -eq 'replace-configuration' -and
                $args[1] -eq (Join-Path $TestDrive 'Winarchy with spaces/config/komorebi/komorebi.json') -and
                $args.Count -eq 2
            }
            Should -Invoke komorebic -Times 0 -ParameterFilter { $args[0] -eq 'reload-configuration' }
        }
    }

    It 'registering a game replaces komorebi configuration after generating its exclusion' {
        InModuleScope Winarchy {
            Add-WinarchyGame -Exe 'Reload Test Game.exe'
            $path = Join-Path $TestDrive 'Winarchy with spaces/config/komorebi/komorebi.json'
            $json = Get-Content $path -Raw | ConvertFrom-Json
            @($json.ignore_rules | Where-Object { $_.id -eq 'Reload Test Game.exe' }).Count | Should -Be 1
            Should -Invoke komorebic -Times 1 -Exactly -ParameterFilter {
                $args[0] -eq 'replace-configuration' -and
                $args[1] -eq (Join-Path $TestDrive 'Winarchy with spaces/config/komorebi/komorebi.json') -and
                $args.Count -eq 2
            }
            Should -Invoke komorebic -Times 0 -ParameterFilter { $args[0] -eq 'reload-configuration' }
        }
    }
}
