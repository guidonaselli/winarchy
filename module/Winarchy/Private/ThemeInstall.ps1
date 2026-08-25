$script:ThemeFileAllowlist = @('theme.toml', 'preview.png', 'vscode.json', 'jetbrains.icls', 'wallpaper-engine.txt')

function Add-WinarchyTheme {
    param(
        [Parameter(Mandatory)][string]$Url,
        [string]$Name
    )
    if (-not (Get-Command git -ErrorAction SilentlyContinue)) { throw 'git is required to install a theme from a repository.' }

    if (-not $Name) { $Name = (($Url -split '[/\\]')[-1] -replace '\.git$', '') }
    if ($Name -notmatch '^[a-z0-9][a-z0-9-]*$') { throw "Theme name '$Name' must be lowercase letters, digits and dashes." }

    $dest = Join-Path (Get-WinarchyThemesDir) $Name
    if (Test-Path $dest) { throw "Theme '$Name' already exists. Remove $dest first." }

    $temp = Join-Path ([IO.Path]::GetTempPath()) "winarchy-theme-$([guid]::NewGuid())"
    try {
        & git clone --depth 1 --quiet $Url $temp 2>&1 | Out-Null
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path $temp)) { throw "git clone failed for $Url" }

        $toml = Join-Path $temp 'theme.toml'
        if (-not (Test-Path $toml)) { throw "The repository has no theme.toml at its root." }

        $theme = Expand-WinarchyThemePalette -Theme (Import-WinarchyToml -Path $toml)
        $missing = @(Test-WinarchyThemeData -Theme $theme)
        if ($missing.Count) { throw "Invalid theme.toml: $($missing -join ', ')" }

        New-Item -ItemType Directory -Path $dest -Force | Out-Null
        foreach ($file in $script:ThemeFileAllowlist) {
            $src = Join-Path $temp $file
            if (Test-Path $src) { Copy-Item $src $dest -Force }
        }
        $backgrounds = Join-Path $temp 'backgrounds'
        if (Test-Path $backgrounds) { Copy-Item $backgrounds $dest -Recurse -Force }

        Write-WinarchyOk "Theme '$Name' installed. Apply it with: winarchy theme set $Name"
    }
    catch {
        if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
        throw
    }
    finally {
        if (Test-Path $temp) { Remove-Item $temp -Recurse -Force -ErrorAction SilentlyContinue }
    }
}
