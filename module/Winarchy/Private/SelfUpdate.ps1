# SelfUpdate.ps1 — actualización de Winarchy mismo (git + GitHub Releases).
# Eje separado del update de terceros (winget/scoop, ver Update.ps1):
#   winarchy update          -> terceros + aviso si hay nueva versión de Winarchy
#   winarchy update --self    -> git pull --ff-only origin release + migración idempotente
# El chequeo es best-effort y cacheado (TTL 6 h) para respetar el rate limit anónimo de GitHub.

$script:WinarchyRepo = 'guidonaselli/winarchy'
$script:WinarchyReleaseBranch = 'release'
$script:WinarchyReleaseCheckTtlHours = 6

function Get-WinarchyVersion {
    <#
      Versión instalada. Fuente de verdad: ModuleVersion del .psd1 (espejado al tag en cada
      release). Si el checkout es git, anexa el describe para diagnóstico (no afecta la compa-
      ración semver). Devuelve un objeto con .Version (semver string) y .Describe.
    #>
    $root = Get-WinarchyRoot
    $manifest = Join-Path $root 'module\Winarchy\Winarchy.psd1'
    $version = $null
    if (Test-Path $manifest) {
        try { $version = (Import-PowerShellDataFile -Path $manifest).ModuleVersion } catch { }
    }
    $describe = $null
    if (Test-WinarchyGitCheckout) {
        try { $describe = (& git -C $root describe --tags --always 2>$null | Select-Object -First 1) } catch { }
    }
    [pscustomobject]@{ Version = $version; Describe = $describe }
}

function Test-WinarchyGitCheckout {
    <# True si el repo es un checkout de git con `git` disponible en PATH. #>
    if (-not (Get-Command git -ErrorAction SilentlyContinue)) { return $false }
    $root = Get-WinarchyRoot
    (& git -C $root rev-parse --is-inside-work-tree 2>$null) -eq 'true'
}

function Get-WinarchyLatestRelease {
    <#
      Última release publicada en GitHub. Best-effort: ante fallo de red/API/timeout devuelve
      $null sin lanzar (degrada en silencio). Cacheado con TTL en state/last-release-check;
      -Force ignora el cache (chequeo fresco antes de aplicar un self-update).
    #>
    param([switch]$Force)
    $cacheFile = Join-Path (Get-WinarchyStateDir) 'last-release-check'
    if (-not $Force -and (Test-Path $cacheFile)) {
        try {
            $cache = Get-Content $cacheFile -Raw | ConvertFrom-Json
            $age = (Get-Date) - [datetime]$cache.checkedAt
            if ($age.TotalHours -lt $script:WinarchyReleaseCheckTtlHours -and $cache.tag) {
                return $cache.tag
            }
        }
        catch { }
    }
    $tag = $null
    try {
        $uri = "https://api.github.com/repos/$($script:WinarchyRepo)/releases/latest"
        $resp = Invoke-RestMethod -Uri $uri -Headers @{ 'User-Agent' = 'winarchy' } -TimeoutSec 5
        $tag = $resp.tag_name
    }
    catch { return $null }
    if ($tag) {
        $payload = [pscustomobject]@{ checkedAt = (Get-Date).ToString('o'); tag = $tag }
        try { $payload | ConvertTo-Json | Set-Content -Path $cacheFile -Encoding UTF8 } catch { }
    }
    $tag
}

function Test-WinarchyUpdateAvailable {
    <#
      Compara la versión local con la última release. Devuelve un objeto con .Available (bool),
      .Local, .Latest. Best-effort: si no se pudo chequear, .Available = $false y .Latest = $null.
    #>
    param([switch]$Force)
    $local = (Get-WinarchyVersion).Version
    $latestTag = Get-WinarchyLatestRelease -Force:$Force
    $available = $false
    if ($local -and $latestTag) {
        $latestVer = $latestTag -replace '^v', ''
        try { $available = [version]$latestVer -gt [version]$local } catch { $available = $false }
    }
    [pscustomobject]@{ Available = $available; Local = $local; Latest = $latestTag }
}

function Write-WinarchyUpdateNotice {
    <# Aviso no intrusivo de versión nueva. Silencioso si no hay update o no se pudo chequear. #>
    param([switch]$Force)
    $check = Test-WinarchyUpdateAvailable -Force:$Force
    if ($check.Available) {
        Write-WinarchyWarn "Winarchy $($check.Latest) is available (you have $($check.Local)). Apply it with: winarchy update --self"
    }
    $check
}

function Invoke-WinarchySelfUpdate {
    <#
      Actualiza Winarchy mismo: fast-forward de la rama `release` + migración idempotente.
      - Aborta seguro si el directorio no es checkout de git (instruye vía manual).
      - Aborta si el working tree tiene cambios sin commitear en archivos versionados.
      - Tras el pull, reusa install.ps1 (sin -Activate) como migración.
    #>
    $root = Get-WinarchyRoot

    if (-not (Test-WinarchyGitCheckout)) {
        Write-WinarchyErr 'This Winarchy install is not a git checkout; self-update is unavailable.'
        Write-WinarchyInfo "Update manually by re-cloning: git clone https://github.com/$($script:WinarchyRepo) (preserve your overrides under config\ahk\user.ahk and your themes)."
        return
    }

    # Guard: no pisar cambios locales sin commitear en archivos versionados.
    $dirty = & git -C $root status --porcelain 2>$null
    if ($dirty) {
        Write-WinarchyErr 'Working tree has uncommitted changes to tracked files; aborting to avoid overwriting your edits.'
        Write-WinarchyInfo 'Commit or stash them (your customization should live in untracked overrides), then retry.'
        return
    }

    $before = (& git -C $root rev-parse HEAD 2>$null)

    Write-WinarchyInfo "Fetching latest from origin/$($script:WinarchyReleaseBranch)..."
    & git -C $root fetch origin $script:WinarchyReleaseBranch 2>&1 | Out-Host
    if ($LASTEXITCODE -ne 0) { Write-WinarchyErr 'git fetch failed; check your network/remote and retry.'; return }

    & git -C $root pull --ff-only origin $script:WinarchyReleaseBranch 2>&1 | Out-Host
    if ($LASTEXITCODE -ne 0) {
        Write-WinarchyErr "git pull --ff-only failed (local branch diverged from $($script:WinarchyReleaseBranch)). Resolve manually or re-clone."
        return
    }

    $after = (& git -C $root rev-parse HEAD 2>$null)
    if ($before -eq $after) {
        Write-WinarchyOk 'Already up to date; nothing to migrate.'
        return
    }

    Write-WinarchyInfo 'Running idempotent migration (install.ps1, mode preserved)...'
    $installer = Join-Path $root 'install.ps1'
    & pwsh -NoProfile -File $installer -SkipPackages
    if ($LASTEXITCODE -ne 0) {
        Write-WinarchyErr "Migration failed. Roll back with: git -C `"$root`" checkout $before  (then re-run install.ps1)."
        return
    }
    Write-WinarchyOk "Winarchy updated. Rollback if needed: git -C `"$root`" checkout $before"
}
