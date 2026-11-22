# Files — diseño

## Fuentes únicas de estilo
- **Color:** solo `config/files/theme.json` (`background`, `foreground`, `accent`, `urgent`, `muted`, `surface`, `selection`, `mode`). Ningún color en código; las vistas piden el color a `Theme`.
- **Espaciado y tamaños:** solo `src/style.rs`. Ningún número suelto en las vistas; si falta un valor, se agrega ahí.
- **Textos:** solo `tr("…")` / `tr_dynamic` con clave en inglés y tabla ES en `src/i18n.rs`.

## Jerarquía de la pantalla (arriba → abajo)
1. Pestañas.
2. Navegación en una fila: Atrás, Adelante, Subir (íconos), migas de pan que se editan como dirección (lápiz o Ctrl+L; Enter va, Esc cancela) y filtro.
3. Barra de comandos: acciones primarias con ícono (crear, cortar/copiar/pegar, renombrar, eliminar); el resto en el menú «…».
4. Franja de texto solo mientras una acción la pide (crear, renombrar, etiquetas, clonar, copiar a…, mover a…): etiqueta, campo con foco, Aceptar/Cancelar; Enter acepta y Esc cancela.
5. Encabezado de orden (Nombre, Tamaño, Modificado).
6. Detalle, vista previa y estado solo si tienen contenido (no ocupan espacio vacío).
7. Lista: casilla, ícono por tipo, nombre, tamaño, fecha.

## Estados
- **Seleccionado:** fondo `selection` en la fila y casilla marcada.
- **Deshabilitado:** acciones sin selección o con una operación en curso (comando `enabled = false`).
- **Error:** el texto de estado usa `urgent`; el normal usa `accent`.
- **Hover/foco:** los de WinUI sobre el tema; pendiente verificar contraste ≥ 3:1 (tarea 5.5).

## Teclado
Ctrl+C/X/V/A/Z/Y/N/T/W/Tab, Supr, Retroceso (subir), F5, flechas (mover selección), Enter (abrir la selección cuando el foco no está en un botón o campo).
