# Runs in the sandbox (Windows PowerShell 5.1). `elevated` runs as SYSTEM, the rest as the non-elevated user.
param([Parameter(Mandatory)][ValidateSet('install', 'boot', 'selfupdate', 'published', 'uninstall', 'elevated')][string]$Scenario)
$ProgressPreference = 'SilentlyContinue'
$out = 'C:\sandbox'
$bare = Join-Path $out 'winarchy.git'
Start-Transcript -Path (Join-Path $out "$Scenario.txt")

function Update-SessionPath {
    $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')
}

function Install-FromClone([int]$Passes, [switch]$Activate) {
    foreach ($id in 'Git.Git', 'Microsoft.PowerShell') {
        winget install --id $id --exact --source winget --silent --accept-package-agreements --accept-source-agreements 2>&1 | Out-Host
        if ($LASTEXITCODE -ne 0) { $script:failed = $true; "winget install $id exited $LASTEXITCODE" }
    }
    Update-SessionPath
    git clone --quiet --branch release $bare C:\winarchy
    if ($LASTEXITCODE -ne 0) { $script:failed = $true; "git clone exited $LASTEXITCODE" }
    foreach ($pass in 1..$Passes) {
        $installArgs = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', 'C:\winarchy\install.ps1')
        if ($Activate) { $installArgs += '-Activate' }
        pwsh @installArgs 2>&1 | Out-Host
        if ($LASTEXITCODE -ne 0) { $script:failed = $true; "install.ps1 pass $pass exited $LASTEXITCODE" }
    }
}

$boot = (Get-Content (Join-Path $out 'boot.ps1') -Raw).Replace('https://github.com/guidonaselli/winarchy.git', $bare)
$failed = $false
switch ($Scenario) {
    'elevated' {
        try { Invoke-Expression $boot; $failed = $true; 'boot ran in an elevated session' }
        catch { "refused: $($_.Exception.Message)" }
        if (Test-Path C:\winarchy) { $failed = $true; 'C:\winarchy was created' }
    }
    'install' { Install-FromClone 2 }
    'boot' {
        foreach ($activate in '', '1') {
            $env:WINARCHY_ACTIVATE = $activate
            try { Invoke-Expression $boot }
            catch { $failed = $true; "boot (WINARCHY_ACTIVATE='$activate') failed: $($_.Exception.Message)" }
        }
    }
    'published' {
        try { Invoke-RestMethod https://raw.githubusercontent.com/guidonaselli/winarchy/release/boot.ps1 | Invoke-Expression }
        catch { $failed = $true; "one-liner failed: $($_.Exception.Message)" }
    }
    'uninstall' {
        Install-FromClone 1 -Activate
        schtasks /Query /TN '\Winarchy\komorebi' 2>$null | Out-Null
        if ($LASTEXITCODE -ne 0) { $failed = $true; 'logon task missing before uninstall' }
        pwsh -NoProfile -ExecutionPolicy Bypass -File C:\winarchy\uninstall.ps1 2>&1 | Out-Host
        if ($LASTEXITCODE -ne 0) { $failed = $true; "uninstall.ps1 exited $LASTEXITCODE" }
        $pins = winget pin list --accept-source-agreements | Out-String -Width 400
        foreach ($id in 'LGUG2Z.komorebi', 'AmN.yasb', 'AutoHotkey.AutoHotkey', 'Flow-Launcher.Flow-Launcher', 'wez.wezterm', 'voidtools.Everything') {
            if ($pins -match [regex]::Escape($id)) { $failed = $true; "still pinned: $id" }
        }
        foreach ($var in 'KOMOREBI_CONFIG_HOME', 'YASB_CONFIG_HOME', 'WEZTERM_CONFIG_FILE') {
            if ([Environment]::GetEnvironmentVariable($var, 'User')) { $failed = $true; "still set: $var" }
        }
        if ([Environment]::GetEnvironmentVariable('Path', 'User') -split ';' -contains 'C:\winarchy\bin') { $failed = $true; 'bin still on PATH' }
        schtasks /Query /TN '\Winarchy\komorebi' 2>$null | Out-Null
        if ($LASTEXITCODE -eq 0) { $failed = $true; 'logon task still registered' }
    }
    'selfupdate' {
        Install-FromClone 1
        git -C C:\winarchy reset --quiet --hard v1.6.0
        foreach ($id in 'AutoHotkey.AutoHotkey', 'Flow-Launcher.Flow-Launcher') { winget pin remove --id $id 2>&1 | Out-Host }
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
