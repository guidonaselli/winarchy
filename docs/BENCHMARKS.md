# Benchmarks

Numbers for the native shell (`winarchy-shell.exe`). Everything here is reproducible with the
commands below; nothing is estimated. Read the caveats before quoting a number.

## Resource use in a live session

`scripts/Measure-Shell.ps1` samples process counters (working set, private bytes, handles,
threads, CPU) for N seconds. It only reads counters: it never starts or stops anything.

```powershell
pwsh -NoProfile -File scripts\Measure-Shell.ps1 -Name winarchy-shell,explorer,komorebi,AutoHotkey64 -Seconds 30
```

Measured on Windows 11 (24 logical cores), 30 s window, normal desktop use, shell with dock and
cluster on:

| Process | Working set | Private bytes | Handles | Threads | CPU (% of one core) |
|---|---|---|---|---|---|
| `winarchy-shell` | 53 MB | 32 MB | 943 | 38 | 0.38 |
| `komorebi` | 52 MB | 133 MB | 810 | 44 | 3.29 |
| `AutoHotkey64` | 24 MB | 8 MB | 275 | 4 | 0.16 |
| `explorer` (same machine, same window) | 294 MB | 392 MB | 5837 | 199 | 2.58 |

The release binary is 1.5 MB (`opt-level = "s"`, LTO, stripped).

## Session without Explorer (same machine, 2 monitors)

Explorer was stopped with its own clean-exit message and the shell restarted with `--session-shell`; 30 s windows with `scripts/Measure-Shell.ps1`. Explorer was restored afterwards.

| Process | Explorer running | Without Explorer (idle) |
|---|---|---|
| `winarchy-shell` | 50 MB, 0.44 % CPU | 72 MB, 0.6 % CPU |
| `komorebi` | 44 MB, 0.6 % | 66 MB, 0.38 % |
| `AutoHotkey64` | 14 MB, 0.22 % | 22 MB, 0.0 % |
| `explorer` | 252 MB, 0.6 % | not running |

The stack without Explorer holds about 160 MB against about 360 MB with it. The shell grows because it then draws the desktop and owns the tray. With the dock set to `hide`, a mouse at the bottom edge revealed it listing the open apps.

Finding: komorebi in `Cloak` mode aborts when Explorer is absent (`Class not registered`, HRESULT 0x80040154, `MoveContainerToWorkspace`), because `Cloak` goes through Explorer's ImmersiveShell COM server. With the custom shell installed, `komorebi.json` is therefore generated with `window_hiding_behaviour: "Hide"`.

## Hot-path microbenchmarks

```powershell
cd shell
cargo test --release --offline bench_ -- --ignored --nocapture --test-threads=1
```

Median of 15 runs. These are pure CPU functions, so they do not include GDI or DWM time.

| Operation | Time |
|---|---|
| `hit_test`, dock with 30 apps | 16 ns |
| `layout_for`, 30 apps on a 4K monitor at 200% | 88 ns |
| `fill_round_a`, dock pill 608x48 (100%) | 186 us |
| `fill_round_a`, dock pill 1216x96 (200%) | 1.2 ms |

The pill fill is the heaviest primitive. It runs only when the dock changes (hover, animation
tick, new window), never on an idle frame.

## Caveats

- `explorer.exe` also hosts the desktop, file windows and search. It does more than the Winarchy
  shell, so the comparison shows the cost of the whole stock shell, not a like-for-like feature
  match. In this session Explorer was still running for the files it hosts.
- The shell figures come from one live session, not a controlled machine. The CPU number is the
  average over the window, so it moves with what the user is doing.
- Startup time, frame time through GDI/DWM and behavior under mixed-DPI hotplug were not
  measured. They need a harness that drives a clean session; `tests/sandbox/` is the place for it.
