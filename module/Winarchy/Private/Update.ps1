# Update.ps1 — `winarchy update`: winget + scoop en una pasada; core solo con --core

$script:CoreWingetIds = @('LGUG2Z.komorebi', 'AmN.yasb')

function Invoke-WinarchyUpdate {
    param([switch]$Core, [switch]$Self)
    if ($Self) { Invoke-WinarchySelfUpdate; return }
    $lockPath = Join-Path (Get-WinarchyRoot) 'versions.lock.toml'
    $lock = Import-WinarchyToml -Path $lockPath

    if ($Core) {
        Write-WinarchyInfo 'Updating CORE components (komorebi, YASB)...'
        foreach ($id in $script:CoreWingetIds) {
            winget pin remove --id $id 2>$null | Out-Null
            winget upgrade --id $id --silent --accept-package-agreements --accept-source-agreements
            winget pin add --id $id 2>$null | Out-Null
        }
        Update-WinarchyLockfile -LockPath $lockPath
        Write-WinarchyOk 'Core updated and versions.lock.toml rewritten. Verify with `winarchy doctor` and check the changelogs.'
        return
    }

    Write-WinarchyInfo 'Updating winget packages (core excluded via pin)...'
    winget upgrade --all --silent --accept-package-agreements --accept-source-agreements

    if (Get-Command scoop -ErrorAction SilentlyContinue) {
        Write-WinarchyInfo 'Updating scoop packages...'
        scoop update *
    }

    # Aviso de updates core disponibles (sin aplicarlos)
    $pending = winget upgrade 2>$null | Out-String
    foreach ($id in $script:CoreWingetIds) {
        if ($pending -match [regex]::Escape($id)) {
            Write-WinarchyWarn "Core update available for $id (lockfile: komorebi=$($lock['core']['komorebi']), yasb=$($lock['core']['yasb'])). Apply it with: winarchy update --core"
        }
    }
    # Aviso no intrusivo si hay una versión nueva de Winarchy mismo (best-effort).
    Write-WinarchyUpdateNotice | Out-Null
    Write-WinarchyOk 'General update complete.'
}

function Update-WinarchyLockfile {
    param([Parameter(Mandatory)][string]$LockPath)
    $versions = @{}
    foreach ($pair in @(@('LGUG2Z.komorebi', 'komorebi'), @('AmN.yasb', 'yasb'))) {
        $out = winget list --id $pair[0] 2>$null | Out-String
        if ($out -match '(\d+\.\d+\.\d+)') { $versions[$pair[1]] = $Matches[1] }
    }
    if ($versions.Count -eq 0) { Write-WinarchyWarn 'Could not read installed versions; lockfile unchanged.'; return }
    $content = Get-Content $LockPath -Raw -Encoding UTF8
    foreach ($name in $versions.Keys) {
        $content = [regex]::Replace($content, "(?m)^$name\s*=\s*`"[^`"]*`"", "$name = `"$($versions[$name])`"")
    }
    Set-Content -Path $LockPath -Value $content -Encoding UTF8 -NoNewline
}
