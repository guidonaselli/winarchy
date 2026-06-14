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
  version: "1.0"
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
- Windows Terminal `settings.json` is touched only by the surgical merge of the
  "Winarchy" scheme — never rewrite the whole file.
- Core components (komorebi, YASB) are pinned in `versions.lock.toml`; only
  `winarchy update --core` may bump them.
- Before system changes (registry, autostart): snapshot in `backups\<timestamp>\`.

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
winarchy update [--core]           # update winget+scoop; --core also komorebi/YASB
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
