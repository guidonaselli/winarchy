[CmdletBinding()]
param(
  [switch]$RealFiles,
  [string]$FilesBinary,
  [string]$FilesCommit,
  [string]$FilesRepo,
  [string]$WindowsAppRuntimeInstaller,
  [string]$VCRuntimeDll,
  [string]$PowerShellDir = 'C:\Program Files\PowerShell\7'
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$wsb = Join-Path $env:LOCALAPPDATA 'Microsoft\WindowsApps\wsb.exe'
$exe = Join-Path $repo 'shell\target\release\winarchy-shell.exe'
if (-not (Test-Path $wsb) -or -not (Test-Path $exe)) { throw 'Windows Sandbox or release shell binary is missing.' }
$runRoot = Join-Path $repo ('state\verify\production-wallpaper\guest-' + [Guid]::NewGuid().ToString('N'))
$inputPath = Join-Path $runRoot 'input'; $output = Join-Path $runRoot 'output'; $payload = Join-Path $inputPath 'root'
New-Item -ItemType Directory -Path (Join-Path $payload 'config\shell'), $output -Force | Out-Null
Copy-Item $exe (Join-Path $payload 'winarchy-shell.exe')
$trayDll = Join-Path $env:ProgramFiles 'YASB\lib\YASBTrayHook.dll'
if (Test-Path -LiteralPath $trayDll) {
  New-Item -ItemType Directory -Path (Join-Path $payload 'YASB\lib') -Force | Out-Null
  Copy-Item -LiteralPath $trayDll (Join-Path $payload 'YASB\lib\YASBTrayHook.dll')
}
Copy-Item (Join-Path $PSScriptRoot 'Probe-ProductionWallpaper.ps1') (Join-Path $inputPath 'Probe-ProductionWallpaper.ps1')
$theme = @{background='#202020';opacity=1.0;foreground='#ffffff';accent='#4da3ff';muted='#808080';font_family='Segoe UI';font_size=13;position='top';dock=$false}
[IO.File]::WriteAllText((Join-Path $payload 'config\shell\theme.json'),($theme|ConvertTo-Json -Compress),[Text.UTF8Encoding]::new($false))
if ($RealFiles) {
  foreach ($required in $FilesBinary, $WindowsAppRuntimeInstaller, $VCRuntimeDll) { if (-not $required -or -not (Test-Path -LiteralPath $required -PathType Leaf)) { throw 'RealFiles needs -FilesBinary, -WindowsAppRuntimeInstaller and -VCRuntimeDll files.' } }
  if (-not $FilesCommit -or -not $FilesRepo -or -not (Test-Path -LiteralPath (Join-Path $PowerShellDir 'pwsh.exe'))) { throw 'RealFiles needs -FilesRepo, -FilesCommit and a PowerShell 7 directory.' }
  if ((Get-Item -LiteralPath $FilesBinary).Length -lt 1MB) { throw 'The Files binary is too small to be the real application.' }
  if ((Get-FileHash -LiteralPath $VCRuntimeDll -Algorithm SHA256).Hash -ne 'D1F4225DF2CD877DBF130D5668A021DCE3F94118455FF5EC952061C30AFC9CE7') { throw 'VCRUNTIME140.dll is not the verified dependency.' }
  $real = Join-Path $inputPath 'real'
  New-Item -ItemType Directory -Path (Join-Path $real 'files') -Force | Out-Null
  Copy-Item -LiteralPath $FilesBinary (Join-Path $real 'files\winarchy-files.exe')
  Copy-Item -LiteralPath $VCRuntimeDll (Join-Path $real 'files\VCRUNTIME140.dll')
  Copy-Item -LiteralPath $WindowsAppRuntimeInstaller (Join-Path $real 'WindowsAppRuntimeInstall-x64.exe')
  # config/files/theme.json lo genera el theme engine (gitignored): no existe en ningun commit, se toma del checkout de Files
  Copy-Item -LiteralPath (Join-Path $FilesRepo 'config\files\theme.json') (Join-Path $real 'files-theme.json')
  Copy-Item -LiteralPath $PowerShellDir (Join-Path $real 'ps7') -Recurse
  $archive = Join-Path $runRoot 'cli.tar'
  git -C $FilesRepo archive --format=tar -o $archive $FilesCommit module bin
  if ($LASTEXITCODE) { throw "git archive $FilesCommit failed." }
  New-Item -ItemType Directory -Path (Join-Path $real 'cli') | Out-Null
  & "$env:WINDIR\System32\tar.exe" -xf $archive -C (Join-Path $real 'cli'); if ($LASTEXITCODE) { throw 'tar extraction failed.' }
  Remove-Item -LiteralPath $archive
}
$nonce = [Guid]::NewGuid().ToString('N')+[Guid]::NewGuid().ToString('N'); $password = [Guid]::NewGuid().ToString('N')+'aA!9'
[IO.File]::WriteAllText((Join-Path $inputPath 'production-wallpaper.nonce'),$nonce,[Text.UTF8Encoding]::new($false))
$hostIdentity = @{ computer_name = $env:COMPUTERNAME; machine_guid = (Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Cryptography' -Name MachineGuid).MachineGuid }
[IO.File]::WriteAllText((Join-Path $inputPath 'host-identity.json'), ($hostIdentity | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
$inputXml = [Security.SecurityElement]::Escape($inputPath)
$outputXml = [Security.SecurityElement]::Escape($output)
$config = "<Configuration><vGPU>Disable</vGPU><Networking>Disable</Networking><MemoryInMB>4096</MemoryInMB><MappedFolders><MappedFolder><HostFolder>$inputXml</HostFolder><SandboxFolder>C:\probe</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder><MappedFolder><HostFolder>$outputXml</HostFolder><SandboxFolder>C:\sandbox</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder></MappedFolders></Configuration>"
$id=$null; $prepared=$false
function Invoke-Guest([string]$As,[string]$Command,[int]$Timeout=120) {
  $job=Start-Job -ArgumentList $wsb,$script:id,$As,$Command -ScriptBlock { param($e,$i,$a,$c); $r=& $e exec --raw --id $i -r $a -c $c 2>&1|Out-String; if($LASTEXITCODE){throw $r}; $r }
  if(-not ($job|Wait-Job -Timeout $Timeout)){Stop-Job $job;Remove-Job $job -Force;throw "Guest command timed out after $Timeout seconds."}
  try{$r=Receive-Job $job -ErrorAction Stop|Out-String}finally{Remove-Job $job -Force}
  if ($job.State -ne 'Completed') { throw "Guest command failed: $r" }
  $reply = $r | ConvertFrom-Json
  if ($null -eq $reply -or $null -eq $reply.PSObject.Properties['ExitCode'] -or $null -eq $reply.ExitCode) { throw "wsb returned no guest exit code: $r" }
  [int]$reply.ExitCode
}
try {
  $startReply = & $wsb start --raw -c $config | Out-String
  if ($LASTEXITCODE -ne 0) { throw "Sandbox start failed: $startReply" }
  $id = ($startReply | ConvertFrom-Json).Id
  if(-not $id){throw 'wsb did not return Sandbox id.'}
  Start-Process $wsb -ArgumentList 'connect','--id',$id -WindowStyle Hidden
  $ready=$false; foreach($n in 1..30){try{if((Invoke-Guest ExistingLogin 'cmd /c exit 0' 10) -eq 0){$ready=$true;break}}catch{};Start-Sleep 5}
  if(-not $ready){throw 'Sandbox interactive session unavailable.'}
  $prepare="powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\probe\Probe-ProductionWallpaper.ps1 -Mode Prepare -Password $password -Nonce $nonce"
  $prepared=$true
  if((Invoke-Guest System $prepare) -ne 0){throw 'Guest prepare failed.'}
  $run="powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\probe\Probe-ProductionWallpaper.ps1 -Mode Run -Password $password -Nonce $nonce"
  if ($RealFiles) { $run += ' -RealFiles' }
  if((Invoke-Guest ExistingLogin $run $(if ($RealFiles) { 900 } else { 240 })) -ne 0){throw "Wallpaper probe failed; evidence: $output"}
  if ($RealFiles) {
    $files = Get-Content -LiteralPath (Join-Path $output 'files-handoff-result.json') -Raw | ConvertFrom-Json
    $hostHash = (Get-FileHash -LiteralPath $FilesBinary -Algorithm SHA256).Hash
    if (@($files.cases).Count -ne 4 -or $files.second_activation.final_processes -ne 1 -or @($files.negatives).Count -ne 2 -or $files.explorer_seen -or @($files.cases | Where-Object { -not $_.marker_visible }).Count -or $files.status -ne 'PASS' -or $files.files_sha256 -ne $hostHash -or $files.shell_sha256 -ne (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash) { throw "Real Files handoff did not pass or hashes differ; evidence: $output" }
    foreach ($capture in 'files-folder', 'files-shortcut', 'files-folder-enter', 'files-folder-double-click', 'files-second-activation', 'files-negative-multiselection', 'files-negative-virtual-folder') { if ((Get-Item -LiteralPath (Join-Path $output "production-wallpaper-$capture.png")).Length -le 0) { throw "Empty screenshot: $capture" } }
    # la copia de PowerShell 7 pesa cientos de MB por corrida; el SHA256 del binario probado ya quedó en el resultado
    Remove-Item -LiteralPath (Join-Path $inputPath 'real\ps7') -Recurse -Force
    $files | ConvertTo-Json -Depth 6
    return
  }
  $comResult = Get-Content -LiteralPath (Join-Path $output 'production-wallpaper-com.json') -Raw | ConvertFrom-Json
  if ($comResult.explorer_absent_before -ne $true -or $comResult.explorer_absent_after -ne $true -or $null -eq $comResult.query.activation_hr) {
    throw 'Read-only wallpaper COM result without Explorer is missing or contaminated by Explorer.'
  }
  $comWithExplorer = Get-Content -LiteralPath (Join-Path $output 'production-wallpaper-com-with-explorer.json') -Raw | ConvertFrom-Json
  if ($comWithExplorer.explorer_present_before -ne $true -or $comWithExplorer.explorer_present_after -ne $true -or $null -eq $comWithExplorer.query.activation_hr) {
    throw 'Read-only wallpaper COM result with Explorer is missing or Explorer exited during the query.'
  }
  [pscustomobject]@{ without_explorer = $comResult; with_explorer = $comWithExplorer } | ConvertTo-Json -Depth 8
  $result = Get-Content -LiteralPath (Join-Path $output 'production-wallpaper-result.json') -Raw | ConvertFrom-Json
  if ($result.corrupt_wallpaper.status -eq 'NOT_TESTABLE') { throw "Corrupt wallpaper fallback is not testable through SPI: $($result.corrupt_wallpaper.reason) Evidence: $output" }
  if ($result.corrupt_wallpaper.status -ne 'PASS' -or -not $result.corrupt_wallpaper.same_pid) { throw 'Corrupt wallpaper fallback did not pass in the original shell process.' }
  $corruptResult = Get-Content -LiteralPath (Join-Path $output 'production-wallpaper-corrupt-result.json') -Raw | ConvertFrom-Json
  if ($corruptResult.status -ne 'PASS' -or -not $corruptResult.spi_accepted) { throw 'Corrupt wallpaper runtime evidence is missing or incomplete.' }
  if ($result.status -ne 'PASS' -or -not $result.theme_reload_same_pid -or -not $result.wallpaper_hwnd_visible -or $result.shell_pid -le 0) { throw 'Guest result did not establish the required production behavior.' }
  $returnResult = Get-Content -LiteralPath (Join-Path $output 'production-wallpaper-return-result.json') -Raw | ConvertFrom-Json
  foreach ($returned in @($result.return_to_explorer, $returnResult)) {
    if ($returned.status -ne 'PASS' -or $returned.reply.ok -isnot [bool] -or $returned.reply.ok -ne $true -or
        -not $returned.shell_exited -or $returned.shell_pid -ne $result.shell_pid -or $returned.explorer_pid -le 0 -or $returned.taskbar_hwnd -eq 0) {
      throw 'Return to Explorer evidence did not establish acknowledgement, original shell exit, and an Explorer-owned taskbar.'
    }
  }
  if ($result.binary_sha256 -ne (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash) { throw 'Guest did not exercise the staged production binary.' }
  foreach ($capture in @('dock-key-before', 'dock-key-focus', 'dock-key-after', 'dock-preview-windows', 'dock-preview-first', 'dock-preview-second', 'dock-preview-back', 'tray-flyout', 'tray-icon-menu', 'dock-cluster', 'dock-menu-network', 'dock-menu-volume', 'dock-menu-clock', 'bmp', 'jpeg', 'transparent', 'transparent-theme', 'corrupt', 'desktop-selected-folder', 'native-context-menu', 'shift-f10-menu', 'keyboard-navigation', 'multiselection', 'pointer-selection', 'f5-refresh', 'unicode-folder', 'foreground-console', 'multiselection-warning', 'virtual-folder-warning')) {
    $file = Get-Item -LiteralPath (Join-Path $output "production-wallpaper-$capture.png")
    if ($file.Length -le 0) { throw "Empty screenshot: $capture" }
  }
  $result | ConvertTo-Json -Depth 5
  Get-Content (Join-Path $output 'production-wallpaper-colors.txt')
  Write-Host "Inspect captures at $output"
} finally {
  if ($id) {
    try {
      if ($prepared) {
        $cleanup="powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\probe\Probe-ProductionWallpaper.ps1 -Mode Cleanup -Nonce $nonce"
        if ((Invoke-Guest System $cleanup 60) -ne 0) { throw 'Guest registry/account cleanup failed.' }
        if (Test-Path -LiteralPath (Join-Path $output 'autorestart-original.json')) {
          if (-not (Test-Path -LiteralPath (Join-Path $output 'autorestart-restored.json'))) { throw 'Guest registry restoration evidence is missing.' }
        }
      }
    } finally {
      & $wsb stop --id $id | Out-Null
      if ($LASTEXITCODE -ne 0) { throw "Could not stop disposable Sandbox $id." }
    }
  }
}
