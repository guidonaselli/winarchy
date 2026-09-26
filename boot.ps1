# Winarchy one-command installer:
#   irm https://raw.githubusercontent.com/guidonaselli/winarchy/release/boot.ps1 | iex
# Optional: $env:WINARCHY_DIR (default C:\winarchy), $env:WINARCHY_ACTIVATE = '1'
& {
    $ErrorActionPreference = 'Stop'
    $repo = 'https://github.com/guidonaselli/winarchy.git'
    $branch = 'release'
    $dir = 'C:\winarchy'
    if ($env:WINARCHY_DIR) { $dir = $env:WINARCHY_DIR }
    $activate = $env:WINARCHY_ACTIVATE -eq '1'

    function Test-App([string]$Name) {
        [bool](Get-Command $Name -CommandType Application -ErrorAction SilentlyContinue)
    }

    if ([Environment]::OSVersion.Version.Build -lt 22000) {
        throw 'Winarchy requires Windows 11 (build 22000 or later).'
    }
    $principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Run this from a normal PowerShell window, not as administrator. winget asks for elevation when it needs it.'
    }

    $wingetOk = $false
    try { $null = winget --version; $wingetOk = $LASTEXITCODE -eq 0 } catch { }
    if (-not $wingetOk) {
        throw 'winget is not available. Install or update "App Installer" from the Microsoft Store, then run this again.'
    }

    foreach ($tool in @(@{ Command = 'git'; Id = 'Git.Git' }, @{ Command = 'pwsh'; Id = 'Microsoft.PowerShell' })) {
        if (Test-App $tool.Command) { continue }
        Write-Host "Installing $($tool.Id)..."
        winget install --id $tool.Id --exact --source winget --silent --accept-package-agreements --accept-source-agreements
        if ($LASTEXITCODE -ne 0) { throw "winget could not install $($tool.Id) (exit code $LASTEXITCODE)." }
        $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')
        if (-not (Test-App $tool.Command)) {
            throw "$($tool.Id) was installed but '$($tool.Command)' is not on PATH yet. Open a new terminal and run this again."
        }
    }

    if (Test-Path (Join-Path $dir '.git')) {
        $current = git -C $dir rev-parse --abbrev-ref HEAD
        if ($current -ne $branch) {
            throw "$dir is a git checkout on '$current', not '$branch'. Set WINARCHY_DIR to another folder."
        }
        Write-Host "Updating $dir..."
        git -C $dir pull --ff-only origin $branch
        if ($LASTEXITCODE -ne 0) { throw "git pull failed in $dir." }
    }
    elseif ((Test-Path $dir) -and (Get-ChildItem -Force $dir | Select-Object -First 1)) {
        throw "$dir already exists and is not a Winarchy checkout. Move it or set WINARCHY_DIR to another folder."
    }
    else {
        git clone --branch $branch $repo $dir
        if ($LASTEXITCODE -ne 0) { throw 'git clone failed.' }
    }

    $installArgs = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $dir 'install.ps1'))
    if ($activate) { $installArgs += '-Activate' }
    pwsh @installArgs
    if ($LASTEXITCODE -ne 0) { throw "install.ps1 failed (exit code $LASTEXITCODE)." }

    $ErrorActionPreference = 'Continue'
    schtasks /Query /TN '\Winarchy\komorebi' 2>$null | Out-Null
    $active = $LASTEXITCODE -eq 0

    Write-Host ''
    if ($active) {
        Write-Host "Winarchy is installed in $dir and active: it starts at logon."
    }
    else {
        Write-Host "Winarchy is installed in $dir in coexistence mode: nothing starts at logon."
        Write-Host "To activate it: pwsh -File `"$dir\install.ps1`" -Activate"
    }
    Write-Host 'Next: open a new terminal and run  winarchy doctor'
}
