# Skill.ps1 — instala la skill "winarchy" en los agentes de IA detectados (estilo Omarchy)

function Get-WinarchyAgentSkillRoots {
    <# Devuelve los dirs de skills de cada agente instalado en la máquina.
       Un agente se considera presente si existe su carpeta base (~/.claude, etc.).
       Estructura agnóstica: cada agente usa <base>\skills\<skill>\SKILL.md. #>
    $home = [Environment]::GetFolderPath('UserProfile')
    $bases = @(
        (Join-Path $home '.claude')              # Claude Code
        (Join-Path $home '.codex')               # OpenAI Codex CLI
        (Join-Path $home '.gemini')              # Google Gemini CLI
        (Join-Path $home '.cursor')              # Cursor
        (Join-Path $home '.windsurf')            # Windsurf
        (Join-Path $home '.codeium\windsurf')    # Windsurf (Codeium legacy path)
        (Join-Path $home '.continue')            # Continue
        (Join-Path $home '.aider')               # Aider
        (Join-Path $home '.cline')               # Cline
        (Join-Path $home '.roo')                 # Roo Code
        (Join-Path $home '.copilot')             # GitHub Copilot CLI
        (Join-Path $home '.opencode')            # opencode
        (Join-Path $home '.amp')                 # Sourcegraph Amp
        (Join-Path $env:APPDATA 'opencode')      # opencode (Windows AppData)
    )
    foreach ($base in $bases) {
        if ($base -and (Test-Path $base)) { Join-Path $base 'skills' }
    }
}

function Install-WinarchySkill {
    <# Copia assets\skills\winarchy\ a <agente>\skills\winarchy\ en cada agente detectado.
       Idempotente: re-ejecutable sin daño (sobreescribe la copia gestionada). #>
    $src = Join-Path (Get-WinarchyRoot) 'assets\skills\winarchy'
    if (-not (Test-Path (Join-Path $src 'SKILL.md'))) {
        Write-WinarchyWarn "Skill source not found in $src; skipping the skill."
        return
    }

    $roots = @(Get-WinarchyAgentSkillRoots)
    if (-not $roots.Count) {
        Write-WinarchyInfo 'No AI coding agent found (~/.claude, ~/.codex, ~/.gemini); skipping the skill.'
        return
    }

    foreach ($skillsRoot in $roots) {
        try {
            $dest = Join-Path $skillsRoot 'winarchy'
            if (-not (Test-Path $skillsRoot)) { New-Item -ItemType Directory -Path $skillsRoot -Force | Out-Null }
            Copy-Item -Path $src -Destination $skillsRoot -Recurse -Force
            Write-WinarchyOk "Skill 'winarchy' installed in $dest"
        }
        catch { Write-WinarchyWarn "Could not install the skill in $skillsRoot : $($_.Exception.Message)" }
    }
}

function Uninstall-WinarchySkill {
    <# Quita la skill "winarchy" de cada agente detectado. Idempotente. #>
    foreach ($skillsRoot in @(Get-WinarchyAgentSkillRoots)) {
        $dest = Join-Path $skillsRoot 'winarchy'
        if (Test-Path $dest) {
            try {
                Remove-Item -Path $dest -Recurse -Force
                Write-WinarchyOk "Skill 'winarchy' removed from $dest"
            }
            catch { Write-WinarchyWarn "Could not remove the skill in $dest : $($_.Exception.Message)" }
        }
    }
}
