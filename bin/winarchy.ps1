# winarchy.ps1 — entrypoint de la CLI; importa el módulo del repo y despacha
[CmdletBinding()]
param([Parameter(ValueFromRemainingArguments)][string[]]$Arguments)

$module = Join-Path $PSScriptRoot '..\module\Winarchy\Winarchy.psd1'
Import-Module $module -Force
$clock = [Diagnostics.Stopwatch]::StartNew()
try {
    Invoke-Winarchy @Arguments
    # quedan registrados los comandos que cambian algo; de los demás, solo los fallos
    if ($Arguments -and $Arguments[0] -in 'theme', 'background', 'reload', 'layout', 'bar', 'taskbar', 'update', 'game-mode', 'mode', 'rules', 'shell', 'accent', 'heal-monitors', 'harden', 'panic') {
        Write-WinarchyLog "OK $Arguments ($($clock.ElapsedMilliseconds) ms)"
    }
}
catch {
    Write-WinarchyLog "FAIL $Arguments @ $($_.ScriptStackTrace)"
    Write-WinarchyErr $_.Exception.Message
    exit 1
}
