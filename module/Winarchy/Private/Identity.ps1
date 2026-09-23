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
    Save-WinarchyFlowSettings -Files @{ $settingsPath = $settings }
    Write-WinarchyOk "Flow identity applied (tray + auto-update off): $settingsPath"
}

function Save-WinarchyFlowSettings {
    <# Escribe archivos de settings de Flow con Flow detenido (sin dejarlo guardar su estado
       en memoria) y lo relanza si estaba corriendo. $Files: path -> hashtable. #>
    param([hashtable]$Files = @{}, [scriptblock]$WhileStopped)
    $flow = Get-Process -Name 'Flow.Launcher' -ErrorAction SilentlyContinue
    if ($flow) {
        $flow | Stop-Process -Force
        $flow | Wait-Process -Timeout 5 -ErrorAction SilentlyContinue
    }
    foreach ($path in $Files.Keys) {
        $Files[$path] | ConvertTo-Json -Depth 50 | Set-Content -Path $path -Encoding UTF8
    }
    if ($WhileStopped) { & $WhileStopped }
    $exe = "$env:LOCALAPPDATA\FlowLauncher\Flow.Launcher.exe"
    if ($flow -and (Test-Path $exe)) { Start-Process $exe }
}

$script:WinarchyFlowProgramPluginId = '791FC278BA414111B8D1886DFE447410'
$script:WinarchyFlowExplorerPluginId = '572be03c74c642baae319fc283e561a8'
# Plugin Everything standalone (archivado, incompatible con Flow 2.x): lo reemplaza Explorer.
$script:WinarchyFlowLegacyEverythingPluginId = 'D2D2C23B084D411DB66FE0C79D6C2A6E'

function Add-WinarchyFlowKeyword {
    <# Agrega $Keyword a los ActionKeywords del plugin; true si cambió. #>
    param([hashtable]$Plugins, [string]$PluginId, [string]$Keyword)
    if (-not $Plugins -or -not $Plugins.ContainsKey($PluginId)) { return $false }
    $keywords = @($Plugins[$PluginId]['ActionKeywords'] | Where-Object { $_ })
    if ($keywords -contains $Keyword) { return $false }
    $Plugins[$PluginId]['ActionKeywords'] = @($keywords) + $Keyword
    $true
}

function Set-WinarchyFlowKeywords {
    <# Keywords scoped de winarchy.ahk, aditivos ("*" sigue siendo la búsqueda global):
       "app " → plugin Program, "f " → búsqueda de archivos de Explorer sobre Everything.
       Quita el plugin Everything legacy. Idempotente y con snapshot previo. #>
    $settingsPath = Get-WinarchyFlowSettingsPath
    $explorerPath = Join-Path "$env:APPDATA\FlowLauncher" 'Settings\Plugins\Flow.Launcher.Plugin.Explorer\Settings.json'
    if (-not (Test-Path $settingsPath)) {
        Write-WinarchyWarn 'Flow Launcher Settings.json not found; scoped keywords not applied.'
        return
    }

    $settings = Get-Content $settingsPath -Raw -Encoding UTF8 | ConvertFrom-Json -AsHashtable
    $plugins = $settings['PluginSettings']['Plugins']
    $files = @{}
    $changes = @()
    if (Add-WinarchyFlowKeyword $plugins $script:WinarchyFlowProgramPluginId 'app') { $changes += "Program 'app'" }
    if (Add-WinarchyFlowKeyword $plugins $script:WinarchyFlowExplorerPluginId 'f') { $changes += "Explorer 'f'" }
    if ($plugins -and $plugins.ContainsKey($script:WinarchyFlowLegacyEverythingPluginId)) {
        $plugins.Remove($script:WinarchyFlowLegacyEverythingPluginId)
        $changes += 'legacy Everything plugin removed'
    }
    if ($changes) { $files[$settingsPath] = $settings }

    if (Test-Path $explorerPath) {
        $explorer = Get-Content $explorerPath -Raw -Encoding UTF8 | ConvertFrom-Json -AsHashtable
        $desired = @{ FileSearchActionKeyword = 'f'; FileSearchKeywordEnabled = $true }
        if (Test-Path "$env:ProgramFiles\Everything\Everything.exe") { $desired['IndexSearchEngine'] = 1 }
        $explorerChanged = $false
        foreach ($k in $desired.Keys) {
            if ($explorer[$k] -ne $desired[$k]) { $explorer[$k] = $desired[$k]; $explorerChanged = $true }
        }
        if ($explorerChanged) {
            $files[$explorerPath] = $explorer
            $changes += 'Explorer file search'
        }
    }
    else {
        Write-WinarchyWarn 'Flow Explorer plugin settings not found (¿corriste Flow al menos una vez?); file search keyword not applied.'
    }

    $legacyDirs = @(Get-ChildItem (Join-Path "$env:APPDATA\FlowLauncher" 'Plugins') -Directory -ErrorAction SilentlyContinue | Where-Object {
        $manifest = Join-Path $_.FullName 'plugin.json'
        (Test-Path $manifest) -and ((Get-Content $manifest -Raw | ConvertFrom-Json).ID -eq $script:WinarchyFlowLegacyEverythingPluginId)
    })
    if (-not $files.Count -and -not $legacyDirs) {
        Write-WinarchyOk "Flow scoped keywords already applied: $settingsPath"
        return
    }

    $snapshot = New-WinarchySnapshot -Label 'flow-settings' -Path @($files.Keys)
    Save-WinarchyFlowSettings -Files $files -WhileStopped {
        foreach ($d in $legacyDirs) { Move-Item $d.FullName (Join-Path $snapshot $d.Name) -Force }
    }
    if ($legacyDirs) { $changes += "legacy Everything plugin moved to $snapshot" }
    Write-WinarchyOk "Flow scoped keywords applied ($($changes -join ', '))"
}

function Get-WinarchyDefenderExclusionPaths {
    <# Paths a excluir del real-time scan de Defender: solo los presentes en la máquina.
       Cubre dos causas de lag:
       - I/O frecuente de apps del stack (ShareX en cada captura, Everything indexando).
       - Procesos que Winarchy spawnea por hotkey (pwsh.exe vía `winarchy <cmd>`,
         komorebic.exe vía los hotkeys de tiling): sin exclusión, Defender escanea el
         binario y los .ps1 del repo la primera vez que se tocan en la sesión, que es
         exactamente el lag "solo la primera vez" que se ve en captura/cierre de ventana.
    #>
    $komorebic = (Get-Command komorebic.exe -ErrorAction SilentlyContinue).Source
    if (-not $komorebic) {
        $komorebic = "$env:ProgramFiles\komorebi\bin\komorebic.exe"
    }
    @(
        (Get-WinarchyShareXExe),
        "$env:USERPROFILE\Documents\ShareX",
        "$env:ProgramFiles\Everything\Everything.exe",
        "${env:ProgramFiles(x86)}\Everything\Everything.exe",
        (Get-Command pwsh.exe -ErrorAction SilentlyContinue).Source,
        $komorebic,
        (Get-WinarchyRoot)
    ) | Where-Object { $_ -and (Test-Path $_) }
}

function Set-WinarchyDefenderExclusions {
    <# Requiere shell elevado; si no lo está, no falla, solo avisa (mismo patrón que el
       resto de install.ps1 con operaciones de sistema). #>
    if (-not ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        Write-WinarchyWarn 'Defender exclusions need an elevated shell; run "winarchy doctor" from an admin PowerShell to apply them.'
        return
    }
    $paths = @(Get-WinarchyDefenderExclusionPaths)
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
