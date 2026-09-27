# Runs in the sandbox (Windows PowerShell 5.1). `elevated` runs as SYSTEM, the rest as the non-elevated user.
param([Parameter(Mandatory)][ValidateSet('install', 'boot', 'selfupdate', 'published', 'uninstall', 'paths', 'ahkv1', 'windhawk', 'elevated')][string]$Scenario)
$ProgressPreference = 'SilentlyContinue'
$out = 'C:\sandbox'
$bare = Join-Path $out 'winarchy.git'
Start-Transcript -Path (Join-Path $out "$Scenario.txt")

function Update-SessionPath {
    $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')
}

function Install-Prerequisites {
    foreach ($id in 'Git.Git', 'Microsoft.PowerShell') {
        winget install --id $id --exact --source winget --silent --accept-package-agreements --accept-source-agreements 2>&1 | Out-Host
        if ($LASTEXITCODE -ne 0) { $script:failed = $true; "winget install $id exited $LASTEXITCODE" }
    }
    Update-SessionPath
    git clone --quiet --branch release $bare C:\winarchy
    if ($LASTEXITCODE -ne 0) { $script:failed = $true; "git clone exited $LASTEXITCODE" }
}

function Invoke-WinarchyScript([string]$Script, [string[]]$Arguments = @()) {
    pwsh -NoProfile -ExecutionPolicy Bypass -File "C:\winarchy\$Script" @Arguments 2>&1 | Out-Host
    if ($LASTEXITCODE -ne 0) { $script:failed = $true; "$Script $Arguments exited $LASTEXITCODE" }
}

function Test-Uninstalled {
    $pins = winget pin list --accept-source-agreements | Out-String -Width 400
    foreach ($id in 'LGUG2Z.komorebi', 'AmN.yasb', 'AutoHotkey.AutoHotkey', 'Flow-Launcher.Flow-Launcher', 'wez.wezterm', 'voidtools.Everything') {
        if ($pins -match [regex]::Escape($id)) { $script:failed = $true; "still pinned: $id" }
    }
    foreach ($var in 'KOMOREBI_CONFIG_HOME', 'YASB_CONFIG_HOME', 'WEZTERM_CONFIG_FILE') {
        if ([Environment]::GetEnvironmentVariable($var, 'User')) { $script:failed = $true; "still set: $var" }
    }
    if ([Environment]::GetEnvironmentVariable('Path', 'User') -split ';' -contains 'C:\winarchy\bin') { $script:failed = $true; 'bin still on PATH' }
    schtasks /Query /TN '\Winarchy\komorebi' 2>$null | Out-Null
    if ($LASTEXITCODE -eq 0) { $script:failed = $true; 'logon task still registered' }
    foreach ($name in 'komorebi', 'yasb') {
        if (Get-Process -Name $name -ErrorAction SilentlyContinue) { $script:failed = $true; "$name still running" }
    }
    $profilePath = Join-Path ([Environment]::GetFolderPath('MyDocuments')) 'PowerShell\profile.ps1'
    if ((Test-Path $profilePath) -and (Select-String -Path $profilePath -SimpleMatch 'managed by winarchy' -Quiet)) { $script:failed = $true; 'profile hook still present' }
    if ((Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StuckRects3').Settings[8] -band 1) { $script:failed = $true; 'taskbar still on auto-hide' }
    if ($null -ne (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Search' -Name BingSearchEnabled -ErrorAction SilentlyContinue)) { $script:failed = $true; 'Windows hardening not reverted' }
    $palette = Join-Path ([Environment]::GetFolderPath('StartMenu')) 'Programs\Winarchy'
    if (Get-ChildItem $palette -Filter '*.lnk' -ErrorAction SilentlyContinue) { $script:failed = $true; 'Start menu commands still present' }
}

function Invoke-WinarchyElevated([string]$Log, [string[]]$Arguments) {
    $command = "& C:\winarchy\bin\winarchy.ps1 $Arguments *>&1 | Out-File C:\sandbox\$Log.txt; exit [int]`$LASTEXITCODE"
    $process = Start-Process pwsh -Verb RunAs -Wait -PassThru -ArgumentList '-NoProfile', '-ExecutionPolicy', 'Bypass', '-Command', $command
    Get-Content "C:\sandbox\$Log.txt" -ErrorAction SilentlyContinue | Out-Host
    if ($process.ExitCode -ne 0) { $script:failed = $true; "winarchy $Arguments exited $($process.ExitCode)" }
}

$boot = (Get-Content (Join-Path $out 'boot.ps1') -Raw).Replace('https://github.com/guidonaselli/winarchy.git', $bare)
$failed = $false
switch ($Scenario) {
    'elevated' {
        try { Invoke-Expression $boot; $failed = $true; 'boot ran in an elevated session' }
        catch { "refused: $($_.Exception.Message)" }
        if (Test-Path C:\winarchy) { $failed = $true; 'C:\winarchy was created' }
    }
    'install' {
        Install-Prerequisites
        Invoke-WinarchyScript install.ps1
        Invoke-WinarchyScript install.ps1
    }
    'ahkv1' {
        winget install --id AutoHotkey.AutoHotkey --exact --source winget --version 1.1.37.02 --silent --accept-package-agreements --accept-source-agreements 2>&1 | Out-Host
        Install-Prerequisites
        Invoke-WinarchyScript install.ps1
    }
    'boot' {
        foreach ($activate in '', '1') {
            $env:WINARCHY_ACTIVATE = $activate
            try { Invoke-Expression $boot }
            catch { $failed = $true; "boot (WINARCHY_ACTIVATE='$activate') failed: $($_.Exception.Message)" }
        }
    }
    'paths' {
        $dir = 'C:\Win ' + [char]0x00C1 + "rchy d'Test\winarchy"
        $dir | Set-Content (Join-Path $out 'root.txt') -Encoding UTF8
        $env:WINARCHY_DIR = $dir
        $env:WINARCHY_ACTIVATE = '1'
        try { Invoke-Expression $boot }
        catch { $failed = $true; "boot failed: $($_.Exception.Message)" }
    }
    'published' {
        try { Invoke-RestMethod https://raw.githubusercontent.com/guidonaselli/winarchy/release/boot.ps1 | Invoke-Expression }
        catch { $failed = $true; "one-liner failed: $($_.Exception.Message)" }
    }
    'uninstall' {
        Install-Prerequisites
        Invoke-WinarchyScript install.ps1 -Arguments '-Activate'
        schtasks /Query /TN '\Winarchy\komorebi' 2>$null | Out-Null
        if ($LASTEXITCODE -ne 0) { $failed = $true; 'logon task missing before uninstall' }
        if (-not ((Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StuckRects3').Settings[8] -band 1)) { $failed = $true; 'taskbar not on auto-hide before uninstall' }
        Invoke-WinarchyScript uninstall.ps1
        Test-Uninstalled
        Invoke-WinarchyScript uninstall.ps1
        Test-Uninstalled
        Invoke-WinarchyScript install.ps1
    }
    'windhawk' {
        Install-Prerequisites
        Invoke-WinarchyScript install.ps1
        $mods = (Get-Content C:\winarchy\extras\windhawk\mods.json -Raw | ConvertFrom-Json).mods
        $modsDir = "$env:ProgramData\Windhawk\Engine\Mods\64"
        Invoke-WinarchyElevated windhawk-add 'extras', 'add', 'windhawk', '--yes'
        foreach ($mod in $mods) {
            $key = Get-ItemProperty "HKLM:\SOFTWARE\Windhawk\Engine\Mods\$($mod.id)" -ErrorAction SilentlyContinue
            if (-not $key -or $key.Version -ne $mod.version -or $key.Disabled -ne 0) { $failed = $true; "mod not enabled at $($mod.version): $($mod.id)"; continue }
            if (-not (Test-Path (Join-Path $modsDir $key.LibraryFileName))) { $failed = $true; "mod dll missing: $($key.LibraryFileName)" }
        }
        foreach ($lib in 'libc++.whl', 'libunwind.whl', 'windhawk-mod-shim.dll') {
            if (-not (Test-Path (Join-Path $modsDir $lib))) { $failed = $true; "runtime lib missing: $lib" }
        }
        $settings = Get-ItemProperty HKLM:\SOFTWARE\Windhawk\Settings -ErrorAction SilentlyContinue
        if ($settings.HideTrayIcon -ne 1 -or $settings.DisableUpdateCheck -ne 1) { $failed = $true; 'Windhawk tray or update check still on' }
        $loaded = $null
        foreach ($i in 1..30) {
            $loaded = (Get-Process explorer).Modules.ModuleName | Where-Object { $_ -like 'dark-menus_*' }
            if ($loaded) { break }
            Start-Sleep -Seconds 2
        }
        if ($loaded) { "explorer loaded $loaded" } else { $failed = $true; 'dark-menus not loaded in explorer' }
        Invoke-WinarchyElevated windhawk-add-again 'extras', 'add', 'windhawk', '--yes'
        Invoke-WinarchyElevated windhawk-remove 'extras', 'remove', 'windhawk'
        if (Get-ChildItem HKLM:\SOFTWARE\Windhawk\Engine\Mods -ErrorAction SilentlyContinue) { $failed = $true; 'mod registry keys left' }
        if (Test-Path "$env:ProgramFiles\Windhawk\windhawk.exe") { $failed = $true; 'Windhawk still installed' }
    }
    'selfupdate' {
        Install-Prerequisites
        Invoke-WinarchyScript install.ps1
        git -C C:\winarchy reset --quiet --hard v1.6.0
        foreach ($id in 'AutoHotkey.AutoHotkey', 'Flow-Launcher.Flow-Launcher') { winget pin remove --id $id --exact --source winget 2>&1 | Out-Host }
        $update = cmd /c 'C:\winarchy\bin\winarchy.cmd update --self 2>&1' | Out-String
        $update
        if ($update -notmatch 'Winarchy updated') { $failed = $true; 'update --self did not report an update' }
        $version = (Import-PowerShellDataFile C:\winarchy\module\Winarchy\Winarchy.psd1).ModuleVersion
        "version after update: $version"
        if ($version -eq '1.6.0') { $failed = $true; 'still on 1.6.0' }
    }
}
Stop-Transcript
if ($failed) { exit 1 }
