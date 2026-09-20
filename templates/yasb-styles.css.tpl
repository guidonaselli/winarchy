/* managed by winarchy — generado desde templates/yasb-styles.css.tpl. NO editar a mano. */
/* Theme: {{name}} */

* {
    font-size: {{bar.font_size}}px;
    font-family: '{{bar.font_family}}', 'Segoe UI', sans-serif;
    color: {{colors.foreground}};
    font-weight: 500;
    margin: 0;
    padding: 0;
}

.yasb-bar {
    background-color: {{computed.bar_background}};
    padding: 0 {{bar.padding}}px;
}

.widget {
    padding: 0 8px;
}

.widget .label {
    color: {{colors.foreground}};
}

.widget .label.alt {
    color: {{colors.accent_ui}};
}

/* Iconos nerd-font (<span> en los labels): YASB les pone clase "icon" (sin "label").
   Qt dimensiona el QLabel por el advance de la fuente y muchos glifos pintan más
   ancho que eso, así que se recortan: min-width les reserva la caja completa. */
.widget .icon {
    font-size: 15px;
    padding: 0 5px;
    min-width: 18px;
    qproperty-alignment: AlignCenter;
}

/* Workspaces komorebi */
.komorebi-workspaces .ws-btn {
    color: {{colors.color8}};
    background-color: transparent;
    border: none;
    padding: 0 6px;
    margin: 0 2px;
    font-size: {{bar.font_size}}px;
}

.komorebi-workspaces .ws-btn.populated_workspace {
    color: {{colors.foreground}};
}

.komorebi-workspaces .ws-btn:hover {
    color: {{colors.foreground}};
}

.komorebi-workspaces .ws-btn.active_populated_workspace,
.komorebi-workspaces .ws-btn.active_empty_workspace,
.komorebi-workspaces .ws-btn.active {
    color: {{colors.on_accent}};
    background-color: {{colors.accent_ui}};
    border-radius: 4px;
}

.komorebi-active-layout .label {
    color: {{colors.accent_ui}};
}

.active-window-widget .label {
    color: {{colors.foreground}};
}

.clock-widget .label {
    color: {{colors.accent_ui}};
    font-weight: 600;
}

.home-widget .label {
    color: {{colors.accent_ui}};
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
    background-color: {{colors.accent_ui}};
    color: {{colors.on_accent}};
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
    color: {{ui.system}};
}

.volume-widget .label,
.microphone-widget .label,
.media-widget .label,
.volume-widget .icon,
.microphone-widget .icon,
.media-widget .icon {
    color: {{ui.media}};
}

.media-widget .label {
    font-size: 14px;
}

.notification-widget .label,
.notification-widget .icon {
    color: {{colors.color8}};
}

.notification-widget .new-notification {
    color: {{colors.accent_ui}};
}

.dnd-widget .label,
.battery-widget .label,
.brightness-widget .label,
.power-plan-widget .label,
.dnd-widget .icon,
.battery-widget .icon,
.brightness-widget .icon,
.power-plan-widget .icon {
    color: {{ui.system}};
}

.extras-grouper .widget,
.ai-grouper .widget,
.system-grouper .widget,
.connections-grouper .widget {
    padding: 0 2px;
}

.extras-grouper .widget .icon,
.ai-grouper .widget .icon,
.system-grouper .widget .icon,
.connections-grouper .widget .icon {
    padding: 0 4px;
}

.tool-widget .icon {
    color: {{ui.system}};
}

.tool-widget .icon:hover {
    color: {{colors.accent_ui}};
}

.extras-grouper .grouper-button,
.ai-grouper .grouper-button,
.system-grouper .grouper-button,
.connections-grouper .grouper-button {
    color: {{ui.net}};
    font-size: 16px;
    background: transparent;
    border: none;
    padding: 0 5px;
    min-width: 18px;
}

.extras-grouper .grouper-button:hover,
.ai-grouper .grouper-button:hover,
.system-grouper .grouper-button:hover,
.connections-grouper .grouper-button:hover {
    color: {{colors.foreground}};
}

.open-meteo-widget .label,
.open-meteo-widget .icon {
    color: {{ui.net}};
}

.claude-usage .label,
.claude-usage .icon,
.codex-usage .label,
.codex-usage .icon {
    color: {{ui.media}};
}

.wifi-widget .label,
.bluetooth-widget .label,
.wifi-widget .icon,
.bluetooth-widget .icon {
    color: {{ui.net}};
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
    color: {{ui.net}};
    padding: 6px 10px;
}

.wifi-menu .controls-container .password {
    background-color: {{colors.color0}};
    color: {{colors.foreground}};
    padding: 4px 8px;
}

.wifi-menu .controls-container .connect {
    background-color: {{colors.accent_ui}};
    color: {{colors.on_accent}};
    padding: 4px 10px;
}

/* Systray. La clase base del widget es `.systray` (no `.systray-widget`);
   el botón de colapsar/expandir es `.unpinned-visibility-btn`. Sin estos
   estilos YASB le deja a la flechita y a los iconos su hover blanco
   translúcido por defecto, ajeno al theme. */
.systray {
    background: transparent;
}

.systray .pinned-container,
.systray .unpinned-container {
    background: transparent;
}

/* La "flechita" que despliega los iconos no fijados */
.systray .unpinned-visibility-btn {
    color: {{ui.net}};
    background: transparent;
    border: none;
    border-radius: 4px;
    padding: 0 5px;
    margin: 0 2px;
    font-size: 16px;
    min-width: 18px;
}

.systray .unpinned-visibility-btn:hover {
    color: {{colors.foreground}};
    background-color: {{colors.color0}};
}

/* Iconos individuales del tray: hover sutil acorde al theme */
.systray .button {
    border-radius: 4px;
    padding: 2px;
    margin: 0 1px;
}

.systray .button:hover {
    background-color: {{colors.color0}};
}

/* Misma identidad que el logo de la izquierda: los dos abren el mismo menu. */
.winarchy-menu-widget .label {
    color: {{colors.accent_ui}};
    font-size: 15px;
    padding: 0 6px;
}

.winarchy-update-widget .label {
    color: {{colors.accent_ui}};
    font-weight: 700;
}

.stay-awake-widget .label {
    color: {{ui.system}};
    font-weight: 700;
}

.game-mode-widget .label {
    color: {{borders.urgent}};
    font-weight: 700;
}

/* Estado de alerta / urgencia */
.urgent, .alert {
    color: {{borders.urgent}};
}

/* Popups de uso de Claude y Codex */
.claude-usage-menu,
.codex-usage-menu {
    background-color: {{colors.background}};
    border: 1px solid {{colors.color8}};
    font-family: 'Segoe UI', sans-serif;
    min-width: 320px;
}

.claude-usage-menu .header,
.codex-usage-menu .header {
    padding: 12px 16px 10px 16px;
    border-bottom: 1px solid {{colors.color8}};
}

.claude-usage-menu .header .text,
.codex-usage-menu .header .text {
    color: {{colors.foreground}};
    font-size: 15px;
    font-weight: 600;
}

.claude-usage-menu .header .refresh,
.claude-usage-menu .header .pin-btn,
.codex-usage-menu .header .refresh {
    color: {{colors.color8}};
    font-size: 15px;
    padding: 2px 6px;
    border-radius: 4px;
    background-color: transparent;
    border: none;
}

.claude-usage-menu .header .refresh:hover,
.claude-usage-menu .header .pin-btn:hover,
.codex-usage-menu .header .refresh:hover {
    color: {{colors.foreground}};
    background-color: {{colors.color0}};
}

.claude-usage-menu .header .pin-btn {
    font-family: 'Segoe Fluent Icons';
}

.claude-usage-menu .header .pin-btn.pinned {
    color: {{colors.accent_ui}};
}

.codex-usage-menu .header .refresh {
    font-family: 'Segoe Fluent Icons';
}

.codex-usage-menu .header .refresh-status {
    color: {{colors.color8}};
    font-size: 12px;
    padding-right: 6px;
}

.codex-usage-menu .header .refresh-status.success {
    color: {{colors.color2}};
}

.codex-usage-menu .header .refresh-status.error {
    color: {{colors.color1}};
}

.claude-usage-menu .section,
.codex-usage-menu .section {
    padding: 12px 16px;
    border-bottom: 1px solid {{colors.color0}};
}

.claude-usage-menu .section.tokens {
    border-bottom: none;
}

.claude-usage-menu .section .title,
.codex-usage-menu .section .title {
    color: {{colors.color8}};
    font-size: 12px;
    font-weight: 600;
    padding-bottom: 6px;
}

.claude-usage-menu .section .progress,
.codex-usage-menu .section .progress,
.claude-usage-menu .section.tokens .model-rows .progress,
.codex-usage-menu .model-bar {
    background-color: {{colors.color0}};
    border-radius: 4px;
    min-height: 8px;
    max-height: 8px;
}

.claude-usage-menu .section .progress .fill,
.codex-usage-menu .section .progress .fill {
    border-radius: 4px;
}

.claude-usage-menu .section .progress.low .fill,
.codex-usage-menu .section .progress .fill {
    background-color: {{colors.color2}};
}

.claude-usage-menu .section .progress.medium .fill,
.codex-usage-menu .section .progress.low .fill {
    background-color: {{colors.color3}};
}

.claude-usage-menu .section .progress.high .fill,
.codex-usage-menu .section .progress.critical .fill {
    background-color: {{colors.color1}};
}

.claude-usage-menu .section .footer .percent,
.codex-usage-menu .section .remaining {
    color: {{colors.foreground}};
    font-size: 14px;
    font-weight: 600;
}

.claude-usage-menu .section .footer .percent.low,
.codex-usage-menu .section .remaining.good {
    color: {{colors.color2}};
}

.claude-usage-menu .section .footer .percent.medium,
.codex-usage-menu .section .remaining.low {
    color: {{colors.color3}};
}

.claude-usage-menu .section .footer .percent.high,
.codex-usage-menu .section .remaining.critical {
    color: {{colors.color1}};
}

.claude-usage-menu .section .footer .reset,
.codex-usage-menu .section .reset,
.codex-usage-menu .section .used {
    color: {{colors.color8}};
    font-size: 12px;
}

.claude-usage-menu .section .date,
.codex-usage-menu .section .date {
    color: {{colors.color8}};
    font-size: 11px;
}

.claude-usage-menu .section .period-btn,
.codex-usage-menu .page-button,
.codex-usage-menu .month-nav {
    color: {{colors.color8}};
    background-color: transparent;
    border: none;
    border-radius: 4px;
    padding: 2px 8px;
}

.claude-usage-menu .section .period-btn.active,
.claude-usage-menu .section .period-btn:hover,
.codex-usage-menu .page-button:hover,
.codex-usage-menu .month-nav:hover {
    color: {{colors.foreground}};
    background-color: {{colors.color0}};
}

.codex-usage-menu .page-button,
.codex-usage-menu .month-nav {
    font-family: 'Segoe Fluent Icons';
}

.codex-usage-menu .page-button:disabled,
.codex-usage-menu .month-nav:disabled {
    color: {{colors.color0}};
}

.claude-usage-menu .section .token-total,
.codex-usage-menu .period-value {
    color: {{colors.foreground}};
    font-size: 16px;
    font-weight: 600;
}

.codex-usage-menu .period-name,
.codex-usage-menu .section-title,
.codex-usage-menu .activity-title,
.codex-usage-menu .page-indicator,
.codex-usage-menu .details .name,
.codex-usage-menu .history-note,
.codex-usage-menu .empty-state {
    color: {{colors.color8}};
    font-size: 12px;
}

.codex-usage-menu .page {
    padding: 12px 16px;
}

.codex-usage-menu .details .value,
.codex-usage-menu .model-name,
.claude-usage-menu .section.tokens .model-name {
    color: {{colors.foreground}};
    font-size: 12px;
}

.codex-usage-menu .model-value,
.claude-usage-menu .section.tokens .model-total {
    color: {{colors.color8}};
    font-size: 12px;
}

.codex-usage-menu .model-bar .fill,
.claude-usage-menu .section.tokens .model-rows .progress .fill {
    background-color: {{colors.accent_ui}};
    border-radius: 3px;
}

.codex-usage-menu .details .status.live {
    color: {{colors.color2}};
}

.codex-usage-menu .details .status.stale,
.codex-usage-menu .details .error {
    color: {{colors.color1}};
}

.codex-usage-menu .heatmap .cell {
    background-color: {{colors.color0}};
    border-radius: 3px;
    min-width: 20px;
    max-width: 20px;
    min-height: 20px;
    max-height: 20px;
}

.codex-usage-menu .heatmap .cell.level-1,
.codex-usage-menu .heatmap .cell.level-2,
.codex-usage-menu .heatmap .cell.level-3,
.codex-usage-menu .heatmap .cell.level-4 {
    background-color: {{colors.accent_ui}};
}

.codex-usage-menu .heatmap .cell.future,
.codex-usage-menu .heatmap .cell.outside {
    background-color: transparent;
}
