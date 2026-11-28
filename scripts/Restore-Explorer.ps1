# Salida de emergencia sin el módulo de Winarchy: devuelve Explorer como shell del usuario.
# Se puede lanzar desde Administrador de tareas > Ejecutar nueva tarea (scripts\Restore-Explorer.cmd).
param([switch]$Elevated, [string]$CallerSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value)
$ErrorActionPreference = 'Stop'
# Windows protege HKCU\...\Policies: tocarlo exige administrador. El hijo elevado edita el registro; Explorer lo arranca quien llamó.
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin -and -not $Elevated) {
    $child = Start-Process powershell.exe -Verb RunAs -Wait -PassThru -ArgumentList '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$PSCommandPath`"", '-Elevated', '-CallerSid', $CallerSid
    if ($child.ExitCode -ne 0) { throw 'The administrator step failed or was declined; the policy was not changed.' }
    if (-not (Get-Process explorer -ErrorAction SilentlyContinue)) { Start-Process explorer.exe }
    return
}
$key = 'Software\Microsoft\Windows\CurrentVersion\Policies\System'
$recovery = Join-Path $PSScriptRoot '..\state\custom-shell-recovery.json'
# la política es la de quien llamó, aunque el aviso de administrador se haya aceptado con otra cuenta
$hive = [Microsoft.Win32.Registry]::Users.OpenSubKey("$CallerSid\$key", $true)
if ($hive) {
    $current = [string]$hive.GetValue('Shell', $null)
    $prior = if (Test-Path -LiteralPath $recovery) { (Get-Content -LiteralPath $recovery -Raw | ConvertFrom-Json).prior } else { $null }
    if ($prior -and $prior.present) {
        $text = [Text.Encoding]::Unicode.GetString([Convert]::FromBase64String($prior.bytes)).TrimEnd([char]0)
        switch ([uint32]$prior.kind) {
            1 { $hive.SetValue('Shell', $text, 'String') }
            2 { $hive.SetValue('Shell', $text, 'ExpandString') }
            default { $hive.DeleteValue('Shell', $false); Write-Warning 'The previous value had an unusual type; the policy was removed instead.' }
        }
    }
    elseif ($current -match 'winarchy-shell\.exe' -or $prior) { $hive.DeleteValue('Shell', $false) }
    else { Write-Warning 'The Shell policy does not point at Winarchy; left as is.' }
    $hive.Close()
}
Remove-Item -LiteralPath $recovery -Force -ErrorAction SilentlyContinue
if (-not $Elevated -and -not (Get-Process explorer -ErrorAction SilentlyContinue)) { Start-Process explorer.exe }
Write-Host 'Explorer restored as the shell. Winarchy stays installed; `winarchy shell activate --yes` turns the shell back on.'
