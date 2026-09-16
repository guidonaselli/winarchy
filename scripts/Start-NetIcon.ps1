# Start-NetIcon.ps1 — watcher residente que escribe el glifo de red a state\net-icon.flag.
#
# Reemplaza el `run_cmd: powershell.exe -File net-icon.ps1` que YASB relanzaba cada 15 s
# (~435 ms de arranque de intérprete por tick, para siempre): mismo patrón que
# Start-WindowSlots.ps1, corre una vez desde el autostart y vive en loop con Start-Sleep.
# El widget de YASB ahora solo hace `cmd /c type` sobre el flag, igual que los otros
# widgets basados en archivo (game_mode, stay_awake, winarchy_update).

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot '..\config\yasb\net-icon.ps1')

$stateDir = Join-Path (Split-Path -Parent $PSScriptRoot) 'state'
New-Item -ItemType Directory -Path $stateDir -Force | Out-Null
$flag = Join-Path $stateDir 'net-icon.flag'

# Instancia única: el mutex es el candado, mismo rol que el named pipe en window-slots.
$mutex = New-Object System.Threading.Mutex($false, 'Global\WinarchyNetIcon')
if (-not $mutex.WaitOne(0)) { return }

$utf8NoBom = New-Object System.Text.UTF8Encoding($false)

try {
    while ($true) {
        try { [System.IO.File]::WriteAllText($flag, (Get-WinarchyNetIconGlyph), $utf8NoBom) } catch { }
        Start-Sleep -Seconds 15
    }
}
finally {
    $mutex.ReleaseMutex()
}
