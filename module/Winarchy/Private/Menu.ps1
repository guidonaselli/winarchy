# Menu.ps1 — `winarchy menu`: menú navegable estilo omarchy-menu (consola).
# Invocable solo desde la CLI (`winarchy menu`, abre en Windows Terminal); el atajo
# global SUPER+Alt+Space abre en cambio el menú nativo de AHK (A_TrayMenu, popup-style).
# Nota: la integración como plugin de Flow quedó como evolución futura (open question del design).

function Show-WinarchyMenu {
    while ($true) {
        Clear-Host
        Write-Host ''
        Write-Host '  ██ WINARCHY' -ForegroundColor Cyan
        Write-Host "  current theme: $((Get-WinarchyCurrentTheme) ?? '(none)')"
        Write-Host ''
        Write-Host '  [1] Switch theme'
        Write-Host '  [2] Screenshot (region / window / screen)'
        Write-Host '  [3] Game-mode (toggle)'
        Write-Host '  [4] Update (winget + scoop)'
        Write-Host '  [5] Webapps'
        Write-Host '  [6] Keybindings'
        Write-Host '  [7] Doctor'
        Write-Host '  [8] Reload stack'
        Write-Host '  [Q] Quit'
        Write-Host ''
        $choice = Read-Host '  Option'
        switch ($choice.ToUpper()) {
            '1' {
                $themes = @(Get-WinarchyThemes)
                for ($i = 0; $i -lt $themes.Count; $i++) { Write-Host "    [$($i + 1)] $($themes[$i])" }
                $sel = Read-Host '  Theme'
                if ($sel -match '^\d+$' -and [int]$sel -ge 1 -and [int]$sel -le $themes.Count) {
                    Set-WinarchyTheme -Name $themes[[int]$sel - 1]
                    Read-Host '  (Enter to continue)'
                }
            }
            '2' {
                $kind = Read-Host '  [r]egion / [w]indow / [s]creen'
                switch ($kind.ToLower()) {
                    'r' { Invoke-WinarchyScreenshot -Kind region }
                    'w' { Invoke-WinarchyScreenshot -Kind window }
                    's' { Invoke-WinarchyScreenshot -Kind full }
                }
            }
            '3' {
                if (Test-Path (Get-WinarchyGameModeFlag)) { Disable-WinarchyGameMode } else { Enable-WinarchyGameMode }
                Read-Host '  (Enter to continue)'
            }
            '4' { Invoke-WinarchyUpdate; Read-Host '  (Enter to continue)' }
            '5' {
                Get-WinarchyWebapps
                Write-Host '    (add: winarchy webapp install <name> <url>)'
                Read-Host '  (Enter to continue)'
            }
            '6' {
                Get-Content (Join-Path (Get-WinarchyRoot) 'docs\keybindings.md') | Write-Host
                Read-Host '  (Enter to continue)'
            }
            '7' { Invoke-WinarchyDoctor | Out-Null; Read-Host '  (Enter to continue)' }
            '8' { Invoke-WinarchyReload; Write-WinarchyOk 'Stack reloaded.'; Start-Sleep 1 }
            'Q' { return }
        }
    }
}
