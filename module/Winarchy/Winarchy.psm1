# Winarchy.psm1 — módulo CLI: carga Private/ y expone el dispatcher `Invoke-Winarchy`

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

foreach ($file in Get-ChildItem -Path (Join-Path $PSScriptRoot 'Private') -Filter '*.ps1') {
    . $file.FullName
}

function Show-WinarchyHelp {
    Write-Host @'

  winarchy — Omarchy-style integration layer for Windows 11

  USAGE: winarchy <subcommand> [args]

    theme set <name>          Apply a theme to every surface
    theme next                Cycle to the next theme (alphabetical)
    theme list                List installed themes (* = active)
    theme preview [name]      Render synthetic preview cards (all themes by default)
    theme gallery             Visual theme picker (WPF grid)
    update [--core]           Update winget+scoop; --core also komorebi/YASB
                              (único canal de updates: YASB/Flow tienen su auto-update off)
    update --self             Update Winarchy itself (git pull release + migration)
    menu                      Navigable menu (themes, screenshots, game-mode, ...)
    webapp install <n> <url>  Create a webapp as a dedicated window
    webapp remove <n>         Remove a webapp
    webapp list               List installed webapps
    screenshot [region|window|full]   Capture via ShareX (default: region)
    game-mode on|off|status   Manual game mode (AHK also enables it on its own)
    game-mode add <exe>       Register a game (no tiling/hotkeys, excluded from Quick Accent)
    game-mode remove <exe>    Unregister a game
    accent sync [--force]     Re-sync the dynamic theme with the Windows accent
    accent status             Show system accent vs applied accent
    reload                    Reload komorebi, YASB, AHK and Flow
    doctor                    Stack diagnostics (green/red + suggested fix)

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
                    if ($rest.Count -lt 2) { throw 'Usage: winarchy theme set <name> [-StageOnly]' }
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
                'preview' {
                    if ($rest.Count -ge 2) { Update-WinarchyThemePreviews -Name $rest[1] }
                    else { Update-WinarchyThemePreviews }
                }
                'gallery' { Show-WinarchyThemeGallery }
                default { throw "Unknown subcommand: theme $sub (set|next|list|preview|gallery)" }
            }
        }
        'update' {
            Invoke-WinarchyUpdate `
                -Core:($rest -contains '--core' -or $rest -contains '-core') `
                -Self:($rest -contains '--self' -or $rest -contains '-self')
        }
        'menu' { Show-WinarchyMenu }
        'webapp' {
            $sub = if ($rest.Count -ge 1) { $rest[0].ToLower() } else { 'list' }
            switch ($sub) {
                'install' {
                    if ($rest.Count -lt 3) { throw 'Usage: winarchy webapp install <name> <url>' }
                    Install-WinarchyWebapp -Name $rest[1] -Url $rest[2]
                }
                'remove' {
                    if ($rest.Count -lt 2) { throw 'Usage: winarchy webapp remove <name>' }
                    Remove-WinarchyWebapp -Name $rest[1]
                }
                'list' { Get-WinarchyWebapps }
                default { throw "Unknown subcommand: webapp $sub (install|remove|list)" }
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
                    if ($rest.Count -lt 2) { throw 'Usage: winarchy game-mode add <exe>' }
                    Add-WinarchyGame -Exe $rest[1]
                }
                'remove' {
                    if ($rest.Count -lt 2) { throw 'Usage: winarchy game-mode remove <exe>' }
                    Remove-WinarchyGame -Exe $rest[1]
                }
                default { throw "Unknown subcommand: game-mode $sub (on|off|status|add|remove)" }
            }
        }
        'accent' {
            $sub = if ($rest.Count -ge 1) { $rest[0].ToLower() } else { 'status' }
            switch ($sub) {
                'sync' { Update-WinarchyAccent -Force:($rest -contains '--force' -or $rest -contains '-force') }
                'status' { Get-WinarchyAccentStatus }
                default { throw "Unknown subcommand: accent $sub (sync|status)" }
            }
        }
        'reload' { Invoke-WinarchyReload; Write-WinarchyOk 'Stack reloaded.' }
        'doctor' { exit (Invoke-WinarchyDoctor) }
        default {
            Write-WinarchyErr "Unknown subcommand: $cmd"
            Show-WinarchyHelp
            exit 1
        }
    }
}
