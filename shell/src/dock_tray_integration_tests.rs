use super::*;

unsafe extern "system" fn probe_window_proc(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    DefWindowProcW(hwnd, message, wp, lp)
}

struct TrayProbeWindow(HWND);

impl TrayProbeWindow {
    unsafe fn create(class: PCWSTR) -> Self {
        let instance = GetModuleHandleW(PCWSTR::null()).unwrap();
        let definition = WNDCLASSW { lpfnWndProc: Some(probe_window_proc), hInstance: instance.into(),
            lpszClassName: class, ..Default::default() };
        assert_ne!(RegisterClassW(&definition), 0);
        Self(CreateWindowExW(WINDOW_EX_STYLE(0), class, w!(""), WS_POPUP,
            0, 0, 1, 1, None, None, instance, None).unwrap())
    }
}

impl Drop for TrayProbeWindow {
    fn drop(&mut self) {
        unsafe { if IsWindow(self.0).as_bool() { let _ = DestroyWindow(self.0); } }
    }
}

struct ProbeTraySession;

impl Drop for ProbeTraySession {
    fn drop(&mut self) {
        TRAY_SESSION.set(None);
    }
}

unsafe fn observer_quit() -> Option<u32> {
    let mut message = MSG::default();
    PeekMessageW(&mut message, None, WM_QUIT, WM_QUIT, PM_REMOVE)
        .as_bool().then_some(message.message)
}

unsafe fn run_tray_probe(expected: &str) {
    let directory = probe_directory(expected);
    std::fs::write(directory.join("dock-pins.json"), "[]").unwrap();
    let mut theme = Theme::default();
    theme.dock_hide = false;
    theme.hide_native_taskbar = false;
    let _docks = ProbeDocks;
    let dock = init(&theme, &directory).expect("Create the real dock window");
    let external = TrayProbeWindow::create(w!("WinarchyTrayProbeExternal"));
    let _session = ProbeTraySession;
    assert_eq!(observer_quit(), None);

    // Seed only the observer identity; show_tray would install global hooks and activate Explorer.
    let generation = 42;
    for class in [w!("Shell_TrayWnd"), w!("Shell_SecondaryTrayWnd"), w!("TopLevelWindowForOverflowXamlIsland")] {
        let allowed = TrayProbeWindow::create(class);
        TRAY_SESSION.set(Some((GetCurrentThreadId(), generation)));
        SendMessageW(dock, WM_TRAY_DISMISS, WPARAM(generation), LPARAM(allowed.0.0 as isize));
        assert_eq!(observer_quit(), None, "An allowed surface must keep the observer alive");
        SendMessageW(dock, WM_TRAY_DISMISS, WPARAM(generation), LPARAM(external.0.0 as isize));
        assert_eq!(observer_quit(), Some(WM_QUIT), "An external window must close the retained session");
    }

    TRAY_SESSION.set(Some((GetCurrentThreadId(), generation)));
    SendMessageW(dock, WM_TRAY_DISMISS, WPARAM(generation - 1), LPARAM(external.0.0 as isize));
    assert_eq!(observer_quit(), None, "An old observer generation must not close the current session");
    SendMessageW(dock, WM_TRAY_DISMISS, WPARAM(generation), LPARAM(external.0.0 as isize));
    assert_eq!(observer_quit(), Some(WM_QUIT), "The current observer generation must still close its session");

    let destroyed = TrayProbeWindow::create(w!("WinarchyTrayProbeDestroyed"));
    DestroyWindow(destroyed.0).unwrap();
    assert_eq!(IsWindow(destroyed.0).as_bool(), false);
    TRAY_SESSION.set(Some((GetCurrentThreadId(), generation)));
    SendMessageW(dock, WM_TRAY_DISMISS, WPARAM(generation), LPARAM(destroyed.0.0 as isize));
    assert_eq!(observer_quit(), Some(WM_QUIT), "A destroyed HWND must safely dismiss the session");
    SendMessageW(dock, WM_TRAY_DISMISS, WPARAM(generation), LPARAM(external.0.0 as isize));
    assert_eq!(observer_quit(), None, "A closed session must not stop the observer twice");
}

unsafe fn run_tray_mouse_probe(expected: &str) {
    let directory = probe_directory(expected);
    std::fs::write(directory.join("dock-pins.json"), "[]").unwrap();
    let mut theme = Theme::default();
    theme.dock_hide = false;
    theme.hide_native_taskbar = false;
    let _docks = ProbeDocks;
    let dock = init(&theme, &directory).expect("Create the real dock window");
    let external = TrayProbeWindow::create(w!("WinarchyTrayMouseProbeExternal"));
    SetWindowPos(external.0, HWND_TOP, 100, 100, 200, 200, SWP_NOACTIVATE | SWP_SHOWWINDOW).unwrap();
    let child = TrayProbeWindow(CreateWindowExW(WINDOW_EX_STYLE(0),
        w!("WinarchyTrayMouseProbeExternal"), w!(""), WS_CHILD | WS_VISIBLE,
        0, 0, 200, 200, external.0, None, GetModuleHandleW(PCWSTR::null()).unwrap(), None).unwrap());
    let point = POINT { x: 150, y: 150 };
    assert_eq!(WindowFromPoint(point), child.0,
        "WindowFromPoint must resolve an owned child on the isolated desktop");
    let _session = ProbeTraySession;
    let generation = 57;
    TRAY_OBSERVER.set((dock.0 as isize, generation));
    for (code, button) in [(-1, WM_LBUTTONDOWN), (0, WM_MOUSEMOVE), (0, WM_LBUTTONUP)] {
        let event = MSLLHOOKSTRUCT { pt: point, ..Default::default() };
        tray_mouse_hook(code, WPARAM(button as usize), LPARAM(&event as *const _ as isize));
        let mut message = MSG::default();
        assert_eq!(PeekMessageW(&mut message, dock, WM_TRAY_DISMISS, WM_TRAY_DISMISS, PM_REMOVE).as_bool(), false);
    }
    for button in [WM_LBUTTONDOWN, WM_RBUTTONDOWN, WM_MBUTTONDOWN] {
        TRAY_SESSION.set(Some((GetCurrentThreadId(), generation)));
        let event = MSLLHOOKSTRUCT { pt: point, ..Default::default() };
        tray_mouse_hook(0, WPARAM(button as usize), LPARAM(&event as *const _ as isize));
        let mut message = MSG::default();
        assert!(PeekMessageW(&mut message, dock, WM_TRAY_DISMISS, WM_TRAY_DISMISS, PM_REMOVE).as_bool());
        assert_eq!(message.wParam.0, 57);
        assert_eq!(message.lParam.0, external.0.0 as isize);
        DispatchMessageW(&message);
        assert_eq!(observer_quit(), Some(WM_QUIT), "The posted callback message must dismiss the session");
    }
    TRAY_OBSERVER.set((0, 0));
    let child_hwnd = child.0;
    let external_hwnd = external.0;
    drop(child);
    drop(external);
    drop(_docks);
    assert_eq!(IsWindow(child_hwnd).as_bool(), false);
    assert_eq!(IsWindow(external_hwnd).as_bool(), false);
    assert_eq!(IsWindow(dock).as_bool(), false);
}

unsafe fn run_tray_observer_lifecycle_probe(expected: &str) {
    probe_directory(expected);
    let target = TrayProbeWindow::create(w!("WinarchyTrayObserverProbe"));
    let target_hwnd = target.0.0 as isize;
    let expected = expected.to_owned();
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
    let observer = std::thread::spawn(move || {
        unsafe { probe_directory(&expected); }
        tray_observer(target_hwnd, 73, ready_tx);
        done_tx.send(()).unwrap();
    });
    let thread = ready_rx.recv_timeout(std::time::Duration::from_secs(5))
        .expect("The observer must report hook installation")
        .expect("Both real observer hooks must install");
    PostThreadMessageW(thread, WM_QUIT, WPARAM(0), LPARAM(0)).unwrap();
    done_rx.recv_timeout(std::time::Duration::from_secs(5))
        .expect("The observer must release its hooks and exit after WM_QUIT");
    observer.join().unwrap();
    let hwnd = target.0;
    drop(target);
    assert_eq!(IsWindow(hwnd).as_bool(), false);
}

#[test]
#[ignore = "creates an isolated desktop; review the harness before explicitly running"]
fn tray_observer_installs_real_hooks_and_exits_on_quit() {
    isolated_probe(concat!(module_path!(), "::tray_observer_installs_real_hooks_and_exits_on_quit"),
        run_tray_observer_lifecycle_probe, &[]);
}

#[test]
#[ignore = "creates an isolated desktop; review the harness before explicitly running"]
fn tray_mouse_callback_posts_the_hit_window_and_dismisses_the_session() {
    isolated_probe(concat!(module_path!(), "::tray_mouse_callback_posts_the_hit_window_and_dismisses_the_session"),
        run_tray_mouse_probe, &["dock-pins.json"]);
}

#[test]
#[ignore = "creates an isolated desktop; review the harness before explicitly running"]
fn tray_dismiss_messages_respect_window_classes_generation_and_destroyed_handles() {
    isolated_probe(concat!(module_path!(), "::tray_dismiss_messages_respect_window_classes_generation_and_destroyed_handles"),
        run_tray_probe, &["dock-pins.json"]);
}
