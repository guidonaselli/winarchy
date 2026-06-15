# Identity.ps1 — identidad unificada del stack: que Winarchy se sienta "un solo
# producto". Apaga el tray icon propio y el auto-update de los componentes de
# terceros para que la única bandeja/canal de updates sea el de Winarchy.
#   - YASB: se configura por su config.yaml (estático), fuera de este archivo.
#   - Flow Launcher: merge quirúrgico de Settings.json (acá abajo).
#   - AHK: su tray ES el de Winarchy (winarchy.ahk). komorebi no tiene tray.

function Get-WinarchyFlowSettingsPath {
    <# Settings.json de Flow Launcher (perfil del usuario). #>
    Join-Path "$env:APPDATA\FlowLauncher" 'Settings\Settings.json'
}

function Set-WinarchyFlowIdentity {
    <# Merge quirúrgico en Settings.json de Flow: oculta su tray icon y desactiva
       sus auto-updates/notificaciones. Solo toca las claves necesarias, preserva
       el resto del archivo del usuario. Idempotente y con snapshot previo. #>
    $settingsPath = Get-WinarchyFlowSettingsPath
    if (-not (Test-Path $settingsPath)) {
        Write-WinarchyWarn 'Flow Launcher Settings.json not found; identity toggles not applied.'
        return
    }

    # Claves de identidad unificada (confirmadas en research): tray off + updates off.
    $desired = @{
        HideNotifyIcon      = $true
        AutoUpdates         = $false
        AutoUpdatePlugins   = $false
        DontPromptUpdateMsg = $true
    }

    $settings = Get-Content $settingsPath -Raw -Encoding UTF8 | ConvertFrom-Json -AsHashtable

    $needsChange = $false
    foreach ($k in $desired.Keys) {
        if (-not $settings.ContainsKey($k) -or $settings[$k] -ne $desired[$k]) {
            $needsChange = $true
            break
        }
    }
    if (-not $needsChange) {
        Write-WinarchyOk "Flow identity already applied: $settingsPath"
        return
    }

    New-WinarchySnapshot -Label 'flow-settings' -Path @($settingsPath) | Out-Null
    foreach ($k in $desired.Keys) { $settings[$k] = $desired[$k] }
    $settings | ConvertTo-Json -Depth 50 | Set-Content -Path $settingsPath -Encoding UTF8
    Write-WinarchyOk "Flow identity applied (tray + auto-update off): $settingsPath"
}
