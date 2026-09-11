# Identity.ps1 — identidad unificada del stack: que Winarchy se sienta "un solo
# producto". Apaga el tray icon propio y el auto-update de los componentes de
# terceros para que la única bandeja/canal de updates sea el de Winarchy.
#   - YASB: se configura por su config.yaml (generado desde templates/), fuera de este archivo.
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

# Plugin oficial Flow-Launcher/Flow.Launcher.Plugin.Everything: sin él, Flow busca
# archivos con su propio indexer (plugin "Explorer"), notablemente más lento y limitado
# a rutas configuradas manualmente en vez del índice NTFS/MFT de voidtools Everything.
$script:WinarchyFlowEverythingPluginId = 'D2D2C23B084D411DB66FE0C79D6C2A6E'
$script:WinarchyFlowEverythingPluginVersion = '1.7.7'
$script:WinarchyFlowEverythingPluginUrl = "https://github.com/Flow-Launcher/Flow.Launcher.Plugin.Everything/releases/download/v$script:WinarchyFlowEverythingPluginVersion/Flow.Launcher.Plugin.Everything.zip"

function Test-WinarchyFlowEverythingPluginInstalled {
    $pluginsDir = Join-Path "$env:APPDATA\FlowLauncher" 'Plugins'
    if (-not (Test-Path $pluginsDir)) { return $false }
    Get-ChildItem $pluginsDir -Directory -ErrorAction SilentlyContinue | Where-Object {
        $manifest = Join-Path $_.FullName 'plugin.json'
        (Test-Path $manifest) -and ((Get-Content $manifest -Raw | ConvertFrom-Json).ID -eq $script:WinarchyFlowEverythingPluginId)
    } | Select-Object -First 1 | ForEach-Object { $true }
}

function Install-WinarchyFlowEverythingPlugin {
    <# Descarga e instala el plugin Everything de Flow (release oficial de GitHub) si
       falta. Requiere que voidtools Everything esté instalado (lo trae install.ps1 vía
       winget); si no está, el plugin queda inerte, así que no vale la pena bajarlo.
       Idempotente: no reinstala si ya está presente. Flow lo detecta en su próximo
       arranque (escanea Plugins\ al iniciar; no requiere editar Settings.json). #>
    $everythingExe = @("$env:ProgramFiles\Everything\Everything.exe", "${env:ProgramFiles(x86)}\Everything\Everything.exe") |
        Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $everythingExe) {
        Write-WinarchyWarn 'voidtools Everything not installed; Flow Everything plugin not installed (winget install voidtools.Everything).'
        return
    }
    if (Test-WinarchyFlowEverythingPluginInstalled) {
        Write-WinarchyOk 'Flow Everything plugin already installed'
        return
    }
    $pluginsDir = Join-Path "$env:APPDATA\FlowLauncher" 'Plugins'
    if (-not (Test-Path $pluginsDir)) {
        Write-WinarchyWarn 'Flow Launcher Plugins dir not found (¿corriste Flow al menos una vez?); Everything plugin not installed.'
        return
    }
    $zipPath = Join-Path ([System.IO.Path]::GetTempPath()) 'Flow.Launcher.Plugin.Everything.zip'
    $destDir = Join-Path $pluginsDir "Everything-$script:WinarchyFlowEverythingPluginVersion"
    try {
        Invoke-WebRequest -Uri $script:WinarchyFlowEverythingPluginUrl -OutFile $zipPath -UseBasicParsing
        Expand-Archive -Path $zipPath -DestinationPath $destDir -Force
        Write-WinarchyOk "Flow Everything plugin installed: $destDir (restart Flow to load it)"
    }
    catch {
        Write-WinarchyWarn "No pude instalar el plugin Everything de Flow: $($_.Exception.Message)"
    }
    finally {
        Remove-Item $zipPath -ErrorAction SilentlyContinue
    }
}

function Set-WinarchyDefenderExclusions {
    <# Exclusiones de Windows Defender para procesos/carpetas del stack que escriben
       archivos seguido (ShareX en cada captura, Everything indexando el volumen):
       el escaneo en tiempo real de esos I/O es la causa más probable de lag esporádico
       no reproducible por config. Requiere shell elevado; si no lo está, no falla,
       solo avisa (mismo patrón que el resto de install.ps1 con operaciones de sistema). #>
    if (-not ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        Write-WinarchyWarn 'Defender exclusions need an elevated shell; run "winarchy doctor" from an admin PowerShell to apply them.'
        return
    }
    $paths = @(
        "$env:ProgramFiles\ShareX\ShareX.exe",
        "${env:ProgramFiles(x86)}\ShareX\ShareX.exe",
        "$env:USERPROFILE\Documents\ShareX",
        "$env:ProgramFiles\Everything\Everything.exe",
        "${env:ProgramFiles(x86)}\Everything\Everything.exe"
    ) | Where-Object { Test-Path $_ }
    if ($paths.Count -eq 0) {
        Write-WinarchyOk 'No ShareX/Everything paths found to exclude yet'
        return
    }
    $current = @((Get-MpPreference).ExclusionPath)
    $missing = @($paths | Where-Object { $current -notcontains $_ })
    if ($missing.Count -eq 0) {
        Write-WinarchyOk 'Defender exclusions already applied (ShareX, Everything)'
        return
    }
    try {
        Add-MpPreference -ExclusionPath $missing
        Write-WinarchyOk "Defender exclusions added: $($missing -join ', ')"
    }
    catch {
        Write-WinarchyWarn "No pude agregar exclusiones de Defender: $($_.Exception.Message)"
    }
}
