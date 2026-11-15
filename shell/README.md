# winarchy-shell

Shell nativa opcional de Winarchy (franja reservada + isla). Se activa con `state/bar=native`; sin ese archivo manda YASB.

- Build: `cargo build --release` (Rust estable, target MSVC).
- Uso: `winarchy-shell.exe [ruta\a\theme.json]` (por defecto `config\shell\theme.json` del repo).
- Control: tubería `\.\pipe\winarchy-shell`, una línea JSON por pedido `{"v":1,"id":1,"cmd":"ping"}`; comandos `ping`, `version`, `reload`, `quit`.

## Dock

Con `winarchy bar dock on` la shell agrega un dock con las apps abiertas, en el borde opuesto al de la isla. Lista ventanas por AppUserModelID (respaldo: exe), enfoca al hacer clic (restaura minimizadas, cambia de workspace con `komorebic` y cicla si el grupo ya está activo) y se oculta con un juego o video a pantalla completa en su monitor. La taskbar nativa queda en auto-hide y sigue alojando la bandeja. Sin anclados, sin preview ni reordenar por ahora. Apagarlo: `winarchy bar dock off`.
