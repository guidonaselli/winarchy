# winarchy-shell

Shell nativa opcional de Winarchy (franja reservada + isla). Se activa con `state/bar=native`; sin ese archivo manda YASB.

- Build: `cargo build --release` (Rust estable, target MSVC).
- Uso: `winarchy-shell.exe [ruta\a\theme.json]` (por defecto `config\shell\theme.json` del repo).
- Control: tubería `\.\pipe\winarchy-shell`, una línea JSON por pedido `{"v":1,"id":1,"cmd":"ping"}`; comandos `ping`, `version`, `reload`, `quit`.
