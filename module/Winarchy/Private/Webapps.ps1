# Webapps.ps1 — webapps como ventanas de primera clase (browser --app=URL)

function Get-WinarchyWebappsDir {
    $dir = Join-Path ([Environment]::GetFolderPath('StartMenu')) 'Programs\Winarchy Webapps'
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
    $dir
}

function Get-WinarchyWebappsRegistry { Join-Path (Get-WinarchyStateDir) 'webapps.json' }

function Get-WinarchyBrowserExe {
    try {
        $progId = (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\Shell\Associations\UrlAssociations\http\UserChoice').ProgId
        $cmd = (Get-ItemProperty "Registry::HKEY_CLASSES_ROOT\$progId\shell\open\command").'(default)'
        if ($cmd -match '^"([^"]+)"') { return $Matches[1] }
    }
    catch { }
    $fallbacks = @(
        "$env:ProgramFiles\Google\Chrome\Application\chrome.exe",
        "$env:ProgramFiles (x86)\Microsoft\Edge\Application\msedge.exe",
        "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe"
    )
    $fallbacks | Where-Object { Test-Path $_ } | Select-Object -First 1
}

function Install-WinarchyWebapp {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string]$Url
    )
    $browser = Get-WinarchyBrowserExe
    if (-not $browser) { throw 'No Chromium browser (Chrome/Edge) found to create the webapp.' }

    $lnkPath = Join-Path (Get-WinarchyWebappsDir) "$Name.lnk"
    $shell = New-Object -ComObject WScript.Shell
    $lnk = $shell.CreateShortcut($lnkPath)
    $lnk.TargetPath = $browser
    # --app => ventana sin chrome de browser; clase/AppId propia => tileable y localizable
    $lnk.Arguments = "--app=$Url --app-id=winarchy-$($Name.ToLower() -replace '[^a-z0-9]', '-')"
    $lnk.Description = "Winarchy webapp: $Name"
    $lnk.Save()

    $registry = Get-WinarchyWebappsRegistry
    $apps = if (Test-Path $registry) { Get-Content $registry -Raw | ConvertFrom-Json -AsHashtable } else { @{} }
    $apps[$Name] = @{ url = $Url; shortcut = $lnkPath; browser = $browser; created = (Get-Date -Format 'o') }
    $apps | ConvertTo-Json -Depth 5 | Set-Content -Path $registry -Encoding UTF8

    Write-WinarchyOk "Webapp '$Name' created → $Url (launcher in the Start Menu, visible to Flow Launcher)."
}

function Remove-WinarchyWebapp {
    param([Parameter(Mandatory)][string]$Name)
    $registry = Get-WinarchyWebappsRegistry
    $apps = if (Test-Path $registry) { Get-Content $registry -Raw | ConvertFrom-Json -AsHashtable } else { @{} }
    if (-not $apps.ContainsKey($Name)) { Write-WinarchyWarn "Webapp '$Name' does not exist."; return }
    if (Test-Path $apps[$Name]['shortcut']) { Remove-Item $apps[$Name]['shortcut'] -Force }
    $apps.Remove($Name)
    $apps | ConvertTo-Json -Depth 5 | Set-Content -Path $registry -Encoding UTF8
    Write-WinarchyOk "Webapp '$Name' removed."
}

function Get-WinarchyWebapps {
    $registry = Get-WinarchyWebappsRegistry
    if (-not (Test-Path $registry)) { Write-WinarchyInfo 'No webapps installed.'; return }
    $apps = Get-Content $registry -Raw | ConvertFrom-Json -AsHashtable
    foreach ($name in ($apps.Keys | Sort-Object)) {
        Write-Host ("  {0,-20} {1}" -f $name, $apps[$name]['url'])
    }
}

$script:CaptureActions = [ordered]@{
    region     = 'RectangleRegion'
    window     = 'ActiveWindow'
    full       = 'PrintScreen'
    last       = 'LastRegion'
    scrolling  = 'ScrollingCapture'
    record     = 'ScreenRecorder'
    'record-gif' = 'ScreenRecorderGIF'
    stop       = 'StopScreenRecording'
    ocr        = 'OCR'
    qr         = 'QRCodeScanRegion'
    color      = 'ScreenColorPicker'
    ruler      = 'Ruler'
    pin        = 'PinToScreen'
}

function Invoke-WinarchyScreenshot {
    param([string]$Kind = 'region')
    if (-not $script:CaptureActions.Contains($Kind)) {
        throw "Unknown capture '$Kind'. Available: $($script:CaptureActions.Keys -join ', ')"
    }
    $sharex = @(
        "$env:ProgramFiles\ShareX\ShareX.exe",
        "${env:ProgramFiles(x86)}\ShareX\ShareX.exe"
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $sharex) { throw 'ShareX is not installed (winget install ShareX.ShareX).' }
    Start-Process $sharex -ArgumentList "-$($script:CaptureActions[$Kind])"
}
