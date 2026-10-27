# Winarchy — Keybindings (SUPER scheme, owner: AHK v2)

`SUPER` = Windows key. Everything is suspended automatically in game-mode,
except `SUPER+1..0` (workspace switching), which stays active as an alt-tab-style
escape — otherwise Windows' native Win+N (launch taskbar app N) would fire instead.

## Apps
| Keybinding | Action |
|---|---|
| `SUPER+Return` | WezTerm (the default terminal) |
| `SUPER+Space` | Flow Launcher — applications and Winarchy's own commands in one search |
| `SUPER+S` | File search — Flow scoped to Everything (`f ` prefix), replaces Windows Search |
| `SUPER+B` | Default browser |
| `SUPER+E` | File explorer |
| `SUPER+N` | Notification center (Windows native — not intercepted, so YASB's bell works) |
| `SUPER+M` | Music (Spotify, fallback YT Music webapp) |
| `SUPER+O` | Obsidian |
| `SUPER+K` | Keybindings list: searchable, auto-generated from winarchy.ahk, themed, on the active monitor |
| `SUPER+I` | Native shell only (`winarchy bar use native`): expands the island with CPU and RAM for a few seconds |
| `Right Ctrl+Space` (hold) | Push-to-talk to the voice assistant: speak while held, release to send (only with `winarchy assistant on`) |

## Menu (popup, Omarchy-style)
| Keybinding | Action |
|---|---|
| `SUPER+Alt+Space` | Winarchy menu (Apps/Themes/Reload/Game mode/Doctor/Update/System/Quit) — same content as right-clicking the tray icon, or the rightmost button on the YASB bar |
| `SUPER+Esc` | System / power submenu directly (Screensaver/Lock/Sleep/Hibernate/Sign out/Restart/Shut down) |

The menu's **Apps** entry opens Flow Launcher pre-scoped to the `Program` plugin
(action keyword `app`), and `SUPER+S` pre-scoped to the `Explorer` plugin's
file search on the Everything engine (action keyword `f`, files only). `Set-WinarchyFlowKeywords` adds both
additively — the default `*` stays, so plain `SUPER+Space` keeps searching
everything. This mirrors Omarchy's Walker-based Apps list without curating a
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
| `SUPER+LButton` (drag) | Move the window under the cursor; drop it on another tile to swap |
| `SUPER+RButton` (drag) | Resize: grab near an edge or corner and drag (the centre grows right and down) |

## Stacks (komorebi draws them as tabs in the stackbar)

| Key | Action |
|---|---|
| `SUPER+Alt+←/→/↑/↓` | Stack the focused window onto its neighbour |
| `SUPER+Alt+u` | Unstack the focused window |
| `SUPER+Ctrl+S` / `SUPER+Ctrl+U` | Stack every window in the workspace / unstack the whole container |
| `SUPER+Alt+,` / `SUPER+Alt+.` | Previous / next tab in the stack |

## Workspaces (komorebi, per monitor)
| Keybinding | Action |
|---|---|
| `SUPER+1..0` | Go to workspace N (1–10) of the monitor under the cursor |
| `SUPER+Shift+1..0` | Move window to workspace N |
| `SUPER+Tab` / `SUPER+Shift+Tab` | Next / previous occupied workspace (empty ones are skipped, wraps around) |
| `SUPER+Ctrl+Tab` | Back to the workspace you came from |
| `SUPER+WheelUp` / `SUPER+WheelDown` | Scroll to the next / previous occupied workspace (stops at the ends) |
| `SUPER+,` / `SUPER+.` | Focus previous / next monitor |
| `SUPER+Shift+,` / `SUPER+Shift+.` | Move window to previous / next monitor |
| `SUPER+Ctrl+,` / `SUPER+Ctrl+.` | Send window to previous / next monitor without following it |
| `SUPER+Alt+Shift+,` / `SUPER+Alt+Shift+.` | Move the whole workspace to the previous / next monitor |

## Screenshots and screen recording (ShareX)
| Keybinding | Action |
|---|---|
| `SUPER+Shift+S` | Region capture |
| `SUPER+Shift+W` | Active window capture |
| `SUPER+Shift+P` | Full screen capture |
| `SUPER+Shift+V` | Screen recording |
| `SUPER+Shift+G` | GIF recording |
| `SUPER+Ctrl+V` | Stop the running recording |
| `SUPER+Ctrl+Q` | Decode a QR code from a screen region |
| `SUPER+Ctrl+O` | Read text from a screen region (OCR) |

The bar's tools drawer (the chevron left of the clock) has the same capture
actions as buttons, plus a colour picker.

## System panels
The bar widgets open their own richer popups on click (per-app audio mixer, device
pickers). These go to the native Windows panels, and mirror Quattro's `SUPER+CTRL`
control panels — `W` is `N` here because `SUPER+Ctrl+W` is stay awake.

| Keybinding | Action |
|---|---|
| `SUPER+Ctrl+A` | Sound |
| `SUPER+Ctrl+B` | Bluetooth |
| `SUPER+Ctrl+N` | Network |
| `SUPER+Ctrl+D` | Display |
| `SUPER+Ctrl+P` | Power and sleep |

## Themes
| Keybinding | Action |
|---|---|
| `SUPER+Shift+T` | Next theme (`winarchy theme next`) |
| `SUPER+Ctrl+T` | Theme gallery (`winarchy theme gallery`) |
| `SUPER+Ctrl+Space` | Next background of the active theme (`winarchy background next`) |
| `SUPER+Ctrl+Shift+A` | Launch the preferred coding agent (`winarchy agent launch`) |
| `SUPER+Shift+Return` | Promote the focused window to the largest tile |
| `SUPER+Ctrl+Return` | Focus the window at the top of the tree |
| `SUPER+Shift+H` / `SUPER+Shift+J` | Mirror the layout left/right or up/down |
| `SUPER+Shift+Z` | Stop tiling this workspace |
| `SUPER+Shift+L` | Cycle to the next layout (BSP, columns, rows, grid, scrolling, …) |
| `SUPER+Alt+Space` → Tiling → Layout | Pick any layout (BSP, grid, columns, rows, the stacks, scrolling) |
| `SUPER+Alt+Home` | Remember the current tile sizes |
| `SUPER+Home` | Restore the remembered tile sizes |
| `SUPER+Shift+D` | Dim unfocused windows (toggle transparency) |
| `SUPER+Ctrl+L` | Lock the container so new windows do not displace it |
| `SUPER+Ctrl+M` | Minimize the focused window |
| `SUPER+Ctrl+F` | Switch between the tiling and floating layers |
| `SUPER+Ctrl+W` | Keep the machine awake (toggle) |
| `SUPER+Ctrl+1`–`0` | Send the window to workspace N without following it |

## Terminal (WezTerm, internal — not global hotkeys)
WezTerm registers **no** global hotkeys: AHK stays the sole owner of those. Everything
below only works inside a WezTerm window and lives behind the leader key `CTRL+Space`
(press the leader, release it, then the key), the same map as Omarchy's tmux. `CTRL+Space`
twice sends a literal `CTRL+Space` to the shell. `ALT+1`–`9` goes straight to a tab.

| Keybinding | Action |
|---|---|
| `LEADER+V` | Split the pane left/right |
| `LEADER+H` | Split the pane top/bottom |
| `LEADER+←/↓/↑/→` | Move focus between panes |
| `LEADER+Z` | Zoom the focused pane (toggle) |
| `LEADER+X` | Close the focused pane |
| `LEADER+C` | New tab |
| `LEADER+K` | Close the current tab |
| `LEADER+R` | Rename the current tab |
| `LEADER+N` / `LEADER+P` | Next / previous tab |
| `ALT+1`–`9` | Go to tab N |
| `LEADER+[` | Copy mode (keyboard selection) |
| `LEADER+Shift+C` | New workspace (a tmux session) |
| `LEADER+Shift+R` | Rename the workspace |
| `LEADER+Shift+N` / `LEADER+Shift+P` | Next / previous workspace |
| `LEADER+S` | Pick a workspace |
| `CTRL+Space` `CTRL+Space` | Send a literal `CTRL+Space` |

Selecting with the mouse already copies (WezTerm default), and so do WezTerm's own
`CTRL+SHIFT+C` / `CTRL+SHIFT+V`; scrollback search is `CTRL+SHIFT+F` and quick select `CTRL+SHIFT+Space` (WezTerm defaults). New tabs open PowerShell 7; right-clicking the new-tab
button lists the other shells (Windows PowerShell, Command Prompt).

Personalize it in `config\wezterm\user.lua` (gitignored, never touched by
`winarchy theme set` or `winarchy update`); `config\wezterm\user.lua.example` is the
starting point. The generated `config\wezterm\wezterm.lua` is managed by Winarchy and
gets overwritten on every theme change.

## User overrides
Create `config\ahk\user.ahk` (gitignored) to add your own hotkeys without touching
the managed script. To redefine an existing binding, use the `Hotkey()` function
(duplicate `::` definitions are a load error in AHK v2). Your hotkeys show up in
the `SUPER+K` overlay under "User", or under your own `; --- Section ---` headers.

## Reserved by Windows (not overridden)
- `Win+L` — lock (not interceptable; design decision).
- `Win+V` — native clipboard history.
- `Win+G` is WhatsApp outside games; in game-mode all SUPER hotkeys are suspended,
  so Game Bar gets `Win+G` back exactly when it is useful.

## Elevated windows (known limit)
AHK runs un-elevated, and Windows (UIPI) does not let it see keys while an elevated
window has focus: Task Manager, regedit, an admin terminal, and any app that runs
"as administrator" (Revo Uninstaller, installers). There `Win+Shift+S` falls through to
the native Windows snipping tool and the other SUPER hotkeys do nothing. They work again
as soon as a non-elevated window is focused. AHK is not elevated on purpose: everything
it launches would inherit admin rights.

## Differences vs Omarchy
- `SUPER+Space` opens Flow unscoped (in Omarchy it opens Walker, apps-only); the
  apps-only equivalent lives in the menu's **Apps** entry instead, to avoid
  changing the existing `SUPER+Space` search behavior. Windows' native keyboard
  layout switcher on `Win+Space` is **overridden** — use the language bar if needed.
- The main menu and system/power submenu are native AHK popup menus (Windows
  context-menu styling), not a custom-themed translucent box like Omarchy's.
- Notifications: native Windows system (no mako equivalent).
