# CommandPalette.ps1 — comandos de Winarchy como accesos directos que Flow indexa

$script:PaletteCommands = @(
    @{ Name = 'Next theme';        Args = 'theme next' }
    @{ Name = 'Theme gallery';     Args = 'theme gallery' }
    @{ Name = 'Next background';   Args = 'background next' }
    @{ Name = 'Coding agent';      Args = 'agent launch' }
    @{ Name = 'Menu';              Args = 'menu' }
    @{ Name = 'Reload stack';      Args = 'reload' }
    @{ Name = 'Doctor';            Args = 'doctor' }
    @{ Name = 'Game mode on';      Args = 'game-mode on' }
    @{ Name = 'Game mode off';     Args = 'game-mode off' }
    @{ Name = 'Save layout';       Args = 'layout save' }
    @{ Name = 'Apply layout';      Args = 'layout apply' }
    @{ Name = 'Order windows';     Args = 'layout order' }
    @{ Name = 'Sync accent';       Args = 'accent sync' }
    @{ Name = 'Screenshot region'; Args = 'screenshot region' }
    @{ Name = 'Screenshot window'; Args = 'screenshot window' }
    @{ Name = 'Update';            Args = 'update' }
    @{ Name = 'Update Winarchy';   Args = 'update --self' }
)

function Get-WinarchyPaletteDir {
    $dir = Join-Path ([Environment]::GetFolderPath('StartMenu')) 'Programs\Winarchy'
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
    $dir
}

function Sync-WinarchyPalette {
    <# Regenera los accesos directos de la paleta y borra los que sobran. #>
    $dir = Get-WinarchyPaletteDir
    $shim = Join-Path (Get-WinarchyRoot) 'bin\winarchy.ps1'
    $pwsh = (Get-Command pwsh -ErrorAction SilentlyContinue)?.Source ?? (Get-Command powershell).Source
    $shell = New-Object -ComObject WScript.Shell

    $expected = foreach ($cmd in $script:PaletteCommands) {
        $file = "Winarchy $($cmd.Name).lnk"
        $lnk = $shell.CreateShortcut((Join-Path $dir $file))
        $lnk.TargetPath = $pwsh
        $lnk.Arguments = "-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File `"$shim`" $($cmd.Args)"
        $lnk.Description = "winarchy $($cmd.Args)"
        $lnk.WindowStyle = 7
        $lnk.Save()
        $file
    }

    Get-ChildItem $dir -Filter '*.lnk' -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -notin $expected } |
        Remove-Item -Force

    Write-WinarchyOk "Command palette: $($expected.Count) commands reachable from the launcher."
}

function Remove-WinarchyPalette {
    $dir = Join-Path ([Environment]::GetFolderPath('StartMenu')) 'Programs\Winarchy'
    if (Test-Path $dir) { Remove-Item $dir -Recurse -Force }
}
