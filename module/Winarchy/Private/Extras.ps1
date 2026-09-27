# Extras.ps1 — extras opcionales fuera del core: solo `winarchy extras add` los instala.
# Windhawk no tiene CLI de mods: la instalación replica el `installMod` de su UI con la
# DLL precompilada de mods.windhawk.net.

$script:WindhawkModsUrl = 'https://mods.windhawk.net/mods/'
$script:WindhawkRegistryRoot = 'HKLM:\SOFTWARE\Windhawk'
$script:WindhawkDataRoot = "$env:ProgramData\Windhawk"

function Get-WinarchyWindhawkExe { "$env:ProgramFiles\Windhawk\windhawk.exe" }

function Get-WinarchyWindhawkManifest {
    Get-Content (Join-Path (Get-WinarchyRoot) 'extras\windhawk\mods.json') -Raw -Encoding UTF8 | ConvertFrom-Json
}

function Test-WinarchyElevated {
    ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Get-WinarchyWindhawkModMetadata {
    <# Campos del bloque `// ==WindhawkMod==`; include/exclude/architecture como arrays. #>
    param([Parameter(Mandatory)][string]$Source)
    $meta = @{ include = @(); exclude = @(); architecture = @() }
    $inBlock = $false
    foreach ($line in $Source -split '\r?\n') {
        if ($line -match '^//\s*==WindhawkMod==') { $inBlock = $true; continue }
        if ($line -match '^//\s*==/WindhawkMod==') { break }
        if ($inBlock -and $line -match '^//\s*@([a-zA-Z]+)\s+(.+?)\s*$') {
            $key, $value = $Matches[1], $Matches[2]
            if ($key -in 'include', 'exclude', 'architecture') { $meta[$key] += $value }
            else { $meta[$key] = $value }
        }
    }
    $meta
}

function Get-WinarchyWindhawkSubfolders {
    <# Carpetas de Engine\Mods según @architecture (default x86 + x86-64), como la UI. #>
    param([string[]]$Architecture)
    $arm64 = $env:PROCESSOR_ARCHITECTURE -eq 'ARM64'
    $archs = if ($Architecture) { $Architecture } else { 'x86', 'x86-64' }
    $folders = foreach ($a in $archs) {
        switch ($a) {
            'x86' { '32' }
            'x86-64' { '64'; if ($arm64) { 'arm64' } }
            'amd64' { '64' }
            'arm64' { if ($arm64) { 'arm64' } }
            default { throw "Unsupported Windhawk mod architecture: $a" }
        }
    }
    @($folders | Select-Object -Unique)
}

function Copy-WinarchyWindhawkRuntimeLibs {
    <# Runtime que las DLL de mods necesitan en Engine\Mods\<arch>; la UI lo copia al abrirse,
       y tras una instalación silenciosa puede no haberse abierto nunca. #>
    param(
        [string]$InstallRoot = "$env:ProgramFiles\Windhawk",
        [string]$DataRoot = $script:WindhawkDataRoot
    )
    $targets = [ordered]@{ 'i686-w64-mingw32' = '32'; 'x86_64-w64-mingw32' = '64' }
    if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { $targets['aarch64-w64-mingw32'] = 'arm64' }
    $libs = [ordered]@{ 'libc++.dll' = 'libc++.whl'; 'libunwind.dll' = 'libunwind.whl'; 'windhawk-mod-shim.dll' = 'windhawk-mod-shim.dll' }
    foreach ($target in $targets.Keys) {
        $destDir = Join-Path $DataRoot "Engine\Mods\$($targets[$target])"
        New-Item -ItemType Directory -Path $destDir -Force | Out-Null
        foreach ($lib in $libs.Keys) {
            $src = Get-Item (Join-Path $InstallRoot "Compiler\$target\bin\$lib")
            $dest = Join-Path $destDir $libs[$lib]
            if (-not (Test-Path $dest) -or (Get-Item $dest).LastWriteTimeUtc -ne $src.LastWriteTimeUtc) {
                Copy-Item $src.FullName $dest -Force
            }
        }
    }
}

function Test-WinarchyWindhawkModCurrent {
    <# True si el mod ya está en la versión del manifiesto, habilitado y con sus settings. #>
    param([Parameter(Mandatory)]$Mod, [string]$RegistryRoot = $script:WindhawkRegistryRoot)
    $key = "$RegistryRoot\Engine\Mods\$($Mod.id)"
    if (-not (Test-Path $key)) { return $false }
    $config = Get-ItemProperty $key
    if ($config.PSObject.Properties['Version']?.Value -ne $Mod.version -or $config.PSObject.Properties['Disabled']?.Value -ne 0 -or -not $config.PSObject.Properties['LibraryFileName']?.Value) { return $false }
    $current = Get-ItemProperty "$key\Settings" -ErrorAction SilentlyContinue
    foreach ($p in $Mod.settings.PSObject.Properties) {
        if ("$($current.PSObject.Properties[$p.Name]?.Value)" -ne "$($p.Value)") { return $false }
    }
    $true
}

function Install-WinarchyWindhawkMod {
    <# Instala una versión fijada de un mod con sus settings completos. No deja estado a
       medias: toda descarga ocurre antes de tocar el registro. #>
    param(
        [Parameter(Mandatory)]$Mod,
        [Parameter(Mandatory)][string]$WindhawkVersion,
        [string]$RegistryRoot = $script:WindhawkRegistryRoot,
        [string]$DataRoot = $script:WindhawkDataRoot
    )
    $id, $version = $Mod.id, $Mod.version
    $base = "$script:WindhawkModsUrl$id/"

    $published = @(Invoke-RestMethod -Uri "${base}versions.json") | Where-Object version -eq $version
    if (-not $published) { throw "Windhawk mod $id $version is not published." }
    $minWindhawk = $published.PSObject.Properties['minWindhawkVersion']?.Value
    if ($minWindhawk -and [version]$WindhawkVersion -lt [version]$minWindhawk) {
        throw "Windhawk mod $id $version needs Windhawk $minWindhawk or later (installed: $WindhawkVersion)."
    }

    $raw = (Invoke-WebRequest -Uri "$base$version.wh.cpp" -UseBasicParsing).RawContentStream.ToArray()
    $source = [Text.Encoding]::UTF8.GetString($raw) -replace '\r?\n', "`r`n"
    $meta = Get-WinarchyWindhawkModMetadata -Source $source
    if ($meta.id -ne $id -or $meta.version -ne $version) { throw "Windhawk mod source does not match $id $version." }

    $dllName = "${id}_${version}_$(Get-Random -Minimum 100000 -Maximum 1000000).dll"
    $written = @()
    try {
        foreach ($folder in (Get-WinarchyWindhawkSubfolders -Architecture $meta.architecture)) {
            $target = Join-Path $DataRoot "Engine\Mods\$folder\$dllName"
            New-Item -ItemType Directory -Path (Split-Path $target) -Force | Out-Null
            Invoke-WebRequest -Uri "$base${version}_$folder.dll" -OutFile $target -UseBasicParsing
            $written += $target
        }
    }
    catch {
        $written | Remove-Item -Force -ErrorAction SilentlyContinue
        throw "Could not download Windhawk mod ${id}: $($_.Exception.Message)"
    }

    $sourcePath = Join-Path $DataRoot "ModsSource\$id.wh.cpp"
    New-Item -ItemType Directory -Path (Split-Path $sourcePath) -Force | Out-Null
    Set-Content -Path $sourcePath -Value $source -NoNewline -Encoding UTF8

    # Settings antes que LibraryFileName: el engine carga el mod apenas ve esa clave.
    $key = "$RegistryRoot\Engine\Mods\$id"
    if (-not (Test-Path $key)) { New-Item -Path $key -Force | Out-Null }
    if (Test-Path "$key\Settings") { Remove-Item "$key\Settings" -Recurse -Force }
    New-Item -Path "$key\Settings" | Out-Null
    foreach ($p in $Mod.settings.PSObject.Properties) {
        $type = if ($p.Value -is [string]) { 'String' } else { 'DWord' }
        New-ItemProperty -Path "$key\Settings" -Name $p.Name -Value $p.Value -PropertyType $type | Out-Null
    }
    $values = [ordered]@{
        Include = $meta.include -join '|'; Exclude = $meta.exclude -join '|'
        Architecture = $meta.architecture -join '|'; Version = $version
    }
    foreach ($name in $values.Keys) { Set-ItemProperty -Path $key -Name $name -Value $values[$name] -Type String }
    Set-ItemProperty -Path $key -Name Disabled -Value 0 -Type DWord
    Set-ItemProperty -Path $key -Name LibraryFileName -Value $dllName -Type String
    Set-ItemProperty -Path $key -Name SettingsChangeTime -Value ([int][DateTimeOffset]::UtcNow.ToUnixTimeSeconds()) -Type DWord

    $profilePath = Join-Path $DataRoot 'userprofile.json'
    $userProfile = if (Test-Path $profilePath) { Get-Content $profilePath -Raw -Encoding UTF8 | ConvertFrom-Json -AsHashtable } else { [ordered]@{} }
    if (-not $userProfile['mods']) { $userProfile['mods'] = [ordered]@{} }
    $entry = if ($userProfile['mods'][$id]) { $userProfile['mods'][$id] } else { [ordered]@{} }
    $entry['version'] = $version
    $entry.Remove('latestVersion')
    $userProfile['mods'][$id] = $entry
    $userProfile | ConvertTo-Json -Depth 20 | Set-Content -Path $profilePath -Encoding UTF8

    Remove-WinarchyWindhawkModFiles -Id $id -DataRoot $DataRoot -Keep $dllName
}

function Remove-WinarchyWindhawkModFiles {
    <# Borra las DLL de un mod (menos -Keep). Las que el engine tiene cargadas quedan, como en la UI. #>
    param([Parameter(Mandatory)][string]$Id, [string]$DataRoot = $script:WindhawkDataRoot, [string]$Keep)
    $pattern = "^$([regex]::Escape($Id))_[^_]+_\d+\.dll$"
    Get-ChildItem (Join-Path $DataRoot 'Engine\Mods') -Directory -ErrorAction SilentlyContinue |
        Get-ChildItem -File -Filter '*.dll' |
        Where-Object { $_.Name -match $pattern -and $_.Name -ne $Keep } |
        Remove-Item -Force -ErrorAction SilentlyContinue
}

function Remove-WinarchyWindhawkMod {
    param(
        [Parameter(Mandatory)][string]$Id,
        [string]$RegistryRoot = $script:WindhawkRegistryRoot,
        [string]$DataRoot = $script:WindhawkDataRoot
    )
    foreach ($sub in 'Mods', 'ModsWritable') {
        $key = "$RegistryRoot\Engine\$sub\$Id"
        if (Test-Path $key) { Remove-Item $key -Recurse -Force }
    }
    Remove-WinarchyWindhawkModFiles -Id $Id -DataRoot $DataRoot
    Remove-Item (Join-Path $DataRoot "ModsSource\$Id.wh.cpp") -Force -ErrorAction SilentlyContinue
    $profilePath = Join-Path $DataRoot 'userprofile.json'
    if (Test-Path $profilePath) {
        $userProfile = Get-Content $profilePath -Raw -Encoding UTF8 | ConvertFrom-Json -AsHashtable
        if ($userProfile['mods'] -and $userProfile['mods'].Contains($Id)) {
            $userProfile['mods'].Remove($Id)
            $userProfile | ConvertTo-Json -Depth 20 | Set-Content -Path $profilePath -Encoding UTF8
        }
    }
}

function Set-WinarchyWindhawkIdentity {
    <# Sin tray propio ni chequeo de updates; la tarea WindhawkUpdateTask se apaga igual que
       lo hace la UI al tocar ese setting. Idempotente. #>
    param([string]$RegistryRoot = $script:WindhawkRegistryRoot)
    $key = "$RegistryRoot\Settings"
    if (-not (Test-Path $key)) { New-Item -Path $key -Force | Out-Null }
    $current = Get-ItemProperty $key
    $desired = [ordered]@{ HideTrayIcon = 1; DisableUpdateCheck = 1 }
    $changed = @($desired.Keys | Where-Object { $current.PSObject.Properties[$_]?.Value -ne $desired[$_] })
    if (-not $changed) {
        Write-WinarchyOk 'Windhawk identity already applied (tray + update check off)'
        return
    }
    foreach ($name in $desired.Keys) { Set-ItemProperty -Path $key -Name $name -Value $desired[$name] -Type DWord }
    Get-ScheduledTask -TaskName 'WindhawkUpdateTask' -ErrorAction SilentlyContinue | Disable-ScheduledTask | Out-Null
    $exe = Get-WinarchyWindhawkExe
    if (Test-Path $exe) { Start-Process $exe -ArgumentList '-restart', '-tray-only' }
    Write-WinarchyOk 'Windhawk identity applied (tray + update check off)'
}

function Get-WinarchyExtras {
    <# Estado de los extras disponibles. #>
    $manifest = Get-WinarchyWindhawkManifest
    $installed = Test-Path (Get-WinarchyWindhawkExe)
    $current = if ($installed) { @($manifest.mods | Where-Object { Test-WinarchyWindhawkModCurrent -Mod $_ }).Count } else { 0 }
    [pscustomobject]@{
        Name      = 'windhawk'
        Installed = $installed
        Detail    = "$current/$($manifest.mods.Count) curated mods applied"
        Command   = if ($installed) { 'winarchy extras remove windhawk' } else { 'winarchy extras add windhawk' }
    }
}

function Add-WinarchyExtra {
    param([Parameter(Mandatory)][ValidateSet('windhawk')][string]$Name, [switch]$Yes)
    if (-not (Test-WinarchyElevated)) {
        throw 'Windhawk writes to HKLM and ProgramData: run "winarchy extras add windhawk" from an elevated PowerShell.'
    }
    Write-WinarchyWarn @'
Windhawk injects code into explorer.exe and other processes to apply its mods.
  - A Windows update can break a mod (explorer crashing): recover with "windhawk.exe -safe-mode"
    or "winarchy extras remove windhawk".
  - Some antivirus products flag the injection.
'@
    if (-not $Yes) {
        if ([Console]::IsInputRedirected) { throw 'Confirmation needed: run it interactively or pass -Yes.' }
        if ((Read-Host 'Install Windhawk and the curated mods? [y/N]') -notmatch '^(y|yes|s|si)$') {
            Write-WinarchyInfo 'Cancelled: nothing changed.'
            return
        }
    }

    $exe = Get-WinarchyWindhawkExe
    if (-not (Test-Path $exe)) {
        Write-WinarchyInfo 'Installing Windhawk (winget)...'
        winget install --id RamenSoftware.Windhawk -e --source winget --silent --accept-package-agreements --accept-source-agreements | Out-Host
        if (-not (Test-Path $exe)) { throw 'Windhawk installation failed (winget install RamenSoftware.Windhawk).' }
    }
    $manifest = Get-WinarchyWindhawkManifest
    $version = (Get-Item $exe).VersionInfo.ProductVersion
    if ($version -ne $manifest.windhawk) {
        Write-WinarchyWarn "Windhawk $version installed; the curated mods were tested with $($manifest.windhawk)."
    }

    $null = Backup-WinarchyRegistryKey -Key 'HKLM\SOFTWARE\Windhawk' -Label 'windhawk'
    $null = New-WinarchySnapshot -Label 'windhawk' -Path @(
        (Join-Path $script:WindhawkDataRoot 'userprofile.json')
        $manifest.mods | ForEach-Object { Join-Path $script:WindhawkDataRoot "ModsSource\$($_.id).wh.cpp" }
    )
    Copy-WinarchyWindhawkRuntimeLibs
    foreach ($mod in $manifest.mods) {
        if (Test-WinarchyWindhawkModCurrent -Mod $mod) { continue }
        Install-WinarchyWindhawkMod -Mod $mod -WindhawkVersion $version
        Write-WinarchyOk "Windhawk mod $($mod.id) $($mod.version)"
    }
    Set-WinarchyWindhawkIdentity
    Write-WinarchyOk "Windhawk ready with $($manifest.mods.Count) curated mods"
}

function Remove-WinarchyExtra {
    param([Parameter(Mandatory)][ValidateSet('windhawk')][string]$Name)
    if (-not (Test-WinarchyElevated)) {
        throw 'Windhawk writes to HKLM and ProgramData: run "winarchy extras remove windhawk" from an elevated PowerShell.'
    }
    if (-not (Test-Path (Get-WinarchyWindhawkExe))) {
        Write-WinarchyOk 'Windhawk is not installed'
        return
    }
    $manifest = Get-WinarchyWindhawkManifest
    $null = Backup-WinarchyRegistryKey -Key 'HKLM\SOFTWARE\Windhawk' -Label 'windhawk'
    foreach ($mod in $manifest.mods) { Remove-WinarchyWindhawkMod -Id $mod.id }

    $others = @(Get-ChildItem "$script:WindhawkRegistryRoot\Engine\Mods" -ErrorAction SilentlyContinue | ForEach-Object PSChildName)
    if ($others) {
        Write-WinarchyWarn "Curated mods removed; Windhawk kept because other mods are installed: $($others -join ', ')"
        return
    }
    winget uninstall --id RamenSoftware.Windhawk -e --source winget --silent | Out-Host
    Write-WinarchyOk 'Windhawk removed'
}
