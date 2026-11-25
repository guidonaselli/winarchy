param([Parameter(Mandatory)][string]$Root)

$ErrorActionPreference = 'Stop'
$module = Join-Path $Root 'module/Winarchy/Winarchy.psd1'
Import-Module $module -Force
if (-not (Stop-WinarchyShell)) { exit 1 }
exit 0
