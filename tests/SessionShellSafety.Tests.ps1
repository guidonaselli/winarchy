#Requires -Modules Pester

BeforeAll {
    $script:SourceRoot = Split-Path $PSScriptRoot -Parent
    $script:FixtureRoot = Join-Path $TestDrive 'session-shell-safety'
    $modulePath = Join-Path $script:FixtureRoot 'module/WinarchySafety'
    New-Item -ItemType Directory -Path "$script:FixtureRoot/module", "$script:FixtureRoot/state" -Force | Out-Null
    Copy-Item "$script:SourceRoot/module/Winarchy" $modulePath -Recurse
    Rename-Item (Join-Path $modulePath 'Winarchy.psd1') 'WinarchySafety.psd1'
    Import-Module (Join-Path $modulePath 'WinarchySafety.psd1') -Force
    & (Get-Module WinarchySafety) {
        if (-not (Get-Command Test-WinarchyExplorerShellReady -ErrorAction SilentlyContinue)) {
            function script:Test-WinarchyExplorerShellReady { throw 'Explorer readiness boundary must be mocked.' }
        }
        if (-not (Get-Command Start-WinarchyExplorer -ErrorAction SilentlyContinue)) {
            function script:Start-WinarchyExplorer { throw 'Explorer launch boundary must be mocked.' }
        }
    }
}

AfterAll {
    Remove-Module WinarchySafety -Force
}

Describe 'Session shell safety regressions' {
    It 'fails closed without shell markers when Explorer is unresponsive' {
        InModuleScope WinarchySafety {
            $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
            $state = Get-WinarchyStateDir
            Remove-Item (Join-Path $state "session-shell-$sessionId"), (Join-Path $state "shell-session-$sessionId") -Force -ErrorAction SilentlyContinue
            Mock Get-WinarchyCurrentLogonIdentity { '00000000000000ab-01dc000000000001' }
            Mock Get-Process { }
            Mock Test-WinarchyExplorerShellReady { $false }
            Mock Start-WinarchyExplorer { }
            Mock Restore-WinarchyNativeTaskbar { }
            Mock Start-Process { }
            Mock Stop-Process { }
            Mock Add-Type { }
            Mock Set-ItemProperty { }
            Mock New-ItemProperty { }
            Mock Remove-ItemProperty { }
            $savedWait = $script:ShellReturnWaitMs
            try {
                $script:ShellReturnWaitMs = 0

                $result = Stop-WinarchyShell

                Should -Invoke Start-Process -Times 0 -Exactly -Scope It
                Should -Invoke Stop-Process -Times 0 -Exactly -Scope It
                Should -Invoke Restore-WinarchyNativeTaskbar -Times 0 -Exactly -Scope It
                Should -Invoke Add-Type -Times 0 -Exactly -Scope It
                Should -Invoke Set-ItemProperty -Times 0 -Exactly -Scope It
                Should -Invoke New-ItemProperty -Times 0 -Exactly -Scope It
                Should -Invoke Remove-ItemProperty -Times 0 -Exactly -Scope It
                $result | Should -BeFalse
                Should -Invoke Test-WinarchyExplorerShellReady -Times 1 -Scope It
            }
            finally { $script:ShellReturnWaitMs = $savedWait }
        }
    }

    It 'returns false when a crashed session shell cannot restore Explorer' {
        InModuleScope WinarchySafety {
            $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
            $state = Get-WinarchyStateDir
            Mock Get-WinarchyCurrentLogonIdentity { '00000000000000ab-01dc000000000001' }
            Set-Content (Join-Path $state "session-shell-$sessionId") '00000000000000ab-01dc000000000001' -NoNewline
            Mock Get-Process { }
            Mock Test-WinarchyExplorerShellReady { $false }
            Mock Start-WinarchyExplorer { }
            $savedWait = $script:ShellReturnWaitMs
            try {
                $script:ShellReturnWaitMs = 10

                Stop-WinarchyShell | Should -BeFalse

                Should -Invoke Test-WinarchyExplorerShellReady -Times 1 -Scope It
                Should -Invoke Start-WinarchyExplorer -Times 1 -Exactly -Scope It
            }
            finally { $script:ShellReturnWaitMs = $savedWait }
        }
    }

    It 'accepts an already ready Explorer after the session shell has crashed' {
        InModuleScope WinarchySafety {
            $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
            $state = Get-WinarchyStateDir
            Mock Get-WinarchyCurrentLogonIdentity { '00000000000000ab-01dc000000000001' }
            Set-Content (Join-Path $state "session-shell-$sessionId") '00000000000000ab-01dc000000000001' -NoNewline
            Mock Get-Process { }
            Mock Test-WinarchyExplorerShellReady { $true }
            Mock Start-WinarchyExplorer { }

            Stop-WinarchyShell | Should -BeTrue

            Should -Invoke Test-WinarchyExplorerShellReady -Times 1 -Scope It
            Should -Invoke Start-WinarchyExplorer -Times 0 -Exactly -Scope It
        }
    }

    It 'rejects matching marker and status from a prior logon after the shell has exited' {
        InModuleScope WinarchySafety {
            $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
            $state = Get-WinarchyStateDir
            $priorIdentity = '0000000000000000-0000000000000000'
            Mock Get-WinarchyCurrentLogonIdentity { '00000000000000ab-01dc000000000001' }
            Set-Content (Join-Path $state "session-shell-$sessionId") $priorIdentity -NoNewline
            Set-Content (Join-Path $state "shell-session-$sessionId") "4242|1|$priorIdentity" -NoNewline
            Mock Get-Process { }

            Test-WinarchySessionShellMarker | Should -BeFalse
        }
    }

    It 'reports false when a session shell stays alive after Explorer return fails' {
        InModuleScope WinarchySafety {
            $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
            $state = Get-WinarchyStateDir
            Mock Get-WinarchyCurrentLogonIdentity { '00000000000000ab-01dc000000000001' }
            Set-Content (Join-Path $state "shell-session-$sessionId") '4242|1|00000000000000ab-01dc000000000001' -NoNewline
            Mock Get-Process {
                [pscustomobject]@{
                    Id = 4242
                    ProcessName = 'winarchy-shell'
                    SessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
                }
            }
            Mock Test-WinarchyExplorerShellReady { $false }
            Mock Start-WinarchyExplorer { }
            $script:returnRequest = $null
            Mock New-Object {
                $pipe = [pscustomobject]@{}
                $pipe | Add-Member ScriptMethod Connect { param($timeout) }
                $pipe | Add-Member ScriptMethod WriteAsync {
                    param($bytes, $offset, $count)
                    $script:returnRequest = [Text.Encoding]::UTF8.GetString($bytes, $offset, $count)
                    [Threading.Tasks.Task]::CompletedTask
                }
                $pipe | Add-Member ScriptMethod Flush { }
                $pipe | Add-Member ScriptMethod Dispose { }
                $pipe
            } -ParameterFilter { $TypeName -eq 'System.IO.Pipes.NamedPipeClientStream' }
            $savedWait = $script:ShellReturnWaitMs
            try {
                $script:ShellReturnWaitMs = 0
                $result = Stop-WinarchyShell

                $script:returnRequest | Should -Be "{`"v`":1,`"id`":1,`"cmd`":`"return`"}`n"
                $result | Should -BeOfType [bool]
                $result | Should -BeFalse
            }
            finally { $script:ShellReturnWaitMs = $savedWait }
        }
    }
}
