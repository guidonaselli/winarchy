# WezTermContextMenu.ps1 — "Abrir en WezTerm" en el click derecho de Explorer (solo HKCU)

$script:WinarchyWeztermContextMenuKeys = @(
    'Registry::HKEY_CURRENT_USER\Software\Classes\Directory\shell\WinarchyWezTerm',
    'Registry::HKEY_CURRENT_USER\Software\Classes\Directory\Background\shell\WinarchyWezTerm'
)

$script:WinarchyWeztermNativeVerbKeys = @(
    'Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Classes\Directory\shell\Open WezTerm here',
    'Registry::HKEY_CURRENT_USER\Software\Classes\Directory\shell\Open WezTerm here',
    'Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Classes\Directory\Background\shell\Open WezTerm here',
    'Registry::HKEY_CURRENT_USER\Software\Classes\Directory\Background\shell\Open WezTerm here'
)

function Test-WinarchyWeztermNativeContextMenu {
    <# $true si el propio instalador de WezTerm ya agregó "Open WezTerm here". #>
    [bool](@($script:WinarchyWeztermNativeVerbKeys | Where-Object { Test-Path $_ }).Count)
}

function New-WinarchyRegistrySnapshot {
    <# Exporta las claves dadas a backups/<timestamp>-<label>/. Devuelve el dir del snapshot. #>
    param([Parameter(Mandatory)][string[]]$RegistryPath, [string]$Label = 'registry-snapshot')
    $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
    $dest = Join-Path (Get-WinarchyBackupsDir) "$stamp-$Label"
    New-Item -ItemType Directory -Path $dest -Force | Out-Null
    $i = 0
    foreach ($regPath in $RegistryPath) {
        $i++
        $file = Join-Path $dest "key-$i.reg"
        if (Test-Path $regPath) {
            $nativePath = $regPath -replace '^Registry::HKEY_CURRENT_USER', 'HKCU'
            reg.exe export $nativePath $file /y 2>$null | Out-Null
        }
        else {
            Set-Content -Path (Join-Path $dest "key-$i.absent") -Value $regPath -Encoding UTF8
        }
    }
    $dest
}

function Get-WinarchyWeztermContextMenuKeys { $script:WinarchyWeztermContextMenuKeys }

function Install-WinarchyWeztermContextMenu {
    if (Test-WinarchyWeztermNativeContextMenu) {
        Write-WinarchyInfo "WezTerm's own installer already added 'Open WezTerm here' to Explorer's right-click menu; skipping to avoid a duplicate entry."
        return
    }

    $exe = Get-WinarchyWeztermExe
    if (-not $exe) { throw 'WezTerm not found; install it first (winarchy update --core).' }
    $cli = Join-Path (Split-Path $exe -Parent) 'wezterm.exe'
    if (-not (Test-Path $cli)) { throw "wezterm.exe (CLI) not found next to $exe." }

    New-WinarchyRegistrySnapshot -RegistryPath (Get-WinarchyWeztermContextMenuKeys) -Label 'wezterm-context-menu' | Out-Null

    foreach ($regPath in Get-WinarchyWeztermContextMenuKeys) {
        New-Item -Path $regPath -Force | Out-Null
        Set-ItemProperty -Path $regPath -Name '(default)' -Value 'Open in WezTerm'
        Set-ItemProperty -Path $regPath -Name 'Icon' -Value $exe
        $cmdPath = Join-Path $regPath 'command'
        New-Item -Path $cmdPath -Force | Out-Null
        Set-ItemProperty -Path $cmdPath -Name '(default)' -Value "`"$cli`" start --cwd `"%V`""
    }
    Write-WinarchyOk "'Open in WezTerm' installed in Explorer's right-click menu (folder + folder background)."
}

function Remove-WinarchyWeztermContextMenu {
    $existing = @(Get-WinarchyWeztermContextMenuKeys | Where-Object { Test-Path $_ })
    if ($existing.Count -eq 0) { Write-WinarchyInfo "'Open in WezTerm' was not installed."; return }

    New-WinarchyRegistrySnapshot -RegistryPath (Get-WinarchyWeztermContextMenuKeys) -Label 'wezterm-context-menu' | Out-Null

    foreach ($regPath in $existing) { Remove-Item -Path $regPath -Recurse -Force }
    Write-WinarchyOk "'Open in WezTerm' removed from Explorer's right-click menu."
}

function Test-WinarchyWeztermContextMenuInstalled {
    if (Test-WinarchyWeztermNativeContextMenu) {
        return [pscustomobject]@{ Installed = $true; Source = 'wezterm-installer'; TargetPath = $null; TargetExists = $true }
    }
    $keys = @(Get-WinarchyWeztermContextMenuKeys | Where-Object { Test-Path $_ })
    if ($keys.Count -eq 0) {
        return [pscustomobject]@{ Installed = $false; Source = $null; TargetPath = $null; TargetExists = $false }
    }
    $cmdPath = Join-Path $keys[0] 'command'
    $target = $null
    if (Test-Path $cmdPath) {
        $value = (Get-ItemProperty -Path $cmdPath -Name '(default)' -ErrorAction SilentlyContinue).'(default)'
        if ($value -match '^"([^"]+)"') { $target = $Matches[1] }
    }
    [pscustomobject]@{
        Installed    = $true
        Source       = 'winarchy'
        TargetPath   = $target
        TargetExists = [bool]($target -and (Test-Path $target))
    }
}

function Get-WinarchyWeztermContextMenuStatus {
    $status = Test-WinarchyWeztermContextMenuInstalled
    if (-not $status.Installed) { Write-WinarchyInfo "'Open in WezTerm' is not installed."; return }
    if ($status.Source -eq 'wezterm-installer') {
        Write-WinarchyOk "'Open WezTerm here' is already installed by WezTerm's own installer; Winarchy does not add a duplicate."
        return
    }
    if (-not $status.TargetExists) {
        Write-WinarchyWarn "'Open in WezTerm' is installed but points to a missing executable ($($status.TargetPath)). Re-run: winarchy wezterm context-menu install"
        return
    }
    Write-WinarchyOk "'Open in WezTerm' is installed -> $($status.TargetPath)"
}
