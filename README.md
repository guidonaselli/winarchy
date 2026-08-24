<p align="center">
  <img src="assets/logo/winarchy-logo.svg" alt="Winarchy" width="220">
</p>

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
winarchy background list | set <file> | next
winarchy agent list | set <id> | launch
winarchy update [--core | --self]
winarchy menu
winarchy webapp install <name> <url> | remove <name> | list
winarchy screenshot [region|window|full]
winarchy game-mode on|off|status|add <exe>|remove <exe>
winarchy layout save | apply | order | list | forget <exe>
winarchy accent sync [--force] | status
winarchy reload
winarchy doctor
```

### Updates — three axes

Winarchy is the single update channel; third-party auto-updaters (YASB, Flow) are off.

| Command | Updates | How |
| --- | --- | --- |
| `winarchy update` | the packages Winarchy declares in `versions.lock.toml`, plus scoop | reports the outcome per package; warns if a core or Winarchy update is available |
| `winarchy update --core` | pinned core: komorebi / YASB / Flow Launcher / AutoHotkey | unpins, upgrades, repins, rewrites `versions.lock.toml` |
| `winarchy update --self` | Winarchy itself (CLI, templates, configs, AHK, themes) | `git pull --ff-only origin release` + idempotent migration (`install.ps1`, mode preserved) |

`winarchy update` deliberately touches only what Winarchy declares, not every package on
the machine: the stack owns its own dependencies, not your unrelated apps. Core components
are pinned because the stack depends on their concrete behaviour, so a major release never
lands without someone reading the changelog first.

The version check is best-effort (GitHub Releases API), cached 6 h, and never blocks the
flow. `winarchy doctor` shows installed vs latest release. Self-update only runs on a git
checkout and aborts if the working tree has uncommitted changes to tracked files (your
customization lives in untracked overrides, so it is never touched). Roll back with
`git checkout <previous-tag>` + `install.ps1`.

---

## Themes

Themes are drop-in folders inside `themes/`.

```text
themes/my-theme/
  theme.toml
  preview.png
  backgrounds/
  wallpaper-engine.txt
  jetbrains.icls
  vscode.json
```

Everything in `backgrounds/` is a wallpaper the theme offers. Applying the theme sets the
one you picked last time for it (the first one otherwise); `winarchy background next` and
`SUPER + CTRL + SPACE` cycle through the rest. A theme without the folder leaves the
wallpaper alone, and `wallpaper-engine.txt` wins over it when Wallpaper Engine is installed.

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

## Terminal profile

Winarchy ships an omarchy-style shell experience on PowerShell 7 + Windows Terminal:

- **starship** prompt, themed by the theme engine (`config/pwsh/starship.toml` is regenerated on every `winarchy theme set`; the prompt recolors live, no restart needed)
- **PSFzf + fzf** — fuzzy history (`Ctrl+R`) and file picker (`Ctrl+T`)
- **zoxide** — smart `cd` (`z proj`, `zi`)
- **eza / bat** — modern `ls`/`ll`/`la`/`lt` and `cat`
- **PSReadLine** — fish-style history predictions in list view
- **fastfetch** — omarchy-style splash on every new interactive terminal (`config/fastfetch/config.jsonc`)

`install.ps1` installs the tools (winget + PSGallery) and adds a single marked block to your pwsh `$PROFILE` that dot-sources the managed profile (`config/pwsh/profile.ps1`). Every integration is guarded: if a binary is missing, that piece is silently skipped — the profile never breaks your shell.

Your own customization goes in `config/pwsh/user.ps1` (gitignored), loaded last so it can override anything. To uninstall the hook: `Remove-WinarchyShellProfile` (a snapshot of your `$PROFILE` is taken before any change).

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

## Window placement memory

komorebi decides where a new window lands based on what is focused at that moment, so the
same app ends up somewhere different depending on the order you opened things. Winarchy
lets you capture the arrangement instead of declaring it:

```powershell
winarchy layout save      # remember where every app currently lives (monitor, workspace, tile)
winarchy layout list      # review what was captured
winarchy layout forget <exe>
```

Preferences land in `config/windows.toml` — your machine, not the repo, so no application
is ever hardcoded into Winarchy. `winarchy layout apply` bakes them into the generated
`komorebi.json`, so they survive reloads and cold starts. Runtime rules
(`komorebic workspace-rule`) are deliberately not used: a config reload wipes those at an
indeterminate point afterwards.

Monitors are stored by their `device_id`, not by index: the index depends on the order
Windows enumerates displays and changes when you reconnect or switch ports. If a saved
monitor is not connected, that entry is skipped with a warning rather than applied to the
wrong screen — and `winarchy doctor` reports it.

By default a preference only applies the **first time** a window appears, so the app is
born where you want it and you can still move it freely afterwards. Set `pin = true` on an
entry to have komorebi keep returning it there.

### Tile position (slots)

The workspace is only half the answer: within it, komorebi orders windows by insertion, so
whichever app opens first takes the main tile. `winarchy layout save` also records the tile
position of each window as a `slot` (0 = the first tile), and a background reconciler keeps
it that way:

```powershell
winarchy layout order     # put every window back in its saved slot, right now
```

The reconciler reacts to komorebi's own events, and what it does depends on who moved the
window:

| What happened | What it does |
| --- | --- |
| A window appears in the workspace | Puts the windows back in their saved slots |
| **You** move a window | **Learns it** — the new position becomes the saved slot |

So a workspace with Discord pinned to `slot = 0` keeps Discord on the left whatever you open
next to it, and the day you decide Discord belongs on the right, you just move it and that
sticks. Nothing to re-run.

Only the slot is learned: the monitor, the workspace and `pin` stay as they were, and an app
without a saved preference never gets one just for being moved.

To opt out, delete the `slot` line from that entry in `config/windows.toml` — an entry with
no slot is never reordered. The reconciler runs as its own hidden process, started at logon
alongside the rest of the stack; `winarchy doctor` reports it when there are slots to keep,
and its log lives in `%LOCALAPPDATA%\winarchy\window-slots.log`.

**What this does not cover:** geometry. Slots fix the *order* of the tiles, not their size —
how wide each tile is stays a komorebi layout decision.

---

## Application rules

Some apps do not behave under a tiling window manager: they minimise to the tray and come
back wrong, spawn ghost child windows, announce their window too late to be tiled, or use
`WS_EX_LAYERED` and break borders. komorebi has a knob for each case, and the community
maintains those answers for hundreds of apps in
[komorebi-application-specific-configuration](https://github.com/LGUG2Z/komorebi-application-specific-configuration).

Winarchy vendors that file (`vendor/asc/applications.json`, pinned in `versions.lock.toml`)
and **compiles it into the generated `komorebi.json`** instead of pointing komorebi at it
with `app_specific_configuration_path`. That way Winarchy owns the precedence, the result is
a single inspectable file, and no third-party path stays live at runtime.

Four layers, lowest priority first:

| # | layer | who edits it |
|---|---|---|
| 1 | `vendor/asc/applications.json` | the community (vendored, pinned) |
| 2 | `templates/komorebi.json.tpl` | Winarchy |
| 3 | `games.toml` | Winarchy |
| 4 | `config/komorebi/rules.toml` | **you** — gitignored, never overwritten |

**Placement** (`ignore` / `manage` / `floating`) is exclusive: the highest layer that has an
opinion about a window wins, and its decision *replaces* the lower ones. That is what makes
your `[[ignore]]` actually beat a community `manage` — komorebi's lists are additive, so
adding the ignore is not enough, the manage has to be removed.

**Attributes** (`tray_and_multi_window`, `object_name_change`, `slow_application`, `layered`,
`transparency_ignore`) describe behaviour rather than placement, so they are unioned across
every layer and never override each other.

Copy `config/komorebi/rules.toml.example` to `config/komorebi/rules.toml` to add your own:

```toml
[[ignore]]
exe = "GalaxyClient.exe"      # beats the community `manage` for this window

[[disable]]
asc = "GOG Galaxy"            # drop that community entry entirely, by app name
```

`rules.toml` is gitignored, so neither `git pull` nor `winarchy update` can touch it — it is
a separate layer, not the same file.

```powershell
winarchy rules explain zen.exe   # which rules match, and which layer won
winarchy rules collisions        # community rules a higher layer overrode
winarchy rules sync              # diff the upstream file (--apply to re-vendor it)
```

`winarchy rules sync` shows the per-app diff before changing anything and flags which of
those apps you have disabled locally. `winarchy doctor` reports the pinned revision and the
current collisions.

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
