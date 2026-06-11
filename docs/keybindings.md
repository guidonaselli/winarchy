# Winarchy — Keybindings (SUPER scheme, owner: AHK v2)

`SUPER` = Windows key. Everything is suspended automatically in game-mode,
except `SUPER+1..9` (workspace switching), which stays active as an alt-tab-style
escape — otherwise Windows' native Win+N (launch taskbar app N) would fire instead.

## Apps
| Keybinding | Action |
|---|---|
| `SUPER+Return` | Windows Terminal |
| `SUPER+Space` | Flow Launcher (launcher) |
| `SUPER+Shift+Space` | winarchy menu |
| `SUPER+B` | Default browser |
| `SUPER+E` | File explorer |
| `SUPER+N` | Notification center (Windows native — not intercepted, so YASB's bell works) |
| `SUPER+M` | Music (Spotify, fallback YT Music webapp) |
| `SUPER+O` | Obsidian |
| `SUPER+K` | Keybindings overlay (auto-generated, themed, shown on the active monitor) |

## Webapps (browser `--app=` windows, tileable)
| Keybinding | Action |
|---|---|
| `SUPER+A` | ChatGPT |
| `SUPER+Shift+A` | Claude |
| `SUPER+Y` | YouTube |
| `SUPER+X` | X |
| `SUPER+C` | Google Calendar |
| `SUPER+G` | WhatsApp Web |

## Windows
| Keybinding | Action |
|---|---|
| `SUPER+W` | Close window |
| `SUPER+F` | Toggle monocle (logical fullscreen) |
| `SUPER+Shift+F` | Toggle maximize |
| `SUPER+T` | Toggle float |
| `SUPER+P` | Pause/resume tiling |
| `SUPER+R` | Force re-tile |
| `SUPER+Shift+R` | Reload the whole stack (`winarchy reload`) |

## Focus and movement
| Keybinding | Action |
|---|---|
| `SUPER+←/→/↑/↓` | Move focus directionally |
| `SUPER+Shift+←/→/↑/↓` | Move window within the layout |
| `SUPER+=` / `SUPER+-` | Horizontal resize +/- |
| `SUPER+Shift+=` / `SUPER+Shift+-` | Vertical resize +/- |

## Workspaces (komorebi, per monitor)
| Keybinding | Action |
|---|---|
| `SUPER+1..9` | Go to workspace N |
| `SUPER+Shift+1..9` | Move window to workspace N |
| `SUPER+,` / `SUPER+.` | Focus previous / next monitor |
| `SUPER+Shift+,` / `SUPER+Shift+.` | Move window to previous / next monitor |

## Screenshots (ShareX)
| Keybinding | Action |
|---|---|
| `SUPER+Shift+S` | Region capture |
| `SUPER+Shift+W` | Active window capture |
| `SUPER+Shift+P` | Full screen capture |

## Themes
| Keybinding | Action |
|---|---|
| `SUPER+Shift+T` | Next theme (`winarchy theme next`) |

## User overrides
Create `config\ahk\user.ahk` (gitignored) to add your own hotkeys without touching
the managed script. To redefine an existing binding, use the `Hotkey()` function
(duplicate `::` definitions are a load error in AHK v2). Your hotkeys show up in
the `SUPER+K` overlay under "Usuario", or under your own `; --- Section ---` headers.

## Reserved by Windows (not overridden)
- `Win+L` — lock (not interceptable; design decision).
- `Win+V` — native clipboard history.
- `Win+G` is WhatsApp outside games; in game-mode all SUPER hotkeys are suspended,
  so Game Bar gets `Win+G` back exactly when it is useful.

## Differences vs Omarchy
- `SUPER+Space` opens Flow (in Omarchy it opens Walker); Windows' native keyboard
  layout switcher on `Win+Space` is **overridden** — use the language bar if needed.
- Notifications: native Windows system (no mako equivalent).
