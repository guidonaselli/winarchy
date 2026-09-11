# managed by winarchy — generado desde templates/starship.toml.tpl por `winarchy theme set`
# NO editar a mano: personalización del prompt va en config/pwsh/user.ps1 (o un
# STARSHIP_CONFIG propio exportado ahí, que pisa este).

add_newline = true
scan_timeout = 100

format = """
$directory$git_branch$git_status$git_state$cmd_duration$python$nodejs$rust
$character"""

[character]
success_symbol = "[❯](bold {{colors.accent_ui}})"
error_symbol = "[❯](bold {{colors.color1}})"

[directory]
style = "bold {{colors.accent_ui}}"
truncation_length = 4
truncate_to_repo = true
format = " [$path]($style)[$read_only]($read_only_style) "

[git_branch]
style = "bold {{colors.color5}}"
format = "[$symbol$branch]($style) "

[git_status]
style = "{{colors.color3}}"
format = "([$all_status$ahead_behind]($style) )"

[git_state]
style = "bold {{colors.color1}}"

[cmd_duration]
min_time = 2000
style = "{{colors.color3}}"
format = "[⏱ $duration]($style) "

[status]
disabled = false
style = "bold {{colors.color1}}"

[python]
style = "{{colors.color6}}"
format = "[$symbol$version]($style) "

[nodejs]
style = "{{colors.color2}}"
format = "[$symbol$version]($style) "

[rust]
style = "{{colors.color1}}"
format = "[$symbol$version]($style) "

[package]
disabled = true
