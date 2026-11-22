#Requires -Modules Pester

BeforeAll {
    $script:Root = Split-Path $PSScriptRoot -Parent
    Import-Module (Join-Path $script:Root 'module\Winarchy\Winarchy.psd1') -Force
}

Describe 'Shell urgent theme rendering' {
    It 'preserves a readable urgent border color independently of accent' {
        Set-WinarchyTheme -Name 'tokyo-night' -StageOnly | Out-Null
        $theme = Get-Content (Join-Path (Get-WinarchyStateDir) 'staging\config\shell\theme.json') -Raw | ConvertFrom-Json
        $theme.urgent | Should -Be '#f7768e'
    }

    It 'lightens an urgent border color that is too close to the shell background' {
        Set-WinarchyTheme -Name 'solitude' -StageOnly | Out-Null
        $theme = Get-Content (Join-Path (Get-WinarchyStateDir) 'staging\config\shell\theme.json') -Raw | ConvertFrom-Json
        $theme.urgent | Should -Be '#676d70'
    }
}
