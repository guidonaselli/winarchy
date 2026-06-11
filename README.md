# Winarchy

```text
__        ___ _   _    _    ____   ____ _   ___   __
\ \      / / | \ | |  / \  |  _ \ / ___| | | \ \ / /
 \ \ /\ / /| |  \| | / _ \ | |_) | |   | |_| |\ V /
  \ V  V / | | |\  |/ ___ \|  _ <| |___|  _  | | |
   \_/\_/  |_|_| \_/_/   \_\_| \_\\____|_| |_| |_|
```

> A love letter to Linux workflows — rebuilt, carefully, for Windows 11.

Winarchy is my attempt to get as close as possible to the **Omarchy** experience on a Windows desktop.

I love Linux. I tried Omarchy, and it instantly clicked: the keyboard-driven flow, the tiling behavior, the cohesive theming, the feeling that the whole machine is working *with* you instead of against you.

But on my main desktop, I still have real reasons to stay on Windows for now: **gaming**, a few **Windows-only tools**, and some day-to-day software I am not ready to leave behind.

So instead of giving up on that workflow, I tried to recreate it here.

**Winarchy is not a clone.**
It is a pragmatic, Windows-native translation of the ideas that made Omarchy feel great.

---

## What Winarchy is

Winarchy is an opinionated integration layer for Windows 11:

- **tiling window management**
- **Waybar-style status bar**
- **SUPER-based keybindings**
- **coordinated cross-app themes**
- **single CLI for setup, updates, themes, and recovery**

All built on top of native Windows-compatible tools instead of pretending Windows is Linux.

---

## Stack mapping

| Omarchy / Linux idea | Winarchy equivalent |
|---|---|
| Hyprland | **komorebi** |
| Waybar | **YASB** |
| Walker | **Flow Launcher** |
| compositor-driven keybinds | **AutoHotkey v2** |
| `omarchy-theme-set` | **`winarchy theme set`** |
| `omarchy-update` | **`winarchy update`** |
| launcher / menu glue | **`winarchy menu`** |
| lightweight notification stack | native Windows toasts |

---

## Why this exists

Because I wanted this:

- the **speed** of a Linux keyboard workflow
- the **cohesion** of a curated desktop
- the **comfort** of familiar Omarchy-inspired patterns
- without giving up the parts of Windows I still need on my main machine

That means Winarchy optimizes for:

- **daily-driver practicality**
- **fast recovery and rollback**
- **minimal friction**
- **gaming compatibility**
- **native Windows coexistence**, not replacement for the sake of replacement

---

## Features

- **Opinionated SUPER keymap**
  - one place for global shortcuts
  - Linux-style muscle memory, adapted to Windows reality

- **Tiling that respects Windows**
  - powered by `komorebi`
  - designed to coexist with regular desktop usage

- **A themed desktop, not just themed apps**
  - Winarchy regenerates and applies coordinated theme outputs for:
    - komorebi
    - YASB
    - Windows Terminal
    - Flow Launcher
    - Neovim
    - btop
    - optional app adapters

- **Atomic theme application with rollback**
  - changes are staged
  - snapshots are created
  - rollback is built in

- **Dynamic accent mode**
  - follows Windows' live accent color
  - especially nice with wallpaper rotation / Wallpaper Engine setups

- **Game mode as a first-class concern**
  - fullscreen apps can suspend Winarchy hotkeys automatically
  - games can be kept out of tiling rules

---

## Installation

```powershell
git clone <repo> C:\winarchy
cd C:\winarchy

.\install.ps1
.\install.ps1 -Activate

winarchy doctor
```

### Notes

- `.\install.ps1` installs in **coexistence mode** by default.
- `-Activate` enables the desktop behavior (autostart, taskbar auto-hide, etc.).
- If you are migrating from Seelen UI:

```powershell
.\scripts\migrate-from-seelen.ps1
```

Rollback:

```powershell
.\scripts\rollback-to-seelen.ps1
```

---

## CLI

```text
winarchy theme set <name> | next | list
winarchy update [--core]
winarchy menu
winarchy webapp install <name> <url> | remove <name> | list
winarchy screenshot [region|window|full]
winarchy game-mode on|off|status|add <exe>|remove <exe>
winarchy accent sync [--force] | status
winarchy reload
winarchy doctor
```

---

## Themes

Themes are drop-in folders inside `themes/`.

```text
themes/my-theme/
  theme.toml
  wallpaper.png
  wallpaper-engine.txt
  jetbrains.icls
  vscode.json
```

Running:

```powershell
winarchy theme set my-theme
```

regenerates and applies the full surface atomically.

Included themes track the current Omarchy catalog, plus Winarchy's own
`auto-accent` dynamic theme:

| Dark themes | Light themes |
|---|---|
| `catppuccin` | `catppuccin-latte` |
| `ethereal` | `flexoki-light` |
| `everforest` | `rose-pine` |
| `gruvbox` | `white` |
| `hackerman` | |
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

Missing apps are skipped silently. Existing rollback snapshots cover these external writes too.

---

## Dynamic accent mode

```powershell
winarchy theme set auto-accent
```

This mode uses **Windows' live accent color** for borders and accent surfaces, then keeps following it automatically.

Recommended if:

- you use automatic accent colors
- you rotate wallpapers
- you use Wallpaper Engine playlists

Manual controls:

```powershell
winarchy accent sync
winarchy accent status
```

---

## Game mode

Winarchy treats gaming as a real requirement, not an afterthought.

- registered games are never tiled
- fullscreen game windows suspend SUPER hotkeys automatically
- manual overrides are available when needed

```powershell
winarchy game-mode on
winarchy game-mode off
winarchy game-mode add <exe>
```

---

## Keybindings

- Readable reference: [docs/keybindings.md](docs/keybindings.md)
- In-session overlay: `SUPER+K`

---

## Architecture

- the repo is the **source of truth**
- generated files come from `templates/*.tpl`
- runtime state lives in `state/`
- snapshots and rollback data live in `backups/`
- pinned core versions live in `versions.lock.toml`

This project prefers:

- explicit configuration
- narrow, reversible changes
- deterministic regeneration
- practical recovery over cleverness

---

## What Winarchy is not

Winarchy is **not**:

- a Linux compatibility layer
- a fake Hyprland port
- a complete Windows shell replacement
- a promise that Windows will behave exactly like Linux

It is a **best-effort, highly practical translation** of a workflow I genuinely enjoy.

That distinction matters.

---

## Limitations

Some things map beautifully from Omarchy to Windows. Some do not.

Accepted limitations include:

- native Windows notification behavior instead of a Linux-style notification daemon
- Windows foreground/focus rules that sometimes require small workarounds
- app-specific theming limits when upstream software does not expose enough control

Winarchy embraces those constraints instead of hiding them behind brittle hacks.

---

## Inspiration

This project exists because **Omarchy made me want this workflow everywhere**.

So if you are someone who:

- loves Linux
- appreciates curated environments
- still needs Windows on a primary desktop
- wants a cleaner, faster, more intentional setup

then Winarchy might feel familiar.

---

## Credits

- **Omarchy** for the inspiration and the workflow taste
- **komorebi**
- **YASB**
- **Flow Launcher**
- **AutoHotkey**
- and the Windows power-user ecosystem that makes projects like this possible

---

## License

[MIT](LICENSE)
