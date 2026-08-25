# winarchy.ps1 — entrypoint de la CLI; importa el módulo del repo y despacha
[CmdletBinding()]
param([Parameter(ValueFromRemainingArguments)][string[]]$Arguments)

$module = Join-Path $PSScriptRoot '..\module\Winarchy\Winarchy.psd1'
Import-Module $module -Force
try { Invoke-Winarchy @Arguments }
catch {
    Write-WinarchyErr $_.Exception.Message
    exit 1
}
