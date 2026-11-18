# CommandPalette.ps1 — comandos de Winarchy como accesos directos que Flow indexa

$script:PaletteCommands = @(
    @{ Name = 'Next theme';        NameEs = 'Siguiente theme';             Args = 'theme next' }
    @{ Name = 'Theme gallery';     NameEs = 'Galería de temas';            Args = 'theme gallery' }
    @{ Name = 'Wallpaper gallery'; NameEs = 'Galería de fondos';           Args = 'background gallery' }
    @{ Name = 'Next background';   NameEs = 'Siguiente fondo';             Args = 'background next' }
    @{ Name = 'Coding agent';      NameEs = 'Agente de código';            Args = 'agent launch' }
    @{ Name = 'Menu';              NameEs = 'Menú Winarchy';               Args = 'menu' }
    @{ Name = 'Reload stack';      NameEs = 'Recargar entorno';            Args = 'reload' }
    @{ Name = 'Doctor';            NameEs = 'Diagnóstico';                  Args = 'doctor' }
    @{ Name = 'Game mode on';      NameEs = 'Activar modo de juego';        Args = 'game-mode on' }
    @{ Name = 'Game mode off';     NameEs = 'Desactivar modo de juego';     Args = 'game-mode off' }
    @{ Name = 'Save layout';       NameEs = 'Guardar distribución';        Args = 'layout save' }
    @{ Name = 'Apply layout';      NameEs = 'Aplicar distribución';         Args = 'layout apply' }
    @{ Name = 'Order windows';     NameEs = 'Ordenar ventanas';             Args = 'layout order' }
    @{ Name = 'Sync accent';       NameEs = 'Sincronizar accent';           Args = 'accent sync' }
    @{ Name = 'Screenshot region'; NameEs = 'Capturar región';              Args = 'screenshot region' }
    @{ Name = 'Screenshot window'; NameEs = 'Capturar ventana';             Args = 'screenshot window' }
    @{ Name = 'Screen recording';  NameEs = 'Grabar pantalla';              Args = 'screenshot record' }
    @{ Name = 'Stop recording';    NameEs = 'Detener grabación';            Args = 'screenshot stop' }
    @{ Name = 'Text from screen';  NameEs = 'Leer texto de pantalla';       Args = 'screenshot ocr' }
    @{ Name = 'Bar top';           NameEs = 'Barra arriba';                 Args = 'bar position top' }
    @{ Name = 'Bar bottom';        NameEs = 'Barra abajo';                  Args = 'bar position bottom' }
    @{ Name = 'Bar transparent';   NameEs = 'Alternar transparencia';       Args = 'bar transparent' }
    @{ Name = 'Update';            NameEs = 'Actualizar aplicaciones';      Args = 'update' }
    @{ Name = 'Update Winarchy';   NameEs = 'Actualizar Winarchy';          Args = 'update --self' }
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
    $icon = Join-Path (Get-WinarchyRoot) 'assets\logo\winarchy.ico'
    $pwsh = (Get-Command pwsh -ErrorAction SilentlyContinue)?.Source ?? (Get-Command powershell).Source
    $shell = New-Object -ComObject WScript.Shell

    $language = Get-WinarchyUiLanguage
    $expected = foreach ($cmd in $script:PaletteCommands) {
        $name = if ($language -eq 'es') { $cmd.NameEs } else { $cmd.Name }
        $file = "Winarchy $name.lnk"
        $lnk = $shell.CreateShortcut((Join-Path $dir $file))
        $lnk.TargetPath = $pwsh
        $lnk.Arguments = "-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File `"$shim`" $($cmd.Args)"
        $lnk.Description = "winarchy $($cmd.Args)"
        $lnk.WindowStyle = 7
        $lnk.IconLocation = "$icon,0"
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
