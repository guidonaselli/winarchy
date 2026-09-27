<#
.SYNOPSIS
  Exporta el set curado de mods de Windhawk de esta máquina a extras/windhawk/mods.json.
.DESCRIPTION
  Lee de HKLM\SOFTWARE\Windhawk\Engine\Mods\<id> la versión y los Settings completos
  (DWORD como número, string como texto) de cada mod pedido. Correrlo en una máquina con
  los mods ya afinados; `winarchy extras add windhawk` aplica exactamente este JSON.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string[]]$ModId,
    [string]$OutFile = (Join-Path $PSScriptRoot '..\extras\windhawk\mods.json')
)
$ErrorActionPreference = 'Stop'

$root = 'HKLM:\SOFTWARE\Windhawk\Engine\Mods'
$exe = "$env:ProgramFiles\Windhawk\windhawk.exe"
if (-not (Test-Path $exe)) { throw "Windhawk is not installed: $exe" }

$mods = foreach ($id in $ModId) {
    $key = Join-Path $root $id
    if (-not (Test-Path $key)) { throw "Mod not installed: $id" }
    $settings = [ordered]@{}
    $settingsKey = Get-Item (Join-Path $key 'Settings') -ErrorAction SilentlyContinue
    if ($settingsKey) {
        foreach ($name in ($settingsKey.GetValueNames() | Sort-Object)) { $settings[$name] = $settingsKey.GetValue($name) }
    }
    [ordered]@{ id = $id; version = (Get-ItemProperty $key).Version; settings = $settings }
}

New-Item -ItemType Directory -Path (Split-Path $OutFile) -Force | Out-Null
[ordered]@{
    windhawk = (Get-Item $exe).VersionInfo.ProductVersion
    mods     = @($mods)
} | ConvertTo-Json -Depth 10 | Set-Content -Path $OutFile -Encoding UTF8
Write-Host "Exported $(@($mods).Count) mods to $OutFile"
