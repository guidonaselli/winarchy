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
