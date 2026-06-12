# ShellProfile.ps1 — hook quirúrgico en el $PROFILE de pwsh que dot-sourcea el
# perfil gestionado del repo (config/pwsh/profile.ps1). Mismo espíritu que el
# merge del settings.json: bloque marcado, idempotente, con snapshot previo.

$script:WinarchyProfileMarkerStart = '# >>> managed by winarchy >>>'
$script:WinarchyProfileMarkerEnd = '# <<< managed by winarchy <<<'

function Get-WinarchyShellProfilePath {
    # CurrentUserAllHosts de pwsh: lo calculamos a mano para no depender de que
    # esta sesión sea pwsh con $PROFILE poblado.
    Join-Path ([Environment]::GetFolderPath('MyDocuments')) 'PowerShell\profile.ps1'
}

function Install-WinarchyShellProfile {
    <# Inserta (o actualiza) el bloque marcado en el $PROFILE de pwsh. #>
    $root = Get-WinarchyRoot
    $profilePath = Get-WinarchyShellProfilePath
    $managed = Join-Path $root 'config\pwsh\profile.ps1'

    $block = @(
        $script:WinarchyProfileMarkerStart
        ". '$managed'"
        $script:WinarchyProfileMarkerEnd
    ) -join "`r`n"

    $existing = if (Test-Path $profilePath) { Get-Content $profilePath -Raw } else { '' }
    $pattern = '(?s)' + [regex]::Escape($script:WinarchyProfileMarkerStart) + '.*?' +
        [regex]::Escape($script:WinarchyProfileMarkerEnd)

    if ($existing -match $pattern) {
        if (($Matches[0] -replace '\r?\n', "`r`n") -eq $block) {
            Write-WinarchyOk "Profile hook already installed: $profilePath"
            return
        }
        New-WinarchySnapshot -Label 'shell-profile' -Path @($profilePath) | Out-Null
        $updated = [regex]::Replace($existing, $pattern, $block.Replace('$', '$$'))
        Set-Content -Path $profilePath -Value $updated -Encoding utf8NoBOM
        Write-WinarchyOk "Profile hook updated: $profilePath"
        return
    }

    if (Test-Path $profilePath) {
        New-WinarchySnapshot -Label 'shell-profile' -Path @($profilePath) | Out-Null
    }
    else {
        New-Item -ItemType Directory -Path (Split-Path $profilePath) -Force | Out-Null
    }
    $newContent = if ($existing.Trim()) { $existing.TrimEnd() + "`r`n`r`n" + $block + "`r`n" }
    else { $block + "`r`n" }
    Set-Content -Path $profilePath -Value $newContent -Encoding utf8NoBOM
    Write-WinarchyOk "Profile hook installed: $profilePath"
}

function Remove-WinarchyShellProfile {
    <# Quita el bloque marcado del $PROFILE de pwsh (deja el resto intacto). #>
    $profilePath = Get-WinarchyShellProfilePath
    if (-not (Test-Path $profilePath)) {
        Write-WinarchyInfo 'No pwsh $PROFILE found; nothing to remove.'
        return
    }
    $existing = Get-Content $profilePath -Raw
    $pattern = '(?s)\r?\n?' + [regex]::Escape($script:WinarchyProfileMarkerStart) + '.*?' +
        [regex]::Escape($script:WinarchyProfileMarkerEnd) + '\r?\n?'
    if ($existing -notmatch $pattern) {
        Write-WinarchyInfo 'Profile hook not present; nothing to remove.'
        return
    }
    New-WinarchySnapshot -Label 'shell-profile' -Path @($profilePath) | Out-Null
    $updated = [regex]::Replace($existing, $pattern, "`r`n").Trim()
    if ($updated) { Set-Content -Path $profilePath -Value ($updated + "`r`n") -Encoding utf8NoBOM }
    else { Remove-Item $profilePath }
    Write-WinarchyOk "Profile hook removed: $profilePath"
}

function Test-WinarchyShellProfileHook {
    <# True si el bloque marcado está presente en el $PROFILE de pwsh. #>
    $profilePath = Get-WinarchyShellProfilePath
    if (-not (Test-Path $profilePath)) { return $false }
    (Get-Content $profilePath -Raw) -match [regex]::Escape($script:WinarchyProfileMarkerStart)
}
