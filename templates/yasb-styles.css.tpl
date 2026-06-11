/* managed by winarchy — generado desde templates/yasb-styles.css.tpl. NO editar a mano. */
/* Theme: {{name}} */

* {
    font-size: 13px;
    font-family: 'JetBrainsMono NF', 'Segoe UI', sans-serif;
    color: {{colors.foreground}};
    font-weight: 500;
    margin: 0;
    padding: 0;
}

.yasb-bar {
    background-color: {{colors.background}};
    padding: 0 8px;
}

.widget {
    padding: 0 8px;
}

.widget .label {
    color: {{colors.foreground}};
}

.widget .label.alt {
    color: {{colors.accent}};
}

/* Iconos nerd-font (<span> en los labels): YASB les pone clase "icon" (sin "label").
   El padding les da aire contra el número y evita que Qt recorte glifos
   más anchos que su caja */
.widget .icon {
    font-size: 15px;
    padding: 0 7px 0 2px;
}

/* Workspaces komorebi */
.komorebi-workspaces .ws-btn {
    color: {{colors.color8}};
    background-color: transparent;
    border: none;
    padding: 0 6px;
    margin: 0 2px;
    font-size: 13px;
}

.komorebi-workspaces .ws-btn.populated_workspace {
    color: {{colors.foreground}};
}

.komorebi-workspaces .ws-btn.active_populated_workspace,
.komorebi-workspaces .ws-btn.active_empty_workspace,
.komorebi-workspaces .ws-btn.active {
    color: {{colors.background}};
    background-color: {{colors.accent}};
    border-radius: 4px;
}

.komorebi-active-layout .label {
    color: {{colors.accent}};
}

.active-window-widget .label {
    color: {{colors.foreground}};
}

.clock-widget .label {
    color: {{colors.accent}};
    font-weight: 600;
}

.home-widget .label {
    color: {{colors.accent}};
    font-size: 15px;
    padding: 0 6px;
}

/* Popup del menú home (sin esto el menú queda transparente) */
.home-menu {
    background-color: {{colors.background}};
    border: 1px solid {{colors.color8}};
}

.home-menu .menu-item {
    color: {{colors.foreground}};
    padding: 7px 24px 7px 14px;
    font-size: 13px;
}

.home-menu .menu-item:hover {
    background-color: {{colors.accent}};
    color: {{colors.background}};
}

.home-menu .separator {
    max-height: 1px;
    background-color: {{colors.color8}};
}

.gpu-widget .label,
.cpu-widget .label,
.memory-widget .label,
.gpu-widget .icon,
.cpu-widget .icon,
.memory-widget .icon {
    color: {{colors.color6}};
}

.volume-widget .label,
.microphone-widget .label,
.media-widget .label,
.volume-widget .icon,
.microphone-widget .icon,
.media-widget .icon {
    color: {{colors.color5}};
}

.notification-widget .label,
.notification-widget .icon {
    color: {{colors.color8}};
}

.notification-widget .new-notification {
    color: {{colors.accent}};
}

.wifi-widget .label,
.bluetooth-widget .label,
.wifi-widget .icon,
.bluetooth-widget .icon {
    color: {{colors.color4}};
}

/* El QLabel del icono de wifi se queda corto cuando el glifo cambia
   (arranca con el de wifi y pasa al de ethernet): ancho mínimo explícito */
.wifi-widget .icon {
    min-width: 20px;
}

/* Widgets de solo icono: padding simétrico para que la separación
   con los vecinos quede pareja (el 0 7px 0 2px es para icono+número) */
.wifi-widget .icon,
.bluetooth-widget .icon,
.microphone-widget .icon {
    padding: 0 3px;
}

/* Popup del selector de redes wifi */
.wifi-menu {
    background-color: {{colors.background}};
    border: 1px solid {{colors.color8}};
}

.wifi-menu .header {
    color: {{colors.foreground}};
    font-weight: 600;
    padding: 8px 10px;
}

.wifi-menu .wifi-item {
    padding: 5px 10px;
}

.wifi-menu .wifi-item .name {
    color: {{colors.foreground}};
}

.wifi-menu .wifi-item .status,
.wifi-menu .wifi-item .strength {
    color: {{colors.color8}};
}

.wifi-menu .error-message {
    color: {{borders.urgent}};
    padding: 8px;
}

.wifi-menu .footer .settings-button {
    color: {{colors.color4}};
    padding: 6px 10px;
}

.wifi-menu .controls-container .password {
    background-color: {{colors.color0}};
    color: {{colors.foreground}};
    padding: 4px 8px;
}

.wifi-menu .controls-container .connect {
    background-color: {{colors.accent}};
    color: {{colors.background}};
    padding: 4px 10px;
}

.systray-widget {
    background: transparent;
}

.game-mode-widget .label {
    color: {{borders.urgent}};
    font-weight: 700;
}

/* Estado de alerta / urgencia */
.urgent, .alert {
    color: {{borders.urgent}};
}
