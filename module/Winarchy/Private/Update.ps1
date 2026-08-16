# Update.ps1 — `winarchy update`: actualiza SOLO lo que Winarchy declara; core solo con --core

# Componentes pinneados: no los toca `winarchy update`, solo `--core`. Flow y AHK están acá
# porque el stack depende de su comportamiento (ToggleFlow, dueño único de hotkeys) y un
# major suyo no debe entrar sin que alguien mire el changelog.
$script:CoreWingetIds = @{
    'LGUG2Z.komorebi'             = 'komorebi'
    'AmN.yasb'                    = 'yasb'
    'Flow-Launcher.Flow-Launcher' = 'flow'
    'AutoHotkey.AutoHotkey'       = 'ahk'
}

function Get-WinarchyDeclaredWingetIds {
    <# IDs de winget que Winarchy declara en versions.lock.toml. #>
    param([Parameter(Mandatory)][hashtable]$Lock)
    if (-not $Lock.ContainsKey('winget')) { return }
    $Lock['winget'].Keys | Sort-Object
}

function Update-WinarchyWingetPackage {
    <#
      Actualiza un paquete y devuelve el resultado. winget devuelve != 0 tanto por "no hay
      update" como por un fallo real (el más común en Windows: la app está corriendo y el
      instalador no puede reemplazar sus archivos), así que hay que mirar la salida.
    #>
    param([Parameter(Mandatory)][string]$Id)
    $out = winget upgrade --id $Id --exact --silent `
        --accept-package-agreements --accept-source-agreements 2>&1 | Out-String
    $status =
        if ($out -match 'Successfully installed|se instaló correctamente') { 'updated' }
        elseif ($out -match 'No available upgrade|No hay ninguna actualización') { 'current' }
        elseif ($out -match 'No installed package|No se encontró') { 'missing' }
        else { 'failed' }
    [pscustomobject]@{ Id = $Id; Status = $status; Output = $out.Trim() }
}

function Invoke-WinarchyUpdate {
    param([switch]$Core, [switch]$Self)
    if ($Self) { Invoke-WinarchySelfUpdate; return }
    $lockPath = Join-Path (Get-WinarchyRoot) 'versions.lock.toml'
    $lock = Import-WinarchyToml -Path $lockPath

    $ids = if ($Core) { @($script:CoreWingetIds.Keys | Sort-Object) }
           else { @(Get-WinarchyDeclaredWingetIds -Lock $lock | Where-Object { -not $script:CoreWingetIds.ContainsKey($_) }) }

    if ($Core) { Write-WinarchyInfo 'Updating CORE components (pinned; review the changelogs)...' }
    else { Write-WinarchyInfo 'Updating the packages Winarchy declares (core excluded)...' }

    $results = foreach ($id in $ids) {
        if ($Core) { winget pin remove --id $id 2>$null | Out-Null }
        $result = Update-WinarchyWingetPackage -Id $id
        if ($Core) { winget pin add --id $id 2>$null | Out-Null }
        Write-Host ("  {0,-12} {1}" -f $result.Status, $id)
        $result
    }
    $results = @($results)

    $scoopUpdated = 0
    if (-not $Core -and $lock.ContainsKey('scoop') -and (Get-Command scoop -ErrorAction SilentlyContinue)) {
        # Solo las apps que Winarchy declara. `scoop update *` actualizaba todo lo que
        # tuvieras instalado con scoop (ffmpeg, 7zip, lo que sea), que no es asunto nuestro.
        $listed = scoop list 2>$null | Out-String
        $targets = @($lock['scoop'].Keys | Where-Object { $listed -match "(?m)^\s*$([regex]::Escape($_))\s" })
        if ($targets.Count -gt 0) {
            Write-WinarchyInfo "Updating scoop packages declared by Winarchy: $($targets -join ', ')"
            scoop update @targets
            $scoopUpdated = $targets.Count
        }
    }

    # Los fallos se reportan: antes se perdían en la salida de winget y un paquete que no
    # se actualizó (típicamente por estar corriendo) pasaba desapercibido.
    $failed = @($results | Where-Object { $_.Status -eq 'failed' })
    $missing = @($results | Where-Object { $_.Status -eq 'missing' })
    foreach ($f in $failed) {
        Write-WinarchyErr "$($f.Id): update failed. If the app is running, close it and retry — the installer cannot replace files in use."
    }
    # "missing" solo significa que winget no lo administra: varias de estas herramientas
    # pueden venir de scoop, y ahí las actualiza el paso de scoop de más abajo. Avisar
    # "no instalado" sería un falso positivo — `winarchy doctor` es quien verifica
    # presencia real, por comando y no por gestor.
    foreach ($m in $missing) {
        Write-WinarchyInfo "$($m.Id): not managed by winget here (installed from another source, or absent — check with: winarchy doctor)."
    }

    if ($Core) {
        Update-WinarchyLockfile -LockPath $lockPath
        Write-WinarchyOk 'Core updated and versions.lock.toml rewritten. Verify with `winarchy doctor`.'
        return
    }

    # Aviso de updates core disponibles, sin aplicarlos. `winget upgrade` oculta los
    # paquetes pinneados, por eso se consulta cada uno con `winget show`.
    foreach ($id in ($script:CoreWingetIds.Keys | Sort-Object)) {
        $latest = if ((winget show --id $id --exact 2>$null | Out-String) -match '(?m)^Version:\s*(.+)$') { $Matches[1].Trim() } else { $null }
        $pinned = $lock['core'][$script:CoreWingetIds[$id]]
        if ($latest -and $pinned -and $latest -ne $pinned) {
            Write-WinarchyWarn "Core update available for ${id}: $pinned -> $latest. Apply it with: winarchy update --core"
        }
    }
    Write-WinarchyUpdateNotice | Out-Null
    $updated = @($results | Where-Object { $_.Status -eq 'updated' }).Count
    $summary = "$updated winget package(s) updated$(if ($scoopUpdated) { ", $scoopUpdated scoop app(s) checked" })."
    if ($failed.Count -gt 0) { Write-WinarchyErr "Update finished with $($failed.Count) failure(s); $summary" }
    else { Write-WinarchyOk "Update complete: $summary" }
}

function Update-WinarchyLockfile {
    param([Parameter(Mandatory)][string]$LockPath)
    $versions = @{}
    foreach ($id in $script:CoreWingetIds.Keys) {
        $out = winget list --id $id --exact 2>$null | Out-String
        if ($out -match '(\d+\.\d+\.\d+(\.\d+)?)') { $versions[$script:CoreWingetIds[$id]] = $Matches[1] }
    }
    if ($versions.Count -eq 0) { Write-WinarchyWarn 'Could not read installed versions; lockfile unchanged.'; return }
    $content = Get-Content $LockPath -Raw -Encoding UTF8
    foreach ($name in $versions.Keys) {
        $content = [regex]::Replace($content, "(?m)^$name\s*=\s*`"[^`"]*`"", "$name = `"$($versions[$name])`"")
    }
    Set-Content -Path $LockPath -Value $content -Encoding UTF8 -NoNewline
}
