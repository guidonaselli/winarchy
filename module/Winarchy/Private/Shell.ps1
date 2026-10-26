function Get-WinarchyShellExe {
    <# Binario de la shell nativa: el instalado (verificado) y, en desarrollo, el build local. #>
    $root = Get-WinarchyRoot
    foreach ($candidate in (Join-Path $root 'state\bin\winarchy-shell.exe'), (Join-Path $root 'shell\target\release\winarchy-shell.exe')) {
        if (Test-Path $candidate) { return $candidate }
    }
}

function Install-WinarchyShellBinary {
    <# Copia un winarchy-shell.exe a state\bin solo si su SHA256 coincide con el de versions.lock.toml. #>
    param([Parameter(Mandatory)][string]$Source)
    $root = Get-WinarchyRoot
    $lock = Import-WinarchyToml -Path (Join-Path $root 'versions.lock.toml')
    $expected = $lock['shell']?['sha256']
    if (-not $expected) { throw 'versions.lock.toml has no [shell] sha256 pinned: nothing to verify against.' }
    $actual = (Get-FileHash -Path $Source -Algorithm SHA256).Hash
    if ($actual -ne $expected.ToUpperInvariant()) {
        throw "winarchy-shell.exe SHA256 mismatch (expected $expected, got $actual): not installed."
    }
    $dest = Join-Path (Join-Path (Get-WinarchyStateDir) 'bin') 'winarchy-shell.exe'
    New-Item -ItemType Directory -Path (Split-Path $dest -Parent) -Force | Out-Null
    Copy-Item -Path $Source -Destination $dest -Force
    $dest
}

function Install-WinarchyShellRelease {
    <# Baja el winarchy-shell.exe del Release de esta versión y lo instala solo si coincide con el SHA256 pinneado. #>
    $url = "https://github.com/$script:WinarchyRepo/releases/download/v$((Get-WinarchyVersion).Version)/winarchy-shell.exe"
    $tmp = Join-Path ([IO.Path]::GetTempPath()) "winarchy-shell-$([guid]::NewGuid().ToString('N')).exe"
    try {
        Invoke-WebRequest -Uri $url -OutFile $tmp -UseBasicParsing -ErrorAction Stop
        Install-WinarchyShellBinary -Source $tmp
    }
    finally { Remove-Item $tmp -Force -ErrorAction SilentlyContinue }
}

function Install-WinarchyYasb {
    <# YASB es opcional: se instala a pedido, a la versión que Winarchy fija. #>
    $lock = Import-WinarchyToml -Path (Join-Path (Get-WinarchyRoot) 'versions.lock.toml')
    Write-WinarchyInfo "Installing YASB $($lock['core']['yasb']) (optional status bar)..."
    winget install --id AmN.yasb --exact --source winget --silent --accept-package-agreements --accept-source-agreements --version $lock['core']['yasb']
    if ($LASTEXITCODE -notin 0, -1978334967, -1978335135, -1978335189) { throw "winget could not install YASB (exit code $LASTEXITCODE)." }
    winget pin add --id AmN.yasb --exact --source winget --accept-source-agreements 2>$null | Out-Null
    $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')
    if (-not (Get-Command yasbc -ErrorAction SilentlyContinue)) { throw 'YASB was installed but yasbc is not on PATH yet: open a new terminal and retry.' }
}

function Initialize-WinarchyBar {
    <# Barra por defecto de la instalación: la shell nativa (YASB solo si ya estaba). Deja el binario verificado en su lugar. #>
    $bar = Get-WinarchyActiveBar
    if ($bar -eq 'native' -and -not (Test-WinarchyShellCurrent)) {
        try { $null = Install-WinarchyShellRelease }
        catch { Write-WinarchyWarn "Could not install the native shell ($($_.Exception.Message)). Retry later with: winarchy bar use native"; return }
    }
    $file = Join-Path (Get-WinarchyStateDir) 'bar'
    if (-not (Test-Path $file)) { Set-WinarchyFileAtomic -Path $file -Value $bar }
    Write-WinarchyOk "Bar: $bar"
}

function Test-WinarchyShellCurrent {
    <# El binario instalado coincide con el SHA256 pinneado; un build local de desarrollo también vale. #>
    $root = Get-WinarchyRoot
    if (Test-Path (Join-Path $root 'shell\target\release\winarchy-shell.exe')) { return $true }
    $exe = Join-Path $root 'state\bin\winarchy-shell.exe'
    $pin = (Import-WinarchyToml -Path (Join-Path $root 'versions.lock.toml'))['shell']?['sha256']
    [bool]($pin -and (Test-Path $exe) -and ((Get-FileHash $exe -Algorithm SHA256).Hash -eq $pin.ToUpperInvariant()))
}

function Set-WinarchyBar {
    param([Parameter(Mandatory)][ValidateSet('yasb', 'native')][string]$Name)
    if ($Name -eq 'yasb' -and -not (Get-Command yasbc -ErrorAction SilentlyContinue)) { Install-WinarchyYasb }
    if ($Name -eq 'native') {
        if (-not (Get-WinarchyShellExe)) { $null = Install-WinarchyShellRelease }
        $theme = Join-Path (Get-WinarchyRoot) 'config\shell\theme.json'
        $current = Join-Path (Get-WinarchyStateDir) 'current-theme'
        if (-not (Test-Path $theme) -and (Test-Path $current)) { Set-WinarchyTheme -Name (Get-Content $current -Raw).Trim() }
    }
    Set-WinarchyFileAtomic -Path (Join-Path (Get-WinarchyStateDir) 'bar') -Value $Name
    if (Test-WinarchyTask -TaskName 'ahk') {
        Register-WinarchyAutostart
        $inactive = if ($Name -eq 'native') { 'yasb' } else { 'shell' }
        & schtasks.exe /Delete /TN (Get-WinarchyTaskFullName -TaskName $inactive) /F *> $null
    }
    Invoke-WinarchyReload -Light
    Write-WinarchyOk "Bar: $Name"
}
