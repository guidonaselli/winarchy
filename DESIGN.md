# Winarchy — diseño

Winarchy tiene que sentirse **otra cosa** que Windows de fábrica: una sola superficie oscura y silenciosa, tipografía mono, un único acento y movimiento corto que explica en vez de decorar. Este archivo documenta lo que ya existe en la shell nativa (`shell/`) y las reglas para lo que se agregue. La app Files tiene el suyo en `files/DESIGN.md`; ambos comparten theme.

## Principios
1. **Un theme manda.** Color, fuente y acento salen de `themes/<nombre>/theme.toml` → `config/shell/theme.json`. Ningún color en código. Una superficie nueva lee `Theme`, no define paleta.
2. **Nada de controles de sistema a la vista.** Menús, deslizadores y paneles se dibujan con el theme. Un `TrackPopupMenu` o un control común en una captura es un defecto.
3. **Densidad baja, información alta.** Una línea por dato, mono, sin íconos decorativos ni emoji. El estado se lee de un vistazo (hora, app de origen, porcentaje).
4. **Movimiento que explica.** Entrar, salir o cambiar de estado se anima; lo demás no. Todo respeta «efectos de animación» de Windows (`dock::motion_enabled`).
5. **Cero sorpresa del sistema.** Lo que Windows hacía y Winarchy reemplaza (menús, OSD, notificaciones, Inicio, bandeja) no puede degradarse a una versión sin theme ni quedar a medias.

## Tokens
- **Color** (`shell/src/theme.rs`): `background`, `foreground`, `accent`, `urgent`, `muted`, más `opacity`. Derivados en el motor de themes: `accent_ui`, `urgent_ui`, `on_accent`. 24 themes en `themes/`.
- **Fuente:** `font_family` del theme (por defecto `JetBrainsMono NF`) para todo el texto, peso 600 en etiquetas del dock y menús, 500 en texto corrido. Íconos: `Segoe MDL2 Assets`.
- **Radios (DIP):** isla 14, menús y paneles 12, celdas y filas 8–10, riel de deslizador 3. El dock usa `dock_radius` (16, rango 0–24).
- **Espaciado (DIP):** relleno de panel 6, relleno horizontal de fila 14, fila de menú 30, celda de dock 40, separación de dock 4 (0–12), ancho de panel de la isla 300. Constantes en `layout.rs`, `dock.rs`, `start.rs`, `tray_flyout.rs`, `menu.rs`.
- **Escala:** todo pasa por `layout::px(dip, scale)`; nada de píxeles sueltos.

## Superficies
| Superficie | Qué es | Archivo |
|---|---|---|
| Isla | Píldora superior con hora, workspaces, medios y OSD; se expande en panel (calendario, agenda, notificaciones paginadas, medidores) | `main.rs`, `layout.rs` |
| Dock | Barra 48 con celdas de 40, hover escala 1.14, miniatura DWM, lugares anclables | `dock.rs`, `dock_menu.rs` |
| Cluster | Píldora derecha del dock: red/volumen/batería, reloj, escritorio, bandeja | `cluster.rs` |
| Panel de estado | Clic izquierdo en red/volumen/batería: estado o deslizador sobre los ajustes | `cluster.rs`, `menu.rs` |
| Menú propio | Modal, tematizado, teclado y mouse; reemplaza todo `TrackPopupMenu` | `menu.rs` |
| Inicio | Búsqueda, pines, grilla, lugares, energía | `start.rs`, `start_model.rs` |
| Bandeja | Flyout de 6 columnas con íconos reales | `tray_flyout.rs` |
| Toast | Aviso con franja de acento, clic abre la app | `tray_notifications.rs` |
| Switcher | Alt+Tab con miniaturas | `switcher.rs` |

## Movimiento
- Resorte críticamente amortiguado para la isla (`OMEGA` 30, dt ≤ 50 ms); easing cúbico para dock (160 ms), Inicio (160), flyout (180), menú (140); entrada con desplazamiento corto (8–16 px) más fundido.
- Hover 120 ms, resaltado de fila por fundido.
- Los temporizadores se apagan al asentarse: nada de repintar en reposo.
- Switcher instantáneo para que Alt+Tab siga al teclado; el OSD suaviza progreso. La isla, el OSD, Inicio y la bandeja respetan «efectos de animación» de Windows (`dock::motion_enabled`).

## Menús y paneles
- Un solo componente (`menu::pick` / `pick_with`): fila de 30, resaltado con acento al 30 %, ↑/↓/Enter/Esc, cierra al perder foco o clic afuera.
- Cabecera opcional: línea de estado o deslizador (clic, arrastre, rueda de ±2 %).
- Los menús del shell de Windows (menú de ícono de escritorio, ícono de bandeja de terceros) son del sistema por diseño y no se tematizan.

## Accesibilidad
- Las ventanas con acciones publican su árbol por `accessibility`; el menú propio todavía no (deuda).
- Contraste de texto sobre `background` ≥ 4.5:1 en los 24 themes: sin verificar.

## Rendimiento (reglas)
- Nada de `powershell.exe` en bucles; sondeos del cluster y de la isla a ≤ 1 s, el volumen a 4 Hz.
- No crear fuente ni DIB por cuadro en superficies que se repintan; hoy lo hacen el dock, el cluster y los toasts (deuda: cachear).
- Al asentarse una animación, matar su temporizador.

## Deuda de diseño conocida
- Menú propio sin árbol de accesibilidad, sin filas deshabilitadas ni separadores, sin estado de pulsado y sin estado silenciado en el deslizador.
- Panel de red sin lista de redes ni Wi-Fi propio: muestra estado y abre los ajustes de Windows.
- Brillo sin control en el panel de estado: el clic en el cluster ya no expande la isla, así que solo se alcanza con el mouse sobre la isla.
