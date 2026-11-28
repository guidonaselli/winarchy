# Salida de emergencia sin el módulo de Winarchy: devuelve Explorer como shell del usuario.
# Se puede lanzar desde Administrador de tareas > Ejecutar nueva tarea (scripts\Restore-Explorer.cmd).
param([switch]$Elevated, [string]$CallerSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value)
$ErrorActionPreference = 'Stop'
# la ventana elevada se cierra sola: sin esta pausa el error no se llega a leer
trap { Write-Host "$_" -ForegroundColor Red; if ($Elevated) { $null = Read-Host 'Press Enter to close' }; exit 1 }
$session = [Diagnostics.Process]::GetCurrentProcess().SessionId
function Test-Explorer { [bool](Get-Process explorer -ErrorAction SilentlyContinue | Where-Object SessionId -eq $session) }
$key = 'Software\Microsoft\Windows\CurrentVersion\Policies\System'
$recovery = Join-Path $PSScriptRoot '..\state\custom-shell-recovery.json'
# la política es la de quien activó (o la de quien llamó), aunque el administrador sea otra cuenta
$record = $null
if (Test-Path -LiteralPath $recovery) {
    # es el último recurso: un registro dañado no puede impedir quitar la política
    try {
        $record = Get-Content -LiteralPath $recovery -Raw | ConvertFrom-Json
        if ($record.prior.present) { $null = [Convert]::FromBase64String($record.prior.bytes), [uint32]$record.prior.kind }
    }
    catch {
        $record = $null
        Write-Warning 'The recovery record is damaged: only a Winarchy policy will be removed. Earlier copies are under backups\.'
    }
}
$prior = $record.prior
$sid = if ($record.owner) { $record.owner } else { $CallerSid }
$oddPrior = [bool]($prior.present -and [uint32]$prior.kind -notin 1, 2)
$account = try { [Security.Principal.SecurityIdentifier]::new($sid).Translate([Security.Principal.NTAccount]).Value } catch { $sid }
function Write-Result([bool]$changed) {
    if ($changed -and $oddPrior) { Write-Warning "The previous shell value of $account had an unusual type, so the policy was removed instead. A copy is under backups\." }
    elseif ($changed) { Write-Host "Shell policy restored for $account. Winarchy stays installed; ``winarchy shell activate --yes`` turns the shell back on." }
    else { Write-Warning "No Winarchy shell policy found for $account; nothing was changed. If another account activated it, sign in with that account and run this again." }
}
# Windows protege HKCU\...\Policies: tocarlo exige administrador. El hijo elevado edita el registro; informa y arranca Explorer quien llamó.
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin -and -not $Elevated) {
    $child = try { Start-Process powershell.exe -Verb RunAs -Wait -PassThru -ArgumentList '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$PSCommandPath`"", '-Elevated', '-CallerSid', $CallerSid }
    catch { Write-Warning "The administrator prompt was declined or failed: $($_.Exception.Message)" }
    # 3 = el hijo no encontró nada que cambiar
    if ($child -and $child.ExitCode -in 0, 3) { Write-Result ($child.ExitCode -eq 0) }
    else { Write-Warning 'The administrator step did not finish: the policy may not have changed. Run this again to check.' }
    if (-not (Test-Explorer)) { Start-Process explorer.exe }
    return
}
if (-not [Microsoft.Win32.Registry]::Users.OpenSubKey($sid)) { throw 'That account is not signed in, so its policy cannot be changed; sign in with it and run this again.' }
$hive = [Microsoft.Win32.Registry]::Users.OpenSubKey("$sid\$key", $true)
$changed = $false
if ($hive) {
    $current = [string]$hive.GetValue('Shell', $null)
    $changed = $true
    if ($prior -and $prior.present) {
        $text = [Text.Encoding]::Unicode.GetString([Convert]::FromBase64String($prior.bytes)).TrimEnd([char]0)
        switch ([uint32]$prior.kind) {
            1 { $hive.SetValue('Shell', $text, 'String') }
            2 { $hive.SetValue('Shell', $text, 'ExpandString') }
            default { $hive.DeleteValue('Shell', $false) }
        }
    }
    elseif ($current -match 'winarchy-shell\.exe' -or $prior) { $hive.DeleteValue('Shell', $false) }
    else { $changed = $false }
    $hive.Close()
}
if ($changed) { Remove-Item -LiteralPath $recovery -Force -ErrorAction SilentlyContinue }
if ($Elevated) { exit $(if ($changed) { 0 } else { 3 }) }
Write-Result $changed
if (-not (Test-Explorer)) { Start-Process explorer.exe }
