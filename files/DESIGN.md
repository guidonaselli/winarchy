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

## Teclado y mouse
- **Teclado:** Ctrl+C/X/V/A/Z/Y/N/T/W/F, Ctrl+Mayús+N (carpeta nueva), Ctrl+Mayús+T (reabrir pestaña), Ctrl+Mayús+C (copiar ruta), Ctrl+1…9 (ir a pestaña), Ctrl+Tab y Ctrl+Mayús+Tab, Alt+←/→/↑ (atrás, adelante, subir), Alt+Enter (propiedades), F2 (renombrar), F5, Supr y Mayús+Supr, Retroceso (subir), flechas, Inicio y Fin (mover; con Mayús extienden el rango) y escribir para saltar a un nombre.
- **Menús:** con un menú abierto, ↑/↓ recorren los comandos habilitados, Enter ejecuta y Esc cierra.
- **Mouse:** clic selecciona, doble clic abre, Ctrl+clic alterna, Mayús+clic selecciona un rango, clic del medio abre la carpeta en una pestaña nueva (o cierra la pestaña), botones laterales van atrás y adelante. Las casillas aparecen al pasar el mouse o con la fila seleccionada.
- **Menú clásico:** «Mostrar más opciones» abre el menú del shell de Windows, con las entradas de terceros.
