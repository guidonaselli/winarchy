#Requires -Modules Pester

BeforeAll {
    $root = Split-Path $PSScriptRoot -Parent
    Import-Module (Join-Path $root 'module\Winarchy\Winarchy.psd1') -Force
}

Describe 'CustomShell activation and recovery' {
    BeforeEach {
        # a throwaway HKCU key stands in for the real policy key: the tests never touch the real one
        $script:subKey = "Software\WinarchyTest\CustomShell-$([guid]::NewGuid())"
        $script:fakeRoot = Join-Path $TestDrive 'root'
        New-Item -ItemType Directory -Path (Join-Path $fakeRoot 'config\shell') -Force | Out-Null
        Set-Content (Join-Path $fakeRoot 'config\shell\theme.json') '{}'
        $script:exe = Join-Path $TestDrive 'winarchy-shell.exe'
        Set-Content $exe 'x'
        $script:recovery = Join-Path $TestDrive "recovery-$([guid]::NewGuid()).json"
        [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($subKey).Close()
    }
    AfterEach { [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($subKey, $false) }

    function Get-ShellValue { [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($subKey).GetValue('Shell', $null) }

    It 'writes the policy, records the absent prior value and removes it again' {
        InModuleScope Winarchy -Parameters @{ k = $subKey; r = $recovery; e = $exe; root = $fakeRoot } {
            param($k, $r, $e, $root)
            Mock Get-WinarchyBackupsDir { $TestDrive }
            (Enable-WinarchyCustomShell -SubKey $k -RecoveryPath $r -ShellExe $e -Root $root).State | Should -Be 'Installed'
            [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($k).GetValue('Shell') | Should -Be "`"$e`" --root `"$root`" --session-shell"
            (Disable-WinarchyCustomShell -SubKey $k -RecoveryPath $r).State | Should -Be 'Inactive'
            [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($k).GetValue('Shell', $null) | Should -BeNullOrEmpty
            Test-Path $r | Should -BeFalse
        }
    }

    It 'restores the exact previous value' {
        [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($subKey, $true).SetValue('Shell', 'C:\old\shell.exe --x', 'String')
        InModuleScope Winarchy -Parameters @{ k = $subKey; r = $recovery; e = $exe; root = $fakeRoot } {
            param($k, $r, $e, $root)
            Mock Get-WinarchyBackupsDir { $TestDrive }
            $null = Enable-WinarchyCustomShell -SubKey $k -RecoveryPath $r -ShellExe $e -Root $root
            $null = Disable-WinarchyCustomShell -SubKey $k -RecoveryPath $r
            [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($k).GetValue('Shell') | Should -Be 'C:\old\shell.exe --x'
        }
    }

    It 'does not overwrite a policy changed by someone else' {
        InModuleScope Winarchy -Parameters @{ k = $subKey; r = $recovery; e = $exe; root = $fakeRoot } {
            param($k, $r, $e, $root)
            Mock Get-WinarchyBackupsDir { $TestDrive }
            $null = Enable-WinarchyCustomShell -SubKey $k -RecoveryPath $r -ShellExe $e -Root $root
            [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($k, $true).SetValue('Shell', 'other.exe', 'String')
            { Disable-WinarchyCustomShell -SubKey $k -RecoveryPath $r } | Should -Throw '*no longer matches*'
            [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($k).GetValue('Shell') | Should -Be 'other.exe'
        }
    }

    It 'refuses to touch a policy recorded by another account' {
        InModuleScope Winarchy -Parameters @{ k = $subKey; r = $recovery; e = $exe; root = $fakeRoot } {
            param($k, $r, $e, $root)
            Mock Get-WinarchyBackupsDir { $TestDrive }
            $null = Enable-WinarchyCustomShell -SubKey $k -RecoveryPath $r -ShellExe $e -Root $root
            $record = Get-Content $r -Raw | ConvertFrom-Json
            $record.owner = 'S-1-5-21-1-2-3-1001'
            $record | ConvertTo-Json -Depth 4 | Set-Content $r
            { Disable-WinarchyCustomShell -SubKey $k -RecoveryPath $r } | Should -Throw '*different Windows account*'
            [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($k).GetValue('Shell') | Should -Match 'session-shell'
        }
    }

    It 'does not record another Winarchy shell as the value to go back to' {
        [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($subKey, $true).SetValue('Shell', '"C:\old\winarchy-shell.exe" --session-shell', 'String')
        InModuleScope Winarchy -Parameters @{ k = $subKey; r = $recovery; e = $exe; root = $fakeRoot } {
            param($k, $r, $e, $root)
            { Enable-WinarchyCustomShell -SubKey $k -RecoveryPath $r -ShellExe $e -Root $root } | Should -Throw '*another Winarchy shell*'
            Test-Path $r | Should -BeFalse
        }
    }

    It 'refuses to activate without the shell binary or the theme' {
        InModuleScope Winarchy -Parameters @{ k = $subKey; r = $recovery; root = $fakeRoot } {
            param($k, $r, $root)
            { Enable-WinarchyCustomShell -SubKey $k -RecoveryPath $r -ShellExe 'C:\nope\winarchy-shell.exe' -Root $root } | Should -Throw '*not found*'
            Get-WinarchyCustomShellStatus -SubKey $k -RecoveryPath $r | Select-Object -ExpandProperty State | Should -Be 'Inactive'
        }
    }

    It 'activating twice leaves a single record and the same value' {
        InModuleScope Winarchy -Parameters @{ k = $subKey; r = $recovery; e = $exe; root = $fakeRoot } {
            param($k, $r, $e, $root)
            Mock Get-WinarchyBackupsDir { $TestDrive }
            $null = Enable-WinarchyCustomShell -SubKey $k -RecoveryPath $r -ShellExe $e -Root $root
            (Enable-WinarchyCustomShell -SubKey $k -RecoveryPath $r -ShellExe $e -Root $root).State | Should -Be 'Installed'
            (Disable-WinarchyCustomShell -SubKey $k -RecoveryPath $r).State | Should -Be 'Inactive'
        }
    }
}

Describe 'komorebi without Explorer' {
    It 'hides windows with Hide, not the Explorer-hosted Cloak, once the custom shell is installed' {
        $root = Split-Path $PSScriptRoot -Parent
        (Get-Content (Join-Path $root 'templates\komorebi.json.tpl') -Raw) | Should -Match '"window_hiding_behaviour": "\{\{computed\.window_hiding\}\}"'
        (Get-Content (Join-Path $root 'module\Winarchy\Private\ThemeEngine.ps1') -Raw) | Should -Match "(?s)computed\.window_hiding'\] = if \(\(Get-WinarchyCustomShellStatus\)\.State -eq 'Installed'\) \{ 'Hide' \} else \{ 'Cloak' \}"
    }
}

Describe 'Restore-Explorer emergency script' {
    # runs a copy pointed at a throwaway key: no UAC, no real policy, no Explorer launched
    BeforeAll {
        $script:testKey = "Software\WinarchyTest\Restore-$([guid]::NewGuid())"
        function New-RestoreCopy([string]$Elevation) {
            $dir = Join-Path $TestDrive ([guid]::NewGuid())
            New-Item -ItemType Directory (Join-Path $dir 'scripts'), (Join-Path $dir 'state') -Force | Out-Null
            $text = Get-Content (Join-Path (Split-Path $PSScriptRoot -Parent) 'scripts\Restore-Explorer.ps1') -Raw
            foreach ($pair in @(
                    @('Software\Microsoft\Windows\CurrentVersion\Policies\System', $testKey),
                    @('if (-not $isAdmin -and -not $Elevated) {', 'if (-not $Elevated) {'),
                    @('Start-Process powershell.exe -Verb RunAs', $Elevation),
                    @('Start-Process explorer.exe }', 'Write-Host "(would start explorer)" }'),
                    @('[bool](Get-Process explorer -ErrorAction SilentlyContinue | Where-Object SessionId -eq $session)', '$false'),
                    # a failing copy must fail, not wait for input
                    @('$null = Read-Host', 'Write-Output'))) {
                if (-not $text.Contains($pair[0])) { throw "pattern missing: $($pair[0])" }
                $text = $text.Replace($pair[0], $pair[1])
            }
            if ($text -match 'Policies\\System|Start-Process explorer|RunAs') { throw 'the copy still reaches the real system' }
            Set-Content (Join-Path $dir 'scripts\Restore-Explorer.ps1') $text
            $dir
        }
        function Invoke-Restore([string]$Dir, [string]$Shell, [string]$Record) {
            $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($testKey)
            $key.SetValue('Shell', $Shell, 'String')
            if ($Record) { Set-Content (Join-Path $Dir 'state\custom-shell-recovery.json') $Record }
            $output = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Dir 'scripts\Restore-Explorer.ps1') 3>&1 2>&1
            [pscustomobject]@{ Output = ($output | Out-String) -replace '\s+', ' '; Shell = $key.GetValue('Shell', $null) }
            $key.Close()
        }
    }
    AfterAll { [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($testKey, $false) }

    It 'removes a Winarchy policy even with a damaged recovery record' {
        $r = Invoke-Restore (New-RestoreCopy 'Start-Process powershell.exe -NoNewWindow') '"C:\x\winarchy-shell.exe"' '{ broken'
        $r.Shell | Should -BeNullOrEmpty
        $r.Output | Should -Match 'recovery record is damaged'
        $r.Output | Should -Match 'could not be restored'
        $r.Output | Should -Not -Match 'Shell policy restored'
        $r.Output | Should -Match 'would start explorer'
    }

    It 'still starts Explorer and explains when the administrator step cannot run' {
        $r = Invoke-Restore (New-RestoreCopy 'Start-Process winarchy-no-such.exe') '"C:\x\winarchy-shell.exe"'
        $r.Shell | Should -Not -BeNullOrEmpty
        $r.Output | Should -Match 'did not finish'
        $r.Output | Should -Match 'would start explorer'
    }

    It 'ignores an unreadable owner and an empty previous value' {
        $record = @{ owner = 'garbage'; prior = @{ present = $true; kind = 1 } } | ConvertTo-Json
        $r = Invoke-Restore (New-RestoreCopy 'Start-Process powershell.exe -NoNewWindow') '"C:\x\winarchy-shell.exe"' $record
        $r.Shell | Should -BeNullOrEmpty
        $r.Output | Should -Match 'could not be restored'
    }

    It 'says the policy was removed when the previous value cannot be restored' {
        $record = @{ prior = @{ present = $true; kind = 3; bytes = 'AAA=' } } | ConvertTo-Json
        $r = Invoke-Restore (New-RestoreCopy 'Start-Process powershell.exe -NoNewWindow') '"C:\x\winarchy-shell.exe"' $record
        $r.Shell | Should -BeNullOrEmpty
        $r.Output | Should -Match 'could not be restored'
        $r.Output | Should -Not -Match 'Shell policy restored'
    }
}
