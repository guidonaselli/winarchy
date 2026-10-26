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

function Set-WinarchyBar {
    param([Parameter(Mandatory)][ValidateSet('yasb', 'native')][string]$Name)
    if ($Name -eq 'native') {
        if (-not (Get-WinarchyShellExe)) { throw 'winarchy-shell.exe not found: build shell/ or install the release binary first.' }
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
