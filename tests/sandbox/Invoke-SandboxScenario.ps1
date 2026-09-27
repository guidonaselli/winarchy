# Runs in the sandbox (Windows PowerShell 5.1). `elevated` runs as SYSTEM, the rest as the non-elevated user.
param([Parameter(Mandatory)][ValidateSet('install', 'boot', 'elevated')][string]$Scenario)
$ProgressPreference = 'SilentlyContinue'
$out = 'C:\sandbox'
$bare = Join-Path $out 'winarchy.git'
Start-Transcript -Path (Join-Path $out "$Scenario.txt")

function Update-SessionPath {
    $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')
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
        foreach ($id in 'Git.Git', 'Microsoft.PowerShell') {
            winget install --id $id --exact --source winget --silent --accept-package-agreements --accept-source-agreements 2>&1 | Out-Host
            if ($LASTEXITCODE -ne 0) { $failed = $true; "winget install $id exited $LASTEXITCODE" }
        }
        Update-SessionPath
        git clone --quiet $bare C:\winarchy
        if ($LASTEXITCODE -ne 0) { $failed = $true; "git clone exited $LASTEXITCODE" }
        foreach ($pass in 1, 2) {
            pwsh -NoProfile -ExecutionPolicy Bypass -File C:\winarchy\install.ps1 2>&1 | Out-Host
            if ($LASTEXITCODE -ne 0) { $failed = $true; "install.ps1 pass $pass exited $LASTEXITCODE" }
        }
    }
    'boot' {
        foreach ($activate in '', '1') {
            $env:WINARCHY_ACTIVATE = $activate
            try { Invoke-Expression $boot }
            catch { $failed = $true; "boot (WINARCHY_ACTIVATE='$activate') failed: $($_.Exception.Message)" }
        }
    }
}
Stop-Transcript
if ($failed) { exit 1 }
