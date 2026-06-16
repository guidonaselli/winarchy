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

function Set-WinarchyFlowAppsKeyword {
    <# Agrega "app" como ActionKeyword extra del plugin Program (additivo: no
       reemplaza "*", la búsqueda global de Flow sigue igual). Permite que el
       popup "Apps" (paridad con Walker de Omarchy) precargue "app " en el query
       box y quede scoped a solo programas instalados — sin curar una lista a
       mano, Flow ya los indexa. Idempotente y con snapshot previo. #>
    $settingsPath = Get-WinarchyFlowSettingsPath
    if (-not (Test-Path $settingsPath)) {
        Write-WinarchyWarn 'Flow Launcher Settings.json not found; Apps keyword not applied.'
        return
    }

    # ID fijo del plugin built-in "Program" (Flow.Launcher.Plugin.Program/plugin.json).
    $programPluginId = '791FC278BA414111B8D1886DFE447410'

    $settings = Get-Content $settingsPath -Raw -Encoding UTF8 | ConvertFrom-Json -AsHashtable
    $plugins = $settings['PluginSettings']['Plugins']
    if (-not $plugins -or -not $plugins.ContainsKey($programPluginId)) {
        Write-WinarchyWarn 'Flow Program plugin settings not found (¿corriste Flow al menos una vez?); Apps keyword not applied.'
        return
    }

    $program = $plugins[$programPluginId]
    $keywords = if ($null -eq $program['ActionKeywords']) { @() } else { @($program['ActionKeywords']) }
    if ($keywords -contains 'app') {
        Write-WinarchyOk "Flow Apps keyword already applied: $settingsPath"
        return
    }

    New-WinarchySnapshot -Label 'flow-settings' -Path @($settingsPath) | Out-Null
    $program['ActionKeywords'] = $keywords + 'app'
    $settings | ConvertTo-Json -Depth 50 | Set-Content -Path $settingsPath -Encoding UTF8
    Write-WinarchyOk "Flow Apps keyword applied (additive, 'app '): $settingsPath"
}
