# Winarchy Island (the default bar)

- a notch glued to the screen edge (flat on the edge, rounded below) with workspace dots and a clock;
  it opens on hover (120 ms) or `SUPER+I` into a panel with media controls, CPU/RAM/volume meters,
  workspace chips, the weather and only the notes that matter (battery, notifications, game mode)
- click a workspace dot or chip to switch to it, the pin button to keep the panel open (it stays until
  you press it again), the weather row to open the forecast in your browser, the VOL bar to set the
  volume, CPU or RAM to open Task Manager; scroll over it for volume, middle-click to mute; with
  `position = bottom` the notch sits on the bottom edge instead
- the weather comes from Open-Meteo (no key); set your place with `{"location": {"lat": -34.6, "lon": -58.4}}`
  in `config/shell/user.json` (without it, the shell falls back to the location YASB cached, if any)
  (an out-of-range location is ignored, and edits apply within ~30 min; clicking CPU/RAM opens Task Manager, which asks for UAC)
- click the clock for a month calendar with the day's agenda; add read-only ICS feeds (Google Calendar: Settings →
  your calendar → "Secret address in iCal format") with `{"calendar": ["https://…/basic.ics"]}` in `config/shell/user.json`
  (refreshed every 15 min; events in other time zones are read as local time). A meeting starting within 15 min
  shows a countdown in the bar. The current temperature sits next to the dots (click it to open the forecast)
- world clock: list cities in `config/shell/user.json` as `{"clocks": [{"city": "NYC", "tz": "Eastern Standard Time"}]}` (Windows zone names, see `tzutil /l`);
  one row with their time appears at the bottom of the clock panel, `+1d`/`-1d` when it is another day there. Without `clocks` nothing is shown
- brightness: when PowerToys PowerDisplay is installed, a BRI meter appears in the panel (average of all monitors);
  scroll over it to change every monitor by 5 points, click the bar to set them all. Without PowerToys it is hidden
- all island text is translated (English/Spanish): it follows `WINARCHY_LANG=es|en` or the Windows language; new texts go in `shell/src/i18n.rs`
- Espectro, the assistant's spirit (a dark, glowing-eyed creature tinted with your theme's accent), shows in the pill only while the assistant listens, thinks or speaks; its eyes follow the mouse and its face changes with each state; `{"pet": false}` in
  `config/shell/user.json` hides it (that file survives `theme set` and updates)
- the voice assistant (`winarchy assistant on`) speaks Spanish or English, taken from your Windows regional
  format or `WINARCHY_LANG=es|en`; it can run PowerShell and look at your screen when you ask. Speech uses
  Microsoft's Edge neural voices (the text is sent to Microsoft) with a local Piper voice as fallback;
  `WINARCHY_TTS=piper` keeps it fully local and `WINARCHY_EDGE_VOICE` picks another voice. The speech models load
  while you hold the key and are released after 10 idle minutes, so an idle assistant holds no VRAM
- source in `shell/` (Rust + Win32); `install.ps1` downloads the binary from the GitHub Release
  and installs it only if its SHA256 matches `versions.lock.toml`
- YASB is optional: `install.ps1 -WithYasb`, or `winarchy bar use yasb` (installs it if missing).
  Machines that already had YASB keep it after an update
