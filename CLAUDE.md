# Winarchy — reglas del repo

Capa de integración estilo Omarchy para Windows 11: komorebi (tiling) + YASB (barra) +
AutoHotkey v2 (único dueño de hotkeys) + Flow Launcher + theme engine + CLI `winarchy`.

## Invariantes
- AHK v2 es el ÚNICO dueño de hotkeys globales. Nunca habilitar whkd ni hotkeys de Seelen.
- komorebi NUNCA se corre elevado.
- Los archivos generados llevan header "managed by winarchy" y NO se editan a mano:
  se regeneran desde `templates/*.tpl` con `winarchy theme set`.
- Personalización del usuario va en archivos override, no en los generados.
- `settings.json` de Windows Terminal se modifica solo por merge quirúrgico del scheme "Winarchy".
- Componentes core (komorebi, YASB) fijados en `versions.lock.toml`; solo `winarchy update --core` los toca.
- Fuera de alcance: novideo_srgb, Twinkle Tray, Windhawk (stack personal de la máquina, no del producto).
- Antes de cambios de sistema (registry, autostart): snapshot en `backups/<timestamp>/`.

## Estructura
- `module/Winarchy/` — módulo PowerShell (CLI). `bin/` — shim `winarchy`.
- `themes/<nombre>/theme.toml` — drop-in: agregar theme = crear carpeta, cero código.
- `templates/*.tpl` — fuente de los configs generados (tokens `{{seccion.clave}}`).
- `config/` — configs (estáticos + generados). KOMOREBI_CONFIG_HOME y YASB_CONFIG_HOME apuntan acá.
- `state/`, `backups/` — runtime, gitignored.

## Workflow
- Spec-driven con OpenSpec (`openspec/`, local y no versionado). Cambios no triviales: proposal primero.
- Validar PowerShell con `[System.Management.Automation.Language.Parser]::ParseFile` antes de commitear.
