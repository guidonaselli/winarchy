$script:ShellPipeConnectMs = 200
$script:ShellPipeWriteMs = 1000
$script:ShellQuitWaitMs = 1500
$script:ShellReturnWaitMs = 25000
$script:ShellRecoveryBudgetMs = 50000
$script:ShellPipeCancelWaitMs = 250
$script:WinarchyLogonIdentityLoaded = $false
$script:WinarchyLogonIdentity = $null

function Get-WinarchyCurrentLogonIdentity {
    if (-not $script:WinarchyLogonIdentityLoaded) {
        $script:WinarchyLogonIdentityLoaded = $true
        try {
            if (-not ('WinarchyLogonIdentity' -as [type])) {
                Add-Type -TypeDefinition @'
using System;
using System.Globalization;
using System.Runtime.InteropServices;
public static class WinarchyLogonIdentity {
    [StructLayout(LayoutKind.Sequential)] struct Luid { public uint Low; public int High; }
    [StructLayout(LayoutKind.Sequential)] struct TokenStatistics {
        public Luid TokenId, AuthenticationId;
        public long ExpirationTime;
        public uint TokenType, ImpersonationLevel, DynamicCharged, DynamicAvailable, GroupCount, PrivilegeCount;
        public Luid ModifiedId;
    }
    [StructLayout(LayoutKind.Sequential)] struct LsaUnicodeString { public ushort Length, MaximumLength; public IntPtr Buffer; }
    [StructLayout(LayoutKind.Sequential)] struct LogonSessionData {
        public uint Size;
        public Luid LogonId;
        public LsaUnicodeString UserName, LogonDomain, AuthenticationPackage;
        public uint LogonType, Session;
        public IntPtr Sid;
        public long LogonTime;
    }
    [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool CloseHandle(IntPtr handle);
    [DllImport("advapi32.dll", SetLastError = true)] static extern bool OpenProcessToken(IntPtr process, uint access, out IntPtr token);
    [DllImport("advapi32.dll", SetLastError = true)] static extern bool GetTokenInformation(IntPtr token, int infoClass, IntPtr info, uint size, out uint returned);
    [DllImport("secur32.dll")] static extern int LsaGetLogonSessionData(ref Luid id, out IntPtr data);
    [DllImport("secur32.dll")] static extern int LsaFreeReturnBuffer(IntPtr data);
    public static string Current() {
        IntPtr token;
        if (!OpenProcessToken(GetCurrentProcess(), 8, out token)) return null;
        try {
            int size = Marshal.SizeOf(typeof(TokenStatistics));
            IntPtr info = Marshal.AllocHGlobal(size);
            try {
                uint returned;
                if (!GetTokenInformation(token, 10, info, (uint)size, out returned) || returned < (uint)size) return null;
                TokenStatistics stats = (TokenStatistics)Marshal.PtrToStructure(info, typeof(TokenStatistics));
                IntPtr data;
                if (LsaGetLogonSessionData(ref stats.AuthenticationId, out data) != 0 || data == IntPtr.Zero) return null;
                try {
                    LogonSessionData session = (LogonSessionData)Marshal.PtrToStructure(data, typeof(LogonSessionData));
                    if (session.LogonTime <= 0) return null;
                    return String.Format(CultureInfo.InvariantCulture, "{0:x8}{1:x8}-{2:x16}", unchecked((uint)stats.AuthenticationId.High), stats.AuthenticationId.Low, unchecked((ulong)session.LogonTime));
                } finally { LsaFreeReturnBuffer(data); }
            } finally { Marshal.FreeHGlobal(info); }
        } finally { CloseHandle(token); }
    }
}
'@
            }
            $script:WinarchyLogonIdentity = [WinarchyLogonIdentity]::Current()
        }
        catch { $script:WinarchyLogonIdentity = $null }
    }
    $script:WinarchyLogonIdentity
}

function Get-WinarchyShellProcess {
    $status = Get-WinarchyShellStatus
    if ($status) { return Get-Process -Id ([uint32]$status[0]) -ErrorAction SilentlyContinue }
    $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
    $root = Get-WinarchyRoot
    $paths = @((Join-Path $root 'state\bin\winarchy-shell.exe'), (Join-Path $root 'shell\target\release\winarchy-shell.exe'))
    Get-Process -Name winarchy-shell -ErrorAction SilentlyContinue | Where-Object {
        if ($_.SessionId -ne $sessionId) { return $false }
        try { [IO.Path]::GetFullPath($_.Path) -in $paths } catch { $false }
    } | Select-Object -First 1
}

function Test-WinarchyExplorerShellReady {
    if (-not ('WinarchyExplorerShellReadiness' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
public static class WinarchyExplorerShellReadiness {
    [DllImport("user32.dll", CharSet = CharSet.Unicode, EntryPoint = "FindWindowExW")]
    static extern IntPtr FindWindowEx(IntPtr parent, IntPtr after, string className, string title);
    [DllImport("user32.dll")]
    static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
    [DllImport("user32.dll", SetLastError = true, EntryPoint = "SendMessageTimeoutW")]
    static extern IntPtr SendMessageTimeout(IntPtr window, uint message, IntPtr wParam, IntPtr lParam, uint flags, uint timeout, out IntPtr result);
    public static bool IsResponsive(int sessionId) {
        IntPtr previous = IntPtr.Zero;
        IntPtr window;
        while ((window = FindWindowEx(IntPtr.Zero, previous, "Shell_TrayWnd", null)) != IntPtr.Zero) {
            previous = window;
            uint processId;
            GetWindowThreadProcessId(window, out processId);
            if (processId == 0) continue;
            try {
                using (Process process = Process.GetProcessById((int)processId)) {
                    if (process.SessionId == sessionId && String.Equals(process.ProcessName, "explorer", StringComparison.OrdinalIgnoreCase)) {
                        IntPtr result;
                        if (SendMessageTimeout(window, 0, IntPtr.Zero, IntPtr.Zero, 0x22, 500, out result) != IntPtr.Zero) return true;
                    }
                }
            } catch { }
        }
        return false;
    }
}
'@
    }
    [WinarchyExplorerShellReadiness]::IsResponsive([Diagnostics.Process]::GetCurrentProcess().SessionId)
}

function Start-WinarchyExplorer {
    $explorer = Join-Path $env:WINDIR 'explorer.exe'
    if (-not (Test-Path -LiteralPath $explorer)) { throw 'Windows Explorer executable is missing.' }
    Start-Process -FilePath $explorer -ErrorAction Stop | Out-Null
}

function Get-WinarchyShellRecoveryRemainingMs {
    param([Diagnostics.Stopwatch]$Timer, [int]$MaximumMs)
    $remaining = $script:ShellRecoveryBudgetMs - [int]$Timer.ElapsedMilliseconds
    [Math]::Max(0, [Math]::Min($MaximumMs, $remaining))
}

function Wait-WinarchyExplorerShell {
    param([int]$TimeoutMs = $script:ShellReturnWaitMs)
    if (Test-WinarchyExplorerShellReady) { return $true }
    if ($TimeoutMs -le 0) { return $false }
    try { Start-WinarchyExplorer }
    catch { return $false }
    $timer = [Diagnostics.Stopwatch]::StartNew()
    while ($timer.ElapsedMilliseconds -lt $TimeoutMs) {
        $remaining = $TimeoutMs - [int]$timer.ElapsedMilliseconds
        if ($remaining -le 0) { break }
        Start-Sleep -Milliseconds ([Math]::Min(100, $remaining))
        if (Test-WinarchyExplorerShellReady) { return $true }
    }
    $false
}
function Enter-WinarchyShellLifecycle {
    $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
    $mutex = [Threading.Mutex]::new($false, "Local\WinarchyShellLifecycle-$sessionId")
    try {
        if (-not $mutex.WaitOne(1000)) {
            $mutex.Dispose()
            return
        }
    }
    catch [Threading.AbandonedMutexException] { }
    catch {
        $mutex.Dispose()
        return
    }
    $mutex
}

function Exit-WinarchyShellLifecycle {
    param([Parameter(Mandatory)][Threading.Mutex]$Mutex)
    try { $Mutex.ReleaseMutex() }
    finally { $Mutex.Dispose() }
}

function Get-WinarchyShellRelaunchInhibitPath {
    $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
    Join-Path (Get-WinarchyStateDir) "shell-relaunch-inhibit-$sessionId"
}

function Test-WinarchyShellRelaunchInhibited {
    $path = Get-WinarchyShellRelaunchInhibitPath
    if (-not (Test-Path -LiteralPath $path)) { return $false }
    try {
        $identity = Get-WinarchyCurrentLogonIdentity
        return $identity -and [IO.File]::ReadAllText($path).Trim() -eq $identity
    }
    catch { return $false }
}

function Clear-WinarchyShellRelaunchInhibit {
    $mutex = Enter-WinarchyShellLifecycle
    if (-not $mutex) { return $false }
    try {
        $path = Get-WinarchyShellRelaunchInhibitPath
        if (-not (Test-WinarchyShellRelaunchInhibited)) { return $true }
        Remove-Item -LiteralPath $path -Force
        return $true
    }
    catch { return $false }
    finally { Exit-WinarchyShellLifecycle -Mutex $mutex }
}


function Assert-WinarchyCustomShellPolicySafeForMaintenance {
    try { $policy = Get-WinarchyCustomShellPolicySnapshot -SubKey 'Software\Microsoft\Windows\CurrentVersion\Policies\System' }
    catch { throw "Could not inspect the CustomShell policy; refusing maintenance: $($_.Exception.Message)" }
    if (-not $policy.Present) { return }
    if ([uint32]$policy.Kind -notin 1, 2 -or ($policy.RawBytes.Length % 2)) {
        throw 'The CustomShell policy has an unsupported registry type or encoding; refusing maintenance.'
    }
    $command = [Text.Encoding]::Unicode.GetString($policy.RawBytes).TrimEnd([char]0)
    if ($command.Contains([char]0)) { throw 'The CustomShell policy contains malformed command data; refusing maintenance.' }
    if ([uint32]$policy.Kind -eq 2) { $command = [Environment]::ExpandEnvironmentVariables($command) }
    $command = $command.Trim()
    if (-not $command) { return }
    if ($command.StartsWith('"')) {
        $endQuote = $command.IndexOf('"', 1)
        if ($endQuote -lt 0) {
            if ($command -match '(?i)winarchy-shell\.exe') {
                throw 'The CustomShell policy may launch winarchy-shell.exe, but its command is malformed; refusing maintenance.'
            }
            return
        }
        $executable = $command.Substring(1, $endQuote - 1)
    }
    else {
        $executable = ($command -split '\s+', 2)[0]
    }

    $targetsWinarchy = [IO.Path]::GetFileName($executable) -ieq 'winarchy-shell.exe'
    if (-not $targetsWinarchy -and -not $command.StartsWith('"') -and
        $executable -match '^(?:[A-Za-z]:\\|\\\\|\.\\|/)' -and
        $command -match '(?i)[\\/]winarchy-shell\.exe(?:\s|$)') {
        $targetsWinarchy = $true
    }
    # con un registro de recuperación válido (`winarchy shell activate`) el valor previo se puede restaurar: el mantenimiento sigue
    if ($targetsWinarchy -and (Get-WinarchyCustomShellStatus).State -eq 'Installed') { return }
    if ($targetsWinarchy) {
        throw 'The CustomShell policy launches winarchy-shell.exe, but this version has no validated policy recovery record. Restore or disable that policy before uninstalling, rolling back, or updating Winarchy.'
    }
}

function Stop-WinarchyShell {
    param([switch]$Restart)
    $recoveryTimer = [Diagnostics.Stopwatch]::StartNew()
    $mutex = Enter-WinarchyShellLifecycle
    if (-not $mutex) {
        Write-WinarchyWarn 'Another shell lifecycle operation is running; retry later.'
        return $false
    }
    $inhibitCreated = $false
    $success = $false
    $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
    $identity = Get-WinarchyCurrentLogonIdentity
    $inhibitPath = Get-WinarchyShellRelaunchInhibitPath
    try {
        $process = Get-WinarchyShellProcess
        $status = Get-WinarchyShellStatus
        $sessionShell = if ($status) { $status[1] -eq '1' } else { Test-WinarchySessionShellMarker }
        if ($sessionShell -and -not $Restart -and -not (Test-WinarchyShellRelaunchInhibited)) {
            Set-WinarchyFileAtomic -Path $inhibitPath -Value $identity
            $inhibitCreated = $true
        }
        if (-not $process) {
            $waitMs = Get-WinarchyShellRecoveryRemainingMs -Timer $recoveryTimer -MaximumMs $script:ShellReturnWaitMs
            if (-not (Wait-WinarchyExplorerShell -TimeoutMs $waitMs)) {
                Write-WinarchyWarn 'Explorer could not be restored; Winarchy services are still running.'
                return $false
            }
            $alive = @(Get-WinarchyShellProcess)
            if ($alive) {
                $alive | Stop-Process -Force -ErrorAction SilentlyContinue
                $stopWaitMs = Get-WinarchyShellRecoveryRemainingMs -Timer $recoveryTimer -MaximumMs 2000
                if ($stopWaitMs -gt 0) { $null = $alive | Wait-Process -Timeout ([Math]::Max(1, [Math]::Ceiling($stopWaitMs / 1000))) -ErrorAction SilentlyContinue }
                if (Get-WinarchyShellProcess) {
                    Write-WinarchyWarn 'A Winarchy shell is still running after Explorer was restored.'
                    return $false
                }
                Restore-WinarchyNativeTaskbar
            }
            foreach ($path in (Join-Path (Get-WinarchyStateDir) "session-shell-$sessionId"), (Join-Path (Get-WinarchyStateDir) "shell-session-$sessionId")) {
                if (-not (Test-Path -LiteralPath $path)) { continue }
                $content = [IO.File]::ReadAllText($path).Trim()
                if (($path -like '*session-shell-*' -and $content -eq $identity) -or ($path -like '*shell-session-*' -and $content -match ('\|' + [regex]::Escape($identity) + '$'))) {
                    Remove-Item -LiteralPath $path -Force
                }
            }
            $success = $true
            return $true
        }
        $sessionShellRestart = $Restart -and $sessionShell
        $command = if ($sessionShellRestart) { 'reload' } elseif ($Restart) { 'restart' } elseif ($sessionShell) { 'return' } else { 'quit' }
        $sent = $false
        $write = $null
        $pipe = New-Object System.IO.Pipes.NamedPipeClientStream('.', "winarchy-shell-$sessionId", [System.IO.Pipes.PipeDirection]::Out, [System.IO.Pipes.PipeOptions]::Asynchronous)
        try {
            $connectMs = Get-WinarchyShellRecoveryRemainingMs -Timer $recoveryTimer -MaximumMs $script:ShellPipeConnectMs
            if ($connectMs -le 0) { throw [TimeoutException]::new('The session shell recovery deadline expired before connecting.') }
            $pipe.Connect($connectMs)
            $bytes = [Text.Encoding]::UTF8.GetBytes("{`"v`":1,`"id`":1,`"cmd`":`"$command`"}`n")
            $write = $pipe.WriteAsync($bytes, 0, $bytes.Length)
            $writeMs = Get-WinarchyShellRecoveryRemainingMs -Timer $recoveryTimer -MaximumMs $script:ShellPipeWriteMs
            if ($writeMs -le 0 -or -not $write.Wait($writeMs)) { throw [TimeoutException]::new('The session shell pipe write exceeded its deadline.') }
            $sent = $true
        }
        catch { }
        finally {
            $pipe.Dispose()
            if ($write -and -not $write.IsCompleted) {
                $cancelWaitMs = Get-WinarchyShellRecoveryRemainingMs -Timer $recoveryTimer -MaximumMs $script:ShellPipeCancelWaitMs
                if ($cancelWaitMs -gt 0) { try { $null = $write.Wait($cancelWaitMs) } catch { } }
            }
        }
        if ($sessionShellRestart) {
            if (-not $sent) { Write-WinarchyWarn 'The session shell could not reload its theme; leaving it running.' }
            $success = $sent
            return $sent
        }
        $waitMs = if ($command -eq 'return' -and $sent) { $script:ShellReturnWaitMs } else { $script:ShellQuitWaitMs }
        $waitMs = Get-WinarchyShellRecoveryRemainingMs -Timer $recoveryTimer -MaximumMs $waitMs
        $waitTimer = [Diagnostics.Stopwatch]::StartNew()
        while ($waitTimer.ElapsedMilliseconds -lt $waitMs) {
            if (-not (Get-WinarchyShellProcess)) { break }
            $remainingMs = $waitMs - [int]$waitTimer.ElapsedMilliseconds
            if ($remainingMs -le 0) { break }
            Start-Sleep -Milliseconds ([Math]::Min(50, $remainingMs))
        }
        if ($command -eq 'return') {
            $explorerWaitMs = Get-WinarchyShellRecoveryRemainingMs -Timer $recoveryTimer -MaximumMs $script:ShellReturnWaitMs
            if (-not (Wait-WinarchyExplorerShell -TimeoutMs $explorerWaitMs)) {
                Write-WinarchyWarn 'Explorer could not be restored; Winarchy is still running.'
                return $false
            }
            $alive = @(Get-WinarchyShellProcess)
            if ($alive) {
                $alive | Stop-Process -Force -ErrorAction SilentlyContinue
                $stopWaitMs = Get-WinarchyShellRecoveryRemainingMs -Timer $recoveryTimer -MaximumMs 2000
                if ($stopWaitMs -gt 0) { $null = $alive | Wait-Process -Timeout ([Math]::Max(1, [Math]::Ceiling($stopWaitMs / 1000))) -ErrorAction SilentlyContinue }
                if (Get-WinarchyShellProcess) {
                    Write-WinarchyWarn 'The session shell could not be stopped after Explorer was restored.'
                    return $false
                }
                Restore-WinarchyNativeTaskbar
            }
            foreach ($path in (Join-Path (Get-WinarchyStateDir) "session-shell-$sessionId"), (Join-Path (Get-WinarchyStateDir) "shell-session-$sessionId")) {
                if (-not (Test-Path -LiteralPath $path)) { continue }
                $content = [IO.File]::ReadAllText($path).Trim()
                if (($path -like '*session-shell-*' -and $content -eq $identity) -or ($path -like '*shell-session-*' -and $content -match ('\|' + [regex]::Escape($identity) + '$'))) {
                    Remove-Item -LiteralPath $path -Force
                }
            }
            $success = $true
            return $true
        }
        $alive = @(Get-WinarchyShellProcess)
        if ($alive) {
            $alive | Stop-Process -Force -ErrorAction SilentlyContinue
            $stopWaitMs = Get-WinarchyShellRecoveryRemainingMs -Timer $recoveryTimer -MaximumMs 2000
            if ($stopWaitMs -gt 0) { $null = $alive | Wait-Process -Timeout ([Math]::Max(1, [Math]::Ceiling($stopWaitMs / 1000))) -ErrorAction SilentlyContinue }
            $stillAlive = Get-WinarchyShellProcess
            Restore-WinarchyNativeTaskbar
            if ($stillAlive) {
                Write-WinarchyWarn 'The Winarchy shell is still running after it was stopped.'
                return $false
            }
        }
        $success = $true
        $true
    }
    finally {
        if (-not $success -and $inhibitCreated -and (Test-WinarchyShellRelaunchInhibited)) {
            Remove-Item -LiteralPath $inhibitPath -Force -ErrorAction SilentlyContinue
        }
        Exit-WinarchyShellLifecycle -Mutex $mutex
    }
}

function Get-WinarchyShellExe {
    <# Binario de la shell nativa: el instalado (verificado) y, en desarrollo, el build local. #>
    $root = Get-WinarchyRoot
    foreach ($candidate in (Join-Path $root 'state\bin\winarchy-shell.exe'), (Join-Path $root 'shell\target\release\winarchy-shell.exe')) {
        if (Test-Path $candidate) { return $candidate }
    }
}

function Get-WinarchyShellStatus {
    $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
    $path = Join-Path (Get-WinarchyStateDir) "shell-session-$sessionId"
    if (-not (Test-Path $path)) { return }
    try { $parts = [IO.File]::ReadAllText($path).Trim() -split '\|' }
    catch { return }
    if ($parts.Count -ne 3 -or $parts[0] -notmatch '^\d+$' -or $parts[1] -notin '0', '1' -or $parts[2] -notmatch '^[0-9a-f]{16}-[0-9a-f]{16}$') { return }
    if ($parts[2] -ne (Get-WinarchyCurrentLogonIdentity)) { return }
    try { $shellPid = [uint32]::Parse($parts[0], [Globalization.CultureInfo]::InvariantCulture) }
    catch { return }
    if ($shellPid -eq 0) { return }
    $process = Get-Process -Id $shellPid -ErrorAction SilentlyContinue
    if (-not $process -or $process.ProcessName -ne 'winarchy-shell' -or $process.SessionId -ne $sessionId) { return }
    $parts
}

function Test-WinarchySessionShell {
    $status = Get-WinarchyShellStatus
    [bool]($status -and $status[1] -eq '1')
}

function Test-WinarchySessionShellMarker {
    if (Test-WinarchySessionShell) { return $true }
    $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
    $markerPath = Join-Path (Get-WinarchyStateDir) "session-shell-$sessionId"
    if (-not (Test-Path $markerPath)) { return $false }
    try {
        $marker = [IO.File]::ReadAllText($markerPath).Trim()
        $identity = Get-WinarchyCurrentLogonIdentity
        return $identity -and $marker -eq $identity
    }
    catch { return $false }
}

function Get-WinarchyShellArguments {
    $root = Get-WinarchyRoot
    $arguments = "--root `"$root`""
    if (Test-WinarchySessionShell) { $arguments += ' --session-shell' }
    $arguments
}

function Install-WinarchyShellBinary {
    <# Copia un winarchy-shell.exe a state\bin solo si su SHA256 coincide con el de versions.lock.toml. #>
    param([Parameter(Mandatory)][string]$Source)
    Assert-WinarchyCustomShellPolicySafeForMaintenance
    $root = Get-WinarchyRoot
    $lock = Import-WinarchyToml -Path (Join-Path $root 'versions.lock.toml')
    $expected = $lock['shell']?['sha256']
    if (-not $expected) { throw 'versions.lock.toml has no [shell] sha256 pinned: nothing to verify against.' }
    $actual = (Get-FileHash -Path $Source -Algorithm SHA256).Hash
    if ($actual -ne $expected.ToUpperInvariant()) {
        throw "winarchy-shell.exe SHA256 mismatch (expected $expected, got $actual): not installed."
    }
    Copy-WinarchyRunningBinary -Source $Source -Name 'winarchy-shell.exe'
}

function Copy-WinarchyRunningBinary {
    <# Deja $Source como state\bin\$Name aunque la versión anterior esté corriendo. #>
    param([Parameter(Mandatory)][string]$Source, [Parameter(Mandatory)][string]$Name)
    $dest = Join-Path (Join-Path (Get-WinarchyStateDir) 'bin') $Name
    New-Item -ItemType Directory -Path (Split-Path $dest -Parent) -Force | Out-Null
    # un .exe en uso no se puede pisar, pero sí renombrar; un .old en uso sobrevive a esta pasada
    Remove-Item "$dest.*.old" -Force -ErrorAction SilentlyContinue
    $old = "$dest.$([guid]::NewGuid().ToString('N')).old"
    if (Test-Path -LiteralPath $dest) { Move-Item -LiteralPath $dest -Destination $old -Force }
    try { Copy-Item -Path $Source -Destination $dest -Force -ErrorAction Stop }
    catch {
        if (Test-Path -LiteralPath $old) { Move-Item -LiteralPath $old -Destination $dest -Force }
        throw
    }
    $dest
}

function Install-WinarchyShellRelease {
    <# Baja el winarchy-shell.exe del Release de esta versión y lo instala solo si coincide con el SHA256 pinneado. #>
    $url = "https://github.com/$script:WinarchyRepo/releases/download/v$((Get-WinarchyVersion).Version)/winarchy-shell.exe"
    $tmp = Join-Path ([IO.Path]::GetTempPath()) "winarchy-shell-$([guid]::NewGuid().ToString('N')).exe"
    try {
        Invoke-WebRequest -Uri $url -OutFile $tmp -UseBasicParsing -ErrorAction Stop
        Install-WinarchyShellBinary -Source $tmp
    }
    finally { Remove-Item $tmp -Force -ErrorAction SilentlyContinue }
}

function Install-WinarchyYasb {
    <# YASB es opcional: se instala a pedido, a la versión que Winarchy fija. #>
    $lock = Import-WinarchyToml -Path (Join-Path (Get-WinarchyRoot) 'versions.lock.toml')
    Write-WinarchyInfo "Installing YASB $($lock['core']['yasb']) (optional status bar)..."
    winget install --id AmN.yasb --exact --source winget --silent --accept-package-agreements --accept-source-agreements --version $lock['core']['yasb']
    if ($LASTEXITCODE -notin 0, -1978334967, -1978335135, -1978335189) { throw "winget could not install YASB (exit code $LASTEXITCODE)." }
    winget pin add --id AmN.yasb --exact --source winget --accept-source-agreements 2>$null | Out-Null
    $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')
    if (-not (Get-Command yasbc -ErrorAction SilentlyContinue)) { throw 'YASB was installed but yasbc is not on PATH yet: open a new terminal and retry.' }
}

function Get-WinarchyTrayHookPath {
    <# La DLL de bandeja de YASB (MIT, firmada) que la shell carga en explorer para mostrar los íconos reales en el dock; $null si YASB no está instalado. #>
    foreach ($root in $env:ProgramW6432, $env:ProgramFiles) {
        if (-not $root) { continue }
        $path = Join-Path $root 'YASB\lib\YASBTrayHook.dll'
        if (Test-Path -LiteralPath $path) { return $path }
    }
}

function Initialize-WinarchyBar {
    <# Barra por defecto de la instalación: la shell nativa (YASB solo si ya estaba). Deja el binario verificado en su lugar. #>
    $bar = Get-WinarchyActiveBar
    if ($bar -eq 'native' -and -not (Test-WinarchyShellCurrent)) {
        try { $null = Install-WinarchyShellRelease }
        catch { Write-WinarchyWarn "Could not install the native shell ($($_.Exception.Message)). Retry later with: winarchy bar use native"; return }
    }
    $file = Join-Path (Get-WinarchyStateDir) 'bar'
    if (-not (Test-Path $file)) { Set-WinarchyFileAtomic -Path $file -Value $bar }
    Write-WinarchyOk "Bar: $bar"
}

function Test-WinarchyShellCurrent {
    <# El binario instalado coincide con el SHA256 pinneado; un build local de desarrollo también vale. #>
    $root = Get-WinarchyRoot
    if (Test-Path (Join-Path $root 'shell\target\release\winarchy-shell.exe')) { return $true }
    $exe = Join-Path $root 'state\bin\winarchy-shell.exe'
    $pin = (Import-WinarchyToml -Path (Join-Path $root 'versions.lock.toml'))['shell']?['sha256']
    [bool]($pin -and (Test-Path $exe) -and ((Get-FileHash $exe -Algorithm SHA256).Hash -eq $pin.ToUpperInvariant()))
}

function Set-WinarchyBar {
    param([Parameter(Mandatory)][ValidateSet('yasb', 'native')][string]$Name)
    if ($Name -eq 'yasb' -and (Test-WinarchySessionShellMarker)) {
        throw 'YASB cannot replace the Winarchy session shell. Exit the custom shell session before changing bars.'
    }
    if ($Name -eq 'yasb' -and -not (Get-Command yasbc -ErrorAction SilentlyContinue)) { Install-WinarchyYasb }
    if ($Name -eq 'native') {
        if (-not (Test-WinarchyShellCurrent)) { $null = Install-WinarchyShellRelease }
        $theme = Join-Path (Get-WinarchyRoot) 'config\shell\theme.json'
        $current = Join-Path (Get-WinarchyStateDir) 'current-theme'
        if (-not (Test-Path $theme) -and (Test-Path $current)) { Set-WinarchyTheme -Name (Get-Content $current -Raw).Trim() }
    }
    Set-WinarchyFileAtomic -Path (Join-Path (Get-WinarchyStateDir) 'bar') -Value $Name
    # El offset de komorebi depende de la barra activa: se re-renderiza solo komorebi (un theme set completo reiniciaría la shell dos veces).
    if (Test-Path (Get-WinarchyBarDockPath)) { Update-WinarchyKomorebiRules }
    if (Test-WinarchyTask -TaskName 'ahk') {
        Register-WinarchyAutostart
        $inactive = if ($Name -eq 'native') { 'yasb' } else { 'shell' }
        & schtasks.exe /Delete /TN (Get-WinarchyTaskFullName -TaskName $inactive) /F *> $null
        if ($Name -eq 'yasb') { & schtasks.exe /Run /TN (Get-WinarchyTaskFullName -TaskName 'net-icon') *> $null }
    }
    Invoke-WinarchyReload -Light
    Write-WinarchyOk "Bar: $Name"
}
