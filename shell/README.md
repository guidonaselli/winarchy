# winarchy-shell

Shell nativa opcional de Winarchy (franja reservada + isla). Se activa con `state/bar=native`; sin ese archivo manda YASB.

- Build: `cargo build --release` (Rust estable, target MSVC).
- Uso: `winarchy-shell.exe [ruta\a\theme.json] [--root <repo>] [--theme <ruta>] [--session-shell]`. Los launchers pasan `--root` porque el binario instalado vive en `state\bin`; `--session-shell` es exclusivo del proceso que Windows inicia como shell y activa el host nativo de bandeja.
- Control: pipe de esta sesión `\\.\pipe\winarchy-shell-{sessionId}`, una línea JSON por pedido `{"v":1,"id":1,"cmd":"ping"}`; comandos `ping`, `version`, `reload`, `restart`, `quit` y `return`. `restart` conserva la sesión shell durante una recarga; `return` inicia Explorer antes de salir del shell de sesión. La suscripción a komorebi usa `winarchy-shell-komorebi-{sessionId}`.

## Dock

El cluster del sistema (Quick Settings, reloj y notificaciones) vive en el dock del monitor primario; `winarchy bar cluster primary|off|N` elige otro. `winarchy bar dock on|float|hide|auto|off` agrega un dock con las apps abiertas y ancladas en cada monitor, en el borde opuesto al de la isla; `hide` no reserva nada y se esconde hasta llevar el mouse al borde. `on` reserva su franja en komorebi; `float` flota sobre las ventanas sin reservar nada; `auto` se esconde como `hide` salvo en un workspace floating, donde se ve.

- **Ventanas**: agrupa por AppUserModelID (respaldo: exe). Clic enfoca (restaura minimizadas, cambia de workspace con `komorebic`); clic en la app que ya está al frente la minimiza; con varias ventanas el clic abre un selector con sus títulos; clic medio abre una instancia nueva. La celda "+N" abre el selector de los grupos que no entraron.
- **Logo del dock**: clic izquierdo abre Inicio; clic derecho abre el menú de control de Winarchy.
- **Teclado**: `Win+Alt+T` enfoca el dock del monitor de la ventana activa. Usa flechas o `Tab` para recorrerlo, `Home`/`End` para ir al primero/último, `Enter` o `Espacio` para activar, la tecla Menú o `Shift+F10` para abrir el menú contextual y `Escape` para cerrar. En el selector, `Up`/`Down` recorren las opciones, `Home`/`End` y `PageUp`/`PageDown` desplazan la selección, `Enter` activa y `Delete` cierra las ventanas elegibles. `Escape` vuelve del menú al dock; otro `Escape` restaura el foco previo.
- **Clic derecho en una app**: abrir/nueva ventana, anclar/desanclar, cerrar y cerrar todas. Las ventanas elevadas no se pueden cerrar desde acá (UIPI): la entrada no aparece.
- **Anclados**: lista propia en `state/dock-pins.json`, sembrada una vez desde la taskbar nativa.
- **Reordenar**: arrastrar una celda sobre otra del mismo tipo (ancladas entre sí, abiertas entre sí) las intercambia; el orden de las ancladas se guarda en `state/dock-pins.json`.
- **Atención**: una ventana que pide atención (`FlashWindow`, como un mensaje nuevo) tiñe su celda de ámbar hasta que pasa al frente o se cierra.
- **Desborde**: la celda "+N" muestra cuántos grupos quedaron fuera y abre un selector con ellos.
- **Motion**: hover con escala, entrada de íconos, feedback al presionar y fundido al ocultar; un timer de 16 ms solo mientras algo anima. Se apaga con `dock_motion: false` o con "efectos de animación" apagado en Windows.
- **Personalización** (`config/shell/user.json`, acotado): `dock_icon` 16-32, `dock_radius` 0-24, `dock_gap` 0-12, `dock_opacity` 0.4-1, `dock_motion`. La altura no cambia, así que la reserva de komorebi sigue valiendo.
- **Se oculta** con un juego o video a pantalla completa en su monitor.

### Taskbar nativa y lado derecho

`winarchy taskbar autohide` (por defecto con el dock) la deja en auto-hide y sigue alojando la bandeja de íconos. `winarchy taskbar hide` la oculta del todo (requiere dock) y la shell la vuelve a ocultar si Windows la muestra; se devuelve con `taskbar autohide|show`, al cerrar la shell, al apagar el dock y ante un panic. Con la taskbar oculta:

- Siguen andando: Win+N (calendario y notificaciones), Inicio, búsqueda; el volumen, la red, la batería y el contador de notificaciones los muestra la Island.
- El botón `^` al extremo del dock abre la bandeja del sistema; su tooltip está localizado. Con la `YASBTrayHook.dll` del YASB pinneado instalada, la shell muestra un flyout propio con los íconos reales (hover animado, nombre del ícono, clic izquierdo, derecho y medio reenviados al ícono, accesible por UI Automation) y se cierra con un clic fuera. Sin la DLL, o con YASB corriendo, abre la bandeja nativa (clic fuera, Escape o cambio de foco la cierran) y `winarchy doctor` lo avisa.
- Hasta 4 íconos de bandeja se fijan a la vista en la píldora derecha: en el flyout, el marcador de pin de cada ícono los fija o los quita (`state/tray-pins.json`, clave ejecutable:id). En la píldora llevan clic izquierdo, derecho y medio a la app.
- Al extremo derecho de la píldora, una franja fina muestra el escritorio (minimiza todo) y al segundo clic lo restaura, con los comandos de `Shell_TrayWnd`, sin mandar teclas.
- Al abrir Inicio o Search, el dock `hide` se muestra en el monitor correspondiente.
