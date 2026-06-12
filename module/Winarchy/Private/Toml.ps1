# Toml.ps1 — parser TOML mínimo para theme.toml / games.toml / versions.lock.toml
# Soporta: [seccion], [[tabla-array]], key = "string" | numero | true/false, comentarios #.
# NO soporta: strings multilínea, arrays inline, tablas inline (no se usan en Winarchy).

function ConvertFrom-WinarchyToml {
    param([Parameter(Mandatory)][string]$Content)

    $root = @{}
    $current = $root

    foreach ($rawLine in ($Content -split "`r?`n")) {
        $line = $rawLine.Trim()
        if ($line.Length -eq 0 -or $line.StartsWith('#')) { continue }

        # [[tabla.array]]
        if ($line -match '^\[\[\s*([^\]]+?)\s*\]\]$') {
            $name = $Matches[1]
            if (-not $root.ContainsKey($name)) {
                $root[$name] = [System.Collections.Generic.List[object]]::new()
            }
            $current = @{}
            $root[$name].Add($current)
            continue
        }

        # [seccion]
        if ($line -match '^\[\s*([^\]]+?)\s*\]$') {
            $name = $Matches[1]
            if (-not $root.ContainsKey($name)) { $root[$name] = @{} }
            $current = $root[$name]
            continue
        }

        # key = value  (key puede venir entre comillas, ej. "LGUG2Z.komorebi")
        if ($line -match '^("(?<qkey>[^"]+)"|(?<key>[A-Za-z0-9_\-\.]+))\s*=\s*(?<val>.+)$') {
            $key = if ($Matches['qkey']) { $Matches['qkey'] } else { $Matches['key'] }
            $raw = $Matches['val'].Trim()

            if ($raw -match '^"(?<s>(?:[^"\\]|\\.)*)"') {
                $value = $Matches['s'] -replace '\\"', '"' -replace '\\\\', '\'
            }
            elseif ($raw -match '^(?<b>true|false)\b') {
                $value = [bool]::Parse($Matches['b'])
            }
            elseif ($raw -match '^(?<n>-?\d+(\.\d+)?)\b') {
                $value = if ($Matches['n'] -like '*.*') { [double]$Matches['n'] } else { [int64]$Matches['n'] }
            }
            else {
                $value = ($raw -split '#', 2)[0].Trim()
            }
            $current[$key] = $value
            continue
        }

        throw "Unsupported TOML line: '$line'"
    }
    $root
}

function Import-WinarchyToml {
    param([Parameter(Mandatory)][string]$Path)
    if (-not (Test-Path $Path)) { throw "TOML file not found: $Path" }
    ConvertFrom-WinarchyToml -Content (Get-Content -Path $Path -Raw -Encoding UTF8)
}
