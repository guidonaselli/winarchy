# managed by winarchy — perfil de shell gestionado (PowerShell 7).
# NO editar a mano: la personalización del usuario va en user.ps1 (misma carpeta,
# gitignored), que se carga al final y puede redefinir todo.
# Cada init va detrás de un guard: sin binario no hay error, solo se omite la pieza.
# Este archivo NUNCA debe tirar excepción (un $PROFILE roto bloquea cada shell nueva).

if ($env:NO_COLOR -eq '') {
    Remove-Item env:NO_COLOR -ErrorAction SilentlyContinue
    if ($PSStyle.OutputRendering -eq 'PlainText') { $PSStyle.OutputRendering = 'Host' }
}

# --- PSReadLine: predicciones estilo fish ---------------------------------------
if (Get-Module PSReadLine) {
    try {
        Set-PSReadLineOption -PredictionSource HistoryAndPlugin -PredictionViewStyle ListView -ErrorAction Stop
    }
    catch {
        try { Set-PSReadLineOption -PredictionSource History } catch { }
    }
    try {
        Set-PSReadLineOption -HistorySearchCursorMovesToEnd
        Set-PSReadLineKeyHandler -Key UpArrow -Function HistorySearchBackward
        Set-PSReadLineKeyHandler -Key DownArrow -Function HistorySearchForward
        Set-PSReadLineKeyHandler -Key Tab -Function MenuComplete
    }
    catch { }
}

# --- Aliases modernos: eza / bat --------------------------------------------------
$script:winarchyPaths = [System.Environment]::GetEnvironmentVariable('PATH').Split(';')
function script:Test-WinarchyBin([string]$Name) {
    foreach ($p in $script:winarchyPaths) {
        if ($p -and [System.IO.File]::Exists([System.IO.Path]::Combine($p, "$Name.exe"))) { return $true }
    }
    return $false
}

if (Test-WinarchyBin 'eza') {
    function ls { eza --group-directories-first --icons @args }
    function ll { eza -l --group-directories-first --icons --git @args }
    function la { eza -la --group-directories-first --icons --git @args }
    function lt { eza --tree --level=2 --icons @args }
}
if (Test-WinarchyBin 'bat') {
    Set-Alias -Name cat -Value bat -Option AllScope -Force
}

# Los scripts de init de zoxide/starship son estáticos por versión del binario:
# se cachean en state\pwsh-init\ para no pagar el spawn del binario en cada shell.
# Devuelve la ruta del script cacheado (hay que dot-sourcearlo en el scope del
# profile: hacerlo dentro de la función perdería las definiciones del init).
function script:Get-WinarchyToolInit {
    param([string]$Tool, [string[]]$InitArgs)
    $cacheDir = Join-Path $PSScriptRoot '..\..\state\pwsh-init'
    $cache = Join-Path $cacheDir "$Tool.ps1"
    if ([System.IO.File]::Exists($cache)) {
        return $cache
    }
    $exe = Get-Command $Tool -CommandType Application -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if (-not $exe) { return $null }
    try {
        [System.IO.Directory]::CreateDirectory($cacheDir) | Out-Null
        & $exe.Source @InitArgs | Set-Content -Path $cache -Encoding utf8NoBOM
        return $cache
    }
    catch { return $null }
}

# --- zoxide: cd inteligente (z / zi) ----------------------------------------------
$winarchyInit = Get-WinarchyToolInit -Tool zoxide -InitArgs @('init', 'powershell')
if ($winarchyInit) { try { . $winarchyInit } catch { } }

# --- fzf: Ctrl+R historial difuso, Ctrl+T archivos --------------------------------
# Invocación directa a fzf.exe: 0 ms de overhead al inicio y sin el freeze (~460 ms) de PSFzf.
if (Test-WinarchyBin 'fzf') {
    try {
        Set-PSReadLineKeyHandler -Key 'Ctrl+t' -ScriptBlock {
            $selected = (& fzf.exe)
            if ($selected) {
                [Microsoft.PowerShell.PSConsoleReadLine]::Insert(" `"$selected`" ")
            }
        }
        Set-PSReadLineKeyHandler -Key 'Ctrl+r' -ScriptBlock {
            $histFile = (Get-PSReadLineOption).HistorySavePath
            if ([System.IO.File]::Exists($histFile)) {
                $cur = [Microsoft.PowerShell.PSConsoleReadLine]::GetBufferState()[0]
                $queryArg = if ($cur) { @('-q', $cur) } else { @() }
                $chosen = [System.IO.File]::ReadAllLines($histFile) | & fzf.exe --tac --no-sort @queryArg
                if ($chosen) {
                    [Microsoft.PowerShell.PSConsoleReadLine]::RevertLine()
                    [Microsoft.PowerShell.PSConsoleReadLine]::Insert($chosen)
                }
            }
        }
    }
    catch { }
}

# --- starship: prompt themeado (al final, para que nada lo pise) --------------------
$env:STARSHIP_CONFIG = Join-Path $PSScriptRoot 'starship.toml'
$winarchyInit = Get-WinarchyToolInit -Tool starship -InitArgs @('init', 'powershell', '--print-full-init')
if ($winarchyInit) { try { . $winarchyInit } catch { } }

# --- Override del usuario (puede redefinir todo lo anterior) -------------------------
$winarchyUserProfile = Join-Path $PSScriptRoot 'user.ps1'
if ([System.IO.File]::Exists($winarchyUserProfile)) {
    try { . $winarchyUserProfile } catch { Write-Warning "winarchy user.ps1: $($_.Exception.Message)" }
}
