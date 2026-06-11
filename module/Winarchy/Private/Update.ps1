# Update.ps1 — `winarchy update`: winget + scoop en una pasada; core solo con --core

$script:CoreWingetIds = @('LGUG2Z.komorebi', 'AmN.yasb')

function Invoke-WinarchyUpdate {
    param([switch]$Core)
    $lockPath = Join-Path (Get-WinarchyRoot) 'versions.lock.toml'
    $lock = Import-WinarchyToml -Path $lockPath

    if ($Core) {
        Write-WinarchyInfo 'Actualizando componentes CORE (komorebi, YASB)...'
        foreach ($id in $script:CoreWingetIds) {
            winget pin remove --id $id 2>$null | Out-Null
            winget upgrade --id $id --silent --accept-package-agreements --accept-source-agreements
            winget pin add --id $id 2>$null | Out-Null
        }
        Update-WinarchyLockfile -LockPath $lockPath
        Write-WinarchyOk 'Core actualizado y versions.lock.toml reescrito. Verificá con `winarchy doctor` y revisá changelogs.'
        return
    }

    Write-WinarchyInfo 'Actualizando paquetes winget (core excluido por pin)...'
    winget upgrade --all --silent --accept-package-agreements --accept-source-agreements

    if (Get-Command scoop -ErrorAction SilentlyContinue) {
        Write-WinarchyInfo 'Actualizando paquetes scoop...'
        scoop update *
    }

    # Aviso de updates core disponibles (sin aplicarlos)
    $pending = winget upgrade 2>$null | Out-String
    foreach ($id in $script:CoreWingetIds) {
        if ($pending -match [regex]::Escape($id)) {
            Write-WinarchyWarn "Update core disponible para $id (lockfile: komorebi=$($lock['core']['komorebi']), yasb=$($lock['core']['yasb'])). Aplicalo con: winarchy update --core"
        }
    }
    Write-WinarchyOk 'Update general completado.'
}

function Update-WinarchyLockfile {
    param([Parameter(Mandatory)][string]$LockPath)
    $versions = @{}
    foreach ($pair in @(@('LGUG2Z.komorebi', 'komorebi'), @('AmN.yasb', 'yasb'))) {
        $out = winget list --id $pair[0] 2>$null | Out-String
        if ($out -match '(\d+\.\d+\.\d+)') { $versions[$pair[1]] = $Matches[1] }
    }
    if ($versions.Count -eq 0) { Write-WinarchyWarn 'No pude leer versiones instaladas; lockfile sin cambios.'; return }
    $content = Get-Content $LockPath -Raw -Encoding UTF8
    foreach ($name in $versions.Keys) {
        $content = [regex]::Replace($content, "(?m)^$name\s*=\s*`"[^`"]*`"", "$name = `"$($versions[$name])`"")
    }
    Set-Content -Path $LockPath -Value $content -Encoding UTF8 -NoNewline
}
