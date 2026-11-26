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
    Remove-Module Winarchy -Force -ErrorAction SilentlyContinue   # otros archivos de test dejan su copia cargada
    Import-Module "$script:FixtureRoot/module/Winarchy/Winarchy.psd1" -Force
}

AfterAll {
    Remove-Module Winarchy -Force
    $env:KOMOREBI_CONFIG_HOME = $script:EnvBefore.K
    $env:YASB_CONFIG_HOME = $script:EnvBefore.Y
    $env:LOCALAPPDATA = $script:EnvBefore.Local
    Import-Module "$script:SourceRoot/module/Winarchy/Winarchy.psd1" -Force
}

Describe 'Komorebi configuration reload' {
    BeforeEach {
        InModuleScope Winarchy {
            if (-not (Get-Command komorebic -ErrorAction SilentlyContinue)) { function komorebic { } }
            Mock komorebic { }
            Mock Get-Process { [pscustomobject]@{ ProcessName = 'komorebi' } } -ParameterFilter { $Name -eq 'komorebi' }
            Mock Get-WinarchyKomorebiState { '{"monitors":{"elements":[{"device_id":"AAA1","workspaces":{"elements":[{"layout":{"Default":"Columns"},"tile":true}]}}]}}' | ConvertFrom-Json }
            Mock Restore-WinarchyFloatingApps { }
            Mock Stop-Process { }
            Mock Stop-WinarchyShell { $true }
            Mock Start-Process { }
        }
    }

    It 'light reload rewrites the watched file with the live view and never rebuilds the window manager' {
        InModuleScope Winarchy {
            $path = Join-Path $TestDrive 'Winarchy with spaces/config/komorebi/komorebi.json'
            Set-Content $path '{"monitors":[{"workspaces":[{"name":"1","layout":"BSP"}]}]}'
            Invoke-WinarchyReload -Light
            (Get-Content $path -Raw | ConvertFrom-Json).monitors[0].workspaces[0].layout | Should -Be 'Columns'
            Should -Invoke komorebic -Times 0 -ParameterFilter { $args[0] -in 'reload-configuration', 'replace-configuration' }
        }
    }

    It 'leaves the config untouched and does not throw when komorebi gives no state' {
        InModuleScope Winarchy {
            Mock Get-WinarchyKomorebiState { throw 'no state' }
            Mock Write-WinarchyWarn { }
            $path = Join-Path $TestDrive 'unreadable.json'
            Set-Content $path '{"monitors":[]}' -NoNewline
            { Invoke-WinarchyKomorebiStaticReload -ConfigPath $path } | Should -Not -Throw
            Get-Content $path -Raw | Should -Be '{"monitors":[]}'
            Should -Invoke Write-WinarchyWarn -Times 1
        }
    }

    It 'registering a game publishes its exclusion without rebuilding the window manager' {
        InModuleScope Winarchy {
            Add-WinarchyGame -Exe 'Reload Test Game.exe'
            $path = Join-Path $TestDrive 'Winarchy with spaces/config/komorebi/komorebi.json'
            $json = Get-Content $path -Raw | ConvertFrom-Json
            @($json.ignore_rules | Where-Object { $_.id -eq 'Reload Test Game.exe' }).Count | Should -Be 1
            $json.monitors[0].workspaces[0].layout | Should -Be 'Columns'
            Should -Invoke komorebic -Times 0 -ParameterFilter { $args[0] -in 'reload-configuration', 'replace-configuration' }
        }
    }
}
