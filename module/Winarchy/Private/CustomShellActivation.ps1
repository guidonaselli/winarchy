# Activación opt-in de la shell nativa como interfaz de sesión (política CustomShell por usuario).
# Contrato en docs/custom-shell-policy.md: se registra el valor previo exacto antes de escribir,
# se restaura solo si nadie más lo cambió, y surte efecto en el próximo inicio de sesión.

function Get-WinarchyCustomShellRecoveryPath {
    Join-Path (Get-WinarchyStateDir) 'custom-shell-recovery.json'
}

function ConvertTo-WinarchyCustomShellRecord {
    param([Parameter(Mandatory)][psobject]$Snapshot)
    [ordered]@{
        present = [bool]$Snapshot.Present
        kind    = if ($Snapshot.Present) { [uint32]$Snapshot.Kind } else { $null }
        bytes   = [Convert]::ToBase64String([byte[]]$Snapshot.RawBytes)
    }
}

function ConvertFrom-WinarchyCustomShellRecord {
    param([Parameter(Mandatory)]$Record)
    if (-not $Record.present) { return New-WinarchyCustomShellPolicySnapshot -Present $false }
    New-WinarchyCustomShellPolicySnapshot -Present $true -Kind ([uint32]$Record.kind) -RawBytes ([Convert]::FromBase64String([string]$Record.bytes))
}

function Read-WinarchyCustomShellRecovery {
    param([string]$Path = (Get-WinarchyCustomShellRecoveryPath))
    if (-not (Test-Path -LiteralPath $Path)) { return $null }
    try {
        $raw = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
        [pscustomobject]@{ Prior = (ConvertFrom-WinarchyCustomShellRecord $raw.prior); Installed = (ConvertFrom-WinarchyCustomShellRecord $raw.installed); Command = [string]$raw.command }
    }
    catch { throw "The CustomShell recovery record is unreadable ($($_.Exception.Message)); not touching the policy. Restore it by hand (docs/custom-shell-policy.md)." }
}

function Get-WinarchyCustomShellCommand {
    param([Parameter(Mandatory)][string]$ShellExe, [Parameter(Mandatory)][string]$Root)
    "`"$ShellExe`" --root `"$Root`" --session-shell"
}

function Get-WinarchyCustomShellStatus {
    param(
        [string]$SubKey = 'Software\Microsoft\Windows\CurrentVersion\Policies\System',
        [string]$RecoveryPath = (Get-WinarchyCustomShellRecoveryPath)
    )
    $current = Get-WinarchyCustomShellPolicySnapshot -SubKey $SubKey
    $record = Read-WinarchyCustomShellRecovery -Path $RecoveryPath
    $state = if ($record) { Get-WinarchyCustomShellPolicyState -Prior $record.Prior -Installed $record.Installed -Current $current }
             elseif ($current.Present) { 'External' } else { 'Inactive' }
    [pscustomobject]@{ State = $state; Current = $current; Record = $record }
}

function Enable-WinarchyCustomShell {
    <# Escribe la política CustomShell con la shell nativa. Efecto: próximo inicio de sesión. #>
    param(
        [string]$SubKey = 'Software\Microsoft\Windows\CurrentVersion\Policies\System',
        [string]$RecoveryPath = (Get-WinarchyCustomShellRecoveryPath),
        [string]$ShellExe = (Get-WinarchyShellExe),
        [string]$Root = (Get-WinarchyRoot)
    )
    if (-not $ShellExe -or -not (Test-Path -LiteralPath $ShellExe)) { throw 'winarchy-shell.exe not found: install or build it first (winarchy bar use native).' }
    if (-not (Test-Path -LiteralPath (Join-Path $Root 'config\shell\theme.json'))) { throw 'config\shell\theme.json is missing: run `winarchy theme set` first, the shell does not start without it.' }
    $status = Get-WinarchyCustomShellStatus -SubKey $SubKey -RecoveryPath $RecoveryPath
    if ($status.State -eq 'Installed') { return $status }
    if ($status.State -eq 'Conflict') { throw 'The CustomShell policy changed after Winarchy recorded it; refusing to overwrite. Check it with `winarchy shell status`.' }

    $command = Get-WinarchyCustomShellCommand -ShellExe $ShellExe -Root $Root
    $bytes = [Text.Encoding]::Unicode.GetBytes($command + [char]0)
    $installed = New-WinarchyCustomShellPolicySnapshot -Present $true -Kind String -RawBytes $bytes
    $prior = $status.Current

    # el registro de recuperación se escribe ANTES de tocar el registro: sin él no hay vuelta atrás verificable
    $record = [ordered]@{ command = $command; created = (Get-Date).ToString('o'); prior = (ConvertTo-WinarchyCustomShellRecord $prior); installed = (ConvertTo-WinarchyCustomShellRecord $installed) }
    $temp = "$RecoveryPath.tmp"
    $record | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $temp -Encoding UTF8
    Move-Item -LiteralPath $temp -Destination $RecoveryPath -Force
    $backup = Join-Path (Get-WinarchyBackupsDir) ((Get-Date).ToString('yyyyMMdd-HHmmss') + '-custom-shell')
    New-Item -ItemType Directory -Path $backup -Force | Out-Null
    Copy-Item -LiteralPath $RecoveryPath -Destination (Join-Path $backup 'custom-shell-recovery.json')

    try { $result = Set-WinarchyCustomShellPolicyIfUnchanged -Expected $prior -Desired $installed -SubKey $SubKey }
    catch {
        Remove-Item -LiteralPath $RecoveryPath -Force -ErrorAction SilentlyContinue
        throw
    }
    if (-not $result.Applied) {
        Remove-Item -LiteralPath $RecoveryPath -Force -ErrorAction SilentlyContinue
        throw 'The CustomShell policy changed while activating; nothing was written. Try again.'
    }
    Get-WinarchyCustomShellStatus -SubKey $SubKey -RecoveryPath $RecoveryPath
}

function Disable-WinarchyCustomShell {
    <# Restaura el valor previo exacto de la política si sigue siendo el que escribió Winarchy. #>
    param(
        [string]$SubKey = 'Software\Microsoft\Windows\CurrentVersion\Policies\System',
        [string]$RecoveryPath = (Get-WinarchyCustomShellRecoveryPath)
    )
    $status = Get-WinarchyCustomShellStatus -SubKey $SubKey -RecoveryPath $RecoveryPath
    switch ($status.State) {
        'Inactive' { return $status }
        'External' { throw 'The CustomShell policy was not set by Winarchy; leaving it alone.' }
        'Conflict' { throw 'The CustomShell policy no longer matches what Winarchy wrote or the value it replaced; not overwriting it. Fix it by hand (see docs/custom-shell-policy.md).' }
        'Installed' {
            $result = Set-WinarchyCustomShellPolicyIfUnchanged -Expected $status.Record.Installed -Desired $status.Record.Prior -SubKey $SubKey
            if (-not $result.Applied) { throw 'The CustomShell policy changed while restoring it; nothing was written. Try again.' }
        }
    }
    Remove-Item -LiteralPath $RecoveryPath -Force -ErrorAction SilentlyContinue
    Get-WinarchyCustomShellStatus -SubKey $SubKey -RecoveryPath $RecoveryPath
}

function Remove-WinarchySessionShellPolicy {
    <# Antes de desinstalar o hacer rollback: con la política puesta, el próximo inicio de sesión quedaría sin shell. #>
    param([switch]$DryRun)
    if ((Get-WinarchyCustomShellStatus).State -ne 'Installed') { return }
    if ($DryRun) { Write-Host '  [ ] Give Explorer back as the Windows shell (asks for administrator rights)'; return }
    $null = Invoke-WinarchyCustomShellChange -Action deactivate
    Set-WinarchyUwphost -Action off
    Write-WinarchyOk 'Explorer is the Windows shell again'
}

function Test-WinarchyElevated {
    ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Invoke-WinarchyCustomShellChange {
    <# Windows protege HKCU\...\Policies: un usuario estándar no puede crear ni borrar ahí, solo un administrador. Si hace falta, se relanza elevado (aviso UAC). #>
    param([Parameter(Mandatory)][ValidateSet('activate', 'deactivate')][string]$Action)
    $function = if ($Action -eq 'activate') { 'Enable-WinarchyCustomShell' } else { 'Disable-WinarchyCustomShell' }
    if (Test-WinarchyElevated) { $null = & $function; return Get-WinarchyCustomShellStatus }
    $manifest = Join-Path (Get-WinarchyRoot) 'module\Winarchy\Winarchy.psd1'
    $errorFile = Join-Path (Get-WinarchyStateDir) 'custom-shell-elevated.err'
    Remove-Item -LiteralPath $errorFile -Force -ErrorAction SilentlyContinue
    $quotedManifest, $quotedError = ($manifest, $errorFile).Replace("'", "''")
    # con credenciales de otra cuenta el HKCU del hijo sería el de ella: se rechaza antes de escribir
    $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $command = "try { if ([Security.Principal.WindowsIdentity]::GetCurrent().User.Value -ne '$sid') { throw 'The administrator prompt was answered with a different account; run this from an account that is an administrator. To get Explorer back use scripts\Restore-Explorer.cmd.' }; Import-Module '$quotedManifest' -Force; & (Get-Module Winarchy) { $function | Out-Null } } catch { Set-Content -LiteralPath '$quotedError' -Value `$_.Exception.Message; exit 1 }"
    Write-WinarchyInfo 'Changing this policy needs administrator rights: accept the Windows prompt (UAC).'
    $exe = (Get-Process -Id $PID).Path
    try { $process = Start-Process $exe -Verb RunAs -Wait -PassThru -WindowStyle Hidden -ArgumentList '-NoProfile', '-ExecutionPolicy', 'Bypass', '-Command', $command }
    catch { throw "The administrator prompt was declined or failed: $($_.Exception.Message)" }
    if ($process.ExitCode -ne 0) {
        $reason = if (Test-Path -LiteralPath $errorFile) { (Get-Content -LiteralPath $errorFile -Raw).Trim() } else { "exit code $($process.ExitCode)" }
        Remove-Item -LiteralPath $errorFile -Force -ErrorAction SilentlyContinue
        throw $reason
    }
    Get-WinarchyCustomShellStatus
}
