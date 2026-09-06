# CodingAgent.ps1 — agente de código preferido: elección, estado y lanzamiento

$script:CodingAgents = [ordered]@{
    'claude'   = @{ Label = 'Claude Code';   Command = 'claude' }
    'codex'    = @{ Label = 'OpenAI Codex';  Command = 'codex' }
    'gemini'   = @{ Label = 'Gemini CLI';    Command = 'gemini' }
    'copilot'  = @{ Label = 'GitHub Copilot'; Command = 'copilot' }
    'opencode' = @{ Label = 'opencode';      Command = 'opencode' }
    'cursor'   = @{ Label = 'Cursor Agent';  Command = 'cursor-agent' }
    'aider'    = @{ Label = 'Aider';         Command = 'aider' }
    'amp'      = @{ Label = 'Sourcegraph Amp'; Command = 'amp' }
    'crush'    = @{ Label = 'Crush';         Command = 'crush' }
}

function Get-WinarchyCodingAgentPath { Join-Path (Get-WinarchyStateDir) 'coding-agent' }

function Get-WinarchyCodingAgents {
    <# Los agentes conocidos, con Installed segun resuelva su comando en el PATH. #>
    foreach ($id in $script:CodingAgents.Keys) {
        $agent = $script:CodingAgents[$id]
        [pscustomobject]@{
            Id        = $id
            Label     = $agent.Label
            Command   = $agent.Command
            Installed = $null -ne (Get-Command $agent.Command -ErrorAction SilentlyContinue)
        }
    }
}

function Get-WinarchyCodingAgent {
    <# El agente elegido, o el primero instalado. $null si no hay ninguno. #>
    $installed = @(Get-WinarchyCodingAgents | Where-Object Installed)
    if ($installed.Count -eq 0) { return $null }

    $path = Get-WinarchyCodingAgentPath
    if (Test-Path $path) {
        $chosen = (Get-Content $path -Raw).Trim()
        $match = $installed | Where-Object Id -EQ $chosen
        if ($match) { return $match }
    }
    $installed[0]
}

function Set-WinarchyCodingAgent {
    param([Parameter(Mandatory)][string]$Id)
    if (-not $script:CodingAgents.Contains($Id)) {
        throw "Unknown agent '$Id'. Available: $($script:CodingAgents.Keys -join ', ')"
    }
    if (-not (Get-Command $script:CodingAgents[$Id].Command -ErrorAction SilentlyContinue)) {
        Write-WinarchyWarn "'$($script:CodingAgents[$Id].Command)' is not on PATH; install it to launch it."
    }
    Set-Content -Path (Get-WinarchyCodingAgentPath) -Value $Id -Encoding UTF8 -NoNewline
    Write-WinarchyOk "Coding agent: $($script:CodingAgents[$Id].Label)"
}

function Start-WinarchyCodingAgent {
    $agent = Get-WinarchyCodingAgent
    if (-not $agent) {
        Write-WinarchyWarn 'No coding agent found on PATH. `winarchy agent list` shows the known ones.'
        return
    }
    # --config-file explícito: sin él, un entorno sin WEZTERM_CONFIG_FILE deja al agente
    # en un WezTerm con los defaults de fábrica (sin theme y con cmd.exe por shell).
    Start-Process 'wezterm-gui.exe' -ArgumentList @(
        '--config-file', (Join-Path (Get-WinarchyRoot) 'config\wezterm\wezterm.lua')
        'start'
        '--cwd', ([Environment]::GetFolderPath('UserProfile'))
        '--', $agent.Command
    )
}

function Get-WinarchyCodingAgentStatus {
    $current = Get-WinarchyCodingAgent
    foreach ($agent in Get-WinarchyCodingAgents) {
        $mark = if ($current -and $agent.Id -eq $current.Id) { '*' } else { ' ' }
        $state = if ($agent.Installed) { '' } else { '  (not installed)' }
        Write-Host "  $mark $($agent.Id.PadRight(10)) $($agent.Label)$state"
    }
}
