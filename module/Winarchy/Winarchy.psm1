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
    theme add <git-url> [name]  Install a theme from a repository
    background list           Backgrounds shipped by the active theme (* = applied)
    background set <file>     Apply one of them
    background next           Cycle to the next background
    bar position [top|bottom] Where the bar sits (no arg: show current)
    bar transparent [on|off]  See through the bar (no arg: toggle)
    agent list                Known coding agents (* = preferred, marks what is installed)
    agent set <id>            Pick the coding agent SUPER+Shift+Ctrl+A launches
    agent launch              Open it in Windows Terminal
    update [--core]           Update the packages Winarchy declares (+ scoop)
                              --core: the pinned ones (komorebi/YASB/Flow/AHK)
                              (único canal de updates: YASB/Flow tienen su auto-update off)
    update --self             Update Winarchy itself (git pull release + migration)
    menu                      Navigable menu (themes, screenshots, game-mode, ...)
    webapp install <n> <url>  Create a webapp as a dedicated window
    webapp remove <n>         Remove a webapp
    webapp list               List installed webapps
    screenshot [kind]         Capture via ShareX: region window full last scrolling
                              record record-gif stop ocr qr color ruler pin
    game-mode on|off|status   Manual game mode (AHK also enables it on its own)
    layout save [exe]         Remember where each app currently lives (monitor+workspace+slot)
                              (one exe if given; --dry-run to preview without saving)
    layout apply              Re-apply the saved placement rules to komorebi
    layout order              Move every window to its saved slot (tile position)
    layout list               Show saved placements (* = monitor not connected)
    layout forget <exe>       Drop a saved placement
    game-mode add <exe>       Register a game (no tiling/hotkeys, excluded from Quick Accent)
    game-mode remove <exe>    Unregister a game
    accent sync [--force]     Re-sync the dynamic theme with the Windows accent
    accent status             Show system accent vs applied accent
    rules add <cat> <field> <value> [strategy]
                              Append a rule to rules.toml and apply it immediately
                              (recompiles komorebi.json + reload, no theme set needed)
                              cat: ignore|manage|floating|layered|object_name_change|
                                   tray_and_multi_window|slow_application|
                                   transparency_ignore|disable
                              field: exe|class|title (asc for disable)
    rules explain <text>      Which app rule applies to a window, and which layer won
    rules collisions          ASC rules dropped because a higher layer decided otherwise
    rules sync [--apply]      Refresh the vendored community ASC (shows the diff first;
                              never touches your config/komorebi/rules.toml)
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
                'add' {
                    if ($rest.Count -lt 2) { throw 'Usage: winarchy theme add <git-url> [name]' }
                    Add-WinarchyTheme -Url $rest[1] -Name $(if ($rest.Count -ge 3) { $rest[2] } else { '' })
                }
                default { throw "Unknown subcommand: theme $sub (set|next|list|preview|gallery|add)" }
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
        'layout' {
            $sub = if ($rest.Count -ge 1) { $rest[0].ToLower() } else { 'list' }
            switch ($sub) {
                'save' {
                    $target = @($rest | Select-Object -Skip 1 | Where-Object { $_ -notlike '-*' }) | Select-Object -First 1
                    Save-WinarchyWindowLayout -Exe $target `
                        -WhatIf:($rest -contains '--dry-run' -or $rest -contains '-WhatIf')
                }
                'apply' { Invoke-WinarchyLayoutApply }
                'order' { $null = Invoke-WinarchySlotReconcile }
                'list' { Get-WinarchyWindowLayout }
                'forget' {
                    if ($rest.Count -lt 2) { throw 'Usage: winarchy layout forget <exe>' }
                    Remove-WinarchyWindowPref -Exe $rest[1]
                }
                default { throw "Unknown subcommand: layout $sub (save|apply|order|list|forget)" }
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
        'bar' {
            $sub = if ($rest.Count -ge 1) { $rest[0].ToLower() } else { 'position' }
            switch ($sub) {
                'position' {
                    if ($rest.Count -lt 2) { Write-Host "  bar position: $(Get-WinarchyBarPosition)"; break }
                    Set-WinarchyBarPosition -Position $rest[1].ToLower()
                }
                'transparent' {
                    $mode = if ($rest.Count -ge 2) { $rest[1].ToLower() } else { 'toggle' }
                    Set-WinarchyBarTransparent -Mode $mode
                }
                default { throw "Unknown subcommand: bar $sub (position|transparent)" }
            }
        }
        'agent' {
            $sub = if ($rest.Count -ge 1) { $rest[0].ToLower() } else { 'list' }
            switch ($sub) {
                'list' { Get-WinarchyCodingAgentStatus }
                'launch' { Start-WinarchyCodingAgent }
                'set' {
                    if ($rest.Count -lt 2) { throw 'Usage: winarchy agent set <id>' }
                    Set-WinarchyCodingAgent -Id $rest[1].ToLower()
                }
                default { throw "Unknown subcommand: agent $sub (list|set|launch)" }
            }
        }
        'background' {
            $sub = if ($rest.Count -ge 1) { $rest[0].ToLower() } else { 'list' }
            switch ($sub) {
                'list' { Get-WinarchyBackgroundStatus }
                'next' { Set-WinarchyBackground -Next }
                'set' {
                    if ($rest.Count -lt 2) { throw 'Usage: winarchy background set <file>' }
                    Set-WinarchyBackground -File $rest[1]
                }
                default { throw "Unknown subcommand: background $sub (list|set|next)" }
            }
        }
        'rules' {
            $sub = if ($rest.Count -ge 1) { $rest[0].ToLower() } else { 'collisions' }
            switch ($sub) {
                'add' {
                    if ($rest.Count -lt 4) { throw 'Usage: winarchy rules add <category> <exe|class|title|asc> <value> [matching_strategy]' }
                    Add-WinarchyUserRule -Category $rest[1].ToLower() -Field $rest[2].ToLower() -Value $rest[3] `
                        -MatchingStrategy $(if ($rest.Count -ge 5) { $rest[4] } else { '' })
                }
                'explain' {
                    if ($rest.Count -lt 2) { throw 'Usage: winarchy rules explain <exe|class|title fragment>' }
                    $report = @(Get-WinarchyAppRuleReport -Match $rest[1])
                    if ($report.Count -eq 0) { Write-WinarchyInfo "No app rule matches '$($rest[1])'."; break }
                    $report | Format-Table Layer, Category, Rule, Applied, Source -AutoSize
                }
                'collisions' {
                    $coll = @(Get-WinarchyAppRuleCollisions)
                    if ($coll.Count -eq 0) { Write-WinarchyOk 'No ASC rule is being overridden.'; break }
                    $coll | Format-Table App, Rule, AscSaid, Winner -AutoSize
                }
                'sync' { Sync-WinarchyAsc -Apply:($rest -contains '--apply' -or $rest -contains '-apply') }
                default { throw "Unknown subcommand: rules $sub (add|explain|collisions|sync)" }
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
