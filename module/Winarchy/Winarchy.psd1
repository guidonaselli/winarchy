@{
    RootModule        = 'Winarchy.psm1'
    ModuleVersion     = '0.1.0'
    GUID              = 'b7e3a9c4-2f5d-4e8a-9c1b-6d0f4a7e2b58'
    Author            = 'Guido Naselli'
    Description       = 'Winarchy — capa de integración estilo Omarchy para Windows 11 (komorebi + YASB + AHK + Flow Launcher + theme engine).'
    PowerShellVersion = '7.0'
    FunctionsToExport = @(
        'Invoke-Winarchy',
        'Set-WinarchyTheme', 'Set-WinarchyNextTheme', 'Get-WinarchyThemes', 'Get-WinarchyCurrentTheme',
        'Update-WinarchyAccent', 'Get-WinarchyAccentStatus',
        'Invoke-WinarchyUpdate', 'Invoke-WinarchyDoctor', 'Invoke-WinarchyReload',
        'Enable-WinarchyGameMode', 'Disable-WinarchyGameMode', 'Get-WinarchyGameModeStatus',
        'Add-WinarchyGame', 'Remove-WinarchyGame',
        'Install-WinarchyWebapp', 'Remove-WinarchyWebapp', 'Get-WinarchyWebapps',
        'Invoke-WinarchyScreenshot', 'Show-WinarchyMenu',
        # helpers usados por install.ps1 y scripts\*.ps1
        'New-WinarchySnapshot', 'Import-WinarchyToml', 'Get-WinarchyAhkExe', 'Get-WinarchyStateDir',
        'Disable-WinarchyXMouse',
        'Write-WinarchyInfo', 'Write-WinarchyOk', 'Write-WinarchyWarn', 'Write-WinarchyErr'
    )
    CmdletsToExport   = @()
    VariablesToExport = @()
    AliasesToExport   = @()
}
