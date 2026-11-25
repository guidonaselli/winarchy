# CustomShell activation policy

## Platform choice

Winarchy targets the per-user **Custom User Interface** policy, which Microsoft marks supported on Windows 11 Pro. It starts the configured interface instead of Explorer at sign-in; disabling or leaving the policy unconfigured restores Explorer. The current Windows ADMX maps it to a string value named `Shell` under `Software\Microsoft\Windows\CurrentVersion\Policies\System`. Windows Shell Launcher is a separate feature and is not supported on Pro.

Sources: [Microsoft CustomShell policy](https://learn.microsoft.com/en-us/windows/client-management/mdm/policy-csp-admx-winlogon), [Shell Launcher editions](https://learn.microsoft.com/en-us/windows/configuration/shell-launcher/).

## Product contract

- CustomShell remains an explicit per-user opt-in and takes effect at the next sign-in. Installing Winarchy or running ordinary `install -Activate` must not silently replace Explorer.
- Activation records the exact prior `Shell` value state (present/absent, registry type, and data) before changing it. Deactivation restores that exact prior state and must detect an external policy change instead of overwriting it.
- Returning to Explorer for the current session is separate from permanent deactivation for the next sign-in.
- Updates and uninstall keep a known-good shell binary and policy recovery path until the new state has been verified.
- Do not activate on a daily profile during development. Do not leave the user without an escape path if Winarchy, AHK, PowerShell, or the session pipe fails.

## Evidence and limits

The installed Windows `WinLogon.admx` on the benchmark host maps `CustomShell` to the `Shell` string under the user policy key. This confirms the registry mapping for this Windows build. Microsoft does not document CustomShell crash recovery or safe-mode behavior; those are Winarchy responsibilities and require isolated sign-in, failure, recovery, and next-logon tests before shipping.

The current code's `install -Activate` configures normal Winarchy autostart; it does not persist CustomShell policy. Session markers and the return bridge handle the current logon, but do not by themselves restore a next-logon policy. See `docs/rust-session-shell-parity.md` and task 3.3 in `lmd/rust-session-shell/tasks.md`.
