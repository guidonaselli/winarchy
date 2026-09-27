# Checks a finished Winarchy install and writes results.json + doctor.txt to the output folder.
param([ValidateSet('coexistence', 'active')][string]$Mode = 'coexistence', [string]$Root = 'C:\winarchy', [string]$Out = 'C:\sandbox')
Start-Transcript -Path (Join-Path $Out 'assert.txt')
$rootFile = Join-Path $Out 'root.txt'
if (Test-Path $rootFile) { $Root = (Get-Content $rootFile -Encoding UTF8 -TotalCount 1).Trim() }
Import-Module (Join-Path $Root 'module\Winarchy\Winarchy.psd1') -Force
Import-Module Microsoft.WinGet.Client
$results = [System.Collections.Generic.List[object]]::new()

function Test-Check([string]$CheckName, [scriptblock]$Test) {
    $ok = $false; $detail = $null
    try { $ok = [bool](& $Test) } catch { $detail = $_.Exception.Message }
    $results.Add([pscustomobject]@{ Check = $CheckName; Ok = $ok; Detail = $detail })
}

$lock = Import-WinarchyToml -Path (Join-Path $Root 'versions.lock.toml')
$pins = winget pin list --accept-source-agreements | Out-String -Width 400
foreach ($pin in (Get-WinarchyCoreVersions -Lock $lock).GetEnumerator()) {
    $id = $pin.Key; $version = $pin.Value
    Test-Check "$id installed at $version" { (Get-WinGetPackage -Id $id -MatchOption Equals -Source winget).InstalledVersion -eq $version }
    Test-Check "$id pinned" { $pins -match [regex]::Escape($id) }
}
foreach ($id in $lock['winget'].Keys) {
    Test-Check "$id installed" { Get-WinGetPackage -Id $id -MatchOption Equals -Source winget }
}

foreach ($target in (& (Get-Module Winarchy) { Get-WinarchyRenderTargets })) {
    Test-Check "generated $($target.Output)" { Test-Path $target.Output }
}
Test-Check 'KOMOREBI_CONFIG_HOME' { [Environment]::GetEnvironmentVariable('KOMOREBI_CONFIG_HOME', 'User') -eq (Join-Path $Root 'config\komorebi') }
Test-Check 'YASB_CONFIG_HOME' { [Environment]::GetEnvironmentVariable('YASB_CONFIG_HOME', 'User') -eq (Join-Path $Root 'config\yasb') }
Test-Check 'WEZTERM_CONFIG_FILE' { [Environment]::GetEnvironmentVariable('WEZTERM_CONFIG_FILE', 'User') -eq (Join-Path $Root 'config\wezterm\wezterm.lua') }
Test-Check 'bin on user PATH' { [Environment]::GetEnvironmentVariable('Path', 'User') -split ';' -contains (Join-Path $Root 'bin') }

$profileErrors = pwsh -NoLogo -Command '$Error | ForEach-Object { $_.ToString() }' 2>&1 | Out-String
Test-Check 'pwsh profile loads without errors' { -not $profileErrors.Trim() }
if ($profileErrors.Trim()) { "profile errors: $profileErrors" }

$doctor = & (Join-Path $Root 'bin\winarchy.cmd') doctor *>&1 | Out-String
$doctor | Set-Content (Join-Path $Out 'doctor.txt')
$allowed = @('Flow Launcher running')
if ($Mode -eq 'coexistence') { $allowed += 'komorebi running', 'YASB running (single instance)', 'AHK winarchy.ahk running', 'autostart registered' }
foreach ($line in $doctor -split "`r?`n") {
    if ($line -notmatch '^\s*\[XX\] (.+?)\s{2,}') { continue }
    $name = $Matches[1]
    Test-Check "doctor: $name" { $name -in $allowed }
}

if ($Mode -eq 'active') {
    Add-Type -Namespace Sbx -Name Token -MemberDefinition @'
[DllImport("kernel32.dll")] public static extern IntPtr OpenProcess(uint access, bool inherit, int pid);
[DllImport("advapi32.dll", SetLastError = true)] public static extern bool OpenProcessToken(IntPtr process, uint access, out IntPtr token);
[DllImport("advapi32.dll")] public static extern bool GetTokenInformation(IntPtr token, int cls, out int info, int len, out int ret);
'@
    foreach ($name in 'komorebi', 'yasb', 'AutoHotkey64') {
        Test-Check "$name running, not elevated" {
            $proc = Get-Process -Name $name -ErrorAction Stop | Select-Object -First 1
            $handle = [Sbx.Token]::OpenProcess(0x1000, $false, $proc.Id)
            $token = [IntPtr]::Zero
            if (-not [Sbx.Token]::OpenProcessToken($handle, 0x8, [ref]$token)) { return $false }
            $elevated = 0; $size = 0
            [void][Sbx.Token]::GetTokenInformation($token, 20, [ref]$elevated, 4, [ref]$size)
            $elevated -eq 0
        }
    }
    foreach ($task in 'komorebi', 'yasb', 'ahk') {
        Test-Check "logon task \Winarchy\$task" { schtasks /Query /TN "\Winarchy\$task" *> $null; $LASTEXITCODE -eq 0 }
    }
}

$results | ConvertTo-Json | Set-Content (Join-Path $Out 'results.json')
$results | Format-Table -AutoSize | Out-String -Width 300
Stop-Transcript
if ($results | Where-Object { -not $_.Ok }) { exit 1 }
