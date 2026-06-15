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
- Winarchy es el ÚNICO canal de updates: los auto-updaters de terceros (YASB `update_check`,
  Flow `AutoUpdates`) quedan apagados y no se reactivan. Tres ejes: `update` (terceros),
  `update --core` (komorebi/YASB pinneados), `update --self` (Winarchy mismo vía git rama
  `release` + migración `install.ps1`). Versión = `ModuleVersion` del `.psd1`, en sync con el
  tag/Release (ver `docs/RELEASING.md`).
- Identidad unificada: un solo tray icon (host `winarchy.ahk`, `assets/logo/winarchy.ico`);
  los tray propios de YASB/Flow quedan ocultos. No reintroducir un segundo tray.
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
