# Winarchy — Keybindings (SUPER scheme, owner: AHK v2)

`SUPER` = Windows key. Everything is suspended automatically in game-mode,
except `SUPER+1..9` (workspace switching), which stays active as an alt-tab-style
escape — otherwise Windows' native Win+N (launch taskbar app N) would fire instead.

## Apps
| Keybinding | Action |
|---|---|
| `SUPER+Return` | Windows Terminal |
| `SUPER+Space` | Flow Launcher — applications and Winarchy's own commands in one search |
| `SUPER+B` | Default browser |
| `SUPER+E` | File explorer |
| `SUPER+N` | Notification center (Windows native — not intercepted, so YASB's bell works) |
| `SUPER+M` | Music (Spotify, fallback YT Music webapp) |
| `SUPER+O` | Obsidian |
| `SUPER+K` | Keybindings overlay (auto-generated, themed, shown on the active monitor) |

## Menu (popup, Omarchy-style)
| Keybinding | Action |
|---|---|
| `SUPER+Alt+Space` | Winarchy menu (Apps/Themes/Reload/Game mode/Doctor/Update/System/Quit) — same content as right-clicking the tray icon, or the rightmost button on the YASB bar |
| `SUPER+Esc` | System / power submenu directly (Screensaver/Lock/Sleep/Hibernate/Sign out/Restart/Shut down) |

The menu's **Apps** entry opens Flow Launcher pre-scoped to the `Program` plugin
(action keyword `app`, added additively by `Set-WinarchyFlowAppsKeyword` — it
does not replace the default `*`, so plain `SUPER+Space` keeps searching
everything). This mirrors Omarchy's Walker-based Apps list without curating a
manual app list: Flow already indexes installed programs.

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

## Stacks (komorebi draws them as tabs in the stackbar)

| Key | Action |
|---|---|
| `SUPER+Alt+←/→/↑/↓` | Stack the focused window onto its neighbour |
| `SUPER+Alt+u` | Unstack the focused window |
| `SUPER+Alt+,` / `SUPER+Alt+.` | Previous / next tab in the stack |

## Workspaces (komorebi, per monitor)
| Keybinding | Action |
|---|---|
| `SUPER+1..9` | Go to workspace N |
| `SUPER+Shift+1..9` | Move window to workspace N |
| `SUPER+,` / `SUPER+.` | Focus previous / next monitor |
| `SUPER+Shift+,` / `SUPER+Shift+.` | Move window to previous / next monitor |

## Screenshots and screen recording (ShareX)
| Keybinding | Action |
|---|---|
| `SUPER+Shift+S` | Region capture |
| `SUPER+Shift+W` | Active window capture |
| `SUPER+Shift+P` | Full screen capture |
| `SUPER+Shift+V` | Screen recording |
| `SUPER+Shift+G` | GIF recording |

## Themes
| Keybinding | Action |
|---|---|
| `SUPER+Shift+T` | Next theme (`winarchy theme next`) |
| `SUPER+Ctrl+T` | Theme gallery (`winarchy theme gallery`) |
| `SUPER+Ctrl+Space` | Next background of the active theme (`winarchy background next`) |
| `SUPER+Ctrl+Shift+A` | Launch the preferred coding agent (`winarchy agent launch`) |
| `SUPER+Shift+Return` | Promote the focused window to the largest tile |
| `SUPER+Ctrl+1`–`9` | Send the window to workspace N without following it |

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
- `SUPER+Space` opens Flow unscoped (in Omarchy it opens Walker, apps-only); the
  apps-only equivalent lives in the menu's **Apps** entry instead, to avoid
  changing the existing `SUPER+Space` search behavior. Windows' native keyboard
  layout switcher on `Win+Space` is **overridden** — use the language bar if needed.
- The main menu and system/power submenu are native AHK popup menus (Windows
  context-menu styling), not a custom-themed translucent box like Omarchy's.
- Notifications: native Windows system (no mako equivalent).
