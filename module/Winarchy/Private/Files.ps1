function Start-WinarchyFiles {
    param([string]$Path)

    $root = Get-WinarchyRoot
    $find = {
        @(
            (Join-Path $root 'state\bin\winarchy-files.exe'),
            (Join-Path $root 'files\target\release\winarchy-files.exe')
        ) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
    }
    $exe = & $find
    if (-not $exe) {
        try { Install-WinarchyFilesRelease }
        catch { throw "winarchy-files.exe is not installed and the release download failed ($($_.Exception.Message)). Build it with: cargo build --release --manifest-path files/Cargo.toml" }
        $exe = & $find
    }
    if ($Path -and -not (Test-Path -LiteralPath $Path -PathType Container)) { throw "Folder not found: $Path" }

    $start = [Diagnostics.ProcessStartInfo]::new($exe)
    $start.UseShellExecute = $false
    $start.Environment['WINARCHY_FILES_THEME'] = Join-Path $root 'config\files\theme.json'
    if ($Path) { $start.ArgumentList.Add((Resolve-Path -LiteralPath $Path).Path) }
    $null = [Diagnostics.Process]::Start($start)
}

function Install-WinarchyFilesBinary {
    <# Copia un winarchy-files.exe a state\bin solo si su SHA256 coincide con el de versions.lock.toml; deja el anterior como .prev y lo restaura si la copia falla. #>
    param([Parameter(Mandatory)][string]$Source)
    $root = Get-WinarchyRoot
    $lock = Import-WinarchyToml -Path (Join-Path $root 'versions.lock.toml')
    $expected = $lock['files']?['sha256']
    if (-not $expected) { throw 'versions.lock.toml has no [files] sha256 pinned: nothing to verify against.' }
    $actual = (Get-FileHash -Path $Source -Algorithm SHA256).Hash
    if ($actual -ne $expected.ToUpperInvariant()) {
        throw "winarchy-files.exe SHA256 mismatch (expected $expected, got $actual): not installed."
    }
    $dest = Join-Path (Join-Path (Get-WinarchyStateDir) 'bin') 'winarchy-files.exe'
    New-Item -ItemType Directory -Path (Split-Path $dest -Parent) -Force | Out-Null
    $prev = "$dest.prev"
    if (Test-Path $dest) { Copy-Item -Path $dest -Destination $prev -Force }
    try { Copy-Item -Path $Source -Destination $dest -Force -ErrorAction Stop }
    catch {
        if (Test-Path $prev) { Copy-Item -Path $prev -Destination $dest -Force }
        throw
    }
}

function Install-WinarchyFilesRelease {
    $url = "https://github.com/$script:WinarchyRepo/releases/download/v$((Get-WinarchyVersion).Version)/winarchy-files.exe"
    $tmp = Join-Path ([IO.Path]::GetTempPath()) "winarchy-files-$([guid]::NewGuid().ToString('N')).exe"
    try {
        Invoke-WebRequest -Uri $url -OutFile $tmp -UseBasicParsing -ErrorAction Stop
        Install-WinarchyFilesBinary -Source $tmp
    }
    finally { Remove-Item $tmp -Force -ErrorAction SilentlyContinue }
}
