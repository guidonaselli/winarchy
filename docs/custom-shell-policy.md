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

## Activation

`winarchy shell status|activate --yes|deactivate`. Activation is explicit and takes effect at the next sign-in.

- `activate` and `deactivate` relaunch themselves elevated (one UAC prompt) because Windows only lets administrators write that key.
- `activate` refuses without `winarchy-shell.exe` or `config\shell\theme.json`, writes `state\custom-shell-recovery.json` (prior value, written value, command) and a copy under `backups\`, then writes the policy with a compare-and-write so a concurrent change aborts it.
- `deactivate` restores the prior value only if the current one is still what Winarchy wrote; otherwise it stops and leaves the value alone.
- While the record is valid, update, rollback and uninstall are allowed; without it they still refuse.
- The rest of the stack (komorebi, AutoHotkey) starts from a scheduled task at logon, not from Explorer, so it does not depend on the shell.

### Escape path if the session comes up without a usable shell

1. Ctrl+Alt+Del, Task Manager, Run new task, `<repo>\scripts\Restore-Explorer.cmd`. It needs neither the module nor the shell; it restores the recorded value (or removes the policy) and starts Explorer; with a damaged record it still removes a Winarchy policy.
2. Changing this key needs administrator rights (Windows owns `Policies`): in Task Manager tick "Create this task with administrative privileges", or let `Restore-Explorer.cmd` raise the UAC prompt itself.
3. Even simpler when the prior value was absent: Run new task, `reg delete "HKCU\Software\Microsoft\Windows\CurrentVersion\Policies\System" /v Shell /f`, then `explorer.exe`.
4. From another account, or from safe mode with the same user: delete that same value.

`install -Activate` only configures normal autostart and never touches this policy.
