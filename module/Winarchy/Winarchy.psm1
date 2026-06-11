# Winarchy.psm1 — módulo CLI: carga Private/ y expone el dispatcher `Invoke-Winarchy`

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

foreach ($file in Get-ChildItem -Path (Join-Path $PSScriptRoot 'Private') -Filter '*.ps1') {
    . $file.FullName
}

function Show-WinarchyHelp {
    Write-Host @'

  winarchy — capa de integración estilo Omarchy para Windows 11

  USO: winarchy <subcomando> [args]

    theme set <nombre>        Aplica un theme a todas las superficies
    theme next                Cicla al siguiente theme (alfabético)
    theme list                Lista themes instalados (* = activo)
    update [--core]           Actualiza winget+scoop; --core también komorebi/YASB
    menu                      Menú navegable (themes, capturas, game-mode, ...)
    webapp install <n> <url>  Crea webapp como ventana dedicada
    webapp remove <n>         Elimina una webapp
    webapp list               Lista webapps instaladas
    screenshot [region|window|full]   Captura vía ShareX (default: region)
    game-mode on|off|status   Modo juego manual (AHK también lo activa solo)
    game-mode add <exe>       Da de alta un juego (excluido de tiling + hotkeys)
    game-mode remove <exe>    Baja de un juego
    accent sync [--force]     Re-sincroniza el theme dinámico con el accent de Windows
    accent status             Muestra accent del sistema vs aplicado
    reload                    Recarga komorebi, YASB, AHK y Flow
    doctor                    Diagnóstico del stack (verde/rojo + fix sugerido)

'@
}

function Invoke-Winarchy {
    <# Dispatcher de la CLI `winarchy`. #>
    param([Parameter(ValueFromRemainingArguments)][string[]]$Arguments)

    $cmd = if ($Arguments -and $Arguments.Count -ge 1) { $Arguments[0].ToLower() } else { '' }
    # @(): la asignación desde un statement `if` desenrolla arrays de 1 elemento a escalar
    $rest = @(if ($Arguments -and $Arguments.Count -gt 1) { $Arguments[1..($Arguments.Count - 1)] } else { @() })

    if ($rest -contains '-?' -or $rest -contains '--help' -or $cmd -in @('', 'help', '-?', '--help')) {
        Show-WinarchyHelp
        return
    }

    switch ($cmd) {
        'theme' {
            $sub = if ($rest.Count -ge 1) { $rest[0].ToLower() } else { 'list' }
            switch ($sub) {
                'set' {
                    if ($rest.Count -lt 2) { throw 'Uso: winarchy theme set <nombre> [-StageOnly]' }
                    Set-WinarchyTheme -Name $rest[1] -StageOnly:($rest -contains '-StageOnly' -or $rest -contains '--stage-only')
                }
                'next' { Set-WinarchyNextTheme }
                'list' {
                    $current = Get-WinarchyCurrentTheme
                    foreach ($t in Get-WinarchyThemes) {
                        $mark = if ($t -eq $current) { '*' } else { ' ' }
                        Write-Host "  $mark $t"
                    }
                }
                default { throw "Subcomando desconocido: theme $sub (set|next|list)" }
            }
        }
        'update' { Invoke-WinarchyUpdate -Core:($rest -contains '--core' -or $rest -contains '-core') }
        'menu' { Show-WinarchyMenu }
        'webapp' {
            $sub = if ($rest.Count -ge 1) { $rest[0].ToLower() } else { 'list' }
            switch ($sub) {
                'install' {
                    if ($rest.Count -lt 3) { throw 'Uso: winarchy webapp install <nombre> <url>' }
                    Install-WinarchyWebapp -Name $rest[1] -Url $rest[2]
                }
                'remove' {
                    if ($rest.Count -lt 2) { throw 'Uso: winarchy webapp remove <nombre>' }
                    Remove-WinarchyWebapp -Name $rest[1]
                }
                'list' { Get-WinarchyWebapps }
                default { throw "Subcomando desconocido: webapp $sub (install|remove|list)" }
            }
        }
        'screenshot' {
            $kind = if ($rest.Count -ge 1) { $rest[0].ToLower() } else { 'region' }
            Invoke-WinarchyScreenshot -Kind $kind
        }
        'game-mode' {
            $sub = if ($rest.Count -ge 1) { $rest[0].ToLower() } else { 'status' }
            switch ($sub) {
                'on' { Enable-WinarchyGameMode }
                'off' { Disable-WinarchyGameMode }
                'status' { Get-WinarchyGameModeStatus }
                'add' {
                    if ($rest.Count -lt 2) { throw 'Uso: winarchy game-mode add <exe>' }
                    Add-WinarchyGame -Exe $rest[1]
                }
                'remove' {
                    if ($rest.Count -lt 2) { throw 'Uso: winarchy game-mode remove <exe>' }
                    Remove-WinarchyGame -Exe $rest[1]
                }
                default { throw "Subcomando desconocido: game-mode $sub (on|off|status|add|remove)" }
            }
        }
        'accent' {
            $sub = if ($rest.Count -ge 1) { $rest[0].ToLower() } else { 'status' }
            switch ($sub) {
                'sync' { Update-WinarchyAccent -Force:($rest -contains '--force' -or $rest -contains '-force') }
                'status' { Get-WinarchyAccentStatus }
                default { throw "Subcomando desconocido: accent $sub (sync|status)" }
            }
        }
        'reload' { Invoke-WinarchyReload; Write-WinarchyOk 'Stack recargado.' }
        'doctor' { exit (Invoke-WinarchyDoctor) }
        default {
            Write-WinarchyErr "Subcomando desconocido: $cmd"
            Show-WinarchyHelp
            exit 1
        }
    }
}
