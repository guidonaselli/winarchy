<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo/winarchy-mark.svg">
    <source media="(prefers-color-scheme: light)" srcset="assets/logo/winarchy-mark-ink.svg">
    <img src="assets/logo/winarchy-mark.svg" alt="Winarchy" width="220">
  </picture>
</p>

> A love letter to Linux workflows — rebuilt, carefully, for Windows 11.

Winarchy brings the [Omarchy](https://omarchy.org) way of working to a Windows desktop: keyboard-driven tiling,
one theme across every app, and a single CLI. It is for people who like that flow and still need Windows for
games and Windows-only software. It is not a clone or a Linux layer.

## What you get

- **Tiling** with [komorebi](https://github.com/LGUG2Z/komorebi): tiling, scrolling or floating per workspace,
  window placement memory and community application rules.
- **A native Rust shell**: the [Island bar](docs/island.md), a per-monitor dock with live thumbnails, a Start menu
  and an Alt+Tab switcher that reaches every workspace.
- **SUPER keybindings**, owned by AutoHotkey v2: [reference](docs/keybindings.md), or `SUPER+K` in session.
- **[Themes](docs/themes.md)** applied atomically, with rollback, to the shell, komorebi, WezTerm, Windows Terminal,
  Flow Launcher, Neovim, btop and optional editors; `auto-accent` follows the Windows accent.
- **A themed terminal**: WezTerm plus starship, fzf, zoxide, eza and bat on PowerShell 7.
- **Game mode**: games are never tiled and hotkeys step aside in fullscreen.
- **Files**, a native file manager in Rust (build it from `files/` until a release ships it), and an optional
  voice assistant.
- **One update channel** and a `doctor` that says what is wrong and how to fix it.

## Two ways to run it

| | Coexistence (default) | Session shell (opt-in) |
|---|---|---|
| Explorer | stays | never starts |
| Desktop, Start, tray, notifications | Windows, plus the Winarchy dock and bar | Winarchy |
| Store apps | work as always | need the experimental UWP host, turned on with the session shell |
| How to switch | `.\install.ps1 -Activate` | `winarchy shell activate --yes`, then sign out |
| How to go back | `.\uninstall.ps1` | `winarchy shell deactivate` |

The session shell is newer and has gaps: read [what it covers and what it does not](docs/session-shell.md) before
activating it, and do not activate it on a machine you cannot recover.

## Installation

From a normal (non-admin) PowerShell window on Windows 11:

```powershell
irm https://raw.githubusercontent.com/guidonaselli/winarchy/release/boot.ps1 | iex
```

It installs Git and PowerShell 7 with winget if they are missing, clones Winarchy into
`C:\winarchy` and runs `install.ps1` in coexistence mode. Running it again updates the
checkout. Set `$env:WINARCHY_DIR` for another folder, or `$env:WINARCHY_ACTIVATE = '1'` to
activate right away.

Manual install (needs Git and PowerShell 7; run it from `pwsh`):

```powershell
git clone --branch release https://github.com/guidonaselli/winarchy.git C:\winarchy
cd C:\winarchy

.\install.ps1
.\install.ps1 -Activate

winarchy doctor
```

When something misbehaves, `winarchy logs` prints the last lines of every log of the stack
(CLI and AutoHotkey share `state\winarchy.log`: changes, warnings, errors, wallpaper, launcher placement).

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

### Uninstall

From `pwsh`, in the Winarchy folder:

```powershell
.\uninstall.ps1 -DryRun   # lists what it would do, changes nothing
.\uninstall.ps1
```

It stops the stack and removes the autostart, the taskbar and Windows tweaks, the environment
variables and PATH entry, the pwsh profile hook, the Start menu commands, the AI agent skill
and the winget pins. Run it from an admin PowerShell to also remove the Defender exclusions.
`-RemovePackages` also uninstalls komorebi and YASB; the other apps stay. Your backups stay in
`backups\`; delete the folder when you no longer need them.

## CLI

```text
winarchy theme set <name> | next | list
winarchy background list | gallery | set <file> | next
winarchy agent list | set <id> | launch
winarchy bar position [top|bottom]
winarchy bar dock <on|float|hide|auto|off>
winarchy bar cluster <primary|off|N>   # dock que lleva reloj, notificaciones y Quick Settings
winarchy bar use <native|yasb>
winarchy files [folder]                 # Rust file manager
winarchy assistant on|off|status       # voice assistant
winarchy shell status | activate --yes | deactivate   # Winarchy as the Windows shell (opt-in)
winarchy taskbar hide|autohide|show
winarchy heal-monitors
winarchy update [--core | --self]
winarchy menu
winarchy webapp install <name> <url> | remove <name> | list
winarchy screenshot [region|window|full|last|scrolling|record|record-gif|stop|ocr|qr|color|ruler|pin]
winarchy game-mode on|off|status|add <exe>|remove <exe>
winarchy layout save | apply | order | list | forget <exe>
winarchy mode [list | save | default [tiling|scrolling|floating]]
winarchy accent sync [--force] | status
winarchy uwphost [status|on|off|retry]  # Store apps inside the session shell
winarchy reload
winarchy doctor
winarchy logs [n]
```

Details: [updates, terminal, game mode, workspace modes, window placement and application rules](docs/guide.md).

## Performance

The native shell is a 1.5 MB Rust binary. In a live session (30 s sample, dock and cluster on) it used
**53 MB** of working set and **0.38 %** of one core, while `explorer.exe` on the same machine used
294 MB and 2.58 %. Hot paths take nanoseconds (`hit_test` 16 ns, dock layout 88 ns). Method, commands
and caveats are in [docs/BENCHMARKS.md](docs/BENCHMARKS.md).

Without Explorer the stack holds about 160 MB against about 360 MB with it. In a clean Windows Sandbox,
installing Winarchy in coexistence mode added 5 processes and about 63 MB of resident Winarchy processes
(`winarchy-shell` 14 MB, `komorebi` 33 MB, `AutoHotkey64` 16 MB), with idle CPU of the whole machine going from
0.01 % to 0.12 %. Those Sandbox figures come from a single run.

## Stack

| Omarchy idea | Winarchy |
|---|---|
| Hyprland | komorebi |
| Waybar | Winarchy shell (Island + dock), YASB optional |
| Walker | Flow Launcher |
| compositor keybindings | AutoHotkey v2 |
| `omarchy-theme-set` / `omarchy-update` | `winarchy theme set` / `winarchy update` |

The repo is the source of truth: generated files come from `templates/*.tpl`, your overrides are untracked files
that updates never touch, runtime state lives in `state/`, snapshots in `backups/` and pinned versions in
`versions.lock.toml`. Changes are meant to be narrow and reversible.

## Limitations

- Windows foreground and focus rules sometimes need small workarounds.
- Apps that expose little theming stay only partly themed.
- Notifications are Windows toasts; without Explorer there is no persistent notification list.

## Credits

- **Omarchy** for the inspiration and the workflow taste
- **komorebi**
- **YASB**
- **Flow Launcher**
- **AutoHotkey**
- and the Windows power-user ecosystem that makes projects like this possible

## License

[MIT](LICENSE)

The Winarchy name and logo are not covered by the MIT license. Forks are welcome; please
give them a different name and logo.
