<#
.SYNOPSIS
  Fija en extras/windhawk/mods.json el SHA256 de cada DLL publicada de cada mod curado.
.DESCRIPTION
  `winarchy extras add windhawk` verifica las DLL descargadas contra estos hashes antes de
  tocar el registro. Correrlo cada vez que cambie una version del manifiesto.
#>
[CmdletBinding()]
param([string]$ManifestPath = (Join-Path $PSScriptRoot '..\extras\windhawk\mods.json'))
$ErrorActionPreference = 'Stop'

$manifest = Get-Content $ManifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
$tmp = Join-Path ([IO.Path]::GetTempPath()) "winarchy-wh-$([guid]::NewGuid())"
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    $mods = foreach ($mod in $manifest.mods) {
        $hashes = [ordered]@{}
        foreach ($folder in '32', '64', 'arm64') {
            $file = Join-Path $tmp "$($mod.id)_$folder.dll"
            try { Invoke-WebRequest -Uri "https://mods.windhawk.net/mods/$($mod.id)/$($mod.version)_$folder.dll" -OutFile $file -UseBasicParsing }
            catch { continue }
            $hashes[$folder] = (Get-FileHash $file -Algorithm SHA256).Hash.ToLowerInvariant()
        }
        if ($hashes.Count -eq 0) { throw "No published DLL for $($mod.id) $($mod.version)" }
        $ordered = [ordered]@{}
        foreach ($p in $mod.PSObject.Properties) { if ($p.Name -ne 'sha256') { $ordered[$p.Name] = $p.Value } }
        $ordered['sha256'] = $hashes
        $ordered
    }
    $manifest.mods = @($mods)
    $manifest | ConvertTo-Json -Depth 10 | Set-Content -Path $ManifestPath -Encoding UTF8
    Write-Host "Pinned SHA256 for $(@($mods).Count) mods in $ManifestPath"
}
finally { Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue }
