@{
    RootModule        = 'Winarchy.psm1'
    ModuleVersion     = '1.0.3'
    GUID              = 'b7e3a9c4-2f5d-4e8a-9c1b-6d0f4a7e2b58'
    Author            = 'Guido Naselli'
    Description       = 'Winarchy — Omarchy-style integration layer for Windows 11 (komorebi + YASB + AHK + Flow Launcher + theme engine).'
    PowerShellVersion = '7.0'
    FunctionsToExport = @(
        'Invoke-Winarchy',
        'Set-WinarchyTheme', 'Set-WinarchyNextTheme', 'Add-WinarchyTheme', 'Get-WinarchyThemes', 'Get-WinarchyCurrentTheme',
        'Update-WinarchyAccent', 'Get-WinarchyAccentStatus',
        'Get-WinarchyBackgrounds', 'Get-WinarchyBackground', 'Set-WinarchyBackground',
        'Get-WinarchyBackgroundStatus',
        'Get-WinarchyBarPosition', 'Set-WinarchyBarPosition',
        'Test-WinarchyBarTransparent', 'Set-WinarchyBarTransparent', 'Get-WinarchyBarBackground',
        'Get-WinarchyCodingAgents', 'Get-WinarchyCodingAgent', 'Set-WinarchyCodingAgent',
        'Start-WinarchyCodingAgent', 'Get-WinarchyCodingAgentStatus',
        'New-WinarchyThemePreview', 'Update-WinarchyThemePreviews', 'Show-WinarchyThemeGallery',
        'Invoke-WinarchyUpdate', 'Invoke-WinarchyDoctor', 'Invoke-WinarchyReload',
        'Invoke-WinarchySelfUpdate', 'Get-WinarchyVersion', 'Get-WinarchyLatestRelease',
        'Test-WinarchyUpdateAvailable', 'Write-WinarchyUpdateNotice', 'Test-WinarchyGitCheckout',
        'Enable-WinarchyGameMode', 'Disable-WinarchyGameMode', 'Get-WinarchyGameModeStatus',
        'Add-WinarchyGame', 'Remove-WinarchyGame',
        'Save-WinarchyWindowLayout', 'Invoke-WinarchyLayoutApply', 'Get-WinarchyWindowLayout',
        'Remove-WinarchyWindowPref',
        'Install-WinarchyWebapp', 'Remove-WinarchyWebapp', 'Get-WinarchyWebapps',
        'Invoke-WinarchyScreenshot', 'Show-WinarchyMenu',
        'Install-WinarchyShellProfile', 'Remove-WinarchyShellProfile',
        'Set-WinarchyFlowIdentity', 'Set-WinarchyFlowAppsKeyword',
        # helpers usados por install.ps1 y scripts\*.ps1
        'Sync-WinarchyPalette', 'Remove-WinarchyPalette',
        'New-WinarchySnapshot', 'Import-WinarchyToml', 'Get-WinarchyAhkExe', 'Get-WinarchyStateDir',
        'Disable-WinarchyXMouse',
        'Register-WinarchyAutostart', 'Unregister-WinarchyAutostart', 'Get-WinarchyAutostartStatus',
        'Write-WinarchyInfo', 'Write-WinarchyOk', 'Write-WinarchyWarn', 'Write-WinarchyErr'
    )
    CmdletsToExport   = @()
    VariablesToExport = @()
    AliasesToExport   = @()
}
