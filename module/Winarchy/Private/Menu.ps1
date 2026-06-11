# Menu.ps1 — `winarchy menu`: menú navegable estilo omarchy-menu (consola).
# Se invoca desde AHK con SUPER+Shift+Space (abre en Windows Terminal).
# Nota: la integración como plugin de Flow quedó como evolución futura (open question del design).

function Show-WinarchyMenu {
    while ($true) {
        Clear-Host
        Write-Host ''
        Write-Host '  ██ WINARCHY' -ForegroundColor Cyan
        Write-Host "  theme actual: $((Get-WinarchyCurrentTheme) ?? '(ninguno)')"
        Write-Host ''
        Write-Host '  [1] Cambiar theme'
        Write-Host '  [2] Captura (región / ventana / pantalla)'
        Write-Host '  [3] Game-mode (toggle)'
        Write-Host '  [4] Update (winget + scoop)'
        Write-Host '  [5] Webapps'
        Write-Host '  [6] Keybindings'
        Write-Host '  [7] Doctor'
        Write-Host '  [8] Reload stack'
        Write-Host '  [Q] Salir'
        Write-Host ''
        $choice = Read-Host '  Opción'
        switch ($choice.ToUpper()) {
            '1' {
                $themes = @(Get-WinarchyThemes)
                for ($i = 0; $i -lt $themes.Count; $i++) { Write-Host "    [$($i + 1)] $($themes[$i])" }
                $sel = Read-Host '  Theme'
                if ($sel -match '^\d+$' -and [int]$sel -ge 1 -and [int]$sel -le $themes.Count) {
                    Set-WinarchyTheme -Name $themes[[int]$sel - 1]
                    Read-Host '  (Enter para continuar)'
                }
            }
            '2' {
                $kind = Read-Host '  [r]egión / [v]entana / [p]antalla'
                switch ($kind.ToLower()) {
                    'r' { Invoke-WinarchyScreenshot -Kind region }
                    'v' { Invoke-WinarchyScreenshot -Kind window }
                    'p' { Invoke-WinarchyScreenshot -Kind full }
                }
            }
            '3' {
                if (Test-Path (Get-WinarchyGameModeFlag)) { Disable-WinarchyGameMode } else { Enable-WinarchyGameMode }
                Read-Host '  (Enter para continuar)'
            }
            '4' { Invoke-WinarchyUpdate; Read-Host '  (Enter para continuar)' }
            '5' {
                Get-WinarchyWebapps
                Write-Host '    (alta: winarchy webapp install <nombre> <url>)'
                Read-Host '  (Enter para continuar)'
            }
            '6' {
                Get-Content (Join-Path (Get-WinarchyRoot) 'docs\keybindings.md') | Write-Host
                Read-Host '  (Enter para continuar)'
            }
            '7' { Invoke-WinarchyDoctor | Out-Null; Read-Host '  (Enter para continuar)' }
            '8' { Invoke-WinarchyReload; Write-WinarchyOk 'Stack recargado.'; Start-Sleep 1 }
            'Q' { return }
        }
    }
}
