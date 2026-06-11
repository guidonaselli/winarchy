# winarchy.ps1 — entrypoint de la CLI; importa el módulo del repo y despacha
[CmdletBinding()]
param([Parameter(ValueFromRemainingArguments)][string[]]$Arguments)

$module = Join-Path $PSScriptRoot '..\module\Winarchy\Winarchy.psd1'
Import-Module $module -Force
Invoke-Winarchy @Arguments
