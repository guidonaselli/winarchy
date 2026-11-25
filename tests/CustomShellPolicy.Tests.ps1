#Requires -Modules Pester

BeforeAll {
    $root = Split-Path $PSScriptRoot -Parent
    Import-Module (Join-Path $root 'module\Winarchy\Winarchy.psd1') -Force
}

Describe 'New-WinarchyCustomShellPolicySnapshot presence, RegistryValueKind and raw bytes contract' {
    It 'represents an absent value separately from a present empty REG_SZ' {
        InModuleScope Winarchy {
            $absent = New-WinarchyCustomShellPolicySnapshot -Present $false
            $empty = New-WinarchyCustomShellPolicySnapshot -Present $true -Kind String -RawBytes ([byte[]]@())

            $absent.Present | Should -BeFalse
            $empty.Present | Should -BeTrue
            $empty.Kind | Should -Be ([Microsoft.Win32.RegistryValueKind]::String)
            @($empty.RawBytes).Count | Should -Be 0
        }
    }

    It 'copies the supplied raw bytes' {
        InModuleScope Winarchy {
            $bytes = [byte[]]@(1, 2, 3)
            $snapshot = New-WinarchyCustomShellPolicySnapshot -Present $true -Kind Binary -RawBytes $bytes
            $bytes[0] = 9

            $snapshot.RawBytes | Should -Be ([byte[]]@(1, 2, 3))
        }
    }

    It 'rejects raw data attached to an absent value' {
        InModuleScope Winarchy {
            { New-WinarchyCustomShellPolicySnapshot -Present $false -RawBytes ([byte[]]@(1)) } | Should -Throw
        }
    }

    It 'requires a type for a present value' {
        InModuleScope Winarchy {
            { New-WinarchyCustomShellPolicySnapshot -Present $true -RawBytes ([byte[]]@()) } | Should -Throw
        }
    }

    It 'preserves REG_SZ whitespace and its terminating bytes' {
        InModuleScope Winarchy {
            $snapshot = New-WinarchyCustomShellPolicySnapshot -Present $true -Kind String -RawBytes ([byte[]]@(32, 0, 9, 0, 0, 0))

            $snapshot.RawBytes | Should -Be ([byte[]]@(32, 0, 9, 0, 0, 0))
        }
    }

    It 'preserves the raw REG_EXPAND_SZ token without expanding it' {
        InModuleScope Winarchy {
            $snapshot = New-WinarchyCustomShellPolicySnapshot -Present $true -Kind ExpandString -RawBytes ([byte[]]@(37, 0, 84, 0, 69, 0, 77, 0, 80, 0, 37, 0, 0, 0))

            $snapshot.Kind | Should -Be ([Microsoft.Win32.RegistryValueKind]::ExpandString)
            $snapshot.RawBytes | Should -Be ([byte[]]@(37, 0, 84, 0, 69, 0, 77, 0, 80, 0, 37, 0, 0, 0))
        }
    }

    It 'preserves registry types missing from RegistryValueKind' {
        InModuleScope Winarchy {
            $snapshot = New-WinarchyCustomShellPolicySnapshot -Present $true -Kind 6 -RawBytes ([byte[]]@(1, 2, 3))

            $snapshot.Kind | Should -Be 6
            $snapshot.RawBytes | Should -Be ([byte[]]@(1, 2, 3))
        }
    }

    It 'preserves <Kind> payload bytes exactly' -TestCases @(
        @{ Kind = 'Binary'; Bytes = [byte[]]@(0, 255, 0, 128) }
        @{ Kind = 'DWord'; Bytes = [byte[]]@(1, 0, 0, 128) }
        @{ Kind = 'QWord'; Bytes = [byte[]]@(1, 0, 0, 0, 0, 0, 0, 128) }
    ) {
        param($Kind, $Bytes)
        InModuleScope Winarchy -Parameters @{ Kind = $Kind; Bytes = $Bytes } {
            param($Kind, $Bytes)
            $snapshot = New-WinarchyCustomShellPolicySnapshot -Present $true -Kind $Kind -RawBytes $Bytes

            $snapshot.Kind | Should -Be ([Microsoft.Win32.RegistryValueKind]$Kind)
            $snapshot.RawBytes | Should -Be $Bytes
        }
    }
}

Describe 'Test-WinarchyCustomShellPolicySnapshotEqual exact comparison contract' {
    It 'considers independently allocated identical snapshots equal' {
        InModuleScope Winarchy {
            $left = [pscustomobject]@{ Present = $true; Kind = [Microsoft.Win32.RegistryValueKind]::Binary; RawBytes = [byte[]]@(0, 255, 0) }
            $right = [pscustomobject]@{ Present = $true; Kind = [Microsoft.Win32.RegistryValueKind]::Binary; RawBytes = [byte[]]@(0, 255, 0) }

            Test-WinarchyCustomShellPolicySnapshotEqual -Left $left -Right $right | Should -BeTrue
        }
    }

    It 'considers two absent snapshots equal' {
        InModuleScope Winarchy {
            $left = [pscustomobject]@{ Present = $false; Kind = $null; RawBytes = [byte[]]@() }
            $right = [pscustomobject]@{ Present = $false; Kind = $null; RawBytes = [byte[]]@() }

            Test-WinarchyCustomShellPolicySnapshotEqual -Left $left -Right $right | Should -BeTrue
        }
    }

    It 'distinguishes <Difference>' -TestCases @(
        @{ Difference = 'absent from present empty'; Left = @{ Present = $false; Kind = $null; RawBytes = [byte[]]@() }; Right = @{ Present = $true; Kind = [Microsoft.Win32.RegistryValueKind]::String; RawBytes = [byte[]]@() } }
        @{ Difference = 'REG_SZ whitespace'; Left = @{ Present = $true; Kind = [Microsoft.Win32.RegistryValueKind]::String; RawBytes = [byte[]]@(32, 0, 0, 0) }; Right = @{ Present = $true; Kind = [Microsoft.Win32.RegistryValueKind]::String; RawBytes = [byte[]]@(0, 0) } }
        @{ Difference = 'kind with identical bytes'; Left = @{ Present = $true; Kind = [Microsoft.Win32.RegistryValueKind]::String; RawBytes = [byte[]]@(65, 0, 0, 0) }; Right = @{ Present = $true; Kind = [Microsoft.Win32.RegistryValueKind]::ExpandString; RawBytes = [byte[]]@(65, 0, 0, 0) } }
        @{ Difference = 'binary byte order'; Left = @{ Present = $true; Kind = [Microsoft.Win32.RegistryValueKind]::Binary; RawBytes = [byte[]]@(1, 2) }; Right = @{ Present = $true; Kind = [Microsoft.Win32.RegistryValueKind]::Binary; RawBytes = [byte[]]@(2, 1) } }
        @{ Difference = 'binary length'; Left = @{ Present = $true; Kind = [Microsoft.Win32.RegistryValueKind]::Binary; RawBytes = [byte[]]@(1) }; Right = @{ Present = $true; Kind = [Microsoft.Win32.RegistryValueKind]::Binary; RawBytes = [byte[]]@(1, 0) } }
        @{ Difference = 'DWORD payload'; Left = @{ Present = $true; Kind = [Microsoft.Win32.RegistryValueKind]::DWord; RawBytes = [byte[]]@(1, 0, 0, 0) }; Right = @{ Present = $true; Kind = [Microsoft.Win32.RegistryValueKind]::DWord; RawBytes = [byte[]]@(2, 0, 0, 0) } }
        @{ Difference = 'QWORD high bytes'; Left = @{ Present = $true; Kind = [Microsoft.Win32.RegistryValueKind]::QWord; RawBytes = [byte[]]@(1, 0, 0, 0, 0, 0, 0, 0) }; Right = @{ Present = $true; Kind = [Microsoft.Win32.RegistryValueKind]::QWord; RawBytes = [byte[]]@(1, 0, 0, 0, 0, 0, 0, 1) } }
    ) {
        param($Left, $Right)
        InModuleScope Winarchy -Parameters @{ Left = [pscustomobject]$Left; Right = [pscustomobject]$Right } {
            param($Left, $Right)

            Test-WinarchyCustomShellPolicySnapshotEqual -Left $Left -Right $Right | Should -BeFalse
        }
    }
}

Describe 'Get-WinarchyCustomShellPolicyState Prior, Installed or Conflict contract' {
    It 'classifies <Expected> from exact snapshots' -TestCases @(
        @{ Current = @{ Present = $false; Kind = $null; RawBytes = [byte[]]@() }; Expected = 'Prior' }
        @{ Current = @{ Present = $true; Kind = [Microsoft.Win32.RegistryValueKind]::String; RawBytes = [byte[]]@(65, 0, 0, 0) }; Expected = 'Installed' }
        @{ Current = @{ Present = $true; Kind = [Microsoft.Win32.RegistryValueKind]::String; RawBytes = [byte[]]@(66, 0, 0, 0) }; Expected = 'Conflict' }
        @{ Current = @{ Present = $true; Kind = [Microsoft.Win32.RegistryValueKind]::String; RawBytes = [byte[]]@() }; Expected = 'Conflict' }
        @{ Current = @{ Present = $true; Kind = [Microsoft.Win32.RegistryValueKind]::ExpandString; RawBytes = [byte[]]@(65, 0, 0, 0) }; Expected = 'Conflict' }
    ) {
        param($Current, $Expected)
        InModuleScope Winarchy -Parameters @{ Current = [pscustomobject]$Current; Expected = $Expected } {
            param($Current, $Expected)
            $prior = [pscustomobject]@{ Present = $false; Kind = $null; RawBytes = [byte[]]@() }
            $installed = [pscustomobject]@{ Present = $true; Kind = [Microsoft.Win32.RegistryValueKind]::String; RawBytes = [byte[]]@(65, 0, 0, 0) }

            Get-WinarchyCustomShellPolicyState -Prior $prior -Installed $installed -Current $Current | Should -Be $Expected
        }
    }

    It 'prefers Prior when both prior and installed match current' {
        InModuleScope Winarchy {
            $prior = [pscustomobject]@{ Present = $true; Kind = [Microsoft.Win32.RegistryValueKind]::String; RawBytes = [byte[]]@(65, 0, 0, 0) }
            $installed = [pscustomobject]@{ Present = $true; Kind = [Microsoft.Win32.RegistryValueKind]::String; RawBytes = [byte[]]@(65, 0, 0, 0) }
            $current = [pscustomobject]@{ Present = $true; Kind = [Microsoft.Win32.RegistryValueKind]::String; RawBytes = [byte[]]@(65, 0, 0, 0) }

            Get-WinarchyCustomShellPolicyState -Prior $prior -Installed $installed -Current $current | Should -Be 'Prior'
        }
    }
}
