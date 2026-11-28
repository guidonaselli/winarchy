# The session shell (optional, no Explorer)

By default Winarchy runs next to Explorer (coexistence mode). `winarchy shell activate --yes` makes
`winarchy-shell.exe` the Windows shell at the next sign-in through the per-user Custom User Interface
policy, so `explorer.exe` never starts. It is opt-in, records the exact prior value, and
`winarchy shell deactivate` or the SYSTEM recovery task put Explorer back
(see [docs/custom-shell-policy.md](custom-shell-policy.md)). Do not activate it on a machine you
cannot recover.

What the shell draws and owns when Explorer is gone:

- **Start menu** (`Win` tap or the dock logo): pinned apps, all apps from `shell:AppsFolder`, type to search,
  power menu. Drop folders or files on the logo to pin them as *places*; right-click or `Shift+F10` unpins.
- **Dock** per monitor with live DWM thumbnails, drag-and-drop to open files with an app, jump-list recents,
  a tray flyout with the real tray icons and a system cluster (Quick Settings, clock, show desktop).
- **Alt+Tab** across every workspace, with live thumbnails of the visible ones; holding `Alt` and stepping only previews, the window changes when you release.
- **Desktop** with icons, wallpaper and the context menu; Windows notifications as toasts, and `Win+N` or the bell
  replays the last five (there is no persistent notification list yet).
- **Volume keys** (`Volume_Up/Down/Mute`) through AutoHotkey, with the volume OSD in the island.
- **Run dialog** on `Win+Alt+R`.
- **Files**, a native file manager in Rust (`files/`), is what `Win+E` opens when it is installed.

**Store (UWP) apps need the UWP host, which is experimental.** `winarchy shell activate` turns it on;
`winarchy uwphost off` turns it off and `on` back, effective at the next sign-in. Without it Settings (and every
`ms-settings:` link), Calculator, Microsoft Store and other UWP apps fail to activate (`0x80040900`, "Class not
registered"), because Explorer is not there to host them. Win32 apps, including packaged ones such as Notepad,
work, and the shell and the hotkeys open the classic applet for Sound, Network and Power instead.

The host (`uwphost/`) relies on private Windows interfaces: it downloads Microsoft's public symbols for the installed
build on first use, checks them before calling anything and refuses to run when they do not match. The shell
supervises it, restarts it after a crash or a hang, and after repeated failures tells you in the island and goes on
without it until the next sign-in (`winarchy uwphost status` and `retry`). Without network on first use it retries
and gives up until the next sign-in.

Its limits today: verified in Windows Sandbox at 100 % scaling and on one real machine; no release ships its binary
yet (build it with `cargo build --release` in `uwphost/`); minimizing and restoring a Store app leaves it blank
(close and reopen it); Game Bar shows nothing and the touch keyboard is not registered. Apps left open across a host
restart are closed by the new one, because Windows refuses to activate them again. If you need Store apps you can
count on, stay in coexistence mode (`winarchy shell deactivate`).

Also covered without Explorer: a notice when a drive is connected and when the battery crosses the power plan's
thresholds, an input-language pill with more than one layout, the main `Win+X` tools and display projection from the
Winarchy menu, the magnifier and on-screen keyboard, taskbar progress on the dock, and a Wi-Fi list on the network
icon (saved, open and WPA personal networks; Windows asks for the password).

Other gaps without Explorer, from an audit of what Explorer normally does: no Focus assist, no enterprise Wi-Fi sign-in,
no task view or virtual desktops, no `Win+X` menu, and no Bluetooth, Display or Printers panels. The parity table, with the evidence level of every row, is
[docs/rust-session-shell-parity.md](rust-session-shell-parity.md). The boot sequence, the single-instance
rule and the logon race between the autostart task and the policy are covered by tests and the Sandbox scenarios
under `tests/sandbox/`.
