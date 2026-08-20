# Autostart.ps1 — arranque del stack vía Scheduled Tasks At-LogOn (disparo más temprano
# que la carpeta Startup, que Explorer procesa tarde). Sin elevación, solo sesión
# interactiva. Fallback a .lnk en Startup si el registro de la tarea falla.
#
# Usa schtasks.exe (nativo) en vez del módulo ScheduledTasks: en algunas máquinas el
# subsistema CIM/MI está roto y los cmdlets *-ScheduledTask* fallan con
# "type initializer for 'Microsoft.Management.Infrastructure.Native...'". schtasks.exe
# no toca esa capa.

$script:WinarchyTaskFolder = 'Winarchy'

function Get-WinarchyAutostartComponents {
    <#
      Definición de los componentes de autostart. Devuelve solo los presentes
      (ejecutable encontrado). Cada item: Key, TaskName, LnkName, Exe, Arguments.
    #>
    $root = Get-WinarchyRoot
    $items = [System.Collections.Generic.List[object]]::new()

    # komorebi.exe directo (no `komorebic start`): start es flaky en algunas máquinas
    # (os error 1920: reintenta en loop y se cuelga al arrancar). Lanzar el exe directo
    # arranca al toque y hereda KOMOREBI_CONFIG_HOME del entorno. Ver Start-WinarchyKomorebi.
    $komorebiExe = Get-WinarchyKomorebiExe
    if ($komorebiExe) {
        # komorebi.exe es app de consola: lanzado directo por el Task Scheduler, Windows 11
        # le asigna Windows Terminal como host y deja una ventana de logs visible al arrancar.
        # `conhost --headless` lo escondía pero NO lo mantenía vivo (queda OFFLINE al arrancar).
        # El `Start-Process komorebi -Hidden` directo tampoco alcanzaba: komorebi paniquea al
        # arrancar muy temprano (Application Error 0xc0000409) y, lanzado una sola vez y sin log,
        # quedaba muerto hasta reiniciarlo a mano. Ahora un powershell oculto corre el launcher
        # scripts\Start-Komorebi.ps1, que espera al shell, reintenta y captura el log del panic.
        $ps = Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe'
        $launcher = Join-Path $root 'scripts\Start-Komorebi.ps1'
        $items.Add([pscustomobject]@{
            Key = 'komorebi'; TaskName = 'komorebi'; LnkName = 'Winarchy komorebi.lnk'
            Exe = $ps
            Arguments = "-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File `"$launcher`""
            Delay = 'PT0S'
        })
    }

    # YASB arranca con delay para que komorebi alcance a bindear su named pipe primero;
    # de lo contrario el widget de workspaces queda en blanco hasta que reconecta. El
    # delay es cosmético (YASB reconecta solo), solo achica la ventana de blank.
    $yasb = (Get-Command yasbc -ErrorAction SilentlyContinue).Source
    if ($yasb) {
        $items.Add([pscustomobject]@{
            Key = 'yasb'; TaskName = 'yasb'; LnkName = 'Winarchy YASB.lnk'
            Exe = $yasb; Arguments = 'start'; Delay = 'PT5S'
        })
    }

    # El reconciliador de slots vive con el mismo patrón que komorebi: un host oculto
    # corriendo el launcher, que espera a komorebi y se re-suscribe cuando vuelve. A
    # diferencia de Start-Komorebi.ps1 necesita pwsh: carga el módulo, que usa sintaxis de
    # PowerShell 7 y no parsea en Windows PowerShell 5.1.
    $pwsh = (Get-Command pwsh -ErrorAction SilentlyContinue).Source
    if ($komorebiExe -and $pwsh) {
        $slots = Join-Path $root 'scripts\Start-WindowSlots.ps1'
        $items.Add([pscustomobject]@{
            Key = 'window-slots'; TaskName = 'window-slots'; LnkName = 'Winarchy window slots.lnk'
            Exe = $pwsh
            Arguments = "-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File `"$slots`""
            Delay = 'PT10S'
        })
    }

    $ahkExe = Get-WinarchyAhkExe
    if ($ahkExe) {
        $items.Add([pscustomobject]@{
            Key = 'ahk'; TaskName = 'ahk'; LnkName = 'Winarchy hotkeys.lnk'
            Exe = $ahkExe; Arguments = "`"$root\config\ahk\winarchy.ahk`""; Delay = 'PT0S'
        })
    }

    $items
}

function Get-WinarchyTaskFullName {
    <# Nombre completo de la tarea en el árbol del Task Scheduler: \Winarchy\<name>. #>
    param([Parameter(Mandatory)][string]$TaskName)
    "\$script:WinarchyTaskFolder\$TaskName"
}

function Test-WinarchyTask {
    <# True si la tarea existe (vía schtasks /Query). #>
    param([Parameter(Mandatory)][string]$TaskName)
    $full = Get-WinarchyTaskFullName -TaskName $TaskName
    & schtasks.exe /Query /TN $full *> $null
    $LASTEXITCODE -eq 0
}

function Remove-WinarchyStartupShortcuts {
    <# Borra los .lnk legacy de Winarchy en la carpeta Startup. #>
    $startup = [Environment]::GetFolderPath('Startup')
    Remove-Item (Join-Path $startup 'Winarchy *.lnk') -Force -ErrorAction SilentlyContinue
}

function New-WinarchyStartupShortcut {
    <# Fallback: crea un .lnk en Startup para un componente. #>
    param([Parameter(Mandatory)][object]$Component)
    $startup = [Environment]::GetFolderPath('Startup')
    $shell = New-Object -ComObject WScript.Shell
    $lnk = $shell.CreateShortcut((Join-Path $startup $Component.LnkName))
    $lnk.TargetPath = $Component.Exe
    $lnk.Arguments = $Component.Arguments
    $lnk.Save()
}

function New-WinarchyTaskXml {
    <#
      XML de definición de tarea: trigger At-LogOn del usuario (delay por componente,
      ver Component.Delay), principal InteractiveToken + LeastPrivilege (sin elevar,
      solo sesión interactiva), MultipleInstancesPolicy IgnoreNew, sin límite de ejecución.
    #>
    param([Parameter(Mandatory)][object]$Component, [Parameter(Mandatory)][string]$User)
    $u = [System.Security.SecurityElement]::Escape($User)
    $cmd = [System.Security.SecurityElement]::Escape($Component.Exe)
    $arg = [System.Security.SecurityElement]::Escape($Component.Arguments)
    $delay = if ($Component.Delay) { $Component.Delay } else { 'PT0S' }
    @"
<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Winarchy autostart: $($Component.Key)</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>$u</UserId>
      <Delay>$delay</Delay>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>$u</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>false</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Enabled>true</Enabled>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>$cmd</Command>
      <Arguments>$arg</Arguments>
    </Exec>
  </Actions>
</Task>
"@
}

function Register-WinarchyAutostart {
    <#
      Registra el autostart de los componentes del stack como Scheduled Tasks At-LogOn del
      usuario actual, delay 0, sin elevar y solo en sesión interactiva. Migra borrando
      primero los .lnk legacy de Startup. Idempotente. Si el registro de una tarea
      falla, cae al .lnk en Startup para ese componente.
    #>
    Remove-WinarchyStartupShortcuts

    $user = "$env:USERDOMAIN\$env:USERNAME"
    $components = Get-WinarchyAutostartComponents
    if ($components.Count -eq 0) {
        Write-WinarchyWarn 'Autostart: ningún componente instalado.'
        return
    }

    foreach ($c in $components) {
        $xmlPath = Join-Path ([System.IO.Path]::GetTempPath()) "winarchy-task-$($c.TaskName).xml"
        try {
            Set-Content -Path $xmlPath -Value (New-WinarchyTaskXml -Component $c -User $user) -Encoding Unicode
            $full = Get-WinarchyTaskFullName -TaskName $c.TaskName
            & schtasks.exe /Create /TN $full /XML $xmlPath /F *> $null
            if ($LASTEXITCODE -ne 0) { throw "schtasks /Create salió con código $LASTEXITCODE" }
        }
        catch {
            Write-WinarchyWarn "Autostart $($c.Key): falló registrar la tarea ($($_.Exception.Message)). Fallback a Startup."
            try { New-WinarchyStartupShortcut -Component $c }
            catch { Write-WinarchyWarn "Autostart $($c.Key): fallback a Startup también falló: $($_.Exception.Message)" }
        }
        finally {
            Remove-Item $xmlPath -Force -ErrorAction SilentlyContinue
        }
    }
    Write-WinarchyOk 'Autostart registrado (Scheduled Tasks At-LogOn): komorebi, YASB, AHK'
}

function Unregister-WinarchyAutostart {
    <# Borra las Scheduled Tasks de Winarchy y los .lnk legacy de Startup. #>
    foreach ($c in Get-WinarchyAutostartComponents) {
        $full = Get-WinarchyTaskFullName -TaskName $c.TaskName
        & schtasks.exe /Delete /TN $full /F *> $null
    }
    Remove-WinarchyStartupShortcuts
}

function Get-WinarchyAutostartStatus {
    <#
      Estado del autostart por componente. Devuelve una hashtable Key -> bool
      (tarea registrada o .lnk de fallback presente).
    #>
    $startup = [Environment]::GetFolderPath('Startup')
    $status = @{}
    foreach ($c in Get-WinarchyAutostartComponents) {
        $hasTask = Test-WinarchyTask -TaskName $c.TaskName
        $hasLnk = Test-Path (Join-Path $startup $c.LnkName)
        $status[$c.Key] = ($hasTask -or $hasLnk)
    }
    $status
}
