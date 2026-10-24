# Hardening.ps1 — apaga la publicidad, sugerencias y búsqueda web de Windows y quita
# retardos y animaciones de la interfaz.
# Solo HKCU (sin admin), idempotente y reversible: revertir borra los valores, que devuelve
# a Windows su default.

function Get-WinarchyHardeningSettings {
    $base = 'HKCU:\Software\Microsoft\Windows\CurrentVersion'
    $cdm = "$base\ContentDeliveryManager"
    $settings = @(
        # búsqueda: sin Bing ni resultados web en el Inicio, sin caja en la taskbar
        , @("$base\Search", 'BingSearchEnabled', 0)
        , @("$base\Search", 'SearchboxTaskbarMode', 0)
        , @("$base\Search", 'CortanaConsent', 0)
        , @("$base\SearchSettings", 'IsDynamicSearchBoxEnabled', 0)
        , @("$base\SearchSettings", 'IsAADCloudSearchEnabled', 0)
        , @("$base\SearchSettings", 'IsMSACloudSearchEnabled', 0)
        , @('HKCU:\Software\Policies\Microsoft\Windows\Explorer', 'DisableSearchBoxSuggestions', 1)
        # taskbar: sin widgets/noticias, chat, vista de tareas ni Copilot
        , @("$base\Explorer\Advanced", 'TaskbarDa', 0)
        , @("$base\Explorer\Advanced", 'TaskbarMn', 0)
        , @("$base\Explorer\Advanced", 'ShowTaskViewButton', 0)
        , @("$base\Explorer\Advanced", 'ShowCopilotButton', 0)
        # Inicio y Explorer: sin recomendaciones, avisos de cuenta ni anuncios de OneDrive
        , @("$base\Explorer\Advanced", 'Start_IrisRecommendations', 0)
        , @("$base\Explorer\Advanced", 'Start_AccountNotifications', 0)
        , @("$base\Explorer\Advanced", 'ShowSyncProviderNotifications', 0)
        # publicidad, apps sugeridas, tips y Spotlight
        , @("$base\AdvertisingInfo", 'Enabled', 0)
        , @("$base\Privacy", 'TailoredExperiencesWithDiagnosticDataEnabled', 0)
        , @("$base\UserProfileEngagement", 'ScoobeSystemSettingEnabled', 0)
        , @('HKCU:\Control Panel\International\User Profile', 'HttpAcceptLanguageOptOut', 1)
        , @('HKCU:\Software\Policies\Microsoft\Windows\CloudContent', 'DisableWindowsSpotlightFeatures', 1)
        # fluidez: submenús en 100 ms (default 400) y sin animar minimizar/maximizar ni la taskbar;
        # el tiling reordena las ventanas al instante. Los dos primeros son REG_SZ.
        , @('HKCU:\Control Panel\Desktop', 'MenuShowDelay', '100', 'String')
        , @('HKCU:\Control Panel\Desktop\WindowMetrics', 'MinAnimate', '0', 'String')
        , @("$base\Explorer\Advanced", 'TaskbarAnimations', 0)
    )
    foreach ($name in 'ContentDeliveryAllowed', 'FeatureManagementEnabled', 'OemPreInstalledAppsEnabled',
        'PreInstalledAppsEnabled', 'PreInstalledAppsEverEnabled', 'SilentInstalledAppsEnabled',
        'SoftLandingEnabled', 'SystemPaneSuggestionsEnabled', 'RotatingLockScreenEnabled',
        'RotatingLockScreenOverlayEnabled', 'SubscribedContent-310093Enabled', 'SubscribedContent-338387Enabled',
        'SubscribedContent-338388Enabled', 'SubscribedContent-338389Enabled', 'SubscribedContent-338393Enabled',
        'SubscribedContent-353694Enabled', 'SubscribedContent-353696Enabled', 'SubscribedContent-353698Enabled',
        'SubscribedContent-88000326Enabled') {
        $settings += , @($cdm, $name, 0)
    }
    $settings
}

function Set-WinarchyWindowsHardening {
    <# Aplica (o con -Revert, quita) los ajustes. Devuelve la cantidad de valores cambiados;
       reinicia Explorer solo si hubo cambios. #>
    param([switch]$Revert)
    $changed = 0
    $blocked = 0
    if (-not $Revert) {
        foreach ($key in 'Explorer\Advanced', 'ContentDeliveryManager', 'Search', 'SearchSettings') {
            $null = Backup-WinarchyRegistryKey -Key "HKCU\Software\Microsoft\Windows\CurrentVersion\$key" -Label 'hardening'
        }
    }
    # Valor previo de cada ajuste (null = no existía), para que revertir devuelva lo que el
    # usuario tenía y no solo el default de Windows.
    $priorFile = Join-Path (Get-WinarchyStateDir) 'hardening-prior.json'
    $prior = if (Test-Path $priorFile) { Get-Content $priorFile -Raw -Encoding UTF8 | ConvertFrom-Json -AsHashtable } else { @{} }
    foreach ($setting in Get-WinarchyHardeningSettings) {
        $path, $name, $value, $type = $setting
        if (-not $type) { $type = 'DWord' }
        $id = "$path|$name"
        try {
            $item = Get-ItemProperty -Path $path -Name $name -ErrorAction Ignore
            $current = if ($item) { $item.$name }
            if ($Revert) {
                $restore = $prior[$id]
                if ($null -ne $restore) {
                    if ($current -eq $restore) { continue }
                    Set-ItemProperty -Path $path -Name $name -Value $restore -Type $type -ErrorAction Stop
                }
                else {
                    if ($null -eq $current) { continue }
                    Remove-ItemProperty -Path $path -Name $name -ErrorAction Stop
                }
            }
            else {
                if (-not $prior.ContainsKey($id)) { $prior[$id] = $current }   # también si ya valía $value: revertir no debe borrar lo que el usuario ya tenía
                if ($current -eq $value) { continue }
                if (-not (Test-Path $path)) { $null = New-Item -Path $path -Force }
                Set-ItemProperty -Path $path -Name $name -Value $value -Type $type -ErrorAction Stop
            }
            $changed++
        }
        catch [System.UnauthorizedAccessException], [System.Security.SecurityException] { $blocked++ }
        catch { Write-WinarchyWarn "${name}: $($_.Exception.Message)" }
    }
    if ($Revert) { Remove-Item $priorFile -Force -ErrorAction SilentlyContinue }
    elseif ($prior.Count) { Set-WinarchyFileAtomic -Path $priorFile -Value ($prior | ConvertTo-Json) }
    if ($blocked) { Write-WinarchyInfo "$blocked Windows setting(s) are locked for non-admin users; skipped." }
    if ($changed -gt 0) { Stop-Process -Name explorer -Force -ErrorAction SilentlyContinue }
    $changed
}
