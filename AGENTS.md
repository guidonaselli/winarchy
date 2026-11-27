# AGENTS.md — Winarchy

Capa de integración estilo Omarchy para Windows 11: komorebi (tiling) + YASB (barra) +
AutoHotkey v2 (único dueño de hotkeys) + Flow Launcher + theme engine + CLI `winarchy`.

## Invariantes
- AHK v2 es el ÚNICO dueño de hotkeys globales. Nunca habilitar whkd ni hotkeys de Seelen.
- komorebi NUNCA se corre elevado.
- Nunca cerrar Explorer, la shell, komorebi ni terminales en el escritorio del usuario para medir o probar (p. ej. `PostMessage 0x5B4` a `Shell_TrayWnd`): se cae su sesión, WezTerm incluido. Una prueba sin Explorer se pregunta antes y la corre el usuario, o va en Windows Sandbox.
- Los archivos generados llevan header "managed by winarchy" y NO se editan a mano:
  se regeneran desde `templates/*.tpl` con `winarchy theme set`.
- Personalización del usuario va en archivos override, no en los generados.
- `install.ps1` es tambien la migracion de `winarchy update --self`: SIEMPRE re-renderiza
  los configs desde `templates/`, si no un cambio de template nunca llega a quien ya
  tenia un theme aplicado.
- Los `exec_options` de YASB corren en loop para siempre: nunca `powershell.exe` ahi
  (~435 ms de arranque por tick). `cmd /c` para chequeos triviales; en PowerShell,
  nada de modulos CIM/CDXML (`Get-NetAdapter` tarda ~1 s) — usar .NET.
- El reconciliador de slots (`scripts/Start-WindowSlots.ps1`) es lo único que reordena
  containers solo: corrige cuando aparece una ventana y aprende cuando la movés vos. No
  registra hotkeys ni tray propio.
- komorebi puede perder un monitor cuando una pantalla se apaga y se prende (queda uno duplicado
  y falta el real); solo un reinicio lo recupera. `winarchy heal-monitors` lo detecta y reinicia:
  AHK lo corre ante `WM_DISPLAYCHANGE` (nunca por polling, y lo pospone con un juego en primer
  plano) y `winarchy reload` lo incluye. Nunca matar komorebi a la fuerza: deja ventanas ocultas.
- komorebi.json se recarga solo: komorebi vigila el archivo y aplica una recarga estática, que pisa layout, flip,
  paddings y tiling de cada workspace, y el modo apilar/mosaico de ventanas nuevas, con lo que diga el archivo.
  Todo render escribe adentro la vista viva (`Set-WinarchyConfigLiveView`); `layout apply` omite la de los
  workspaces. Si komorebi corre y no da su estado, no se escribe komorebi.json. La vista viva queda en el archivo, así que
  también sobrevive a un reinicio de komorebi. Nunca `komorebic replace-configuration`: reconstruye
  el WM y pierde las flotantes de workspaces ocultos. Con komorebi en pausa la recarga se descarta.
- `config/windows.toml` (ubicación de ventanas) es dato de la máquina, no del producto:
  gitignored y nunca con apps hardcodeadas en el repo. Sus reglas se inyectan DENTRO del
  komorebi.json generado; no usar `komorebic workspace-rule` por runtime, que un reload
  limpia en un momento indeterminado posterior.
- Las reglas por app de la comunidad (`vendor/asc/`) se COMPILAN dentro del komorebi.json
  generado; nunca `app_specific_configuration_path` (la precedencia la define Winarchy).
  Capas, de menor a mayor: ASC -> template -> games.toml + games.user.toml -> `config/komorebi/rules.toml`.
  `games.toml` es el catálogo versionado; lo que el usuario registra con `game-mode add` va
  a `games.user.toml` (gitignored) para que `update --self` nunca vea el árbol sucio.
  Ubicacion (ignore/manage/floating) es excluyente y la capa alta reemplaza; los atributos
  se unen. `rules.toml` es del usuario y gitignored: ningun update lo toca.
- WezTerm es el terminal por defecto (SUPER+Return, agentes, `winarchy menu`). Su config
  se genera ENTERA en `config/wezterm/wezterm.lua`; la personalización del usuario va en
  `config/wezterm/user.lua` (gitignored), que ningún theme set ni update toca. Un WezTerm en `state\bin\wezterm` (override de la máquina) gana al instalado. Windows
  Terminal se conserva y se sigue tematizando, pero Winarchy no lo abre.
- `settings.json` de Windows Terminal se modifica solo por merge quirúrgico del scheme "Winarchy".
- Componentes core (komorebi, YASB, WezTerm) fijados en `versions.lock.toml`; solo `winarchy update --core` los toca.
- Winarchy es el ÚNICO canal de updates: los auto-updaters de terceros (YASB `update_check`,
  Flow `AutoUpdates`) quedan apagados y no se reactivan. Tres ejes: `update` (terceros),
  `update --core` (komorebi/YASB pinneados), `update --self` (Winarchy mismo vía git rama
  `release` + migración `install.ps1`). Versión = `ModuleVersion` del `.psd1`, en sync con el
  tag/Release (ver `docs/RELEASING.md`).
- Identidad unificada: un solo tray icon (host `winarchy.ahk`, `assets/logo/winarchy.ico`);
  los tray propios de YASB/Flow/ShareX/Everything quedan ocultos. No reintroducir un segundo tray.
- Barra: la shell nativa (`shell/`, Rust + Win32, `winarchy-shell.exe`, "Winarchy Island") es el default; YASB es
  opcional (`install.ps1 -WithYasb` o `winarchy bar use yasb`, que lo instala pinneado si falta) y una máquina que ya
  tenía YASB lo conserva (`state/bar` ausente + `yasbc` presente = yasb). AHK la vigila (`ShellWatch`: dos chequeos fallidos de 5 s, máximo 3 relanzamientos en 5 min) y la relanza si muere. Una shell con el hilo de interfaz colgado se aborta sola (~40 s) y la relanzan Windows (shell de sesión) o AHK; por eso AHK da 12 chequeos a una shell de sesión viva antes de recuperar Explorer. Tras dos abortos en 5 min la shell deja de abortarse, para que esa recuperación llegue. El hilo de interfaz de la shell no hace llamadas síncronas a ventanas ajenas (`ShowWindowAsync`, `SWP_ASYNCWINDOWPOS`). La shell nunca registra hotkeys ni hooks de teclado
  (AHK es el dueño), no corre elevada, lee komorebi desde un hilo dedicado (un suscriptor sin leer
  degrada a komorebi) y toma sus colores de `config/shell/theme.json`, generado desde
  `templates/shell-theme.json.tpl`. El binario sale como asset del Release con SHA256 en `[shell]`
  de `versions.lock.toml` y el instalador lo verifica.
- Dock (`winarchy bar dock on|float|hide|auto|off`, `state/bar-dock`): hay uno por monitor; clic izquierdo en el logo abre Inicio (soltar carpetas o archivos sobre el logo los ancla como "lugares" en una franja bajo la grilla; clic derecho o Shift+F10 sobre un pin lo desancla; `state/start-pins.json` sigue siendo un array de strings), clic derecho abre el menú Winarchy y el botón de bandeja abre Explorer. Sobre una celda, el hover suma una miniatura DWM en vivo de la ventana (ventana propia sin layered) y soltar archivos los abre con esa app (`IDropTarget` por dock; solo anclas exe/.lnk sin argumentos propios). El clic derecho antepone hasta 5 recientes de la app (`IApplicationDocumentLists` por AppUserModelID; solo apps con ID explícito, como Explorer). `hide` se revela al llevar el mouse al borde; `auto` es `hide` salvo en un workspace floating, donde se ve; `hide`, `auto` y `float` reservan 0 con la taskbar completamente oculta y conservan una franja de 2 px con auto-hide nativo. La shell puede usar un observador temporal de mouse y WinEvent, pero nunca hooks de teclado, hotkeys ni `SHAppBarMessage`. La reserva de komorebi sale de `computed.work_top/bottom` (espeja `BAR_DIP`+`MARGIN_DIP` de `shell/src/dock.rs`). El cluster del sistema (píldora derecha del dock: Quick Settings, reloj con contador de notificaciones y, al extremo, una franja que muestra el escritorio y lo restaura con los comandos de `Shell_TrayWnd`; `winarchy bar cluster primary|off|N`, `state/bar-cluster`) abre los paneles nativos de Windows con `Win+A`/`Win+N` que AHK envía al ver un flag en `state/`; la shell no manda teclas. La bandeja se ve en el dock: el botón de bandeja abre un flyout propio con los íconos reales (hasta 4 se fijan a la píldora derecha, `state/tray-pins.json`), que llegan por la `YASBTrayHook.dll` del YASB pinneado (MIT, firmada; la shell la carga en explorer con un hook `WH_GETMESSAGE` y es dueña del pipe `yasb_systray_monitor` y del mutex `Global\YASBTrayHookAlive`, que la DLL vigila para descargarse); sin la DLL o con YASB corriendo el botón abre la bandeja nativa y `winarchy doctor` lo avisa. Es la única excepción a "la shell no registra hooks" y no cubre teclado. La taskbar nativa sigue existiendo oculta; `winarchy taskbar hide` (opt-in, con dock) la oculta con `ShowWindow` y la shell la restaura al cerrarse, al apagar el dock o ante un panic.
- Host UWP (`uwphost/`, `winarchy-uwphost.exe`; opt-in `winarchy uwphost on`, `state/uwphost.enabled`): proceso propio, solo en la shell de sesión, sin hotkeys, sin hooks de teclado y sin `Cloak` de ventanas ajenas a las que presenta. Usa interfaces privadas: nunca llama una dirección que no verificó contra los símbolos del build instalado, y falla cerrado. Él registra la ventana de shell (la shell no), así que la shell vuelve a bajar su escritorio ante cada cambio de orden Z. La shell lo vigila (job object, latido, backoff 1/5/30 s, 3 relanzamientos en 5 min) y al rendirse registra su escritorio y avisa. El host recuerda las apps que presentó (`state/uwphost.apps`) y el siguiente las cierra, porque Windows ya no las deja activar; Explorer sigue siendo el respaldo de la shell. El binario del host solo se ejecuta en Windows Sandbox mientras se desarrolla. El código portado de nickel-uwu lleva su aviso MIT y figura en `THIRD_PARTY_NOTICES.md`.
- Fuera de alcance: novideo_srgb, Twinkle Tray (dependen del hardware de la máquina).
- Antes de cambios de sistema (registry, autostart): snapshot en `backups/<timestamp>/`.

## Estructura
- `module/Winarchy/` — módulo PowerShell (CLI). `bin/` — shim `winarchy`.
- `shell/`, `uwphost/` — shell nativa y host UWP (Rust).
- `themes/<nombre>/theme.toml` — drop-in: agregar theme = crear carpeta, cero código.
- `templates/*.tpl` — fuente de los configs generados (tokens `{{seccion.clave}}`).
- `config/` — configs (estáticos + generados). KOMOREBI_CONFIG_HOME y YASB_CONFIG_HOME apuntan acá.
- `state/`, `backups/` — runtime, gitignored.

## Verificación
- Cambio visible (barra YASB, theme, WezTerm) o de lógica: seguir `.claude/skills/verify-winarchy/SKILL.md` (doctor, chequeos, captura y vuelta al estado anterior) y su mapa de features.

## Workflow
- Planning con lmd (`lmd/`, local y no versionado). Cambios no triviales: proposal primero; el `tasks.md` de cada change es el único registro de avance (`lmd status`).
- Antes de commitear: `.\tests\Invoke-WinarchyChecks.ps1` (parseo de todo el PowerShell + Pester).
  Lo mismo corre en CI (`.github/workflows/ci.yml`). Requiere Pester 5+.
- `bin\winarchy.cmd` debe quedar en CRLF (`.gitattributes` lo fuerza): con LF, cmd.exe no
  parsea los bloques `if (...) else (...)`.
