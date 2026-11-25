#Requires -Modules Pester

BeforeAll {
    $script:LifecycleSourceRoot = Split-Path $PSScriptRoot -Parent
    $script:LifecycleFixtureRoot = Join-Path $TestDrive 'shell-lifecycle'
    $modulePath = Join-Path $script:LifecycleFixtureRoot 'module/WinarchyLifecycle'
    New-Item -ItemType Directory -Path "$script:LifecycleFixtureRoot/module", "$script:LifecycleFixtureRoot/state" -Force | Out-Null
    Copy-Item "$script:LifecycleSourceRoot/module/Winarchy" $modulePath -Recurse
    Rename-Item (Join-Path $modulePath 'Winarchy.psd1') 'WinarchyLifecycle.psd1'
    Import-Module (Join-Path $modulePath 'WinarchyLifecycle.psd1') -Force
    if (-not ('WinarchyLifecycleMutexProbe' -as [type])) {
        Add-Type @'
using System;
using System.Threading;
public static class WinarchyLifecycleMutexProbe {
    public static bool IsHeld() {
        bool held = false;
        Exception error = null;
        Thread thread = new Thread(() => {
            try {
                using (Mutex mutex = new Mutex(false, @"Local\WinarchyShellLifecycle-" + System.Diagnostics.Process.GetCurrentProcess().SessionId)) {
                    bool acquired;
                    try { acquired = mutex.WaitOne(0); }
                    catch (AbandonedMutexException) { acquired = true; }
                    held = !acquired;
                    if (acquired) mutex.ReleaseMutex();
                }
            } catch (Exception ex) { error = ex; }
        });
        thread.Start();
        thread.Join();
        if (error != null) throw error;
        return held;
    }
}
'@
    }
}

AfterAll {
    Remove-Module WinarchyLifecycle -Force
}

Describe 'Session shell lifecycle coordination' {
    BeforeEach {
        InModuleScope WinarchyLifecycle {
            $script:lifecycleSession = [Diagnostics.Process]::GetCurrentProcess().SessionId
            $script:lifecycleIdentity = Get-WinarchyCurrentLogonIdentity
            $script:lifecycleState = Get-WinarchyStateDir
            $script:lifecycleMarker = Join-Path $script:lifecycleState "session-shell-$script:lifecycleSession"
            $script:lifecycleStatus = Join-Path $script:lifecycleState "shell-session-$script:lifecycleSession"
            $script:lifecycleInhibit = Join-Path $script:lifecycleState "shell-relaunch-inhibit-$script:lifecycleSession"
            Remove-Item "$script:lifecycleState/*" -Force -ErrorAction SilentlyContinue
            Set-Content $script:lifecycleMarker $script:lifecycleIdentity -NoNewline
            Set-Content $script:lifecycleStatus "4242|1|$script:lifecycleIdentity" -NoNewline
            $script:lifecycleAlive = $false
            $script:lifecyclePipeLeavesAlive = $false
            $script:lifecycleExplorerReady = $true
            $script:lifecycleLockSamples = [Collections.Generic.List[bool]]::new()
            $script:lifecycleHandoffInhibit = $null
            $script:lifecycleRecoveryInhibit = $null
            $script:lifecycleSavedWait = $script:ShellReturnWaitMs
            $script:ShellReturnWaitMs = 0
            Mock Get-Process {
                $script:lifecycleLockSamples.Add([WinarchyLifecycleMutexProbe]::IsHeld())
                if ($script:lifecycleAlive) {
                    [pscustomobject]@{
                        Id = 4242
                        ProcessName = 'winarchy-shell'
                        SessionId = $script:lifecycleSession
                        Path = Join-Path (Get-WinarchyRoot) 'state/bin/winarchy-shell.exe'
                    }
                }
            }
            Mock Test-WinarchyExplorerShellReady {
                $script:lifecycleLockSamples.Add([WinarchyLifecycleMutexProbe]::IsHeld())
                if (Test-Path $script:lifecycleInhibit) {
                    $script:lifecycleRecoveryInhibit = [IO.File]::ReadAllText($script:lifecycleInhibit).Trim()
                }
                $script:lifecycleExplorerReady
            }
            Mock Start-Process { } -ParameterFilter { $FilePath -like '*explorer.exe' }
            Mock Start-Sleep { }
            Mock Stop-Process { }
            Mock Wait-Process { }
            Mock New-Object {
                $pipe = [pscustomobject]@{}
                $pipe | Add-Member ScriptMethod Connect { param($timeout) }
                $pipe | Add-Member ScriptMethod WriteAsync {
                    param($bytes, $offset, $count)
                    if (Test-Path $script:lifecycleInhibit) {
                        $script:lifecycleHandoffInhibit = [IO.File]::ReadAllText($script:lifecycleInhibit).Trim()
                    }
                    if (-not $script:lifecyclePipeLeavesAlive) { $script:lifecycleAlive = $false }
                    [Threading.Tasks.Task]::CompletedTask
                }
                $pipe | Add-Member ScriptMethod Dispose { }
                $pipe
            } -ParameterFilter { $TypeName -eq 'System.IO.Pipes.NamedPipeClientStream' }
        }
    }

    AfterEach {
        InModuleScope WinarchyLifecycle { $script:ShellReturnWaitMs = $script:lifecycleSavedWait }
    }

    It 'holds the lifecycle mutex before sampling shell state and releases it on return' {
        InModuleScope WinarchyLifecycle {
            $result = Stop-WinarchyShell

            $result | Should -BeTrue
            $script:lifecycleLockSamples.Count | Should -BeGreaterThan 0
            @($script:lifecycleLockSamples | Where-Object { -not $_ }).Count | Should -Be 0
            [WinarchyLifecycleMutexProbe]::IsHeld() | Should -BeFalse
        }
    }

    It 'keeps the logon inhibit after successful crash recovery and clears only current shell state' {
        InModuleScope WinarchyLifecycle {
            $otherMarker = Join-Path $script:lifecycleState "session-shell-$($script:lifecycleSession + 1)"
            $otherStatus = Join-Path $script:lifecycleState "shell-session-$($script:lifecycleSession + 1)"
            Set-Content $otherMarker 'other-logon' -NoNewline
            Set-Content $otherStatus '4343|1|other-logon' -NoNewline

            Stop-WinarchyShell | Should -BeTrue

            Test-Path $script:lifecycleMarker | Should -BeFalse
            Test-Path $script:lifecycleStatus | Should -BeFalse
            Get-Content $script:lifecycleInhibit -Raw | Should -Be $script:lifecycleIdentity
            Get-Content $otherMarker -Raw | Should -Be 'other-logon'
            Get-Content $otherStatus -Raw | Should -Be '4343|1|other-logon'
        }
    }

    It 'publishes the logon inhibit before handing a live session shell back to Explorer' {
        InModuleScope WinarchyLifecycle {
            $script:lifecycleAlive = $true
            Stop-WinarchyShell | Should -BeTrue

            $script:lifecycleHandoffInhibit | Should -Be $script:lifecycleIdentity
        }
    }

    It 'preserves shell state and removes its new inhibit when Explorer recovery fails' {
        InModuleScope WinarchyLifecycle {
            $script:lifecycleExplorerReady = $false
            Stop-WinarchyShell | Should -BeFalse

            Get-Content $script:lifecycleMarker -Raw | Should -Be $script:lifecycleIdentity
            Get-Content $script:lifecycleStatus -Raw | Should -Be "4242|1|$script:lifecycleIdentity"
            $script:lifecycleRecoveryInhibit | Should -Be $script:lifecycleIdentity
            Test-Path $script:lifecycleInhibit | Should -BeFalse
            Should -Invoke Stop-Process -Times 0 -Exactly
            [WinarchyLifecycleMutexProbe]::IsHeld() | Should -BeFalse
        }
    }

    It 'shares one monotonic recovery budget across shell exit and Explorer readiness' {
        InModuleScope WinarchyLifecycle {
            $savedBudget = Get-Variable ShellRecoveryBudgetMs -Scope Script -ErrorAction SilentlyContinue
            $script:ShellRecoveryBudgetMs = 400
            $script:ShellReturnWaitMs = 800
            $script:lifecycleAlive = $true
            $script:lifecyclePipeLeavesAlive = $true
            $script:lifecycleExplorerReady = $false
            Mock Start-Sleep { param($Milliseconds) [Threading.Thread]::Sleep($Milliseconds) }
            try {
                $timer = [Diagnostics.Stopwatch]::StartNew()
                $result = Stop-WinarchyShell
                $timer.Stop()

                $result | Should -BeFalse
                $script:lifecycleAlive | Should -BeTrue
                Get-Content $script:lifecycleMarker -Raw | Should -Be $script:lifecycleIdentity
                Get-Content $script:lifecycleStatus -Raw | Should -Be "4242|1|$script:lifecycleIdentity"
                Test-Path $script:lifecycleInhibit | Should -BeFalse
                [WinarchyLifecycleMutexProbe]::IsHeld() | Should -BeFalse
                Should -Invoke Stop-Process -Times 0 -Exactly
                $timer.ElapsedMilliseconds | Should -BeLessThan 1000 -Because 'shell exit and Explorer readiness must consume the same 400 ms recovery budget'
            }
            finally {
                if ($savedBudget) { $script:ShellRecoveryBudgetMs = $savedBudget.Value }
                else { Remove-Variable ShellRecoveryBudgetMs -Scope Script }
            }
        }
    }

    It 'preserves a preexisting inhibit when Explorer recovery fails' {
        InModuleScope WinarchyLifecycle {
            Set-Content $script:lifecycleInhibit $script:lifecycleIdentity -NoNewline
            $script:lifecycleExplorerReady = $false
            Stop-WinarchyShell | Should -BeFalse

            Get-Content $script:lifecycleInhibit -Raw | Should -Be $script:lifecycleIdentity
        }
    }

    It 'does not report success with a watchdog replacement shell alive during Explorer recovery' {
        InModuleScope WinarchyLifecycle {
            $script:lifecycleExplorerReady = $false
            $script:ShellReturnWaitMs = 1000
            Mock Start-Process {
                $script:lifecycleAlive = $true
                $script:lifecycleExplorerReady = $true
            } -ParameterFilter { $FilePath -like '*explorer.exe' }

            $result = Stop-WinarchyShell

            $script:lifecycleAlive | Should -BeTrue
            $result | Should -BeFalse
            Get-Content $script:lifecycleMarker -Raw | Should -Be $script:lifecycleIdentity
            Get-Content $script:lifecycleStatus -Raw | Should -Be "4242|1|$script:lifecycleIdentity"
        }
    }
}

Describe 'Session shell pipe recovery deadline' {
    It 'returns from a non-reading shell pipe and releases recovery state on failure' {
        $probeRoot = Join-Path $TestDrive 'blocked-shell-pipe'
        $moduleRoot = Join-Path $probeRoot 'module/Winarchy'
        $stateRoot = Join-Path $probeRoot 'state'
        New-Item -ItemType Directory -Path (Split-Path $moduleRoot), $stateRoot -Force | Out-Null
        Copy-Item (Join-Path $script:LifecycleSourceRoot 'module/Winarchy') $moduleRoot -Recurse
        $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
        $identity = InModuleScope WinarchyLifecycle { Get-WinarchyCurrentLogonIdentity }
        $marker = Join-Path $stateRoot "session-shell-$sessionId"
        $status = Join-Path $stateRoot "shell-session-$sessionId"
        $inhibit = Join-Path $stateRoot "shell-relaunch-inhibit-$sessionId"
        $resultPath = Join-Path $probeRoot 'result.json'
        $shellExe = Join-Path $probeRoot 'winarchy-shell.exe'
        Copy-Item (Join-Path $env:WINDIR 'System32/cmd.exe') $shellExe
        $shellStart = [Diagnostics.ProcessStartInfo]::new($shellExe)
        $shellStart.UseShellExecute = $false
        $shellStart.CreateNoWindow = $true
        $shellStart.RedirectStandardInput = $true
        $shellStart.RedirectStandardOutput = $true
        $shellStart.RedirectStandardError = $true
        $shellStart.ArgumentList.Add('/d')
        $shellStart.ArgumentList.Add('/q')
        $shell = $null
        $recovery = $null
        $server = $null
        try {
            $shell = [Diagnostics.Process]::Start($shellStart)
            Set-Content $marker $identity -NoNewline
            $expectedStatus = "$($shell.Id)|1|$identity"
            Set-Content $status $expectedStatus -NoNewline
            # the test binds the real per-session pipe: with the live shell running it cannot, so it is skipped there
            try {
                $server = [IO.Pipes.NamedPipeServerStream]::new(
                    "winarchy-shell-$sessionId", [IO.Pipes.PipeDirection]::In, 1,
                    [IO.Pipes.PipeTransmissionMode]::Byte,
                    ([IO.Pipes.PipeOptions]::Asynchronous -bor [IO.Pipes.PipeOptions]::FirstPipeInstance), 1, 1)
            }
            catch [System.IO.IOException] { Set-ItResult -Skipped -Because 'the live shell owns the session pipe'; return }
            $connection = $server.WaitForConnectionAsync()
            $probePath = Join-Path $probeRoot 'recover.ps1'
            Set-Content $probePath @'
param($Root)
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $Root 'module/Winarchy/Winarchy.psd1') -Force
& (Get-Module Winarchy) { $script:ShellReturnWaitMs = 0 }
Add-Type 'public static class WinarchyExplorerShellReadiness { public static bool IsResponsive(int sessionId) { return false; } }'
$env:WINDIR = $Root
$timer = [Diagnostics.Stopwatch]::StartNew()
$result = Stop-WinarchyShell
@{ Result = $result; ElapsedMs = $timer.ElapsedMilliseconds } |
    ConvertTo-Json | Set-Content (Join-Path $Root 'result.pending')
Move-Item (Join-Path $Root 'result.pending') (Join-Path $Root 'result.json')
$null = [Console]::ReadLine()
'@
            $start = [Diagnostics.ProcessStartInfo]::new((Join-Path $PSHOME 'pwsh.exe'))
            $start.UseShellExecute = $false
            $start.CreateNoWindow = $true
            $start.RedirectStandardInput = $true
            $start.RedirectStandardOutput = $true
            $start.RedirectStandardError = $true
            foreach ($argument in '-NoLogo', '-NoProfile', '-File', $probePath, $probeRoot) {
                $start.ArgumentList.Add($argument)
            }
            $recovery = [Diagnostics.Process]::Start($start)
            $connection.Wait(10000) | Should -BeTrue -Because 'the real recovery command must reach the local shell pipe'
            $deadline = [Diagnostics.Stopwatch]::StartNew()
            while (-not (Test-Path $resultPath) -and -not $recovery.HasExited -and $deadline.ElapsedMilliseconds -lt 5000) {
                Start-Sleep -Milliseconds 20
            }
            Test-Path $resultPath | Should -BeTrue -Because 'a connected shell that never reads must not keep recovery blocked in Write or Flush'
            $result = Get-Content $resultPath -Raw | ConvertFrom-Json
            $result.Result | Should -BeFalse
            $result.ElapsedMs | Should -BeLessThan 5000
            $recovery.HasExited | Should -BeFalse
            $disconnectTimer = [Diagnostics.Stopwatch]::StartNew()
            $buffer = [byte[]]::new(1)
            $disconnected = $false
            do {
                $read = $server.ReadAsync($buffer, 0, 1)
                $remainingMs = [Math]::Max(1, 1000 - [int]$disconnectTimer.ElapsedMilliseconds)
                if (-not $read.Wait($remainingMs)) { break }
                if ($read.Result -eq 0) { $disconnected = $true; break }
            } while ($disconnectTimer.ElapsedMilliseconds -lt 1000)
            $disconnected | Should -BeTrue -Because 'disposing the timed-out client must eventually close its pipe endpoint'
            [WinarchyLifecycleMutexProbe]::IsHeld() | Should -BeFalse
            Test-Path $inhibit | Should -BeFalse
            Get-Content $marker -Raw | Should -Be $identity
            Get-Content $status -Raw | Should -Be $expectedStatus
            $shell.HasExited | Should -BeFalse
        }
        finally {
            if ($server) { $server.Dispose() }
            if ($recovery) {
                $recovery.StandardInput.Dispose()
                if (-not $recovery.WaitForExit(2000)) {
                    $recovery.Kill($true)
                    $null = $recovery.WaitForExit(2000)
                }
                $recovery.Dispose()
            }
            if ($shell) {
                $shell.StandardInput.Dispose()
                if (-not $shell.WaitForExit(2000)) {
                    $shell.Kill($true)
                    $null = $shell.WaitForExit(2000)
                }
                $shell.Dispose()
            }
        }
    }
}

Describe 'Session shell recovery bridge process' {
    BeforeAll {
        $script:BridgeModuleRoot = Join-Path $TestDrive 'module/Winarchy'
        New-Item -ItemType Directory -Path $script:BridgeModuleRoot -Force | Out-Null
        Set-Content (Join-Path $script:BridgeModuleRoot 'Winarchy.psd1') @'
@{
    RootModule = 'Winarchy.psm1'
    ModuleVersion = '1.0.0'
    FunctionsToExport = @('Stop-WinarchyShell')
}
'@
        Set-Content (Join-Path $script:BridgeModuleRoot 'Winarchy.psm1') @'
function Stop-WinarchyShell {
    $env:WINARCHY_TEST_SHELL_STOP_RESULT -eq 'true'
}
'@
    }

    It 'exits <ExitCode> when the imported module returns <Result>' -ForEach @(
        @{ Result = 'true'; ExitCode = 0 }
        @{ Result = 'false'; ExitCode = 1 }
    ) {
        $start = [Diagnostics.ProcessStartInfo]::new((Join-Path $PSHOME 'pwsh.exe'))
        $start.UseShellExecute = $false
        $start.CreateNoWindow = $true
        foreach ($argument in '-NoLogo', '-NoProfile', '-File', (Join-Path $script:LifecycleSourceRoot 'scripts/Stop-WinarchySessionShell.ps1'), '-Root', $TestDrive) {
            $start.ArgumentList.Add($argument)
        }
        $start.Environment['WINARCHY_TEST_SHELL_STOP_RESULT'] = $Result
        $process = [Diagnostics.Process]::Start($start)
        try {
            $process.WaitForExit()
            $process.ExitCode | Should -Be $ExitCode
        }
        finally { $process.Dispose() }
    }
}

Describe 'AHK bounded session recovery bridge' {
    BeforeAll {
        $script:BridgeAhkExe = @(
            (Join-Path $env:LOCALAPPDATA 'Programs/AutoHotkey/v2/AutoHotkey64.exe')
            (Join-Path $env:ProgramFiles 'AutoHotkey/v2/AutoHotkey64.exe')
        ) | Where-Object { Test-Path $_ } | Select-Object -First 1

        function Invoke-AhkRecoveryProbe {
            param([string]$Bridge, [string]$Body, [string]$Functions)
            $probeRoot = Join-Path $TestDrive ([guid]::NewGuid().ToString())
            New-Item -ItemType Directory -Path (Join-Path $probeRoot 'scripts') -Force | Out-Null
            Set-Content (Join-Path $probeRoot 'scripts/Stop-WinarchySessionShell.ps1') $Bridge
            $source = Get-Content (Join-Path $script:LifecycleSourceRoot 'config/ahk/winarchy.ahk') -Raw
            $helper = if ($Functions) { $Functions } else {
                [regex]::Match($source, '(?ms)^RunShellRecoveryBridge\([^\r\n]*\) \{.*?^\}').Value
            }
            $helper | Should -Not -BeNullOrEmpty
            $probe = @"
#Requires AutoHotkey v2.0
#SingleInstance Off
#Warn All, StdOut
OnError((err, mode) => (FileAppend(err.Message, '*'), ExitApp(2)))
RepoRoot := '$probeRoot'
ShellRecoveryBridgeHandle := 0
ShellRecoveryBridgeBusy := false
$helper
Report(value) {
    global RepoRoot
    FileAppend(value Chr(10), RepoRoot '\result.txt')
}
$Body
ExitApp()
"@
            $probePath = Join-Path $probeRoot 'probe.ahk'
            Set-Content $probePath $probe
            $start = [Diagnostics.ProcessStartInfo]::new($script:BridgeAhkExe)
            $start.UseShellExecute = $false
            $start.CreateNoWindow = $true
            $start.RedirectStandardOutput = $true
            $start.RedirectStandardError = $true
            $start.ArgumentList.Add('/ErrorStdOut=UTF-8')
            $start.ArgumentList.Add($probePath)
            $process = [Diagnostics.Process]::Start($start)
            try {
                if (-not $process.WaitForExit(20000)) {
                    $process.Kill($true)
                    $partial = Get-Content (Join-Path $probeRoot 'result.txt') -ErrorAction SilentlyContinue
                    throw "Isolated AHK recovery probe exceeded 20 seconds. Results: $($partial -join ', '); $($process.StandardOutput.ReadToEnd())$($process.StandardError.ReadToEnd())"
                }
                $diagnostics = $process.StandardOutput.ReadToEnd() + $process.StandardError.ReadToEnd()
                $process.ExitCode | Should -Be 0 -Because $diagnostics
                Get-Content (Join-Path $probeRoot 'result.txt')
            }
            finally {
                Set-Content (Join-Path $probeRoot 'release.txt') 'release'
                foreach ($childId in (Get-Content (Join-Path $probeRoot 'launches.txt') -ErrorAction SilentlyContinue)) {
                    try {
                        $child = [Diagnostics.Process]::GetProcessById([int]$childId)
                        try { $null = $child.WaitForExit(15000) }
                        finally { $child.Dispose() }
                    }
                    catch [ArgumentException] { }
                }
                $process.Dispose()
            }
        }
    }

    It 'returns <Expected> for a bridge that <Behavior>' -ForEach @(
        @{ Behavior = 'succeeds'; Bridge = 'param($Root); exit 0'; Expected = '1' }
        @{ Behavior = 'exits nonzero'; Bridge = 'param($Root); exit 7'; Expected = '0' }
        @{ Behavior = 'throws'; Bridge = 'param($Root); throw "fixture failure"'; Expected = '0' }
    ) {
        if (-not $script:BridgeAhkExe) { Set-ItResult -Skipped -Because 'AutoHotkey v2 is not installed.'; return }

        $actual = Invoke-AhkRecoveryProbe -Bridge $Bridge -Body 'Report(RunShellRecoveryBridge(10000))'

        $actual | Should -Be $Expected
    }

    It 'executes QuitStack with real status parsing and watchdog timer but records process and presentation effects on bridge failure' {
        if (-not $script:BridgeAhkExe) { Set-ItResult -Skipped -Because 'AutoHotkey v2 is not installed.'; return }
        $source = Get-Content (Join-Path $script:LifecycleSourceRoot 'config/ahk/winarchy.ahk') -Raw
        $quit = [regex]::Match($source, '(?ms)^QuitStack\(\) \{.*?^\}').Value
        $quit | Should -Not -BeNullOrEmpty
        $quit = $quit.Replace('TrayTip(', 'RecordTrayTip(').
            Replace("Komorebic('stop')", "Report('teardown:komorebi')").
            Replace("RunWait('taskkill /IM yasb.exe /F', , 'Hide')", "Report('teardown:yasb')").
            Replace("RunWait('taskkill /IM Flow.Launcher.exe /F', , 'Hide')", "Report('teardown:flow')").
            Replace("ShellIsland('quit')", "Report('teardown:shell')").
            Replace('ProcessWaitClose(shellPid, 1.5)', "RecordShellWait()").
            Replace('ProcessClose(shellPid)', "Report('teardown:close-shell')").
            Replace('ShowTaskbars()', "Report('teardown:taskbars')").
            Replace('ExitApp()', "Report('teardown:exit')")
        $functions = @($quit)
        foreach ($name in 'GetShellPid', 'FindSessionProcessPid', 'GetAssistantCommandKind') {
            $function = [regex]::Match($source, "(?ms)^${name}\([^\r\n]*\) \{.*?^\}").Value
            $function | Should -Not -BeNullOrEmpty
            $functions += $function
        }
        $identity = InModuleScope WinarchyLifecycle { Get-WinarchyCurrentLogonIdentity }
        $body = @'
ShellLogonIdentity := '__LOGON_IDENTITY__'
probePid := DllCall('GetCurrentProcessId', 'uint')
sessionId := 0
DllCall('ProcessIdToSessionId', 'uint', probePid, 'uint*', &sessionId, 'int')
ShellStatusFile := RepoRoot '\shell-session-' sessionId
ShellSessionMarker := RepoRoot '\session-shell-' sessionId
FileAppend(probePid '|1|' ShellLogonIdentity, ShellStatusFile)
ShellLifecycleStopping := false
ShellWatchBusy := false
RunShellRecoveryBridge() {
    global ShellLifecycleStopping, ShellWatchBusy
    Report('bridge:' ShellLifecycleStopping ':' ShellWatchBusy)
    return false
}
RecordTrayTip(text, title) {
    Report('notice:' title ':' text)
}
RecordShellWait() {
    Report('teardown:wait-shell')
    return true
}
YasbWatch() {
    Report('teardown:yasb-watch')
}
ShellWatch() {
    global ShellLifecycleStopping, ShellWatchBusy
    Report('watchdog:' ShellLifecycleStopping ':' ShellWatchBusy)
    ExitApp()
}
SetTimer(() => (Report('watchdog:missing'), ExitApp()), -10000)
QuitStack()
Report('returned:' ShellLifecycleStopping ':' ShellWatchBusy)
Sleep(11000)
'@
        $actual = Invoke-AhkRecoveryProbe -Functions ($functions -join "`n") -Body $body.Replace('__LOGON_IDENTITY__', $identity)

        $actual | Should -Be @(
            'bridge:1:1'
            'notice:Winarchy:Explorer recovery did not finish; Winarchy services are still running.'
            'returned:0:0'
            'watchdog:0:0'
        ) -Because 'the failure path must return with both guards cleared and a live watchdog, without any teardown event'
    }

    It 'recovers a missing session shell once and restores watchdog guards when its bridge fails' {
        if (-not $script:BridgeAhkExe) { Set-ItResult -Skipped -Because 'AutoHotkey v2 is not installed.'; return }
        $source = Get-Content (Join-Path $script:LifecycleSourceRoot 'config/ahk/winarchy.ahk') -Raw
        $functions = foreach ($name in 'ShellWatch', 'ReapShellRecoveryBridge', 'ShellRelaunchInhibited', 'GetShellPid', 'FindSessionProcessPid', 'GetAssistantCommandKind', 'ShellResponsive', 'RunShellRecoveryBridge') {
            $function = [regex]::Match($source, "(?ms)^${name}\([^\r\n]*\) \{.*?^\}").Value
            $function | Should -Not -BeNullOrEmpty
            if ($name -eq 'ShellWatch') {
                $function = $function.Replace('TrayTip(', 'RecordTrayTip(').
                    Replace('ProcessClose(pid)', "Report('teardown:close-shell')").
                    Replace("Run('`"' exe '`" --root `"' RepoRoot '`"' (sessionShell ? ' --session-shell' : ''), , 'Hide')", "Report('teardown:launch-shell')")
            }
            $function
        }
        $identity = InModuleScope WinarchyLifecycle { Get-WinarchyCurrentLogonIdentity }
        $bridge = @'
param($Root)
Add-Content (Join-Path $Root 'launches.txt') $PID
exit 7
'@
        $body = @'
ShellLogonIdentity := '__LOGON_IDENTITY__'
sessionId := 0
DllCall('ProcessIdToSessionId', 'uint', DllCall('GetCurrentProcessId', 'uint'), 'uint*', &sessionId, 'int')
StateDir := RepoRoot
ShellStatusFile := RepoRoot '\shell-session-' sessionId
ShellSessionMarker := RepoRoot '\session-shell-' sessionId
BarFile := RepoRoot '\bar'
YasbcExe := ''
FileAppend(ShellLogonIdentity, ShellSessionMarker)
FileAppend('0|1|' ShellLogonIdentity, ShellStatusFile)
ShellMisses := 0
ShellRelaunches := []
ShellBootUntil := 0
ShellWatchBusy := false
ShellLifecycleStopping := false
ShellRecoveryAttempted := false
RecordTrayTip(text, title) {
    Report('notice:' title ':' text)
}
ShowTaskbars() {
    Report('teardown:taskbars')
}
ShellWatch()
ShellWatch()
Report('returned:' ShellLifecycleStopping ':' ShellWatchBusy ':' ShellRecoveryAttempted)
ShellWatch()
ShellWatch()
SetTimer(ShellWatch, 0)
Report('launches:' StrSplit(Trim(FileRead(RepoRoot '\launches.txt'), Chr(13) Chr(10)), "`n").Length)
Report('state:' FileRead(ShellSessionMarker) ':' FileRead(ShellStatusFile))
'@
        $actual = Invoke-AhkRecoveryProbe -Bridge $bridge -Functions ($functions -join "`n") -Body $body.Replace('__LOGON_IDENTITY__', $identity)

        $actual | Should -Be @(
            'notice:Winarchy:Automatic Explorer recovery did not finish; Winarchy services remain running.'
            'returned:0:0:1'
            'launches:1'
            "state:${identity}:0|1|${identity}"
        ) -Because 'failed recovery must run once, preserve session state, clear both guards and return without teardown'
    }

    It 'preserves a live unresponsive session shell after failed recovery with <MarkerState>' -ForEach @(
        @{ MarkerState = 'a matching marker'; WriteMarker = 'FileAppend(ShellLogonIdentity, ShellSessionMarker)' }
        @{ MarkerState = 'only valid session-shell status'; WriteMarker = '' }
    ) {
        if (-not $script:BridgeAhkExe) { Set-ItResult -Skipped -Because 'AutoHotkey v2 is not installed.'; return }
        $source = Get-Content (Join-Path $script:LifecycleSourceRoot 'config/ahk/winarchy.ahk') -Raw
        $functions = foreach ($name in 'ShellWatch', 'ReapShellRecoveryBridge', 'ShellRelaunchInhibited', 'GetShellPid', 'FindSessionProcessPid', 'GetAssistantCommandKind', 'ShellResponsive', 'RunShellRecoveryBridge') {
            $function = [regex]::Match($source, "(?ms)^${name}\([^\r\n]*\) \{.*?^\}").Value
            $function | Should -Not -BeNullOrEmpty
            if ($name -eq 'ShellWatch') {
                $function = $function.Replace('TrayTip(', 'RecordTrayTip(').
                    Replace('ProcessClose(pid)', "Report('teardown:close-shell')").
                    Replace("Run('`"' exe '`" --root `"' RepoRoot '`"' (sessionShell ? ' --session-shell' : ''), , 'Hide')", "Report('teardown:launch-shell')")
            }
            $function
        }
        $identity = InModuleScope WinarchyLifecycle { Get-WinarchyCurrentLogonIdentity }
        $bridge = @'
param($Root)
Add-Content (Join-Path $Root 'launches.txt') $PID
exit 7
'@
        $body = @'
ShellLogonIdentity := '__LOGON_IDENTITY__'
sessionId := 0
DllCall('ProcessIdToSessionId', 'uint', DllCall('GetCurrentProcessId', 'uint'), 'uint*', &sessionId, 'int')
StateDir := RepoRoot
ShellStatusFile := RepoRoot '\shell-session-' sessionId
ShellSessionMarker := RepoRoot '\session-shell-' sessionId
BarFile := RepoRoot '\bar'
YasbcExe := ''
DirCreate(RepoRoot '\state\bin')
shellExe := RepoRoot '\state\bin\winarchy-shell.exe'
FileCopy(A_AhkPath, shellExe)
FileAppend('#Requires AutoHotkey v2.0' Chr(10) '#SingleInstance Off' Chr(10) 'Sleep(15000)', RepoRoot '\fake-shell.ahk')
Run('"' shellExe '" "' RepoRoot '\fake-shell.ahk"', , 'Hide', &shellPid)
try {
    __WRITE_MARKER__
    expectedStatus := shellPid '|1|' ShellLogonIdentity
    FileAppend(expectedStatus, ShellStatusFile)
    ShellMisses := 0
    ShellRelaunches := []
    ShellBootUntil := 1
    ShellWatchBusy := false
    ShellLifecycleStopping := false
    ShellRecoveryAttempted := false
    Report('responsive:' ShellResponsive(shellPid))
    ShellWatch()
    ShellWatch()
    Report('returned:' ShellLifecycleStopping ':' ShellWatchBusy ':' ShellRecoveryAttempted)
    ShellBootUntil := 1
    ShellWatch()
    ShellWatch()
    SetTimer(ShellWatch, 0)
    launches := FileExist(RepoRoot '\launches.txt') ? StrSplit(Trim(FileRead(RepoRoot '\launches.txt'), Chr(13) Chr(10)), "`n").Length : 0
    Report('launches:' launches)
    Report('alive:' (ProcessExist(shellPid) != 0))
    Report('status:' (FileRead(ShellStatusFile) = expectedStatus))
    Report('marker:' (FileExist(ShellSessionMarker) ? FileRead(ShellSessionMarker) = ShellLogonIdentity : 1))
} finally {
    SetTimer(ShellWatch, 0)
    if ProcessExist(shellPid)
        ProcessClose(shellPid)
}
RecordTrayTip(text, title) {
    Report('notice:' title ':' text)
}
ShowTaskbars() {
    Report('teardown:taskbars')
}
'@
        $body = $body.Replace('__LOGON_IDENTITY__', $identity).Replace('__WRITE_MARKER__', $WriteMarker)
        $actual = Invoke-AhkRecoveryProbe -Bridge $bridge -Functions ($functions -join "`n") -Body $body

        $actual | Should -Be @(
            'responsive:0'
            'notice:Winarchy:Automatic Explorer recovery did not finish; Winarchy services remain running.'
            'returned:0:0:1'
            'launches:1'
            'alive:1'
            'status:1'
            'marker:1'
        ) -Because 'a live session shell must survive failed Explorer recovery and later watchdog ticks without close or replacement attempts'
    }

    It 'bounds a pending bridge and prevents duplicate recovery until its late completion' {
        if (-not $script:BridgeAhkExe) { Set-ItResult -Skipped -Because 'AutoHotkey v2 is not installed.'; return }
        $bridge = @'
param($Root)
Add-Content (Join-Path $Root 'launches.txt') $PID
Set-Content (Join-Path $Root 'ready.txt') 'ready'
if (Test-Path (Join-Path $Root 'completed.txt')) { exit 7 }
$deadline = [datetime]::UtcNow.AddSeconds(12)
while (-not (Test-Path (Join-Path $Root 'release.txt'))) {
    if ([datetime]::UtcNow -gt $deadline) { exit 9 }
    Start-Sleep -Milliseconds 20
}
Set-Content (Join-Path $Root 'completed.txt') 'done'
exit 0
'@
        $body = @'
started := A_TickCount
Report(RunShellRecoveryBridge(100))
Report(A_TickCount - started < 5000)
deadline := A_TickCount + 10000
while !FileExist(RepoRoot '\ready.txt') && A_TickCount < deadline
    Sleep(20)
childPid := Integer(Trim(FileRead(RepoRoot '\launches.txt'), Chr(13) Chr(10)))
Report(ProcessExist(childPid) != 0)
Report(RunShellRecoveryBridge(100))
Report(StrSplit(Trim(FileRead(RepoRoot '\launches.txt'), Chr(13) Chr(10)), "`n").Length)
FileAppend('release', RepoRoot '\release.txt')
deadline := A_TickCount + 10000
while ProcessExist(childPid) && A_TickCount < deadline
    Sleep(20)
Report(ProcessExist(childPid) = 0)
Report(Trim(FileRead(RepoRoot '\completed.txt'), Chr(13) Chr(10)))
Report(RunShellRecoveryBridge(10000))
Report(StrSplit(Trim(FileRead(RepoRoot '\launches.txt'), Chr(13) Chr(10)), "`n").Length)
Report(RunShellRecoveryBridge(10000))
Report(StrSplit(Trim(FileRead(RepoRoot '\launches.txt'), Chr(13) Chr(10)), "`n").Length)
'@

        $actual = Invoke-AhkRecoveryProbe -Bridge $bridge -Body $body

        $actual | Should -Be @('0', '1', '1', '0', '1', '1', 'done', '0', '1', '0', '2')
    }
}

Describe 'AHK shell watchdog lifecycle contract' {
    BeforeAll {
        $script:LifecycleAhk = Get-Content (Join-Path $script:LifecycleSourceRoot 'config/ahk/winarchy.ahk') -Raw
        $script:LifecycleWatch = [regex]::Match($script:LifecycleAhk, '(?ms)^ShellWatch\(\) \{.*?^\}').Value
    }

    It 'recovers a crashed current-logon session shell before stopping services' {
        $quit = [regex]::Match($script:LifecycleAhk, '(?ms)^QuitStack\(\) \{.*?^\}').Value

        $quit | Should -Match '(?s)if !statusValid && ShellSessionMarker && FileExist\(ShellSessionMarker\).*sessionShell := Trim\(FileRead\(ShellSessionMarker\)\) = ShellLogonIdentity'
        $quit | Should -Match '(?s)if sessionShell \{.*RunShellRecoveryBridge\(\).*Komorebic\(''stop''\)'
    }

    It 'returns without teardown and restores the watchdog when the quit bridge fails' {
        $quit = [regex]::Match($script:LifecycleAhk, '(?ms)^QuitStack\(\) \{.*?^\}').Value
        $failure = [regex]::Match($quit, '(?ms)^        if !result \{.*?^        \}').Value

        $failure | Should -Match '(?s)ShellLifecycleStopping := false.*SetTimer\(ShellWatch, 5000\).*return'
        $failure | Should -Not -Match 'Komorebic|taskkill|ProcessClose|ExitApp'
        $quit | Should -Match '(?s)result := RunShellRecoveryBridge\(\).*if !result.*return.*Komorebic\(''stop''\)'
    }

    It 'schedules budget or executable failure recovery only once per AHK lifetime' {
        $script:LifecycleWatch | Should -Match '(?s)recoverSessionShell := false.*if sessionShell \{\s+if !ShellRecoveryAttempted\s+recoverSessionShell := true\s+\} else if !exe \|\| ShellRelaunches.Length >= 3'
        $script:LifecycleWatch | Should -Match '(?s)if recoverSessionShell \{\s+ShellRecoveryAttempted := true.*RunShellRecoveryBridge\(\)'
        ([regex]::Matches($script:LifecycleAhk, '(?m)^\s*ShellRecoveryAttempted := false')).Count | Should -Be 1
        ([regex]::Matches($script:LifecycleWatch, 'RunShellRecoveryBridge\(')).Count | Should -Be 1
    }

    It 'runs bridge recovery after releasing the shared lifecycle mutex' {
        $script:LifecycleWatch | Should -Match '(?s)finally \{\s+DllCall\(''ReleaseMutex''[^\r\n]*\s+DllCall\(''CloseHandle''[^\r\n]*\s+ShellWatchBusy := recoverSessionShell\s+\}\s+if recoverSessionShell \{.*RunShellRecoveryBridge\('
        $lockedRegion = [regex]::Match($script:LifecycleWatch, '(?s)try \{.*?finally \{').Value

        $lockedRegion | Should -Not -Match 'RunShellRecoveryBridge\(|RunWait\('
    }

    It 'initializes recovery state before protected inhibit and healthy-shell returns can reach finally' {
        $lockedWatch = $script:LifecycleWatch.Substring($script:LifecycleWatch.IndexOf('WaitForSingleObject'))

        $lockedWatch | Should -Match '(?s)recoverSessionShell := false\s+try \{\s+if ShellRelaunchInhibited\(\) \{\s+ShellMisses := 0\s+ShellBootUntil := 0\s+return\s+\}.*if pid && !modeMismatch && ShellResponsive\(pid\) \{\s+ShellMisses := 0\s+ShellBootUntil := 0\s+return\s+\}.*finally \{.*ShellWatchBusy := recoverSessionShell'
    }

    It 'keeps the watchdog busy across mutex release until bridge recovery completes' {
        $recovery = [regex]::Match($script:LifecycleWatch, '(?ms)^    \} finally \{.*').Value
        $beforeBridge = [regex]::Match($recovery, '(?s)^.*?result := RunShellRecoveryBridge\(').Value

        $beforeBridge | Should -Match '(?s)ShellWatchBusy := recoverSessionShell\s+\}\s+if recoverSessionShell \{.*ShellWatchBusy := true.*result := RunShellRecoveryBridge\('
        $beforeBridge | Should -Not -Match 'ShellWatchBusy := false'
        $recovery | Should -Match '(?s)result := RunShellRecoveryBridge\(\)\s+ShellLifecycleStopping := false\s+ShellWatchBusy := false\s+SetTimer\(ShellWatch, 5000\)'
    }

    It 'imports the bridge module from the supplied repository root' {
        $bridge = Get-Content (Join-Path $script:LifecycleSourceRoot 'scripts/Stop-WinarchySessionShell.ps1') -Raw

        $bridge | Should -Match 'param\(\[Parameter\(Mandatory\)\]\[string\]\$Root\)'
        $bridge | Should -Match '(?s)\$module = Join-Path \$Root ''module/Winarchy/Winarchy.psd1''\s+Import-Module \$module -Force'
    }

    It 'returns a failing bridge exit code when shell recovery fails' {
        $bridge = Get-Content (Join-Path $script:LifecycleSourceRoot 'scripts/Stop-WinarchySessionShell.ps1') -Raw

        $bridge | Should -Match '(?m)^\$ErrorActionPreference = ''Stop''$'
        $bridge | Should -Match '(?s)Import-Module \$module -Force\s+if \(-not \(Stop-WinarchyShell\)\) \{ exit 1 \}\s+exit 0\s*$'
    }

    It 'shares the session-local lifecycle mutex with PowerShell' {
        [regex]::IsMatch($script:LifecycleAhk, 'Local\\WinarchyShellLifecycle-') | Should -BeTrue
    }

    It 'checks the logon inhibit before the final watchdog relaunch' {
        $script:LifecycleAhk.Contains('shell-relaunch-inhibit-') | Should -BeTrue
        [regex]::IsMatch($script:LifecycleWatch, '(?is)inhibit.*Run\(') | Should -BeTrue
        [regex]::IsMatch($script:LifecycleAhk, '(?is)inhibit[^\r\n]*(?:\r?\n.*?){0,20}ShellLogonIdentity') | Should -BeTrue
    }

    It 'rejects a responsive normal shell under the mutex when the current-logon marker has invalid status' {
        $lockedWatch = $script:LifecycleWatch.Substring($script:LifecycleWatch.IndexOf('WaitForSingleObject'))
        $healthyRecheck = [regex]::Match($lockedWatch, '(?s)^.*?if [^\r\n]*ShellResponsive\(pid\)').Value
        $modeMismatch = [regex]::Match($healthyRecheck, '(?m)^\s*modeMismatch := ([^\r\n]+)').Groups[1].Value

        $healthyRecheck | Should -Match '(?s)sessionShell := false.*sessionShell := Trim\(FileRead\(ShellSessionMarker\)\) = ShellLogonIdentity.*statusValid := false, reportedSessionShell := false.*FileRead\(ShellStatusFile\).*status\[3\] = ShellLogonIdentity.*statusValid := true.*reportedSessionShell := status\[2\] = ''1''.*modeMismatch :=.*if (?=[^\r\n]*!modeMismatch)[^\r\n]*ShellResponsive\(pid\)'
        $modeMismatch | Should -Be 'sessionShell ? (!statusValid || !reportedSessionShell) : statusValid && reportedSessionShell != sessionShell'
    }

    It 'uses a nonblocking mutex wait before killing or relaunching the shell and releases it' {
        [regex]::IsMatch($script:LifecycleWatch, '(?s)WaitForSingleObject[^\r\n]*0.*ProcessClose\(pid\).*Run\(') | Should -BeTrue
        [regex]::IsMatch($script:LifecycleWatch, '(?s)finally.*ReleaseMutex') | Should -BeTrue
    }
}
