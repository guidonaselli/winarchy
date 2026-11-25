# Salida de emergencia sin el módulo de Winarchy: devuelve Explorer como shell del usuario.
# Se puede lanzar desde Administrador de tareas > Ejecutar nueva tarea (scripts\Restore-Explorer.cmd).
$ErrorActionPreference = 'Stop'
$key = 'Software\Microsoft\Windows\CurrentVersion\Policies\System'
$recovery = Join-Path $PSScriptRoot '..\state\custom-shell-recovery.json'
$hive = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($key, $true)
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
if (-not (Get-Process explorer -ErrorAction SilentlyContinue)) { Start-Process explorer.exe }
Write-Host 'Explorer restored as the shell. Winarchy stays installed; `winarchy shell activate --yes` turns the shell back on.'
