function New-WinarchyCustomShellPolicySnapshot {
    param(
        [Parameter(Mandatory)][bool]$Present,
        [object]$Kind,
        [AllowEmptyCollection()][byte[]]$RawBytes = @()
    )

    if (-not $Present) {
        if ($PSBoundParameters.ContainsKey('Kind') -or $RawBytes.Count) {
            throw 'An absent CustomShell policy snapshot cannot have a kind or raw bytes.'
        }
        return [pscustomobject]@{ Present = $false; Kind = $null; RawBytes = [byte[]]@() }
    }
    if (-not $PSBoundParameters.ContainsKey('Kind') -or $null -eq $Kind) {
        throw 'A present CustomShell policy snapshot requires a registry type.'
    }
    if ($Kind -is [string]) {
        try { $Kind = [Microsoft.Win32.RegistryValueKind][Enum]::Parse([Microsoft.Win32.RegistryValueKind], $Kind, $true) }
        catch { throw "Invalid CustomShell registry type: $Kind" }
    }
    elseif ($Kind -isnot [Microsoft.Win32.RegistryValueKind]) {
        try { $Kind = [uint32]$Kind }
        catch { throw "Invalid CustomShell registry type: $Kind" }
    }
    [pscustomobject]@{ Present = $true; Kind = $Kind; RawBytes = [byte[]]$RawBytes.Clone() }
}

function Test-WinarchyCustomShellPolicySnapshotEqual {
    param(
        [Parameter(Mandatory)][psobject]$Left,
        [Parameter(Mandatory)][psobject]$Right
    )

    if ($Left.Present -ne $Right.Present) { return $false }
    if (-not $Left.Present) { return $true }
    if ([uint32]$Left.Kind -ne [uint32]$Right.Kind) { return $false }
    if ($Left.RawBytes.Length -ne $Right.RawBytes.Length) { return $false }
    for ($i = 0; $i -lt $Left.RawBytes.Length; $i++) {
        if ($Left.RawBytes[$i] -ne $Right.RawBytes[$i]) { return $false }
    }
    $true
}

function Get-WinarchyCustomShellPolicyState {
    param(
        [Parameter(Mandatory)][psobject]$Prior,
        [Parameter(Mandatory)][psobject]$Installed,
        [Parameter(Mandatory)][psobject]$Current
    )

    if (Test-WinarchyCustomShellPolicySnapshotEqual -Left $Current -Right $Prior) { return 'Prior' }
    if (Test-WinarchyCustomShellPolicySnapshotEqual -Left $Current -Right $Installed) { return 'Installed' }
    'Conflict'
}

function Initialize-WinarchyCustomShellPolicyNative {
    if (-not ('Winarchy.CustomShellPolicyNative' -as [type])) {
        Add-Type -Path (Join-Path $PSScriptRoot 'CustomShellPolicy.Native.cs') -ErrorAction Stop
    }
}

function Get-WinarchyCustomShellPolicySnapshot {
    param([string]$SubKey = 'Software\Microsoft\Windows\CurrentVersion\Policies\System')
    Initialize-WinarchyCustomShellPolicyNative
    $value = [Winarchy.CustomShellPolicyNative]::ReadCurrent($SubKey, 'Shell')
    if ($value.Present) { New-WinarchyCustomShellPolicySnapshot -Present $true -Kind $value.Kind -RawBytes $value.RawBytes }
    else { New-WinarchyCustomShellPolicySnapshot -Present $false }
}

function Set-WinarchyCustomShellPolicyIfUnchanged {
    param(
        [Parameter(Mandatory)][psobject]$Expected,
        [Parameter(Mandatory)][psobject]$Desired,
        [string]$SubKey = 'Software\Microsoft\Windows\CurrentVersion\Policies\System'
    )
    Initialize-WinarchyCustomShellPolicyNative
    $expectedKind = if ($Expected.Present) { [uint32]$Expected.Kind } else { 0 }
    $desiredKind = if ($Desired.Present) { [uint32]$Desired.Kind } else { 0 }
    $result = [Winarchy.CustomShellPolicyNative]::CompareAndWrite(
        $SubKey, 'Shell', [bool]$Expected.Present, $expectedKind, [byte[]]$Expected.RawBytes,
        [bool]$Desired.Present, $desiredKind, [byte[]]$Desired.RawBytes
    )
    [pscustomobject]@{
        Applied = $result.Applied
        Changed = $result.Changed
        Current = if ($result.Current.Present) {
            New-WinarchyCustomShellPolicySnapshot -Present $true -Kind $result.Current.Kind -RawBytes $result.Current.RawBytes
        } else {
            New-WinarchyCustomShellPolicySnapshot -Present $false
        }
    }
}
