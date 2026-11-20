use super::*;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

struct ProbeWindows(Vec<HWND>);

impl Drop for ProbeWindows {
    fn drop(&mut self) {
        unsafe {
            dismiss();
            for hwnd in &self.0 {
                if IsWindow(*hwnd).as_bool() { let _ = DestroyWindow(*hwnd); }
            }
        }
    }
}

#[test]
#[ignore = "opens two owned desktop windows and requires an external UIA client"]
fn real_uia_menu_closes_only_its_owned_window() {
    run_probe(false, ProbeLifetime::Keep);
}

#[derive(Clone, Copy, PartialEq)]
enum ProbeLifetime { Keep, RecreateHost, RecreateWindow }

fn run_probe(scroll_only: bool, lifetime: ProbeLifetime) {
    let recreate_host = lifetime != ProbeLifetime::Keep;
    let recreate_window = lifetime == ProbeLifetime::RecreateWindow;
    unsafe {
        let mut windows = ProbeWindows(Vec::new());
        let instance = GetModuleHandleW(PCWSTR::null()).unwrap();
        let suffix = if scroll_only { format!(" {}", std::env::var("WINARCHY_UIA_DESKTOP").unwrap()) } else { String::new() };
        let titles = vec![format!("Winarchy UIA probe A{suffix}"), format!("Winarchy UIA probe B{suffix}")];
        for title in &titles {
            let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
            windows.0.push(CreateWindowExW(
                WS_EX_TOOLWINDOW, w!("STATIC"), PCWSTR(title.as_ptr()), WS_OVERLAPPEDWINDOW,
                80, 80, 300, 180, None, None, instance, None,
            ).unwrap());
        }
        let ids: Vec<isize> = windows.0.iter().map(|h| h.0 as isize).collect();
        let mut theme = Theme::default();
        theme.dock_motion = false;
        let context = || Ctx {
            owner: ids[0], groups: Vec::new(), picker: true,
            titles: titles.clone(),
            active: None, theme: theme.clone(), scale: 1.0, cx: 400,
            bar: R { x: 200, y: 200, w: 400, h: 32 }, bottom: false,
            mon: (0, 0, 900, 330), wins: ids.clone(), last: None,
            launch: None, key: String::new(), pinned: false, pinnable: false,
            closable: true, closeable_wins: ids.clone(), recents: Vec::new(),
        };
        show(context());
        let menu = MENU.with(|m| m.borrow().as_ref().unwrap().hwnd);
        if scroll_only {
            let mut rect = RECT::default();
            GetClientRect(menu, &mut rect).unwrap();
            assert_eq!(rect.bottom, 44, "Probe must expose only one of its two window rows");
            for x in [rect.right - 14, rect.right - 2] {
                let point = LPARAM(((22 << 16) | x) as isize);
                let _ = SendMessageW(menu, WM_LBUTTONUP, WPARAM(0), point);
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                assert_eq!((is_open(), IsWindow(menu).as_bool()), (true, true),
                    "A gutter click must leave the picker open");
                assert_eq!((IsWindow(windows.0[0]).as_bool(), IsWindow(windows.0[1]).as_bool()),
                    (true, true), "A gutter click must leave both windows alive");
            }
        }
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../state/verify");
        std::fs::create_dir_all(&dir).unwrap();
        let pid = std::process::id();
        let metadata = dir.join(format!("dock-menu-uia-metadata-{pid}.json"));
        let result = dir.join(format!("dock-menu-uia-result-{pid}.json"));
        let request = metadata.with_extension("recreate");
        let ready = metadata.with_extension("ready");
        let check = metadata.with_extension("check");
        let checked = metadata.with_extension("checked");
        for path in [&result, &request, &ready, &check, &checked] { let _ = std::fs::remove_file(path); }
        std::fs::write(&metadata, serde_json::to_vec(&serde_json::json!({
            "menu": menu.0 as isize, "first": ids[0], "second": ids[1],
            "pid": std::process::id(), "rootName": t(K::Applications),
            "closeName": format!("{} · {}", t(K::MenuClose), titles[1]),
            "rowNames": titles, "desktop": std::env::var("WINARCHY_UIA_DESKTOP").ok(),
        })).unwrap()).unwrap();
        let mut child = Command::new("pwsh")
            .args(["-NoProfile", "-File"])
            .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/Invoke-DockMenuUiaProbe.ps1"))
            .arg("-MetadataPath").arg(&metadata)
            .arg("-ResultPath").arg(&result)
            .args(if recreate_window { vec!["-ScrollOnly", "-RecreateWindow"] } else if recreate_host { vec!["-ScrollOnly", "-RecreateHost"] } else if scroll_only { vec!["-ScrollOnly"] } else { Vec::new() })
            .stdout(Stdio::null()).stderr(Stdio::null())
            .creation_flags(0x08000000)
            .spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(45);
        let mut recreated = false;
        let mut active_menu = menu;
        let status = loop {
            if recreate_host && !recreated && request.exists() {
                let fresh_menu = if recreate_window {
                    DestroyWindow(menu).unwrap();
                    assert!(!IsWindow(menu).as_bool(), "Original menu HWND must be destroyed before replacement");
                    show(context());
                    MENU.with(|m| m.borrow().as_ref().unwrap().hwnd)
                } else {
                    crate::accessibility::destroy(menu);
                    crate::accessibility::create(menu, t(K::Applications), accesskit::Role::Menu);
                    let _ = SendMessageW(menu, WM_ACCESS_UPDATE, WPARAM(0), LPARAM(0));
                    menu
                };
                active_menu = fresh_menu;
                let pending = ready.with_extension("pending");
                std::fs::write(&pending, serde_json::to_vec(&serde_json::json!({
                    "menu": fresh_menu.0 as isize, "destroyedMenuWindow": recreate_window,
                })).unwrap()).unwrap();
                std::fs::rename(&pending, &ready).unwrap();
                recreated = true;
            }
            let check_requested = recreate_host && check.exists() && !checked.exists();
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if check_requested {
                assert!(IsWindow(active_menu).as_bool(), "Stale UIA provider dismissed the replacement menu");
                assert_eq!((IsWindow(windows.0[0]).as_bool(), IsWindow(windows.0[1]).as_bool()),
                    (true, true), "Stale UIA provider acted on the recreated host");
                std::fs::write(&checked, b"checked").unwrap();
            }
            if let Some(status) = child.try_wait().unwrap() { break Some(status) }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let report = std::fs::read_to_string(&result).unwrap_or_else(|e| e.to_string());
        assert!(status.is_some_and(|s| s.success()), "UIA client failed: {report}");
        assert_eq!(
            (IsWindow(windows.0[0]).as_bool(), IsWindow(windows.0[1]).as_bool()),
            if scroll_only && !recreate_host { (true, true) } else { (true, false) }, "Unexpected probe window lifetime",
        );
    }
}

use std::os::windows::process::CommandExt;
use std::os::windows::ffi::OsStrExt;
use windows::Win32::System::JobObjects::*;
use windows::Win32::System::StationsAndDesktops::*;

struct InputDesktop(HDESK);

impl Drop for InputDesktop {
    fn drop(&mut self) {
        unsafe { let _ = CloseDesktop(self.0); }
    }
}

unsafe fn desktop_name(desktop: HDESK) -> String {
    let mut name = [0u16; 256];
    let mut needed = 0;
    GetUserObjectInformationW(HANDLE(desktop.0), UOI_NAME, Some(name.as_mut_ptr() as *mut _),
        std::mem::size_of_val(&name) as u32, Some(&mut needed)).unwrap();
    let end = name.iter().position(|c| *c == 0).unwrap();
    String::from_utf16_lossy(&name[..end])
}

fn validate_probe_desktop(actual: &str, expected: &str, input: &str) -> Result<(), &'static str> {
    if actual.eq_ignore_ascii_case(input) { return Err("Probe must never run on the input desktop"); }
    if actual != expected { return Err("Probe must run on its isolated desktop"); }
    Ok(())
}

#[test]
fn isolated_probe_rejects_input_desktop_even_when_its_name_matches() {
    assert_eq!(validate_probe_desktop("WinarchyUia_123", "WinarchyUia_123", "WinarchyUia_123"),
        Err("Probe must never run on the input desktop"));
}

#[test]
fn isolated_probe_accepts_a_distinct_expected_desktop() {
    assert_eq!(validate_probe_desktop("WinarchyUia_123", "WinarchyUia_123", "Default"), Ok(()));
}

#[test]
#[ignore = "requires a Windows desktop and an external UIA client; never switches the input desktop"]
fn real_uia_scroll_into_view_on_isolated_desktop() {
    run_isolated_probe("real_uia_scroll_into_view_on_isolated_desktop", ProbeLifetime::Keep);
}

#[test]
#[ignore = "requires an isolated Windows desktop and an external UIA client; never switches the input desktop"]
fn real_uia_stale_provider_cannot_invoke_recreated_host() {
    run_isolated_probe("real_uia_stale_provider_cannot_invoke_recreated_host", ProbeLifetime::RecreateHost);
}

#[test]
#[ignore = "requires an isolated Windows desktop and an external UIA client; never switches the input desktop"]
fn real_uia_stale_provider_cannot_invoke_recreated_menu_window() {
    run_isolated_probe("real_uia_stale_provider_cannot_invoke_recreated_menu_window", ProbeLifetime::RecreateWindow);
}

fn run_isolated_probe(test_name: &str, lifetime: ProbeLifetime) {
    let recreate_host = lifetime != ProbeLifetime::Keep;
    use windows::core::PWSTR;
    use windows::Win32::System::StationsAndDesktops::*;
    use windows::Win32::System::Threading::*;
    if let Ok(expected) = std::env::var("WINARCHY_UIA_DESKTOP") {
        assert!(expected.starts_with("WinarchyUia_"), "Only a probe desktop may run the UIA child");
        unsafe {
            let input = InputDesktop(OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS).unwrap());
            let actual = desktop_name(GetThreadDesktop(GetCurrentThreadId()).unwrap());
            validate_probe_desktop(&actual, &expected, &desktop_name(input.0)).unwrap();
        }
        run_probe(true, lifetime);
        return;
    }
    struct IsolatedProbe { desktop: HDESK, process: HANDLE, thread: HANDLE, job: HANDLE }
    impl Drop for IsolatedProbe {
        fn drop(&mut self) {
            unsafe {
                if !self.job.is_invalid() { let _ = CloseHandle(self.job); }
                if !self.process.is_invalid() {
                    let _ = WaitForSingleObject(self.process, 5_000);
                    let _ = CloseHandle(self.process);
                }
                if !self.thread.is_invalid() { let _ = CloseHandle(self.thread); }
                let _ = CloseDesktop(self.desktop);
            }
        }
    }
    unsafe {
        let name = format!("WinarchyUia_{}_{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
        let wide_name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        let desktop = CreateDesktopW(PCWSTR(wide_name.as_ptr()), PCWSTR::null(), None,
            DESKTOP_CONTROL_FLAGS(0), 0x01ff, None).unwrap();
        let mut probe = IsolatedProbe { desktop, process: HANDLE::default(), thread: HANDLE::default(), job: HANDLE::default() };
        probe.job = CreateJobObjectW(None, PCWSTR::null()).unwrap();
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(probe.job, JobObjectExtendedLimitInformation,
            &limits as *const _ as *const _, std::mem::size_of_val(&limits) as u32).unwrap();
        let exe = std::env::current_exe().unwrap();
        let application: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
        let test = format!("{}::{test_name}", module_path!());
        let test = test.strip_prefix(concat!(env!("CARGO_CRATE_NAME"), "::")).unwrap();
        let mut command: Vec<u16> = format!("\"{}\" --exact {test} --ignored --nocapture --test-threads=1", exe.display()).encode_utf16().chain(Some(0)).collect();
        let mut desktop_path: Vec<u16> = format!("WinSta0\\{name}").encode_utf16().chain(Some(0)).collect();
        let mut environment: Vec<_> = std::env::vars_os().filter(|(key, _)| key != "WINARCHY_UIA_DESKTOP").collect();
        environment.push(("WINARCHY_UIA_DESKTOP".into(), name.clone().into()));
        environment.sort_by_key(|(key, _)| key.to_string_lossy().to_uppercase());
        let environment: Vec<u16> = environment.iter().flat_map(|(key, value)|
            key.encode_wide().chain(Some('=' as u16)).chain(value.encode_wide()).chain(Some(0))).chain(Some(0)).collect();
        let startup = STARTUPINFOW { cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            lpDesktop: PWSTR(desktop_path.as_mut_ptr()), ..Default::default() };
        let mut process = PROCESS_INFORMATION::default();
        CreateProcessW(PCWSTR(application.as_ptr()), PWSTR(command.as_mut_ptr()), None, None, false,
            CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT | CREATE_SUSPENDED,
            Some(environment.as_ptr() as *const _), PCWSTR::null(), &startup, &mut process).unwrap();
        probe.process = process.hProcess;
        probe.thread = process.hThread;
        if let Err(error) = AssignProcessToJobObject(probe.job, probe.process) {
            let _ = TerminateProcess(probe.process, 1);
            panic!("Could not contain isolated UIA process: {error}");
        }
        assert_ne!(ResumeThread(probe.thread), u32::MAX);
        assert_eq!(WaitForSingleObject(probe.process, 60_000), WAIT_OBJECT_0, "Isolated UIA probe timed out");
        let mut code = 1;
        GetExitCodeProcess(probe.process, &mut code).unwrap();
        let report_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../state/verify/dock-menu-uia-result-{}.json", process.dwProcessId));
        let report = std::fs::read_to_string(&report_path).unwrap_or_default();
        assert_eq!(code, 0, "Isolated UIA probe failed: {report}");
        let report: serde_json::Value = serde_json::from_str(&report).expect("UIA client must emit its report");
        assert_eq!(report["passed"], true);
        assert_eq!(report["scrollOnly"], true);
        assert_eq!(report["recreatedHost"], recreate_host);
        assert_eq!(report["destroyedMenuWindow"], lifetime == ProbeLifetime::RecreateWindow);
        if recreate_host {
            assert_eq!(report["staleProviderLeftBothWindowsAlive"], true);
            assert_eq!(report["staleProviderLeftMenuAlive"], true);
            assert_eq!(report["freshProviderClosedSecondWindow"], true);
        }
        assert_eq!(report["expectedPid"], process.dwProcessId);
        assert_eq!(report["desktop"], name);
    }
}

