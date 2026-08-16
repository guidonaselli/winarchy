# Invoke-WinarchyChecks.ps1 — gate único de Winarchy: sintaxis PowerShell + Pester.
# Lo corren igual el desarrollador (antes de commitear) y CI.

[CmdletBinding()]
param([switch]$SyntaxOnly)

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent

Write-Host 'Parsing PowerShell sources...' -ForegroundColor Cyan
$errorCount = 0
$files = Get-ChildItem -Path $root -Include '*.ps1', '*.psm1', '*.psd1' -Recurse -File |
    Where-Object { $_.FullName -notmatch '\\(state|backups|openspec|\.git|\.memsearch)\\' }
foreach ($file in $files) {
    $parseErrors = $null
    [System.Management.Automation.Language.Parser]::ParseFile($file.FullName, [ref]$null, [ref]$parseErrors) | Out-Null
    if ($parseErrors) {
        $errorCount += $parseErrors.Count
        Write-Host "  [XX] $($file.FullName)" -ForegroundColor Red
        foreach ($e in $parseErrors) { Write-Host "       line $($e.Extent.StartLineNumber): $($e.Message)" -ForegroundColor Red }
    }
}
if ($errorCount -gt 0) { throw "$errorCount parse error(s)." }
Write-Host "  [OK] $($files.Count) files parse cleanly" -ForegroundColor Green

if ($SyntaxOnly) { return }

Import-Module Pester -MinimumVersion 5.0
$config = New-PesterConfiguration
$config.Run.Path = $PSScriptRoot
$config.Run.Throw = $true
$config.Output.Verbosity = 'Normal'
Invoke-Pester -Configuration $config
