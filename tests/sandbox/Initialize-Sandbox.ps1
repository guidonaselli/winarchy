# Runs in the sandbox as its (still elevated) user before the restart: winget + UAC with automatic approval.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Start-Transcript -Path C:\sandbox\initialize.txt
Install-PackageProvider -Name NuGet -Force | Out-Null
Install-Module -Name Microsoft.WinGet.Client -Force -Repository PSGallery -Scope AllUsers
Repair-WinGetPackageManager -AllUsers -Latest
$uac = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System'
Set-ItemProperty $uac -Name EnableLUA -Value 1
Set-ItemProperty $uac -Name ConsentPromptBehaviorAdmin -Value 0
Set-ItemProperty $uac -Name PromptOnSecureDesktop -Value 0
"[safe]`n`tdirectory = *" | Set-Content -Path "$env:USERPROFILE\.gitconfig" -Encoding ASCII
Stop-Transcript
