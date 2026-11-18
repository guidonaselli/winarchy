use super::*;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use windows::core::PWSTR;
use windows::Win32::System::JobObjects::*;
use windows::Win32::System::StationsAndDesktops::*;

struct PointerProbe {
    directory: PathBuf,
    desktop: HDESK,
    process: HANDLE,
    thread: HANDLE,
    job: HANDLE,
}

impl Drop for PointerProbe {
    fn drop(&mut self) {
        unsafe {
            if !self.job.is_invalid() { let _ = CloseHandle(self.job); }
            if !self.process.is_invalid() {
                let _ = WaitForSingleObject(self.process, 5_000);
                let _ = CloseHandle(self.process);
            }
            if !self.thread.is_invalid() { let _ = CloseHandle(self.thread); }
            if !self.desktop.is_invalid() { let _ = CloseDesktop(self.desktop); }
        }
        for name in ["dock-pins.json", "show-start.flag"] {
            let _ = std::fs::remove_file(self.directory.join(name));
        }
        let _ = std::fs::remove_dir(&self.directory);
    }
}

unsafe fn desktop_name(desktop: HDESK) -> String {
    let mut name = [0u16; 256];
    let mut needed = 0;
    GetUserObjectInformationW(HANDLE(desktop.0), UOI_NAME, Some(name.as_mut_ptr() as *mut _),
        std::mem::size_of_val(&name) as u32, Some(&mut needed)).unwrap();
    String::from_utf16_lossy(&name[..name.iter().position(|c| *c == 0).unwrap()])
}

struct ProbeDocks;

impl Drop for ProbeDocks {
    fn drop(&mut self) {
        let docks = DOCKS.with(|d| std::mem::take(&mut *d.borrow_mut()));
        unsafe {
            let _ = ReleaseCapture();
            for d in docks {
                crate::accessibility::destroy(d.hwnd);
                let _ = DestroyWindow(d.tip);
                let _ = DestroyWindow(d.hwnd);
            }
        }
    }
}

#[path = "dock_tray_integration_tests.rs"]
mod tray_tests;

unsafe fn probe_directory(expected: &str) -> PathBuf {
    assert!(expected.starts_with("WinarchyPointer_"));
    assert!(expected.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'));
    let input = OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS).unwrap();
    let input_name = desktop_name(input);
    CloseDesktop(input).unwrap();
    let actual = desktop_name(GetThreadDesktop(GetCurrentThreadId()).unwrap());
    assert_ne!(actual.to_lowercase(), input_name.to_lowercase(), "Never run on the input desktop");
    assert_eq!(actual, expected);
    let directory = std::env::temp_dir().join(expected);
    assert!(directory.is_dir(), "The parent must create the probe directory");
    directory
}

unsafe fn run_pointer_probe(expected: &str) {
    let directory = probe_directory(expected);
    std::fs::write(directory.join("dock-pins.json"), "[]").unwrap();
    let mut theme = Theme::default();
    theme.dock_motion = true;
    theme.dock_hide = false;
    theme.hide_native_taskbar = false;
    let _docks = ProbeDocks;
    let hwnd = init(&theme, &directory).expect("Create the real dock window");
    let logo = || with(|d| {
        let x = d.layout.brand.x + d.layout.brand.w / 2;
        let y = d.layout.brand.y + d.layout.brand.h / 2;
        LPARAM(((y as u16 as u32) << 16 | x as u16 as u32) as isize)
    });
    let flag = directory.join("show-start.flag");
    SendMessageW(hwnd, WM_LBUTTONDOWN, WPARAM(1), logo());
    assert_eq!(GetCapture(), hwnd, "The real pointer down must capture the dock");
    // A stale monitor snapshot exercises reconciliation without changing the machine's displays.
    with(|d| d.mon.2 += 1);
    SendMessageW(hwnd, WM_DISPLAYCHANGE, WPARAM(0), LPARAM(0));
    assert_eq!(GetCapture(), HWND::default(), "Topology reconciliation must release capture");
    SendMessageW(hwnd, WM_LBUTTONUP, WPARAM(0), logo());
    assert_eq!(flag.exists(), false, "A cancelled release must not request Start");
    SendMessageW(hwnd, WM_LBUTTONDOWN, WPARAM(1), logo());
    SendMessageW(hwnd, WM_LBUTTONUP, WPARAM(0), logo());
    assert_eq!(std::fs::read(&flag).unwrap(), b"", "A fresh click must request Start");
    for cancellation in [WM_CAPTURECHANGED, WM_CANCELMODE] {
        std::fs::remove_file(&flag).unwrap();
        SendMessageW(hwnd, WM_LBUTTONDOWN, WPARAM(1), logo());
        assert_eq!(GetCapture(), hwnd, "The real pointer down must capture the dock");
        if cancellation == WM_CAPTURECHANGED {
            ReleaseCapture().unwrap();
        } else {
            SendMessageW(hwnd, cancellation, WPARAM(0), LPARAM(0));
        }
        assert_eq!(GetCapture(), HWND::default(), "Cancellation {cancellation} must release capture");
        SendMessageW(hwnd, WM_LBUTTONUP, WPARAM(0), logo());
        assert_eq!(flag.exists(), false, "Cancellation {cancellation} must suppress Start");
        SendMessageW(hwnd, WM_LBUTTONDOWN, WPARAM(1), logo());
        SendMessageW(hwnd, WM_LBUTTONUP, WPARAM(0), logo());
        assert_eq!(std::fs::read(&flag).unwrap(), b"", "A fresh click after {cancellation} must request Start");
    }
}

#[test]
#[ignore = "creates an isolated desktop; review the harness before explicitly running"]
fn topology_cancels_a_real_pointer_gesture_and_the_next_logo_click_requests_start() {
    isolated_probe(concat!(module_path!(), "::topology_cancels_a_real_pointer_gesture_and_the_next_logo_click_requests_start"),
        run_pointer_probe, &["dock-pins.json", "show-start.flag"]);
}

fn isolated_probe(test: &str, run: unsafe fn(&str), expected_files: &[&str]) {
    unsafe {
        if let Ok(expected) = std::env::var("WINARCHY_POINTER_DESKTOP") {
            run(&expected);
            return;
        }
        let name = format!("WinarchyPointer_{}_{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
        let directory = std::env::temp_dir().join(&name);
        std::fs::create_dir(&directory).unwrap();
        let mut probe = PointerProbe { directory, desktop: HDESK::default(),
            process: HANDLE::default(), thread: HANDLE::default(), job: HANDLE::default() };
        let wide_name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        probe.desktop = CreateDesktopW(PCWSTR(wide_name.as_ptr()), PCWSTR::null(), None,
            DESKTOP_CONTROL_FLAGS(0), 0x01ff, None).unwrap();
        probe.job = CreateJobObjectW(None, PCWSTR::null()).unwrap();
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(probe.job, JobObjectExtendedLimitInformation,
            &limits as *const _ as *const _, std::mem::size_of_val(&limits) as u32).unwrap();
        let exe = std::env::current_exe().unwrap();
        let application: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
        let test = test.strip_prefix(concat!(env!("CARGO_CRATE_NAME"), "::")).unwrap();
        let mut command: Vec<u16> = format!("\"{}\" --exact {test} --ignored --nocapture --test-threads=1", exe.display())
            .encode_utf16().chain(Some(0)).collect();
        let mut desktop_path: Vec<u16> = format!("WinSta0\\{name}").encode_utf16().chain(Some(0)).collect();
        let mut environment: Vec<_> = std::env::vars_os()
            .filter(|(key, _)| !key.to_string_lossy().eq_ignore_ascii_case("WINARCHY_POINTER_DESKTOP")).collect();
        environment.push(("WINARCHY_POINTER_DESKTOP".into(), name.clone().into()));
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
            panic!("Could not contain the pointer probe: {error}");
        }
        assert_ne!(ResumeThread(probe.thread), u32::MAX);
        assert_eq!(WaitForSingleObject(probe.process, 45_000), WAIT_OBJECT_0, "Pointer probe timed out");
        let mut code = 1;
        GetExitCodeProcess(probe.process, &mut code).unwrap();
        assert_eq!(code, 0, "The isolated pointer probe must pass");
        let mut files: Vec<_> = std::fs::read_dir(&probe.directory).unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
        files.sort();
        assert_eq!(files, expected_files);
    }
}
