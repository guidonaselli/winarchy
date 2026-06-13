// managed by winarchy — generado desde templates/fastfetch.jsonc.tpl por `winarchy theme set`.
// Logo: W de winarchy en los 4 cuadrantes de Windows; la W va en el accent del theme.
{
    "$schema": "https://github.com/fastfetch-cli/fastfetch/raw/dev/doc/json_schema.json",
    "logo": {
        "type": "file",
        "source": "{{computed.root}}/config/fastfetch/logo.txt",
        "color": {
            "1": "blue",
            "2": "38;2;{{colors.accent_ansi_rgb}}"
        },
        "padding": { "top": 1, "left": 2, "right": 4 }
    },
    "display": {
        "separator": "  ",
        "color": { "keys": "blue" }
    },
    "modules": [
        "break",
        { "type": "os", "key": "󰍹 os" },
        { "type": "kernel", "key": "󰒋 kernel" },
        { "type": "wm", "key": " wm" },
        { "type": "terminal", "key": " term" },
        { "type": "shell", "key": " shell" },
        { "type": "cpu", "key": " cpu" },
        { "type": "gpu", "key": "󰢮 gpu" },
        { "type": "memory", "key": " mem" },
        { "type": "uptime", "key": "󰔟 up" },
        "break",
        { "type": "colors", "paddingLeft": 2, "symbol": "circle" }
    ]
}
