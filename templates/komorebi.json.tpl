{
  "$schema": "https://raw.githubusercontent.com/LGUG2Z/komorebi/v0.1.41/schema.json",
  "_managed": "managed by winarchy — generado desde templates/komorebi.json.tpl + themes/{{computed.theme_dir}}/theme.toml. NO editar a mano.",
  "window_hiding_behaviour": "Cloak",
  "cross_monitor_move_behaviour": "Insert",
  "default_workspace_padding": 6,
  "default_container_padding": 4,
  "mouse_follows_focus": false,
  "focus_follows_mouse": "Windows",
  "border": true,
  "border_width": 3,
  "border_offset": -1,
  "border_colours": {
    "single": "{{borders.focused}}",
    "stack": "{{borders.stack}}",
    "monocle": "{{borders.monocle}}",
    "unfocused": "{{borders.unfocused}}"
  },
  "monitors": [
    {
      "workspaces": [
        { "name": "1", "layout": "BSP" },
        { "name": "2", "layout": "BSP" },
        { "name": "3", "layout": "BSP" },
        { "name": "4", "layout": "BSP" },
        { "name": "5", "layout": "BSP" },
        { "name": "6", "layout": "BSP" },
        { "name": "7", "layout": "BSP" },
        { "name": "8", "layout": "BSP" },
        { "name": "9", "layout": "BSP" }
      ]
    },
    {
      "workspaces": [
        { "name": "A", "layout": "BSP" },
        { "name": "B", "layout": "BSP" },
        { "name": "C", "layout": "BSP" }
      ]
    }
  ],
  "ignore_rules": [
    { "kind": "Class", "id": "#32770", "matching_strategy": "Equals" },
    { "kind": "Title", "id": "Control Panel", "matching_strategy": "Equals" },
    { "kind": "Exe", "id": "msiexec.exe", "matching_strategy": "Equals" },
    { "kind": "Exe", "id": "PowerToys.PowerLauncher.exe", "matching_strategy": "Equals" },
    { "kind": "Exe", "id": "Flow.Launcher.exe", "matching_strategy": "Equals" },
    { "kind": "Exe", "id": "Twinkle Tray.exe", "matching_strategy": "Equals" },
    { "kind": "Class", "id": "TaskManagerWindow", "matching_strategy": "Equals" },
    { "kind": "Title", "id": "Picture-in-Picture", "matching_strategy": "Equals" },
    { "kind": "Title", "id": "Winarchy — Themes", "matching_strategy": "Equals" }{{computed.game_ignore_rules}}
  ]
}
