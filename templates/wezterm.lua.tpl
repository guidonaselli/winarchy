-- managed by winarchy — generado desde templates/wezterm.lua.tpl. No editar a mano:
-- se regenera con `winarchy theme set`. Tu personalización va en config/wezterm/user.lua.
local wezterm = require 'wezterm'
local act = wezterm.action

local config = wezterm.config_builder()

config.color_scheme = nil
config.colors = {
  background = '{{colors.background}}',
  foreground = '{{colors.foreground}}',
  cursor_bg = '{{colors.cursor}}',
  cursor_border = '{{colors.cursor}}',
  cursor_fg = '{{colors.background}}',
  selection_bg = '{{colors.selection_background}}',
  selection_fg = '{{colors.selection_foreground}}',
  split = '{{borders.unfocused}}',
  ansi = {
    '{{colors.color0}}', '{{colors.color1}}', '{{colors.color2}}', '{{colors.color3}}',
    '{{colors.color4}}', '{{colors.color5}}', '{{colors.color6}}', '{{colors.color7}}',
  },
  brights = {
    '{{colors.color8}}', '{{colors.color9}}', '{{colors.color10}}', '{{colors.color11}}',
    '{{colors.color12}}', '{{colors.color13}}', '{{colors.color14}}', '{{colors.color15}}',
  },
  tab_bar = {
    background = '{{colors.color0}}',
    active_tab = { bg_color = '{{borders.focused}}', fg_color = '{{colors.background}}' },
    inactive_tab = { bg_color = '{{colors.color0}}', fg_color = '{{colors.color8}}' },
    inactive_tab_hover = { bg_color = '{{borders.unfocused}}', fg_color = '{{colors.foreground}}' },
    new_tab = { bg_color = '{{colors.color0}}', fg_color = '{{colors.color8}}' },
    new_tab_hover = { bg_color = '{{borders.unfocused}}', fg_color = '{{colors.foreground}}' },
  },
}

-- El terminal profile del stack (starship, fzf, zoxide, eza, bat) vive en
-- PowerShell 7: sin esto WezTerm levanta cmd.exe y no carga nada de eso.
config.default_prog = { 'pwsh.exe', '-NoLogo' }
config.default_cwd = wezterm.home_dir
config.launch_menu = {
  { label = 'PowerShell 7', args = { 'pwsh.exe', '-NoLogo' } },
  { label = 'Windows PowerShell', args = { 'powershell.exe', '-NoLogo' } },
  { label = 'Command Prompt', args = { 'cmd.exe' } },
}

-- Winarchy es el único canal de updates: el auto-updater propio de WezTerm queda apagado,
-- como los de YASB y Flow. Lo mueve `winarchy update --core` y nada más.
config.check_for_updates = false

config.font = wezterm.font_with_fallback {
  '{{bar.font_family}}',
  'Consolas',
  'Segoe UI Emoji',
}
config.font_size = 12.0
config.harfbuzz_features = { 'calt=1', 'clig=1', 'liga=1' }
config.window_padding = { left = 12, right = 12, top = 10, bottom = 8 }
config.window_background_opacity = {{computed.terminal_opacity}}
config.scrollback_lines = 10000
config.hide_tab_bar_if_only_one_tab = true
config.use_fancy_tab_bar = false
config.tab_bar_at_bottom = false
config.inactive_pane_hsb = { saturation = 0.9, brightness = 0.7 }
-- Monitores de alto refresco: el default (60) se ve entrecortado al scrollear.
config.max_fps = 120
-- Máquinas con GPU integrada + discreta: sin esto el render puede caer en la integrada.
config.webgpu_power_preference = 'HighPerformance'
-- komorebi tilea la ventana: sin confirmación al cerrar un pane con proceso vivo y sin
-- que WezTerm intente recordar tamaños de ventana que el tiling va a pisar igual.
config.window_close_confirmation = 'NeverPrompt'
config.audible_bell = 'Disabled'
-- komorebi manda el tamaño y la posición, y dibuja el borde de foco: la barra de título
-- solo come alto útil. Quien la quiera de vuelta: window_decorations en user.lua.
config.window_decorations = 'RESIZE'
config.adjust_window_size_when_changing_font_size = false

-- Los hotkeys globales son de AHK: acá solo lo interno a la ventana, detrás del leader
-- (mismo mapa que las teclas de tmux de Omarchy) y Alt+1..9 para ir a una tab.
config.leader = { key = 'Space', mods = 'CTRL', timeout_milliseconds = 1000 }
config.keys = {
  { key = 'Space', mods = 'LEADER|CTRL', action = act.SendKey { key = 'Space', mods = 'CTRL' } },
  { key = 'v', mods = 'LEADER', action = act.SplitHorizontal { domain = 'CurrentPaneDomain' } },
  { key = 'h', mods = 'LEADER', action = act.SplitVertical { domain = 'CurrentPaneDomain' } },
  { key = 'LeftArrow', mods = 'LEADER', action = act.ActivatePaneDirection 'Left' },
  { key = 'DownArrow', mods = 'LEADER', action = act.ActivatePaneDirection 'Down' },
  { key = 'UpArrow', mods = 'LEADER', action = act.ActivatePaneDirection 'Up' },
  { key = 'RightArrow', mods = 'LEADER', action = act.ActivatePaneDirection 'Right' },
  { key = 'z', mods = 'LEADER', action = act.TogglePaneZoomState },
  { key = 'x', mods = 'LEADER', action = act.CloseCurrentPane { confirm = false } },
  { key = 'c', mods = 'LEADER', action = act.SpawnTab 'CurrentPaneDomain' },
  { key = 'k', mods = 'LEADER', action = act.CloseCurrentTab { confirm = false } },
  { key = 'n', mods = 'LEADER', action = act.ActivateTabRelative(1) },
  { key = 'p', mods = 'LEADER', action = act.ActivateTabRelative(-1) },
  { key = '[', mods = 'LEADER', action = act.ActivateCopyMode },
  {
    key = 'r', mods = 'LEADER',
    action = act.PromptInputLine {
      description = 'Tab name',
      action = wezterm.action_callback(function(window, _, line)
        if line then window:active_tab():set_title(line) end
      end),
    },
  },
  -- Las "sessions" de tmux son los workspaces de WezTerm.
  {
    key = 'C', mods = 'LEADER|SHIFT',
    action = act.PromptInputLine {
      description = 'New workspace',
      action = wezterm.action_callback(function(window, pane, line)
        if line and line ~= '' then
          window:perform_action(act.SwitchToWorkspace { name = line }, pane)
        end
      end),
    },
  },
  {
    key = 'R', mods = 'LEADER|SHIFT',
    action = act.PromptInputLine {
      description = 'Rename workspace',
      action = wezterm.action_callback(function(_, _, line)
        if line and line ~= '' then
          wezterm.mux.rename_workspace(wezterm.mux.get_active_workspace(), line)
        end
      end),
    },
  },
  { key = 'N', mods = 'LEADER|SHIFT', action = act.SwitchWorkspaceRelative(1) },
  { key = 'P', mods = 'LEADER|SHIFT', action = act.SwitchWorkspaceRelative(-1) },
  { key = 's', mods = 'LEADER', action = act.ShowLauncherArgs { flags = 'FUZZY|WORKSPACES' } },
}
for i = 1, 9 do
  table.insert(config.keys, { key = tostring(i), mods = 'ALT', action = act.ActivateTab(i - 1) })
end

-- Titulo de tab: indice + programa. El default muestra la linea de comando entera, que
-- con un pwsh.exe -NoLogo -File ... deja todas las tabs con el mismo texto ilegible.
wezterm.on('format-tab-title', function(tab, _, _, _, _, max_width)
  local pane = tab.active_pane
  local name = pane.foreground_process_name
  name = name and name:gsub('.*[/\\]', ''):gsub('%.exe$', '') or pane.title
  local title = ' ' .. (tab.tab_index + 1) .. ': ' .. name .. ' '
  if #title > max_width then title = wezterm.truncate_right(title, max_width - 1) .. '… ' end
  return title
end)

-- Override del usuario: gitignored, ningún update lo toca. Devuelve una tabla y sus
-- claves pisan a las de acá. Un error de sintaxis se loguea y no impide arrancar.
local user_config = [==[{{computed.root}}/config/wezterm/user.lua]==]
local ok, user = pcall(dofile, user_config)
if ok and type(user) == 'table' then
  for k, v in pairs(user) do config[k] = v end
elseif not ok and not tostring(user):match 'No such file or directory' then
  wezterm.log_error('winarchy: ' .. user_config .. ' ignorado: ' .. tostring(user))
end

return config
