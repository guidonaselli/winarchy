[CmdletBinding()]
param(
    [switch]$Prepare,
    [switch]$Run,
    [switch]$UserProbe,
    [switch]$Cleanup,
    [string]$FilesBinary,
    [string]$FilesTheme,
    [string]$WindowsAppRuntimeInstaller,
    [string]$VCRuntimeDll,
    [string]$Password
)

$ErrorActionPreference = 'Stop'
$probeUser = 'WinarchyDesktopProbe'
$probeRoot = 'C:\probe'
$resultRoot = 'C:\sandbox'
$nonceName = 'desktop-view-guest.nonce'
$testName = 'dock::pointer_tests::integration_tests::native_shell_desktop_view_shows_exact_guest_fixtures_without_explorer'

function Assert-DesktopViewGuest {
    $noncePath = Join-Path $probeRoot $nonceName
    if (-not (Test-Path -LiteralPath 'C:\Users\WDAGUtilityAccount' -PathType Container) -or
        -not (Test-Path -LiteralPath $noncePath -PathType Leaf) -or
        (Get-Content -LiteralPath $noncePath -Raw).Trim() -notmatch '^[0-9a-f]{64}$') {
        throw 'Internal probe modes may run only inside the staged Windows Sandbox guest.'
    }
}

if ($Prepare -or $Run -or $UserProbe -or $Cleanup) { Assert-DesktopViewGuest }

if ($Prepare) {
    try {
    if ([Security.Principal.WindowsIdentity]::GetCurrent().User.Value -ne 'S-1-5-18') {
        throw 'Guest preparation must run as SYSTEM.'
    }
    foreach ($phase in @('before', 'after')) {
        $runtimeState = [ordered]@{ Account = [Security.Principal.WindowsIdentity]::GetCurrent().Name }
        foreach ($scope in @('current', 'all-users', 'provisioned')) {
            try {
                $packages = switch ($scope) {
                    'current' { Get-AppxPackage -Name '*WindowsAppRuntime*' | Select-Object Name, Version, Architecture, PackageFullName, Status, PackageUserInformation }
                    'all-users' { Get-AppxPackage -AllUsers -Name '*WindowsAppRuntime*' | Select-Object Name, Version, Architecture, PackageFullName, Status, PackageUserInformation }
                    'provisioned' { Get-AppxProvisionedPackage -Online | Where-Object DisplayName -Like '*WindowsAppRuntime*' | Select-Object DisplayName, Version, Architecture, PackageName }
                }
                $runtimeState[$scope] = @($packages)
            } catch {
                $runtimeState[$scope] = @{ Error = $_.Exception.ToString() }
            }
        }
        [IO.File]::WriteAllText((Join-Path $resultRoot "windows-app-runtime-$phase-packages.json"), ($runtimeState | ConvertTo-Json -Depth 6), [Text.UTF8Encoding]::new($false))
        if ($phase -eq 'after') {
            [IO.File]::WriteAllText((Join-Path $resultRoot 'windows-app-runtime-packages.json'), (ConvertTo-Json -InputObject $runtimeState['all-users'] -Depth 6), [Text.UTF8Encoding]::new($false))
            break
        }
        $runtimeProcess = [Diagnostics.Process]::new()
        $runtimeProcess.StartInfo.FileName = Join-Path $probeRoot 'WindowsAppRuntimeInstall-x64.exe'
        $runtimeProcess.StartInfo.UseShellExecute = $false
        $runtimeProcess.StartInfo.CreateNoWindow = $true
        $runtimeProcess.StartInfo.RedirectStandardOutput = $true
        $runtimeProcess.StartInfo.RedirectStandardError = $true
        $timeoutSeconds = 90
        $stdout = $null
        $stderr = $null
        try {
            $stdout = [IO.File]::Create((Join-Path $resultRoot "windows-app-runtime-install.stdout.txt"))
            $stderr = [IO.File]::Create((Join-Path $resultRoot "windows-app-runtime-install.stderr.txt"))
            $runtimeProcess.Start() | Out-Null
            $outputCopies = [Threading.Tasks.Task[]]@(
                $runtimeProcess.StandardOutput.BaseStream.CopyToAsync($stdout)
                $runtimeProcess.StandardError.BaseStream.CopyToAsync($stderr)
            )
            $timedOut = -not $runtimeProcess.WaitForExit($timeoutSeconds * 1000)
            if ($timedOut) {
                $runtimeProcess.Kill()
                if (-not $runtimeProcess.WaitForExit(5000)) { throw "Windows App Runtime install did not stop after its deadline." }
            }
            $runtimeResult = @{ ExitCode = $runtimeProcess.ExitCode; TimedOut = $timedOut; TimeoutSeconds = $timeoutSeconds }
            if (-not [Threading.Tasks.Task]::WaitAll($outputCopies, 5000)) {
                throw "Windows App Runtime install output capture did not finish after its deadline."
            }
        } finally {
            if ($null -ne $stdout) { $stdout.Dispose() }
            if ($null -ne $stderr) { $stderr.Dispose() }
            $runtimeProcess.Dispose()
        }
        [IO.File]::WriteAllText((Join-Path $resultRoot "windows-app-runtime-install-result.json"), ($runtimeResult | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
        $runtimeInstall = $runtimeResult
    }
    [IO.File]::WriteAllText((Join-Path $resultRoot 'windows-app-runtime-install-exit.txt'), [string]$runtimeInstall.ExitCode, [Text.UTF8Encoding]::new($false))
    if ($runtimeInstall.TimedOut) { throw 'Windows App Runtime installation exceeded the 90-second guest deadline.' }
    if ($null -eq $runtimeInstall.ExitCode -or $runtimeInstall.ExitCode -ne 0) { throw "Windows App Runtime installation failed with exit code $($runtimeInstall.ExitCode)." }
    $key = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon'
    $reg = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon', $false)
    $exists = $null -ne $reg -and $reg.GetValueNames() -contains 'AutoRestartShell'
    $value = $null
    $kind = $null
    if ($exists) {
        $value = $reg.GetValue('AutoRestartShell')
        $kind = $reg.GetValueKind('AutoRestartShell').ToString()
    }
    if ($null -ne $reg) { $reg.Dispose() }
    $state = @{ Exists = $exists; Value = $value; Kind = $kind }
    [IO.File]::WriteAllText((Join-Path $resultRoot 'autorestart-original.json'), ($state | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
    [Microsoft.Win32.Registry]::SetValue('HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon', 'AutoRestartShell', 0, [Microsoft.Win32.RegistryValueKind]::DWord)
    Get-Process explorer -ErrorAction SilentlyContinue | Stop-Process -Force
    $securePassword = ConvertTo-SecureString $Password -AsPlainText -Force
    New-LocalUser -Name $probeUser -Password $securePassword -AccountNeverExpires -PasswordNeverExpires | Out-Null
    $usersGroup = Get-LocalGroup -SID 'S-1-5-32-545'
    $account = "$env:COMPUTERNAME\$probeUser"
    if ((Get-LocalGroupMember -Group $usersGroup.Name).Name -notcontains $account) {
        Add-LocalGroupMember -Group $usersGroup.Name -Member $probeUser
    }
    $adminsGroup = Get-LocalGroup -SID 'S-1-5-32-544'
    if ((Get-LocalGroupMember -Group $adminsGroup.Name).Name -contains "$env:COMPUTERNAME\$probeUser") {
        throw 'The temporary probe account must not be an administrator'
    }
    Copy-Item -LiteralPath (Join-Path $probeRoot $nonceName) -Destination (Join-Path $resultRoot $nonceName)
    $system = Get-CimInstance Win32_OperatingSystem
    [IO.File]::WriteAllText((Join-Path $resultRoot 'guest-identity.json'),
        (@{ Caption = $system.Caption; Version = $system.Version; Build = $system.BuildNumber; Computer = $env:COMPUTERNAME } | ConvertTo-Json -Compress),
        [Text.UTF8Encoding]::new($false))
    exit 0
    } catch {
        [IO.File]::WriteAllText((Join-Path $resultRoot 'guest-prepare-error.txt'), ($_ | Format-List * -Force | Out-String), [Text.UTF8Encoding]::new($false))
        exit 1
    }
}

if ($Run) {
    $securePassword = ConvertTo-SecureString $Password -AsPlainText -Force
    $credential = [PSCredential]::new(".\$probeUser", $securePassword)
    $process = Start-Process -FilePath powershell.exe -Credential $credential -LoadUserProfile -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $PSCommandPath, '-UserProbe') -PassThru -Wait -WindowStyle Hidden
    [IO.File]::WriteAllText((Join-Path $resultRoot 'desktop-view-process-exit.txt'), [string]$process.ExitCode, [Text.UTF8Encoding]::new($false))
    exit $process.ExitCode
}

if ($UserProbe) {
    $runtimeInstall = [ordered]@{ ExitCode = $null; TimedOut = $false; TimeoutSeconds = 90 }
    foreach ($phase in @('before', 'after')) {
        $runtimeState = [ordered]@{
            Account = [Security.Principal.WindowsIdentity]::GetCurrent().Name
            Packages = @(Get-AppxPackage -Name '*WindowsAppRuntime*' | Select-Object Name, Version, Architecture, PackageFullName, Status)
        }
        [IO.File]::WriteAllText((Join-Path $resultRoot "windows-app-runtime-probe-user-$phase-packages.json"), ($runtimeState | ConvertTo-Json -Depth 4), [Text.UTF8Encoding]::new($false))
        if ($phase -eq 'after') { break }
        $runtimeProcess = [Diagnostics.Process]::new()
        $runtimeProcess.StartInfo.FileName = Join-Path $probeRoot 'WindowsAppRuntimeInstall-x64.exe'
        $runtimeProcess.StartInfo.Arguments = '--quiet'
        $runtimeProcess.StartInfo.UseShellExecute = $false
        $runtimeProcess.StartInfo.CreateNoWindow = $true
        $runtimeProcess.StartInfo.RedirectStandardOutput = $true
        $runtimeProcess.StartInfo.RedirectStandardError = $true
        $stdout = $null
        $stderr = $null
        $started = $false
        try {
            $stdout = [IO.File]::Create((Join-Path $resultRoot 'windows-app-runtime-probe-user-install.stdout.txt'))
            $stderr = [IO.File]::Create((Join-Path $resultRoot 'windows-app-runtime-probe-user-install.stderr.txt'))
            $started = $runtimeProcess.Start()
            if (-not $started) { throw 'Windows App Runtime installer did not start for the standard probe user.' }
            $outputCopies = [Threading.Tasks.Task[]]@(
                $runtimeProcess.StandardOutput.BaseStream.CopyToAsync($stdout)
                $runtimeProcess.StandardError.BaseStream.CopyToAsync($stderr)
            )
            $runtimeInstall.TimedOut = -not $runtimeProcess.WaitForExit($runtimeInstall.TimeoutSeconds * 1000)
            if ($runtimeInstall.TimedOut) {
                $runtimeProcess.Kill()
                if (-not $runtimeProcess.WaitForExit(5000)) { throw 'Windows App Runtime user installation did not stop after its deadline.' }
            }
            $runtimeInstall.ExitCode = $runtimeProcess.ExitCode
            if (-not [Threading.Tasks.Task]::WaitAll($outputCopies, 5000)) {
                throw 'Windows App Runtime user installation output capture did not finish after its deadline.'
            }
        } catch {
            $runtimeInstall.Error = $_.Exception.ToString()
        } finally {
            if ($started -and -not $runtimeProcess.HasExited) {
                $runtimeProcess.Kill()
                $null = $runtimeProcess.WaitForExit(5000)
            }
            if ($null -ne $stdout) { $stdout.Dispose() }
            if ($null -ne $stderr) { $stderr.Dispose() }
            $runtimeProcess.Dispose()
            [IO.File]::WriteAllText((Join-Path $resultRoot 'windows-app-runtime-probe-user-install-result.json'), ($runtimeInstall | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
        }
    }
    if ($runtimeInstall.TimedOut) { throw 'Windows App Runtime user installation exceeded the 90-second guest deadline.' }
    if ($runtimeInstall.Error) { throw "Windows App Runtime user installation failed: $($runtimeInstall.Error)" }
    if ($null -eq $runtimeInstall.ExitCode -or $runtimeInstall.ExitCode -ne 0) { throw "Windows App Runtime user installation failed with exit code $($runtimeInstall.ExitCode)." }
    $runtimePackages = @(Get-AppxPackage -Name 'Microsoft.WindowsAppRuntime.2' | Select-Object Name, Version, Architecture, PackageFullName, Status)
    [IO.File]::WriteAllText((Join-Path $resultRoot 'windows-app-runtime-probe-user-packages.json'), (ConvertTo-Json -InputObject $runtimePackages -Depth 4), [Text.UTF8Encoding]::new($false))
    if (-not ($runtimePackages | Where-Object { $_.Name -eq 'Microsoft.WindowsAppRuntime.2' -and $_.Version -eq '2.4.0.0' -and [int]$_.Architecture -eq 9 -and $null -ne $_.Status -and [int]$_.Status -eq 0 })) {
        throw 'Windows App Runtime 2.4 x64 is unavailable for the standard probe user.'
    }
    $desktop = [Environment]::GetFolderPath([Environment+SpecialFolder]::Desktop)
    New-Item -ItemType Directory -Path $desktop -Force | Out-Null
    [IO.File]::WriteAllBytes((Join-Path $desktop 'WinarchyDesktopProbe.txt'), [Text.Encoding]::UTF8.GetBytes('guest desktop fixture' + [char]10))
    New-Item -ItemType Directory -Path (Join-Path $desktop 'WinarchyDesktopProbeFolder') -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $desktop 'WinarchyDesktopProbeFolder\Files-handoff-confirmation.txt'), 'Synthetic folder activation fixture', [Text.UTF8Encoding]::new($false))
    $nonce = (Get-Content -LiteralPath (Join-Path $probeRoot $nonceName) -Raw).Trim()
    [IO.File]::WriteAllText((Join-Path $resultRoot $nonceName), $nonce, [Text.UTF8Encoding]::new($false))
    $env:WINARCHY_DESKTOP_VIEW_GUEST_NONCE = $nonce
    $capture = Start-Job -ArgumentList $resultRoot -ScriptBlock {
        param($output)
        $ErrorActionPreference = 'Stop'
        Add-Type -AssemblyName System.Drawing
        Add-Type -AssemblyName System.Windows.Forms
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class DesktopViewMenuCapture {
    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool IsWindowVisible(IntPtr hwnd);
}
'@
        foreach ($phase in @('menu', 'files', 'green', 'magenta', 'wallpaper-child', 'wallpaper-parent', 'parent')) {
            $progress = Join-Path $output "desktop-view-$phase-progress.txt"
            $bitmap = $null
            $graphics = $null
            try {
                [IO.File]::AppendAllText($progress, "$([DateTime]::UtcNow.ToString('o')) before-ready-wait" + [Environment]::NewLine)
                $ready = Join-Path $output "desktop-view-$phase-ready.json"
                $deadline = [DateTime]::UtcNow.AddSeconds(40)
                $request = $null
                $readyError = 'No request was published.'
                while ([DateTime]::UtcNow -lt $deadline) {
                    if (Test-Path -LiteralPath $ready) {
                        try {
                            $request = Get-Content -LiteralPath $ready -Raw | ConvertFrom-Json -ErrorAction Stop
                            if ($null -eq $request) { throw 'The ready request is empty.' }
                            break
                        } catch {
                            $request = $null
                            $readyError = $_.Exception.Message
                        }
                    }
                    Start-Sleep -Milliseconds 50
                }
                if ($null -eq $request) { throw "The probe did not publish complete ready JSON for phase ${phase}: $readyError" }
                [IO.File]::AppendAllText($progress, "$([DateTime]::UtcNow.ToString('o')) after-ready" + [Environment]::NewLine)
                if ($request.skip) {
                    [IO.File]::WriteAllText((Join-Path $output "desktop-view-$phase-skipped.txt"),
                        "INCONCLUSIVE: $($request.reason)", [Text.UTF8Encoding]::new($false))
                    continue
                }
                $menuVisible = $null
                if ($phase -eq 'menu') {
                    $menuHwnd = [IntPtr]::new([long]$request.menu_hwnd)
                    $menuVisible = [DesktopViewMenuCapture]::IsWindowVisible($menuHwnd)
                    if (-not $request.menu_visible -or -not $menuVisible) { throw 'The native menu must be visible before capture.' }
                }
                if ($phase -eq 'files' -and -not [DesktopViewMenuCapture]::IsWindowVisible([IntPtr]::new([long]$request.files_hwnd))) {
                    throw 'Files must be visible before capture.'
                }
                $bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
                $bitmap = [System.Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
                $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
                [IO.File]::AppendAllText($progress, "$([DateTime]::UtcNow.ToString('o')) before-copy bounds=$bounds points=$($request.points.Count)" + [Environment]::NewLine)
                $graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
                [IO.File]::AppendAllText($progress, "$([DateTime]::UtcNow.ToString('o')) after-copy" + [Environment]::NewLine)
                if ($phase -eq 'menu') {
                    $menuVisible = [DesktopViewMenuCapture]::IsWindowVisible($menuHwnd)
                    if (-not $menuVisible) { throw 'The native menu disappeared during capture.' }
                }
                if ($phase -eq 'files' -and -not [DesktopViewMenuCapture]::IsWindowVisible([IntPtr]::new([long]$request.files_hwnd))) {
                    throw 'Files disappeared during capture.'
                }
                $bitmap.Save((Join-Path $output "desktop-view-$phase.png"), [System.Drawing.Imaging.ImageFormat]::Png)
                [IO.File]::AppendAllText($progress, "$([DateTime]::UtcNow.ToString('o')) after-save" + [Environment]::NewLine)
                $samples = @(
                    foreach ($point in $request.points) {
                        $pixel = $bitmap.GetPixel(([int]$point[0] - $bounds.X), ([int]$point[1] - $bounds.Y))
                        @{ x = [int]$point[0]; y = [int]$point[1]
                           colorref = [int]$pixel.R -bor ([int]$pixel.G -shl 8) -bor ([int]$pixel.B -shl 16) }
                    }
                )
                [IO.File]::AppendAllText($progress, "$([DateTime]::UtcNow.ToString('o')) after-sample count=$($samples.Count)" + [Environment]::NewLine)
                [IO.File]::WriteAllText((Join-Path $output "desktop-view-$phase-captured.txt"),
                    (@{ phase = $phase; samples = $samples; expected = $request.expected_colorref
                        child_visible = $request.child_visible; screen = @($bounds.X, $bounds.Y, $bounds.Width, $bounds.Height)
                        menu_visible = $menuVisible; menu_bounds = $request.menu_bounds
                        pixel_comparison_skipped = [bool]$request.skip_pixel_comparison
                    } | ConvertTo-Json -Depth 6 -Compress), [Text.UTF8Encoding]::new($false))
                [IO.File]::AppendAllText($progress, "$([DateTime]::UtcNow.ToString('o')) after-ack" + [Environment]::NewLine)
            }
            catch {
                [IO.File]::WriteAllText((Join-Path $output "desktop-view-$phase-error.txt"),
                    (($_ | Format-List * -Force | Out-String) + $_.Exception.ToString()), [Text.UTF8Encoding]::new($false))
                throw
            }
            finally {
                if ($null -ne $graphics) { $graphics.Dispose() }
                if ($null -ne $bitmap) { $bitmap.Dispose() }
                [IO.File]::AppendAllText($progress, "$([DateTime]::UtcNow.ToString('o')) after-dispose" + [Environment]::NewLine)
            }
        }
    }
    $exe = Join-Path $probeRoot 'winarchy-shell-tests.exe'
    $env:RUST_BACKTRACE = '1'
    $stdout = Join-Path $resultRoot 'desktop-view.stdout.txt'
    $stderr = Join-Path $resultRoot 'desktop-view.stderr.txt'
    $progress = Join-Path $resultRoot 'desktop-view-user-probe-progress.txt'
    [IO.File]::AppendAllText($progress, "$([DateTime]::UtcNow.ToString('o')) before-exe pid=$PID" + [Environment]::NewLine)
    $previousErrorActionPreference = $ErrorActionPreference
    $exitCode = 1
    $ErrorActionPreference = 'Continue'
    try {
        $testProcess = Start-Process -FilePath $exe -ArgumentList @('--exact', $testName, '--ignored', '--nocapture', '--test-threads=1') -Wait -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr -ErrorAction Stop
        $exitCode = $testProcess.ExitCode
        [IO.File]::AppendAllText($progress, "$([DateTime]::UtcNow.ToString('o')) after-exe exit_code=$exitCode" + [Environment]::NewLine)
    } catch {
        [IO.File]::WriteAllText((Join-Path $resultRoot 'desktop-view-user-probe-exe-error.txt'),
            (($_ | Format-List * -Force | Out-String) + $_.Exception.ToString()), [Text.UTF8Encoding]::new($false))
        throw
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
        Get-Process winarchy-files -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
        [IO.File]::AppendAllText($progress, "$([DateTime]::UtcNow.ToString('o')) exe-finally exit_code=$exitCode" + [Environment]::NewLine)
    }
    $hashError = $null
    $hashLog = Join-Path $resultRoot 'desktop-view-wallpaper-hash.txt'
    try {
        [IO.File]::AppendAllText($progress, "$([DateTime]::UtcNow.ToString('o')) entering-hash" + [Environment]::NewLine)
        [IO.File]::WriteAllText($hashLog, "$([DateTime]::UtcNow.ToString('o')) entering-hash" + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
        $wallpaperRecord = Join-Path $resultRoot 'desktop-view-wallpaper.json'
        if (Test-Path -LiteralPath $wallpaperRecord) {
            $wallpaper = Get-Content -LiteralPath $wallpaperRecord -Raw | ConvertFrom-Json
            [IO.File]::AppendAllText($hashLog,
                "account=$env:USERDOMAIN\$env:USERNAME; path=$($wallpaper.path); bytes=$($wallpaper.bytes)" + [Environment]::NewLine)
            if ($wallpaper.bytes -gt 0) {
                $wallpaper.sha256 = (Get-FileHash -LiteralPath $wallpaper.path -Algorithm SHA256 -ErrorAction Stop).Hash
                [IO.File]::AppendAllText($hashLog, "sha256=$($wallpaper.sha256)" + [Environment]::NewLine)
            }
            [IO.File]::WriteAllText($wallpaperRecord, ($wallpaper | ConvertTo-Json -Depth 10 -Compress), [Text.UTF8Encoding]::new($false))
            [IO.File]::AppendAllText($progress, "$([DateTime]::UtcNow.ToString('o')) hash-result bytes=$($wallpaper.bytes) sha256=$($wallpaper.sha256) record-written" + [Environment]::NewLine)
        } else {
            [IO.File]::AppendAllText($hashLog, "wallpaper-record-missing path=$wallpaperRecord" + [Environment]::NewLine)
            [IO.File]::AppendAllText($progress, "$([DateTime]::UtcNow.ToString('o')) hash-result wallpaper-record-missing" + [Environment]::NewLine)
        }
    } catch {
        $hashError = ($_ | Format-List * -Force | Out-String) + $_.Exception.ToString()
        [IO.File]::AppendAllText($progress, "$([DateTime]::UtcNow.ToString('o')) hash-catch" + [Environment]::NewLine + $hashError + [Environment]::NewLine)
        [IO.File]::AppendAllText($hashLog, $hashError)
        Write-Warning "Wallpaper hash failed: $hashError"
    }
    [IO.File]::AppendAllText($progress, "$([DateTime]::UtcNow.ToString('o')) before-capture-wait state=$($capture.State)" + [Environment]::NewLine)
    $capture | Wait-Job -Timeout 45 | Out-Null
    $captureState = [string]$capture.State
    [IO.File]::AppendAllText($progress, "$([DateTime]::UtcNow.ToString('o')) after-capture-wait state=$captureState" + [Environment]::NewLine)
    $captureError = $null
    try {
        if ($captureState -notin @('Completed', 'Failed', 'Stopped')) {
            [IO.File]::WriteAllText((Join-Path $resultRoot 'desktop-view-capture-job-timeout.txt'),
                "timeout_seconds=45; state=$captureState; utc=$([DateTime]::UtcNow.ToString('o'))", [Text.UTF8Encoding]::new($false))
            Stop-Job $capture
        }
        Receive-Job $capture -ErrorAction Stop | Out-Null
    } catch {
        $captureError = ($_ | Format-List * -Force | Out-String) + $_.Exception.ToString()
        [IO.File]::WriteAllText((Join-Path $resultRoot 'desktop-view-capture-job-error.txt'),
            "state=$captureState" + [Environment]::NewLine + $captureError, [Text.UTF8Encoding]::new($false))
        Write-Warning "Screenshot job failed: $captureError"
    } finally {
        Remove-Job $capture -Force
        [IO.File]::AppendAllText($progress, "$([DateTime]::UtcNow.ToString('o')) capture-finished state=$captureState error=$([bool]$captureError)" + [Environment]::NewLine)
    }
    if ($captureState -ne 'Completed' -or $captureError) {
        throw "The guest screenshot helper did not complete (state=$captureState). $captureError"
    }
    if ($hashError) { throw "The guest wallpaper hash failed. $hashError" }
    if ($exitCode -eq 0 -and -not (Test-Path -LiteralPath (Join-Path $resultRoot 'desktop-view-result.txt'))) {
        throw 'The probe exited successfully without writing its completion record.'
    }
    exit $exitCode
}

if ($Cleanup) {
    Get-Process winarchy-files -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    $result = Join-Path $resultRoot 'autorestart-original.json'
    if (Test-Path -LiteralPath $result) {
        $state = Get-Content -LiteralPath $result -Raw | ConvertFrom-Json
        $key = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon'
        if ($state.Exists) {
            $kind = [Enum]::Parse([Microsoft.Win32.RegistryValueKind], [string]$state.Kind)
            [Microsoft.Win32.Registry]::SetValue('HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon',
                'AutoRestartShell', $state.Value, $kind)
        } else {
            Remove-ItemProperty -Path $key -Name AutoRestartShell -ErrorAction SilentlyContinue
        }
        $restored = Get-ItemProperty -Path $key -Name AutoRestartShell -ErrorAction SilentlyContinue
        $restoredExists = $null -ne $restored
        $restoredValue = $null
        if ($restoredExists) { $restoredValue = [int]$restored.AutoRestartShell }
        [IO.File]::WriteAllText((Join-Path $resultRoot 'autorestart-restored.json'),
            (@{ Exists = $restoredExists; Value = $restoredValue } | ConvertTo-Json -Compress),
            [Text.UTF8Encoding]::new($false))
    }
    exit 0
}

if ($Prepare -or $Run -or $UserProbe -or $Cleanup) {
    throw 'Invalid probe mode'
}

if (-not $FilesBinary -or -not (Test-Path -LiteralPath $FilesBinary -PathType Leaf)) {
    throw 'Pass -FilesBinary with the separately-built Files executable.'
}
if (-not $FilesTheme -or -not (Test-Path -LiteralPath $FilesTheme -PathType Leaf)) {
    throw 'Pass -FilesTheme with the Files theme JSON.'
}
if (-not $WindowsAppRuntimeInstaller -or -not (Test-Path -LiteralPath $WindowsAppRuntimeInstaller -PathType Leaf)) {
    throw 'Pass -WindowsAppRuntimeInstaller with the Windows App Runtime 2.4 x64 installer.'
}
if (-not $VCRuntimeDll -or -not (Test-Path -LiteralPath $VCRuntimeDll -PathType Leaf)) {
    throw 'Pass -VCRuntimeDll with the verified Microsoft VCRUNTIME140.dll.'
}
if ((Get-FileHash -LiteralPath $VCRuntimeDll -Algorithm SHA256).Hash -ne 'D1F4225DF2CD877DBF130D5668A021DCE3F94118455FF5EC952061C30AFC9CE7') {
    throw 'The VCRUNTIME140.dll SHA256 does not match the verified probe dependency.'
}
$vcSignature = Get-AuthenticodeSignature -LiteralPath $VCRuntimeDll
if ($vcSignature.Status -ne 'Valid' -or $vcSignature.SignerCertificate.Subject -notmatch '(^|,\s*)O=Microsoft Corporation(,|$)') {
    throw 'VCRUNTIME140.dll must have a valid Microsoft signature.'
}
$root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$shell = Join-Path $root 'shell'
$wsb = Join-Path $env:LOCALAPPDATA 'Microsoft\WindowsApps\wsb.exe'
if (-not (Test-Path -LiteralPath $wsb)) { throw 'wsb.exe not found; enable Windows Sandbox and reboot.' }
Write-Host 'Building the isolated desktop view test.'
Push-Location $shell
try {
    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $build = & cargo test --release --no-run native_shell_desktop_view_shows_exact_guest_fixtures_without_explorer 2>&1
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    if ($LASTEXITCODE -ne 0) { $build | Out-Host; throw 'The guest probe test did not compile.' }
} finally {
    Pop-Location
}
$exeLine = $build | Where-Object { $_ -match 'Executable unittests' } | Select-Object -Last 1
if ($exeLine -notmatch '\((?<path>.*\.exe)\)') { $build | Out-Host; throw 'Could not locate the compiled test executable.' }
$sourceExe = $Matches.path
if (-not [IO.Path]::IsPathRooted($sourceExe)) { $sourceExe = [IO.Path]::GetFullPath((Join-Path $shell $sourceExe)) }
$runRoot = Join-Path $root ('state\verify\desktop-api-probes\guest-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [Guid]::NewGuid().ToString('N').Substring(0, 8))
$inputDir = Join-Path $runRoot 'input'
$output = Join-Path $runRoot 'output'
New-Item -ItemType Directory -Path $inputDir -Force | Out-Null
New-Item -ItemType Directory -Path $output -Force | Out-Null
Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $inputDir 'Probe-DesktopView.ps1')
Copy-Item -LiteralPath $sourceExe -Destination (Join-Path $inputDir 'winarchy-shell-tests.exe')
Copy-Item -LiteralPath $FilesBinary -Destination (Join-Path $inputDir 'winarchy-files.exe')
Copy-Item -LiteralPath $FilesTheme -Destination (Join-Path $inputDir 'files-theme.json')
Copy-Item -LiteralPath $WindowsAppRuntimeInstaller -Destination (Join-Path $inputDir 'WindowsAppRuntimeInstall-x64.exe')
Copy-Item -LiteralPath $VCRuntimeDll -Destination (Join-Path $inputDir 'VCRUNTIME140.dll')
$nonce = [Guid]::NewGuid().ToString('N') + [Guid]::NewGuid().ToString('N')
[IO.File]::WriteAllText((Join-Path $inputDir $nonceName), $nonce, [Text.UTF8Encoding]::new($false))
Write-Host "Probe input staged at $inputDir; results will be written to $output."
$password = [Guid]::NewGuid().ToString('N') + 'aA!9'
$inputXml = [Security.SecurityElement]::Escape($inputDir)
$outputXml = [Security.SecurityElement]::Escape($output)
$config = @"
<Configuration>
  <vGPU>Disable</vGPU>
  <Networking>Disable</Networking>
  <ClipboardRedirection>Disable</ClipboardRedirection>
  <MemoryInMB>4096</MemoryInMB>
  <MappedFolders>
    <MappedFolder><HostFolder>$inputXml</HostFolder><SandboxFolder>C:\probe</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder>
    <MappedFolder><HostFolder>$outputXml</HostFolder><SandboxFolder>C:\sandbox</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder>
  </MappedFolders>
</Configuration>
"@
$id = $null
$pass = $false
function Invoke-Guest([string]$As, [string]$Command, [int]$TimeoutSeconds = 180) {
    $job = Start-Job -ArgumentList $wsb, $script:id, $As, $Command -ScriptBlock {
        param($exe, $sandboxId, $runAs, $guestCommand)
        $result = & $exe exec --raw --id $sandboxId -r $runAs -c $guestCommand 2>&1 | Out-String
        if ($LASTEXITCODE -ne 0) { throw "wsb exec failed: $result" }
        $result
    }
    $completed = $job | Wait-Job -Timeout $TimeoutSeconds
    if (-not $completed) {
        Stop-Job $job
        Remove-Job $job -Force
        throw "Guest command exceeded the $TimeoutSeconds-second host deadline."
    }
    try { $result = Receive-Job $job -ErrorAction Stop | Out-String } finally { Remove-Job $job -Force }
    $parsed = $result | ConvertFrom-Json
    [int]$parsed.ExitCode
}
try {
    $id = ((& $wsb start --raw -c ($config -replace '\r?\n\s*', '') | Out-String) | ConvertFrom-Json).Id
    if (-not $id) { throw 'wsb did not return a Sandbox id.' }
    Write-Host "Sandbox started: $id"
    Start-Process $wsb -ArgumentList 'connect', '--id', $id -WindowStyle Minimized
    $ready = $false
    foreach ($attempt in 1..60) {
        try {
            if ((Invoke-Guest ExistingLogin 'cmd /c exit 0') -eq 0) { $ready = $true; break }
        } catch { }
        Start-Sleep -Seconds 5
    }
    if (-not $ready) { throw 'The Sandbox interactive session did not start.' }
    Write-Host 'Preparing the disposable guest.'
    $code = Invoke-Guest System "powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\probe\Probe-DesktopView.ps1 -Prepare -Password $password" -TimeoutSeconds 360
    if ($code -ne 0) { throw "Guest preparation failed with exit code $code." }
    Write-Host 'Running the standard-user Desktop view probe.'
    $code = Invoke-Guest ExistingLogin "powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\probe\Probe-DesktopView.ps1 -Run -Password $password"
    if ($code -ne 0) { throw "Desktop view probe failed with exit code $code; evidence is in $output." }
    $selection = Get-Content -LiteralPath (Join-Path $output 'desktop-view-selection.json') -Raw | ConvertFrom-Json
    if ($selection.count -ne 2 -or -not $selection.exact -or $selection.selected_paths.Count -ne 2) {
        throw 'The Desktop file and folder selection was not exact.'
    }
    $menu = Get-Content -LiteralPath (Join-Path $output 'desktop-view-menu.json') -Raw | ConvertFrom-Json
    $menuCapture = Get-Content -LiteralPath (Join-Path $output 'desktop-view-menu-captured.txt') -Raw | ConvertFrom-Json
    if (-not $menu.visible -or -not $menu.capture_acknowledged -or -not $menu.navigation_sent -or
        $null -eq $menu.highlighted_index -or $menu.highlighted_index -eq $menu.initial_highlighted_index -or
        -not $menu.cancelled -or $menu.returned_command -ne 0 -or
        $menu.command_invoked -or $menu.selection_count -ne 2 -or -not $menuCapture.menu_visible -or
        -not (Test-Path -LiteralPath (Join-Path $output 'desktop-view-menu.png'))) {
        throw 'The native selection menu was not captured, navigated and cancelled without a command.'
    }
    Write-Host 'Native Desktop menu: PASS (exact file+folder selection, visible capture, Down navigation, cancellation).'
    $handoff = Get-Content -LiteralPath (Join-Path $output 'desktop-view-files-handoff.json') -Raw | ConvertFrom-Json
    $expectedFolder = 'C:\Users\' + $probeUser + '\Desktop\WinarchyDesktopProbeFolder'
    if ($handoff.path -ne $expectedFolder -or $handoff.activation -ne 'selected-folder Enter' -or
        -not (Test-Path -LiteralPath (Join-Path $output 'desktop-view-files.png')) -or
        -not (Test-Path -LiteralPath (Join-Path $output 'desktop-view-files-captured.txt'))) {
        throw 'Native folder activation did not hand the exact fixture folder to Files with capture evidence.'
    }
    Write-Host 'Files folder handoff: PASS (native Enter, exact BrowseObject target, visible Files capture).'
    $wallpaper = Get-Content -LiteralPath (Join-Path $output 'desktop-view-wallpaper.json') -Raw | ConvertFrom-Json
    if ($wallpaper.status -notin @('PASS', 'INCONCLUSIVE')) {
        throw "SPI wallpaper verification did not complete: $($wallpaper.status)"
    }
    if ($wallpaper.status -eq 'PASS') {
        if (-not $wallpaper.sha256 -or -not $wallpaper.nonuniform -or -not $wallpaper.exact_child_parent_match) {
            throw 'SPI wallpaper verification is missing its hash, nonuniformity or exact pixel match.'
        }
        foreach ($phase in @('wallpaper-child', 'wallpaper-parent')) {
            if (-not (Test-Path -LiteralPath (Join-Path $output "desktop-view-$phase.png"))) {
                throw "Missing SPI wallpaper capture: $phase"
            }
            $capture = Get-Content -LiteralPath (Join-Path $output "desktop-view-$phase-captured.txt") -Raw | ConvertFrom-Json
            $actual = @($capture.samples | ForEach-Object { [long]$_.colorref })
            $expected = @($wallpaper.expected_colorrefs | ForEach-Object { [long]$_ })
            if ($actual.Count -lt 8 -or ($actual -join ',') -ne ($expected -join ',')) {
                throw "Screenshot pixels differ from the decoded SPI wallpaper: $phase"
            }
        }
        Write-Host 'SPI image path: PASS (decoded image, exact child/parent samples, nonuniform).'
    } else {
        Write-Warning "SPI image path: INCONCLUSIVE. $($wallpaper.reason)"
    }
    Write-Host 'PaintDesktop is a separate diagnostic; a black control is not evidence of the SPI image path.'
    $pass = $true
}
finally {
    if ($id) {
        try {
            $null = Invoke-Guest System 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\probe\Probe-DesktopView.ps1 -Cleanup'
        } catch { Write-Warning "Guest cleanup reported: $_" }
        & $wsb stop --id $id | Out-Null
    }
}
if (-not $pass) { throw "Probe did not pass. Evidence retained in $output." }
Write-Host "Probe passed. Guest evidence: $output" -ForegroundColor Green
Get-Content (Join-Path $output 'desktop-view.stdout.txt') -ErrorAction SilentlyContinue
