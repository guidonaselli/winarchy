---
name: winarchy
description: >
  Required for end-user customization of a Winarchy system — the Omarchy-style
  integration layer for Windows 11 (komorebi tiling + YASB bar + AutoHotkey v2
  hotkeys + Flow Launcher + theme engine + `winarchy` CLI). Use for themes,
  hotkeys, the bar, webapps, game mode, screenshots, autostart and the
  user-facing `winarchy` command. Do not use for Winarchy source development.
license: Apache-2.0
metadata:
  author: official
  version: "1.1"
---

# Winarchy Skill

## When to Use
- running or discovering `winarchy` commands
- switching/applying themes, or adding a theme
- editing hotkeys in `config\ahk\winarchy.ahk`
- editing the bar (`config\yasb\`) or the tiling WM (`config\komorebi\`)
- managing webapps, game mode, screenshots, the Windows accent sync
- diagnosing the stack (`winarchy doctor`)

## Critical invariants (do NOT break)
- **AutoHotkey v2 is the ONLY owner of global hotkeys.** Never enable whkd or
  Seelen/PowerToys hotkeys. New hotkeys go in `config\ahk\winarchy.ahk`.
- **komorebi is never run elevated.**
- **Generated configs carry a "managed by winarchy" header and are never edited
  by hand.** They are regenerated from `templates\*.tpl` via `winarchy theme set`.
  User customization goes in override files, not in generated ones.
- **WezTerm is the default terminal** (`SUPER+Return`, coding agents, `winarchy menu`).
  Its whole config is generated at `config\wezterm\wezterm.lua`; user customization
  goes in `config\wezterm\user.lua` (gitignored, never touched by an update).
- Windows Terminal stays installed and themed, but Winarchy never launches it. Its
  `settings.json` is touched only by the surgical merge of the "Winarchy" scheme —
  never rewrite the whole file.
- Core components (komorebi, YASB, WezTerm) are pinned in `versions.lock.toml`; only
  `winarchy update --core` may bump them.
- Before system changes (registry, autostart): snapshot in `backups\<timestamp>\`.
- **Winarchy is the single update channel.** Third-party auto-updaters are off
  (YASB `update_check`, Flow `AutoUpdates`); do not re-enable them. Likewise the
  stack shows **one tray icon** (hosted by `winarchy.ahk`) — YASB's own tray and
  Flow's notify icon are hidden on purpose. Do not turn them back on.

## Discovery-first workflow
Prefer discovering the current command surface dynamically:

```powershell
winarchy help            # full subcommand list
winarchy doctor          # green/red diagnostics + suggested fix
winarchy theme list      # installed themes (* = active)
```

The repo is the source of truth: `KOMOREBI_CONFIG_HOME` and `YASB_CONFIG_HOME`
point at `config\`. Adding a theme = create `themes\<name>\theme.toml` (drop-in,
zero code).

## Commands
```powershell
winarchy theme set <name>          # apply a theme to every surface
winarchy theme next                # cycle to the next theme
winarchy theme list                # list themes (* = active)
winarchy theme gallery             # visual WPF theme picker
winarchy update                    # update third-party apps (winget+scoop); warns if a new Winarchy release exists
winarchy update --core             # also bump pinned komorebi/YASB and rewrite versions.lock.toml
winarchy update --self             # update Winarchy itself (git pull release + idempotent migration)
winarchy menu                      # navigable menu
winarchy webapp install <n> <url>  # create a webapp as a dedicated window
winarchy webapp remove <n>         # remove a webapp
winarchy webapp list               # list webapps
winarchy screenshot [region|window|full]   # capture via ShareX (default: region)
winarchy game-mode on|off|status   # manual game mode
winarchy game-mode add <exe>       # register a game (no tiling/hotkeys; excluded from Quick Accent)
winarchy game-mode remove <exe>    # unregister a game
winarchy accent sync [--force]     # re-sync the dynamic theme with the Windows accent
winarchy accent status             # system accent vs applied accent
winarchy reload                    # reload komorebi, YASB, AHK and Flow
winarchy doctor                    # stack diagnostics
```

## Updates — three axes
Keep the axes separate; never auto-apply a self-update.

| Command | Updates | Mechanism |
| --- | --- | --- |
| `winarchy update` | third-party apps (winget + scoop) | core pinned out; also a best-effort warning if a newer Winarchy release exists |
| `winarchy update --core` | komorebi / YASB | unpin → upgrade → repin → rewrite `versions.lock.toml` |
| `winarchy update --self` | Winarchy itself | `git pull --ff-only origin release` → `install.ps1` migration (mode preserved) |

- The Winarchy version is `ModuleVersion` in `Winarchy.psd1`, kept in sync with the
  git tag and GitHub Release. See `docs\RELEASING.md` for cutting a release.
- The version check hits the GitHub Releases API, is **best-effort** (degrades
  silently with no network) and **cached 6h** in `state\last-release-check`.
- `winarchy update --self` only runs on a git checkout and **aborts if the working
  tree has uncommitted changes to tracked files** — user customization must live in
  untracked overrides so it is never touched. Rollback: `git checkout <previous-tag>`
  + `install.ps1`.

## Unified tray
The whole stack presents **one** Winarchy tray icon, hosted by `winarchy.ahk`
(icon = `assets\logo\winarchy.ico`). Its menu dispatches to the CLI (themes,
reload, game mode, doctor, update, quit). When adding stack-level UX, extend this
tray menu rather than introducing a second tray icon.

## Safe edit targets
- Hotkeys: `config\ahk\winarchy.ahk` (then `winarchy reload`).
- Themes: `themes\<name>\theme.toml`.
- Templates (regenerate generated configs): `templates\*.tpl`, then `winarchy theme set <active>`.
- Never hand-edit files under `config\` that carry the "managed by winarchy" header.

## Validation
- After hotkey changes: `winarchy reload`.
- After theme/template changes: `winarchy theme set <name>` regenerates configs.
- Always confirm with `winarchy doctor` (exit code reflects health).
- Validate PowerShell with
  `[System.Management.Automation.Language.Parser]::ParseFile` before committing.

## Notes for this machine
- Shell is PowerShell; run bash-only snippets explicitly via `bash -lc` if needed.
- PowerToys is optional: Winarchy only merges game `.exe`s into Quick Accent's
  `excluded_apps` when present — it does not otherwise manage PowerToys.
