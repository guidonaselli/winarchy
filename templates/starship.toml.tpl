# managed by winarchy — generado desde templates/starship.toml.tpl por `winarchy theme set`
# NO editar a mano: personalización del prompt va en config/pwsh/user.ps1 (o un
# STARSHIP_CONFIG propio exportado ahí, que pisa este).

add_newline = true

[character]
success_symbol = "[❯](bold {{colors.accent_ui}})"
error_symbol = "[❯](bold {{colors.color1}})"

[directory]
style = "bold {{colors.accent_ui}}"
truncation_length = 4
truncate_to_repo = true

[git_branch]
style = "bold {{colors.color5}}"

[git_status]
style = "{{colors.color3}}"

[git_state]
style = "bold {{colors.color1}}"

[cmd_duration]
min_time = 2000
style = "{{colors.color3}}"

[status]
disabled = false
style = "bold {{colors.color1}}"

[python]
style = "{{colors.color6}}"

[nodejs]
style = "{{colors.color2}}"

[rust]
style = "{{colors.color1}}"

[package]
disabled = true
