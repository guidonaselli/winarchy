# Uwphost.ps1 — winarchy-uwphost.exe: el helper que abre las apps UWP cuando Winarchy es el shell de sesión

function Get-WinarchyUwphostExe {
    $root = Get-WinarchyRoot
    foreach ($candidate in (Join-Path $root 'uwphost\target\release\winarchy-uwphost.exe'), (Join-Path $root 'state\bin\winarchy-uwphost.exe')) {
        if (Test-Path -LiteralPath $candidate) { return $candidate }
    }
}

function Install-WinarchyUwphostBinary {
    <# Copia un winarchy-uwphost.exe a state\bin solo si su SHA256 coincide con el de versions.lock.toml. #>
    param([Parameter(Mandatory)][string]$Source)
    $lock = Import-WinarchyToml -Path (Join-Path (Get-WinarchyRoot) 'versions.lock.toml')
    $expected = $lock['uwphost']?['sha256']
    if (-not $expected) { throw 'versions.lock.toml has no [uwphost] sha256 pinned: nothing to verify against.' }
    $actual = (Get-FileHash -Path $Source -Algorithm SHA256).Hash
    if ($actual -ne $expected.ToUpperInvariant()) {
        throw "winarchy-uwphost.exe SHA256 mismatch (expected $expected, got $actual): not installed."
    }
    Copy-WinarchyRunningBinary -Source $Source -Name 'winarchy-uwphost.exe'
}

function Install-WinarchyUwphostRelease {
    $url = "https://github.com/$script:WinarchyRepo/releases/download/v$((Get-WinarchyVersion).Version)/winarchy-uwphost.exe"
    $tmp = Join-Path ([IO.Path]::GetTempPath()) "winarchy-uwphost-$([guid]::NewGuid().ToString('N')).exe"
    try {
        Invoke-WebRequest -Uri $url -OutFile $tmp -UseBasicParsing -ErrorAction Stop
        Install-WinarchyUwphostBinary -Source $tmp
    }
    finally { Remove-Item $tmp -Force -ErrorAction SilentlyContinue }
}

function Sync-WinarchyUwphostBinary {
    <# state\bin sigue al SHA256 pinneado; un build local manda y sin pin vale lo que ya haya. #>
    $root = Get-WinarchyRoot
    if (Test-Path (Join-Path $root 'uwphost\target\release\winarchy-uwphost.exe')) { return }
    $exe = Join-Path $root 'state\bin\winarchy-uwphost.exe'
    $pin = (Import-WinarchyToml -Path (Join-Path $root 'versions.lock.toml'))['uwphost']?['sha256']
    if (-not $pin) {
        if (Test-Path $exe) { return }
        throw 'No UWP host binary: no release pins one yet. Build it with: cargo build --release (in uwphost\).'
    }
    if ((Test-Path $exe) -and (Get-FileHash $exe -Algorithm SHA256).Hash -eq $pin.ToUpperInvariant()) { return }
    $null = Install-WinarchyUwphostRelease
}

function Update-WinarchyUwphost {
    <# Migración de update --self: solo con el helper activado. #>
    if (-not (Test-Path (Join-Path (Get-WinarchyStateDir) 'uwphost.enabled'))) { return }
    try { Sync-WinarchyUwphostBinary }
    catch { Write-WinarchyWarn "Could not update the UWP host ($($_.Exception.Message)). Retry later with: winarchy uwphost on" }
}

function Get-WinarchyUwphostState {
    <# off | ready | starting | blocked | down, con el detalle que informa el propio helper en state\uwphost.status. #>
    $state = Get-WinarchyStateDir
    if (-not (Test-Path (Join-Path $state 'uwphost.enabled'))) { return [pscustomobject]@{ State = 'off'; Detail = 'not enabled' } }
    $file = Join-Path $state 'uwphost.status'
    $fields = @(if (Test-Path $file) { ([IO.File]::ReadAllText($file)).Trim().Split('|', 4) })
    $twinui = if ($fields.Count -ge 3) { "twinui $($fields[2])" } else { 'no status yet' }
    $reason = if ($fields.Count -ge 4 -and $fields[3]) { ": $($fields[3])" } else { '' }
    if (Test-Path (Join-Path $state 'uwphost.blocked')) { return [pscustomobject]@{ State = 'blocked'; Detail = "$twinui$reason" } }
    $id = 0
    $alive = $fields.Count -ge 2 -and [int]::TryParse($fields[0], [ref]$id) -and [bool](Get-Process -Id $id -ErrorAction SilentlyContinue | Where-Object ProcessName -eq 'winarchy-uwphost')
    $phase = if (-not $alive) { 'down' } elseif ($fields[1] -eq 'ready') { 'ready' } else { 'starting' }
    [pscustomobject]@{ State = $phase; Detail = "$twinui$reason" }
}

function Set-WinarchyUwphost {
    <# El cambio rige en el próximo inicio de sesión: la ventana de shell se reparte al arrancar. #>
    param([Parameter(Mandatory)][ValidateSet('on', 'off', 'retry')][string]$Action)
    $state = Get-WinarchyStateDir
    $flag = Join-Path $state 'uwphost.enabled'
    switch ($Action) {
        'on' {
            Sync-WinarchyUwphostBinary
            New-Item -ItemType Directory -Path $state -Force | Out-Null
            Set-WinarchyFileAtomic -Path $flag -Value '1'
        }
        'off' { Remove-Item $flag -Force -ErrorAction SilentlyContinue }
        'retry' { Remove-Item (Join-Path $state 'uwphost.blocked') -Force -ErrorAction SilentlyContinue }
    }
}
