# Themes

Themes are drop-in folders inside `themes/`.

```text
themes/my-theme/
  theme.toml
  preview.png
  backgrounds/
  lockscreen.png
  wallpaper-engine.txt
  jetbrains.icls
  vscode.json
```

Everything in `backgrounds/` is a wallpaper the theme offers. Applying the theme sets the
one you picked last time for it (the first one otherwise); `winarchy background gallery` opens
 a visual picker. `winarchy background next` or `SUPER + CTRL + SPACE` cycles through the
rest. A theme without the folder leaves the
wallpaper alone, and `wallpaper-engine.txt` wins over it when Wallpaper Engine is installed.

Applying a theme also sets the lock screen image, which the sign-in screen reuses: the
theme's `lockscreen.png` (or `.jpg`), otherwise its current wallpaper. Themes using
`wallpaper-engine.txt` leave it alone unless they ship a `lockscreen.*`. Dynamic themes such as
auto-accent draw it from the live accent (the Winarchy mark with its last bar in the accent) and
redraw it whenever the accent changes. If the lock screen is set to Windows Spotlight it switches
to Picture, and the sign-in screen only follows it with "Show the lock screen background picture
on the sign-in screen" on.

Only `theme.toml` is required, and any other file is ignored: keep leftovers from other
ecosystems out of the folder.

Values are substituted into generated JSON, CSS, YAML, XAML, INI, TOML and Lua, so their
format is checked on the way in and the whole theme is rejected before anything is written:
`colors.*`, `borders.*` and `ui.*` must be `#rrggbb`; the `[bar]` measures must be integers
and `font_family` a font name; the `extension` in `vscode.json` must be `publisher.name`.

Running:

```powershell
winarchy theme set my-theme
```

regenerates and applies the full surface atomically.

A theme can also name its colors instead of numbering them, and Winarchy derives the rest:

```toml
[colors]
accent = "#3264eb"
selection = "#d0d0d0"
muted = "#9e9e9e"
background = "#fafafa"
darker_background = "#dedede"
foreground = "#212121"
light_foreground = "#424242"
bright_foreground = "#000000"
red = "#c900c4"
green = "#4a2fd0"
# ... yellow, blue, magenta, cyan and their bright_ variants
```

`color0`-`color15`, `cursor`, `selection_background`, `selection_foreground` and the whole
`[borders]` section are filled in from those names. Whatever the theme declares itself always
wins, so both forms can be mixed and an existing ANSI theme keeps working untouched. Porting
a theme is then a copy of its upstream palette rather than a manual remap.

`theme.toml` also takes an optional `[bar]` section that drives the YASB bar geometry and
typography (and the stackbar font komorebi draws over stacked windows). Omit it and the
theme inherits the defaults:

```toml
[bar]
height = 34
font_family = "JetBrainsMono NF"
font_size = 13
padding = 8
```

`color0`-`color15` are the ANSI palette (terminal, btop, nvim), where red has to stay red.
The bar reads them by default for its icon groups, but a theme can decouple the two with an
optional `[ui]` section — useful when the accent is dynamic and a fixed palette would clash
(see `auto-accent`):

```toml
[ui]
system = "#c8c8c8"   # gpu / cpu / memory   (default: color6)
media = "#9ba3b8"    # volume / mic / media (default: color5)
net = "#7e8699"      # wifi / bluetooth     (default: color4)
```

Included themes track the current Omarchy catalog, plus Winarchy's own
`auto-accent` dynamic theme:

| Dark themes | Light themes |
|---|---|
| `catppuccin` | `catppuccin-latte` |
| `ethereal` | `flexoki-light` |
| `everforest` | `rose-pine` |
| `gruvbox` | `lupine` |
| `hackerman` | `white` |
| `kanagawa` | |
| `last-horizon` | |
| `lumon` | |
| `matte-black` | |
| `miasma` | |
| `nord` | |
| `osaka-jade` | |
| `retro-82` | |
| `ristretto` | |
| `solitude` | |
| `tokyo-night` | |
| `vantablack` | |
| `auto-accent` | |

Light themes automatically switch Windows apps/system surfaces to light mode;
dark themes switch them back to dark mode. `auto-accent` stays dark, but follows
Windows' live accent color.

### App adapters

If a theme ships the optional drop-ins, Winarchy can also sync:

- **JetBrains IDEs**
- **VS Code / Cursor**
- **Obsidian**
- **Claude Code** — follows the theme's `mode` (dark/light); no drop-in needed

Missing apps are skipped silently. Existing rollback snapshots cover these external writes too.

## Dynamic accent mode

```powershell
winarchy theme set auto-accent
```

This mode reads **Windows' live accent color** for borders and accent surfaces at the moment you run it.

**There is no background watcher.** Nothing re-applies the theme when the Windows
accent changes on its own (auto accent color, a wallpaper rotation, a Wallpaper
Engine playlist) — every surface (starship, YASB, komorebi borders, WezTerm, …)
keeps showing the colors from the last `theme set`/`accent sync` until you run
one again.

Recommended if:

- you use automatic accent colors
- you rotate wallpapers
- you use Wallpaper Engine playlists

Manual controls (run after the accent actually changes):

```powershell
winarchy accent sync
winarchy accent status
```
