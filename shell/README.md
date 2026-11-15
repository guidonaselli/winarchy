# winarchy-shell

Shell nativa opcional de Winarchy (franja reservada + isla). Se activa con `state/bar=native`; sin ese archivo manda YASB.

- Build: `cargo build --release` (Rust estable, target MSVC).
- Uso: `winarchy-shell.exe [ruta\a\theme.json]` (por defecto `config\shell\theme.json` del repo).
- Control: tubería `\.\pipe\winarchy-shell`, una línea JSON por pedido `{"v":1,"id":1,"cmd":"ping"}`; comandos `ping`, `version`, `reload`, `quit`.

## Dock

`winarchy bar dock on|float|off` agrega un dock con las apps abiertas y ancladas, en el borde opuesto al de la isla. `on` reserva su franja en komorebi; `float` flota sobre las ventanas sin reservar nada.

- **Ventanas**: agrupa por AppUserModelID (respaldo: exe). Clic enfoca (restaura minimizadas, cambia de workspace con `komorebic`); clic en la app que ya está al frente la minimiza; con varias ventanas el clic abre un selector con sus títulos; clic medio abre una instancia nueva. La celda "+N" abre el selector de los grupos que no entraron.
- **Clic derecho**: abrir/nueva ventana, anclar/desanclar, cerrar y cerrar todas, y "Bandeja del sistema" si la taskbar nativa está oculta. Las ventanas elevadas no se pueden cerrar desde acá (UIPI): la entrada no aparece.
- **Anclados**: lista propia en `state/dock-pins.json`, sembrada una vez desde la taskbar nativa.
- **Atención**: una ventana que pide atención (`FlashWindow`, como un mensaje nuevo) tiñe su celda de ámbar hasta que pasa al frente o se cierra.
- **Desborde**: la celda "+N" muestra cuántos grupos quedaron fuera y abre un selector con ellos.
- **Motion**: hover con escala, entrada de íconos, feedback al presionar y fundido al ocultar; un timer de 16 ms solo mientras algo anima. Se apaga con `dock_motion: false` o con "efectos de animación" apagado en Windows.
- **Personalización** (`config/shell/user.json`, acotado): `dock_icon` 16-32, `dock_radius` 0-24, `dock_gap` 0-12, `dock_opacity` 0.4-1, `dock_motion`. La altura no cambia, así que la reserva de komorebi sigue valiendo.
- **Se oculta** con un juego o video a pantalla completa en su monitor.

### Taskbar nativa y lado derecho

`winarchy taskbar autohide` (por defecto con el dock) la deja en auto-hide y sigue alojando la bandeja de íconos. `winarchy taskbar hide` la oculta del todo (requiere dock) y la shell la vuelve a ocultar si Windows la muestra; se devuelve con `taskbar autohide|show`, al cerrar la shell, al apagar el dock y ante un panic. Con la taskbar oculta:

- Siguen andando: Win+N (calendario y notificaciones), Inicio, búsqueda; el volumen, la red, la batería y el contador de notificaciones los muestra la Island.
- Se pierde el acceso directo a la bandeja de íconos: "Bandeja del sistema" en el menú del dock muestra la taskbar nativa 12 s para usarla. No hay API pública para alojar esos íconos, así que Winarchy no los reimplementa.
