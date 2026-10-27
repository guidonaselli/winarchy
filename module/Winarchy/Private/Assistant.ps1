function Get-WinarchyAssistantExe {
    <# Binario del asistente: el instalado y, en desarrollo, el build local. #>
    $root = Get-WinarchyRoot
    foreach ($candidate in (Join-Path $root 'state\bin\winarchy-assistant.exe'), (Join-Path $root 'assistant\target\release\winarchy-assistant.exe')) {
        if (Test-Path $candidate) { return $candidate }
    }
}

function Install-WinarchyAssistantBinary {
    <# Copia un winarchy-assistant.exe a state\bin solo si su SHA256 coincide con el de versions.lock.toml. #>
    param([Parameter(Mandatory)][string]$Source)
    $lock = Import-WinarchyToml -Path (Join-Path (Get-WinarchyRoot) 'versions.lock.toml')
    $expected = $lock['assistant']?['sha256']
    if (-not $expected) { throw 'versions.lock.toml has no [assistant] sha256 pinned: nothing to verify against.' }
    $actual = (Get-FileHash -Path $Source -Algorithm SHA256).Hash
    if ($actual -ne $expected.ToUpperInvariant()) {
        throw "winarchy-assistant.exe SHA256 mismatch (expected $expected, got $actual): not installed."
    }
    $dest = Join-Path (Join-Path (Get-WinarchyStateDir) 'bin') 'winarchy-assistant.exe'
    New-Item -ItemType Directory -Path (Split-Path $dest -Parent) -Force | Out-Null
    Copy-Item -Path $Source -Destination $dest -Force
}

function Install-WinarchyAssistantRelease {
    $url = "https://github.com/$script:WinarchyRepo/releases/download/v$((Get-WinarchyVersion).Version)/winarchy-assistant.exe"
    $tmp = Join-Path ([IO.Path]::GetTempPath()) "winarchy-assistant-$([guid]::NewGuid().ToString('N')).exe"
    try {
        Invoke-WebRequest -Uri $url -OutFile $tmp -UseBasicParsing -ErrorAction Stop
        Install-WinarchyAssistantBinary -Source $tmp
    }
    finally { Remove-Item $tmp -Force -ErrorAction SilentlyContinue }
}

function Get-WinarchyAssistantDir { Join-Path (Get-WinarchyStateDir) 'assistant' }

function Test-WinarchyAssistantRuntime {
    $dir = Get-WinarchyAssistantDir
    (Test-Path (Join-Path $dir 'venv\Scripts\python.exe')) -and (Test-Path (Join-Path $dir 'es_AR-daniela-high.onnx'))
}

function Install-WinarchyAssistantRuntime {
    <# Python aislado con voz a texto y texto a voz locales; el audio nunca sale de la PC. #>
    $dir = Get-WinarchyAssistantDir
    $py = Join-Path $dir 'venv\Scripts\python.exe'
    if (-not (Test-Path $py)) {
        $launcher = if (Get-Command py -ErrorAction SilentlyContinue) { @('py', '-3') } elseif (Get-Command python -ErrorAction SilentlyContinue) { @('python') } else { throw 'Python 3 is required for the assistant (winget install Python.Python.3.12).' }
        Write-WinarchyInfo 'Creating the assistant Python environment...'
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
        & $launcher[0] @($launcher | Select-Object -Skip 1) -m venv (Join-Path $dir 'venv')
        if ($LASTEXITCODE -ne 0) { throw 'Could not create the Python environment.' }
    }
    $hasGpu = [bool](Get-Command nvidia-smi -ErrorAction SilentlyContinue)
    $packages = @(@("faster-whisper", "piper-tts") + $(if ($hasGpu) { "nvidia-cublas-cu12", "nvidia-cudnn-cu12" }) | Where-Object { $_ })
    Write-WinarchyInfo "Installing speech packages ($($packages -join ', '))..."
    & $py -m pip install --quiet --disable-pip-version-check @packages
    if ($LASTEXITCODE -ne 0) { throw 'pip could not install the speech packages.' }
    if (-not (Test-Path (Join-Path $dir 'es_AR-daniela-high.onnx'))) {
        Write-WinarchyInfo 'Downloading the voice (es_AR-daniela-high)...'
        & $py -m piper.download_voices es_AR-daniela-high --data-dir $dir
        if ($LASTEXITCODE -ne 0) { throw 'Could not download the voice.' }
    }
}

function Test-WinarchyAssistantRunning { [bool](Get-Process winarchy-assistant -ErrorAction SilentlyContinue) }

function Start-WinarchyAssistant {
    $exe = Get-WinarchyAssistantExe
    if (-not $exe) { throw 'winarchy-assistant.exe not found: build it with `cargo build --release` in assistant\ (release asset pending).' }
    if (Test-WinarchyAssistantRunning) { return }
    $env:WINARCHY_ROOT = Get-WinarchyRoot
    Start-Process -FilePath $exe -ArgumentList 'serve' -WindowStyle Hidden
}

function Stop-WinarchyAssistant {
    $p = New-Object System.IO.Pipes.NamedPipeClientStream('.', 'winarchy-assistant', 'Out')
    try { $p.Connect(1000); $b = [Text.Encoding]::UTF8.GetBytes("quit`n"); $p.Write($b, 0, $b.Length) } catch { } finally { $p.Dispose() }
    Start-Sleep -Milliseconds 500
    Get-Process winarchy-assistant -ErrorAction SilentlyContinue | Stop-Process -Force
}

function Get-WinarchyAssistantStatus {
    $exe = Get-WinarchyAssistantExe
    $login = if ($exe) { (& $exe status) } else { 'n/a' }
    [pscustomobject]@{
        Enabled  = Test-Path (Join-Path (Get-WinarchyStateDir) 'assistant.enabled')
        Running  = Test-WinarchyAssistantRunning
        Binary   = [bool]$exe
        Runtime  = Test-WinarchyAssistantRuntime
        Login    = $login
    }
}

function Invoke-WinarchyAssistant {
    param([string]$Sub = 'status')
    $flag = Join-Path (Get-WinarchyStateDir) 'assistant.enabled'
    switch ($Sub) {
        'on' {
            if (-not (Get-WinarchyAssistantExe)) { Install-WinarchyAssistantRelease }
            Install-WinarchyAssistantRuntime
            if (-not (& (Get-WinarchyAssistantExe) status | Select-String '^logged in')) {
                Write-WinarchyInfo 'Opening the browser to sign in with ChatGPT...'
                & (Get-WinarchyAssistantExe) login
                if ($LASTEXITCODE -ne 0) { throw 'Sign-in did not complete.' }
            }
            Set-WinarchyFileAtomic -Path $flag -Value '1'
            Start-WinarchyAssistant
            Write-WinarchyOk 'Assistant on: hold Right Ctrl + Space to talk.'
        }
        'off' {
            Remove-Item $flag -Force -ErrorAction SilentlyContinue
            Stop-WinarchyAssistant
            Write-WinarchyOk 'Assistant off.'
        }
        'login' {
            $exe = Get-WinarchyAssistantExe
            if (-not $exe) { throw 'winarchy-assistant.exe not found.' }
            & $exe login
        }
        'logout' {
            $exe = Get-WinarchyAssistantExe
            if ($exe) { & $exe logout }
            Write-WinarchyOk 'Signed out (the local token is deleted; revoke the app in ChatGPT Settings too).'
        }
        'status' { Get-WinarchyAssistantStatus | Format-List | Out-Host }
        default { throw "Unknown subcommand: assistant $Sub (on|off|status|login|logout)" }
    }
}
