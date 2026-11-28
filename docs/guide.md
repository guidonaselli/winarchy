# User guide

## Terminal profile

**WezTerm is the default terminal** — `SUPER+Return`, the coding-agent launcher and
`winarchy menu` all open it. Its whole config is generated at
`config\wezterm\wezterm.lua` from the active theme (colors, JetBrainsMono NF with
ligatures, padding, opacity, 10k scrollback, tabs and panes behind the `CTRL+Space`
leader — see [keybindings.md](keybindings.md)). Your own settings go in `config\wezterm\user.lua`
(gitignored, never touched by `theme set` or `update`; copy `user.lua.example` to start).
Windows Terminal stays installed and keeps getting the "Winarchy" color scheme, but
Winarchy no longer launches it.

Winarchy ships an omarchy-style shell experience on PowerShell 7 + WezTerm:

- **starship** prompt, themed by the theme engine (`config/pwsh/starship.toml` is regenerated on every `winarchy theme set`; the prompt recolors live, no restart needed)
- **PSFzf + fzf** — fuzzy history (`Ctrl+R`) and file picker (`Ctrl+T`)
- **zoxide** — smart `cd` (`z proj`, `zi`)
- **eza / bat** — modern `ls`/`ll`/`la`/`lt` and `cat`
- **PSReadLine** — fish-style history predictions in list view

`install.ps1` installs the tools (winget + PSGallery) and adds a single marked block to your pwsh `$PROFILE` that dot-sources the managed profile (`config/pwsh/profile.ps1`). Every integration is guarded: if a binary is missing, that piece is silently skipped — the profile never breaks your shell.

Your own customization goes in `config/pwsh/user.ps1` (gitignored), loaded last so it can override anything. To uninstall the hook: `Remove-WinarchyShellProfile` (a snapshot of your `$PROFILE` is taken before any change).

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

`game-mode add` writes to `games.user.toml` (yours, gitignored), so `winarchy update --self`
never collides with it. The shipped catalog stays in `games.toml`. Names with spaces work
quoted or not: `winarchy game-mode add The iTero Coach.exe`.

## Workspace modes

Every workspace is in one of three modes, like Omarchy 4.5: **tiling** (BSP), **scrolling**
(a strip of columns) or **floating** (windows stay where you put them, as in plain Windows).
`SUPER+Shift+L` cycles the focused workspace forward and `SUPER+Alt+Shift+L` back; the
`SUPER+Alt+Space` layout menu picks one directly. Each workspace keeps its own mode.

The mode is saved to `config/windows.toml` (machine data, gitignored) a moment after you
change it, and `winarchy reload` or a komorebi restart restores it. `winarchy mode` lists the
live modes and `winarchy mode default floating` sets the mode of workspaces with no saved one
(applied by the next `winarchy layout apply`). The default stays `tiling`.

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
| 3 | `games.toml` + `games.user.toml` | Winarchy / **you** (`game-mode add`, gitignored) |
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

## Updates — three axes

Winarchy is the single update channel; third-party auto-updaters (YASB, Flow) are off.

| Command | Updates | How |
| --- | --- | --- |
| `winarchy update` | the packages Winarchy declares in `versions.lock.toml`, plus scoop | reports the outcome per package; warns if a core or Winarchy update is available |
| `winarchy update --core` | pinned core: komorebi / YASB / Flow Launcher / AutoHotkey | unpins, upgrades, repins, rewrites `versions.lock.toml` |
| `winarchy update --self` | Winarchy itself (CLI, templates, configs, AHK, themes) | `git fetch` + `git merge --ff-only` of `release` (must be checked out) + idempotent migration (`install.ps1`, mode preserved) |

`winarchy update` deliberately touches only what Winarchy declares, not every package on
the machine: the stack owns its own dependencies, not your unrelated apps. Core components
are pinned because the stack depends on their concrete behaviour, so a major release never
lands without someone reading the changelog first.

The version check is best-effort (GitHub Releases API), cached 6 h, and never blocks the
flow. `winarchy doctor` shows installed vs latest release. Self-update only runs on a git
checkout and aborts if the working tree has uncommitted changes to tracked files (your
customization lives in untracked overrides, so it is never touched). Roll back with
`git checkout <previous-tag>` + `install.ps1`.
