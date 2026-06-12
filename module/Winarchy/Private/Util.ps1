# Util.ps1 — paths, logging, backups y motor de templates de Winarchy

function Get-WinarchyRoot {
    # Private/ -> Winarchy/ -> module/ -> <repo>
    (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
}

function Get-WinarchyStateDir {
    $dir = Join-Path (Get-WinarchyRoot) 'state'
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
    $dir
}

function Get-WinarchyBackupsDir {
    $dir = Join-Path (Get-WinarchyRoot) 'backups'
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
    $dir
}

function Get-WinarchyAhkExe {
    <# AutoHotkey v2 per-machine o per-user; $null si no está instalado. #>
    $found = @(
        "$env:ProgramFiles\AutoHotkey\v2\AutoHotkey64.exe",
        "$env:LOCALAPPDATA\Programs\AutoHotkey\v2\AutoHotkey64.exe"
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $found) { $found = (Get-Command AutoHotkey64.exe -ErrorAction SilentlyContinue).Source }
    $found
}

function Write-WinarchyInfo { param([string]$Message) Write-Host "  $Message" }
function Write-WinarchyOk   { param([string]$Message) Write-Host "  [OK] $Message" -ForegroundColor Green }
function Write-WinarchyWarn { param([string]$Message) Write-Host "  [!!] $Message" -ForegroundColor Yellow }
function Write-WinarchyErr  { param([string]$Message) Write-Host "  [XX] $Message" -ForegroundColor Red }

function New-WinarchySnapshot {
    <# Copia los paths dados a backups/<timestamp>/ antes de tocar nada. Devuelve el dir del snapshot. #>
    param([Parameter(Mandatory)][string[]]$Path, [string]$Label = 'snapshot')
    $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
    $dest = Join-Path (Get-WinarchyBackupsDir) "$stamp-$Label"
    New-Item -ItemType Directory -Path $dest -Force | Out-Null
    foreach ($p in $Path) {
        if (Test-Path $p) {
            $leaf = Split-Path $p -Leaf
            Copy-Item -Path $p -Destination (Join-Path $dest $leaf) -Recurse -Force
        }
    }
    $dest
}

function ConvertTo-WinarchyFlatContext {
    <# Aplana hashtable anidada a claves "seccion.clave" para el renderer. #>
    param([Parameter(Mandatory)][hashtable]$Data, [string]$Prefix = '')
    $flat = @{}
    foreach ($key in $Data.Keys) {
        $name = if ($Prefix) { "$Prefix.$key" } else { "$key" }
        $value = $Data[$key]
        if ($value -is [hashtable]) {
            $nested = ConvertTo-WinarchyFlatContext -Data $value -Prefix $name
            foreach ($nk in $nested.Keys) { $flat[$nk] = $nested[$nk] }
        }
        else {
            $flat[$name] = $value
        }
    }
    $flat
}

function Invoke-WinarchyTemplate {
    <#
      Renderiza un template reemplazando tokens {{clave}} / {{seccion.clave}}.
      Falla si queda algún token sin resolver (theme incompleto = error temprano).
    #>
    param(
        [Parameter(Mandatory)][string]$TemplatePath,
        [Parameter(Mandatory)][hashtable]$Context
    )
    if (-not (Test-Path $TemplatePath)) { throw "Template not found: $TemplatePath" }
    $content = Get-Content -Path $TemplatePath -Raw -Encoding UTF8
    $rendered = [regex]::Replace($content, '\{\{\s*([\w\.\-]+)\s*\}\}', {
        param($m)
        $token = $m.Groups[1].Value
        if ($Context.ContainsKey($token)) { [string]$Context[$token] }
        else { throw "Unresolved token in $(Split-Path $TemplatePath -Leaf): {{$token}}" }
    })
    $rendered
}

function Test-WinarchyProcess {
    param([Parameter(Mandatory)][string]$Name)
    $null -ne (Get-Process -Name $Name -ErrorAction SilentlyContinue)
}

function Disable-WinarchyXMouse {
    <#
      Apaga el focus-follows-mouse a nivel SO (SPI_SETACTIVEWINDOWTRACKING).
      El modo "Windows" de komorebi enciende este flag y PERSISTE después de
      cerrar komorebi: todo camino de salida (uninstall, rollback) debe llamarlo.
    #>
    if (-not ('WinarchyNative.XMouse' -as [type])) {
        Add-Type -Namespace WinarchyNative -Name XMouse -MemberDefinition @'
[DllImport("user32.dll", SetLastError = true)]
public static extern bool SystemParametersInfo(uint uiAction, uint uiParam, IntPtr pvParam, uint fWinIni);
'@
    }
    # 0x1001 = SPI_SETACTIVEWINDOWTRACKING; 0x03 = SPIF_UPDATEINIFILE | SPIF_SENDCHANGE
    [void][WinarchyNative.XMouse]::SystemParametersInfo(0x1001, 0, [IntPtr]::Zero, 0x03)
}
