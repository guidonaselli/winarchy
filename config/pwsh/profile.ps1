# managed by winarchy — perfil de shell gestionado (PowerShell 7).
# NO editar a mano: la personalización del usuario va en user.ps1 (misma carpeta,
# gitignored), que se carga al final y puede redefinir todo.
# Cada init va detrás de un guard: sin binario no hay error, solo se omite la pieza.
# Este archivo NUNCA debe tirar excepción (un $PROFILE roto bloquea cada shell nueva).

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
if (Get-Command eza -CommandType Application -ErrorAction SilentlyContinue) {
    function ls { eza --group-directories-first --icons @args }
    function ll { eza -l --group-directories-first --icons --git @args }
    function la { eza -la --group-directories-first --icons --git @args }
    function lt { eza --tree --level=2 --icons @args }
}
if (Get-Command bat -CommandType Application -ErrorAction SilentlyContinue) {
    Set-Alias -Name cat -Value bat -Option AllScope -Force
}

# Los scripts de init de zoxide/starship son estáticos por versión del binario:
# se cachean en state\pwsh-init\ para no pagar el spawn del binario en cada shell.
# Devuelve la ruta del script cacheado (hay que dot-sourcearlo en el scope del
# profile: hacerlo dentro de la función perdería las definiciones del init).
function script:Get-WinarchyToolInit {
    param([string]$Tool, [string[]]$InitArgs)
    # puede haber múltiples instalaciones (choco/scoop/winget): tomar la primera
    $exe = Get-Command $Tool -CommandType Application -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if (-not $exe) { return $null }
    try {
        $cacheDir = Join-Path $PSScriptRoot '..\..\state\pwsh-init'
        $cache = Join-Path $cacheDir "$Tool.ps1"
        $stale = -not (Test-Path $cache) -or
            ((Get-Item $cache).LastWriteTimeUtc -lt (Get-Item $exe.Source).LastWriteTimeUtc)
        if ($stale) {
            New-Item -ItemType Directory -Path $cacheDir -Force | Out-Null
            & $exe.Source @InitArgs | Set-Content -Path $cache -Encoding utf8NoBOM
        }
        return $cache
    }
    catch { return $null }
}

# --- zoxide: cd inteligente (z / zi) ----------------------------------------------
$winarchyInit = Get-WinarchyToolInit -Tool zoxide -InitArgs @('init', 'powershell')
if ($winarchyInit) { try { . $winarchyInit } catch { } }

# --- PSFzf: Ctrl+R historial difuso, Ctrl+T archivos -------------------------------
# Lazy: importar PSFzf cuesta ~300 ms, así que se difiere al primer Ctrl+R/Ctrl+T.
if (Get-Command fzf -CommandType Application -ErrorAction SilentlyContinue) {
    try {
        Set-PSReadLineKeyHandler -Key 'Ctrl+r' -ScriptBlock {
            if (-not (Get-Module PSFzf)) { Import-Module PSFzf -ErrorAction Stop }
            Invoke-FzfPsReadlineHandlerHistory
        }
        Set-PSReadLineKeyHandler -Key 'Ctrl+t' -ScriptBlock {
            if (-not (Get-Module PSFzf)) { Import-Module PSFzf -ErrorAction Stop }
            Invoke-FzfPsReadlineHandlerProvider
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
if (Test-Path $winarchyUserProfile) {
    try { . $winarchyUserProfile } catch { Write-Warning "winarchy user.ps1: $($_.Exception.Message)" }
}
