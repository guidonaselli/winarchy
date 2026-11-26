# Runs in the sandbox (Windows PowerShell 5.1). `elevated` runs as SYSTEM, the rest as the non-elevated user.
param([Parameter(Mandatory)][ValidateSet('install', 'boot', 'selfupdate', 'published', 'uninstall', 'paths', 'ahkv1', 'elevated', 'barswitch', 'interrupted', 'uninstall-interrupted', 'hungpipe', 'benchmark')][string]$Scenario)
$ProgressPreference = 'SilentlyContinue'
$out = 'C:\sandbox'
$bare = Join-Path $out 'winarchy.git'
Start-Transcript -Path (Join-Path $out "$Scenario.txt")

function Update-SessionPath {
    $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')
}

function Install-Prerequisites {
    foreach ($id in 'Git.Git', 'Microsoft.PowerShell') {
        winget install --id $id --exact --source winget --silent --accept-package-agreements --accept-source-agreements 2>&1 | Out-Host
        if ($LASTEXITCODE -ne 0) { $script:failed = $true; "winget install $id exited $LASTEXITCODE" }
    }
    Update-SessionPath
    git clone --quiet --branch release $bare C:\winarchy
    if ($LASTEXITCODE -ne 0) { $script:failed = $true; "git clone exited $LASTEXITCODE" }
}

function Install-AhkV1 {
    $process = Start-Process winget -Verb RunAs -PassThru -ArgumentList 'install', '--id', 'AutoHotkey.AutoHotkey', '--exact', '--source', 'winget', '--version', '1.1.37.02', '--silent', '--accept-package-agreements', '--accept-source-agreements'
    if (-not $process.WaitForExit(600000)) {
        $process | Stop-Process -Force
        Get-Process setup, AutoHotkey_* -ErrorAction SilentlyContinue | Stop-Process -Force
        $script:failed = $true; 'AutoHotkey 1.1 install hung'
    }
}

function Invoke-WinarchyScript([string]$Script, [string[]]$Arguments = @()) {
    pwsh -NoProfile -ExecutionPolicy Bypass -File "C:\winarchy\$Script" @Arguments 2>&1 | Out-Host
    if ($LASTEXITCODE -ne 0) { $script:failed = $true; "$Script $Arguments exited $LASTEXITCODE" }
}

function Invoke-WinarchyElevated([string]$Root, [string]$Log, [string[]]$Arguments) {
    $command = "& '$($Root.Replace("'", "''"))\bin\winarchy.ps1' $Arguments *>&1 | Out-File C:\sandbox\$Log.txt; exit [int]`$LASTEXITCODE"
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($command))
    $process = Start-Process pwsh -Verb RunAs -Wait -PassThru -ArgumentList '-NoProfile', '-ExecutionPolicy', 'Bypass', '-EncodedCommand', $encoded
    Get-Content "C:\sandbox\$Log.txt" -ErrorAction SilentlyContinue | Out-Host
    if ($process.ExitCode -ne 0) { $script:failed = $true; "winarchy $Arguments exited $($process.ExitCode)" }
}


function Test-Uninstalled {
    $pins = winget pin list --accept-source-agreements | Out-String -Width 400
    foreach ($id in 'LGUG2Z.komorebi', 'AmN.yasb', 'AutoHotkey.AutoHotkey', 'Flow-Launcher.Flow-Launcher', 'wez.wezterm', 'voidtools.Everything') {
        if ($pins -match [regex]::Escape($id)) { $script:failed = $true; "still pinned: $id" }
    }
    foreach ($var in 'KOMOREBI_CONFIG_HOME', 'YASB_CONFIG_HOME', 'WEZTERM_CONFIG_FILE') {
        if ([Environment]::GetEnvironmentVariable($var, 'User')) { $script:failed = $true; "still set: $var" }
    }
    if ([Environment]::GetEnvironmentVariable('Path', 'User') -split ';' -contains 'C:\winarchy\bin') { $script:failed = $true; 'bin still on PATH' }
    schtasks /Query /TN '\Winarchy\komorebi' 2>$null | Out-Null
    if ($LASTEXITCODE -eq 0) { $script:failed = $true; 'logon task still registered' }
    foreach ($name in 'komorebi', 'yasb') {
        if (Get-Process -Name $name -ErrorAction SilentlyContinue) { $script:failed = $true; "$name still running" }
    }
    $profilePath = Join-Path ([Environment]::GetFolderPath('MyDocuments')) 'PowerShell\profile.ps1'
    if ((Test-Path $profilePath) -and (Select-String -Path $profilePath -SimpleMatch 'managed by winarchy' -Quiet)) { $script:failed = $true; 'profile hook still present' }
    if ((Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StuckRects3').Settings[8] -band 1) { $script:failed = $true; 'taskbar still on auto-hide' }
    if ($null -ne (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Search' -Name BingSearchEnabled -ErrorAction SilentlyContinue)) { $script:failed = $true; 'Windows hardening not reverted' }
    $palette = Join-Path ([Environment]::GetFolderPath('StartMenu')) 'Programs\Winarchy'
    if (Get-ChildItem $palette -Filter '*.lnk' -ErrorAction SilentlyContinue) { $script:failed = $true; 'Start menu commands still present' }
}

$boot = (Get-Content (Join-Path $out 'boot.ps1') -Raw).Replace('https://github.com/guidonaselli/winarchy.git', $bare).Replace('pwsh @installArgs', '& { $ErrorActionPreference = ''Continue''; pwsh @installArgs 2>&1 | Out-Host }')
$failed = $false
function Measure-Idle([int]$Seconds = 30) {
    $cores = [Environment]::ProcessorCount
    $cpu = { Get-Process | ForEach-Object { try { [pscustomobject]@{ Id = $_.Id; Cpu = $_.TotalProcessorTime.TotalSeconds } } catch { } } }
    $a = & $cpu
    Start-Sleep $Seconds
    $b = & $cpu
    $before = @{}; foreach ($p in $a) { $before[$p.Id] = $p.Cpu }
    $busy = ($b | ForEach-Object { if ($before.ContainsKey($_.Id)) { $_.Cpu - $before[$_.Id] } } | Measure-Object -Sum).Sum
    $os = Get-CimInstance Win32_OperatingSystem
    $procs = @(Get-Process)
    [ordered]@{
        cpu_percent_of_machine = [math]::Round(100 * $busy / ($Seconds * $cores), 2)
        memory_used_mb = [math]::Round(($os.TotalVisibleMemorySize - $os.FreePhysicalMemory) / 1KB)
        processes = $procs.Count
        handles = ($procs | Measure-Object Handles -Sum).Sum
        top_working_set_mb = @($procs | Sort-Object WorkingSet64 -Descending | Select-Object -First 8 | ForEach-Object { "$($_.ProcessName)=$([math]::Round($_.WorkingSet64 / 1MB))" })
    }
}

switch ($Scenario) {
    'elevated' {
        try { Invoke-Expression $boot; $failed = $true; 'boot ran in an elevated session' }
        catch { "refused: $($_.Exception.Message)" }
        if (Test-Path C:\winarchy) { $failed = $true; 'C:\winarchy was created' }
    }
    'install' {
        Install-Prerequisites
        Invoke-WinarchyScript install.ps1
        Invoke-WinarchyScript install.ps1
    }
    'ahkv1' {
        Install-AhkV1
        Install-Prerequisites
        Invoke-WinarchyScript install.ps1
    }
    'boot' {
        foreach ($activate in '', '1') {
            $env:WINARCHY_ACTIVATE = $activate
            try { Invoke-Expression $boot }
            catch { $failed = $true; "boot (WINARCHY_ACTIVATE='$activate') failed: $($_.Exception.Message)" }
        }
    }
    'paths' {
        $dir = 'C:\Win ' + [char]0x00C1 + "rchy d'Test\winarchy"
        $dir | Set-Content (Join-Path $out 'root.txt') -Encoding UTF8
        $env:WINARCHY_DIR = $dir
        $env:WINARCHY_ACTIVATE = '1'
        try { Invoke-Expression $boot }
        catch { $failed = $true; "boot failed: $($_.Exception.Message)" }
    }
    'published' {
        try { Invoke-RestMethod https://raw.githubusercontent.com/guidonaselli/winarchy/release/boot.ps1 | Invoke-Expression }
        catch { $failed = $true; "one-liner failed: $($_.Exception.Message)" }
    }
    'uninstall' {
        Install-Prerequisites
        Invoke-WinarchyScript install.ps1 -Arguments '-Activate'
        schtasks /Query /TN '\Winarchy\komorebi' 2>$null | Out-Null
        if ($LASTEXITCODE -ne 0) { $failed = $true; 'logon task missing before uninstall' }
        if (-not ((Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StuckRects3').Settings[8] -band 1)) { $failed = $true; 'taskbar not on auto-hide before uninstall' }
        Invoke-WinarchyScript uninstall.ps1
        Test-Uninstalled
        Invoke-WinarchyScript uninstall.ps1
        Test-Uninstalled
        Invoke-WinarchyScript install.ps1
    }
    'interrupted' {
        Install-Prerequisites
        $install = Start-Process pwsh -PassThru -ArgumentList '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', 'C:\winarchy\install.ps1', '-Activate'
        Start-Sleep 60
        if ($install.HasExited) { $failed = $true; "install finished (exit $($install.ExitCode)) before the interruption; raise the delay" }
        else { taskkill /F /T /PID $install.Id 2>&1 | Out-Host; 'install interrupted after 60 s' }
        # el instalador que quedó en vuelo conserva el mutex de Windows Installer (1618); se espera a que termine
        Start-Sleep 90
        $drain = [Diagnostics.Stopwatch]::StartNew()
        $busy = { @(Get-CimInstance Win32_Process | Where-Object { $_.Name -in 'winget.exe', 'setup.exe', 'AppInstallerCLI.exe' -or ($_.Name -eq 'msiexec.exe' -and $_.CommandLine -notmatch '/V') }).Count }
        while ((& $busy) -gt 0 -and $drain.Elapsed.TotalMinutes -lt 8) { Start-Sleep 10 }
        "installers drained after $([int]$drain.Elapsed.TotalSeconds) s"
        Invoke-WinarchyScript install.ps1 -Arguments '-Activate'
        Invoke-WinarchyScript install.ps1
    }
    'uninstall-interrupted' {
        Install-Prerequisites
        Invoke-WinarchyScript install.ps1 -Arguments '-Activate'
        $run = Start-Process pwsh -PassThru -ArgumentList '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', 'C:\winarchy\uninstall.ps1'
        # se corta apenas desaparece la tarea de logon (paso 3 de 12): faltan env vars, PATH y pins
        $watch = [Diagnostics.Stopwatch]::StartNew()
        while (-not $run.HasExited -and $watch.Elapsed.TotalSeconds -lt 120) {
            schtasks /Query /TN '\Winarchy\komorebi' 2>$null | Out-Null
            if ($LASTEXITCODE -ne 0) { break }
            Start-Sleep -Milliseconds 100
        }
        if ($run.HasExited) { $failed = $true; "uninstall finished (exit $($run.ExitCode)) before it could be interrupted" }
        else { taskkill /F /T /PID $run.Id 2>&1 | Out-Host; 'uninstall interrupted after its autostart was removed' }
        Invoke-WinarchyScript uninstall.ps1
        Test-Uninstalled
        Invoke-WinarchyScript install.ps1
    }
    'hungpipe' {
        Install-Prerequisites
        # el test enlaza el pipe real de la sesión: solo corre donde no hay una shell viva, por eso va en el Sandbox
        $probe = @'
Install-Module Pester -MinimumVersion 5.0 -Force -Scope CurrentUser -SkipPublisherCheck
Import-Module Pester -MinimumVersion 5.0
$r = Invoke-Pester -Path C:\winarchy\tests\ShellLifecycleGate.Tests.ps1 -FullNameFilter '*pipe recovery deadline*' -PassThru -Output Detailed
"passed=$($r.PassedCount) failed=$($r.FailedCount) skipped=$($r.SkippedCount)"
exit $(if ($r.PassedCount -ge 1 -and $r.FailedCount -eq 0 -and $r.SkippedCount -eq 0) { 0 } else { 1 })
'@
        $path = Join-Path $out 'hungpipe-pester.ps1'
        $probe | Set-Content $path -Encoding UTF8
        pwsh -NoProfile -ExecutionPolicy Bypass -File $path 2>&1 | Out-Host
        if ($LASTEXITCODE -ne 0) { $failed = $true; "hung-pipe recovery test exited $LASTEXITCODE" }
    }
    'barswitch' {
        Install-Prerequisites
        Invoke-WinarchyScript install.ps1
        Update-SessionPath
        $bar = { (Get-Content C:\winarchy\state\bar -Raw).Trim() }
        if ((& $bar) -ne 'native') { $failed = $true; "default bar is $(& $bar), expected native" }
        if (Get-Command yasbc -ErrorAction SilentlyContinue) { $failed = $true; 'YASB present before asking for it' }
        cmd /c 'C:\winarchy\bin\winarchy.cmd bar use yasb 2>&1' | Out-Host
        Update-SessionPath
        if (-not (Get-Command yasbc -ErrorAction SilentlyContinue)) { $failed = $true; 'bar use yasb did not install YASB' }
        if ((& $bar) -ne 'yasb') { $failed = $true; "bar is $(& $bar), expected yasb" }
        if ((winget pin list --accept-source-agreements | Out-String) -notmatch 'AmN.yasb') { $failed = $true; 'YASB not pinned' }
        cmd /c 'C:\winarchy\bin\winarchy.cmd bar use native 2>&1' | Out-Host
        if ((& $bar) -ne 'native') { $failed = $true; "bar is $(& $bar), expected native" }
        Start-Sleep 5
        if (@(Get-Process winarchy-shell -ErrorAction SilentlyContinue).Count -ne 1) { $failed = $true; 'expected exactly one winarchy-shell' }
        if (Get-Process yasb -ErrorAction SilentlyContinue) { $failed = $true; 'yasb still running after switching to native' }
    }
    'benchmark' {
        Install-Prerequisites
        Start-Sleep 60
        $baseline = Measure-Idle
        $watch = [Diagnostics.Stopwatch]::StartNew()
        Invoke-WinarchyScript install.ps1
        $installSeconds = [math]::Round($watch.Elapsed.TotalSeconds)
        Start-Sleep 90
        $after = Measure-Idle
        $winarchy = @(Get-Process winarchy-shell, komorebi, AutoHotkey64, yasb -ErrorAction SilentlyContinue | ForEach-Object { "$($_.ProcessName)=$([math]::Round($_.WorkingSet64 / 1MB))MB" })
        $result = [ordered]@{ install_seconds = $installSeconds; baseline = $baseline; after = $after; winarchy_processes = $winarchy }
        $result | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $out 'benchmark.json')
        $result | ConvertTo-Json -Depth 5 | Out-Host
    }
    'selfupdate' {
        Install-Prerequisites
        Invoke-WinarchyScript install.ps1
        git -C C:\winarchy reset --quiet --hard v1.11.0
        foreach ($id in 'AutoHotkey.AutoHotkey', 'Flow-Launcher.Flow-Launcher') { winget pin remove --id $id --exact --source winget 2>&1 | Out-Host }
        $update = cmd /c 'C:\winarchy\bin\winarchy.cmd update --self 2>&1' | Out-String
        $update
        if ($update -notmatch 'Winarchy updated') { $failed = $true; 'update --self did not report an update' }
        $version = (Import-PowerShellDataFile C:\winarchy\module\Winarchy\Winarchy.psd1).ModuleVersion
        "version after update: $version"
        if ($version -eq '1.11.0') { $failed = $true; 'still on 1.11.0' }
    }
}
Stop-Transcript
if ($failed) { exit 1 }
