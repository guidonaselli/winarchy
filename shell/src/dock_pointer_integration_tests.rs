use super::*;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;
use windows::core::{Interface, PWSTR};
use windows::Win32::System::SystemServices::SFGAO_FOLDER;
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
        for name in ["dock-pins.json", "show-start.flag", "desktop-paint.txt", "desktop-wallpaper.log", "desktop-namespace.txt"] {
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

struct DesktopPaintWindow {
    hwnd: HWND,
    dc: HDC,
}

impl Drop for DesktopPaintWindow {
    fn drop(&mut self) {
        unsafe {
            if !self.dc.is_invalid() { ReleaseDC(self.hwnd, self.dc); }
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

unsafe fn run_desktop_paint_probe(expected: &str) {
    unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        DefWindowProcW(hwnd, message, wp, lp)
    }
    let directory = probe_directory(expected);
    let instance = GetModuleHandleW(PCWSTR::null()).unwrap();
    let class = WNDCLASSW { lpfnWndProc: Some(window_proc), hInstance: instance.into(),
        lpszClassName: w!("WinarchyDesktopPaintProbe"), ..Default::default() };
    assert_ne!(RegisterClassW(&class), 0);
    let mut window = DesktopPaintWindow {
        hwnd: CreateWindowExW(WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW, class.lpszClassName,
            w!(""), WS_POPUP | WS_VISIBLE, 0, 0, 96, 96, None, None, instance, None).unwrap(),
        dc: HDC::default(),
    };
    window.dc = GetDC(window.hwnd);
    assert!(!window.dc.is_invalid(), "The owned window must provide a DC");
    let points = [(8, 8), (48, 8), (88, 8), (8, 48), (48, 48), (88, 48),
        (8, 88), (48, 88), (88, 88)];
    let seed = COLORREF(0x00563412);
    for (x, y) in points {
        assert_eq!(SetPixel(window.dc, x, y, seed), seed);
    }
    let painted = PaintDesktop(window.dc).as_bool();
    let pixels = points.map(|(x, y)| GetPixel(window.dc, x, y).0);
    let signature = pixels.map(|pixel| format!("{pixel:08x}")).join(",");
    std::fs::write(directory.join("desktop-paint.txt"),
        format!("PaintDesktop success={painted}; pixels={signature}; seed=00563412; visual parity unverified")).unwrap();
    assert_eq!(painted, true, "PaintDesktop must accept the owned window DC");
    assert!(pixels.iter().all(|pixel| *pixel != u32::MAX), "All sampled pixels must be readable");
    assert!(pixels.iter().any(|pixel| *pixel != seed.0), "Painting must replace a seeded pixel");
}

#[test]
#[ignore = "feasibility only: paints an owned window on an isolated desktop; no visual parity claim"]
fn paint_desktop_can_paint_an_owned_window_on_a_private_desktop() {
    isolated_probe(concat!(module_path!(), "::paint_desktop_can_paint_an_owned_window_on_a_private_desktop"),
        run_desktop_paint_probe, &["desktop-paint.txt"]);
}

struct WallpaperProbeApartment;

impl Drop for WallpaperProbeApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize(); }
    }
}

struct WallpaperProbeString(PWSTR);

impl Drop for WallpaperProbeString {
    fn drop(&mut self) {
        unsafe { windows::Win32::System::Com::CoTaskMemFree(Some(self.0.0.cast())); }
    }
}

unsafe fn run_desktop_wallpaper_query_probe(expected: &str) {
    use windows::core::Interface;
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};

    fn check(operation: &str, status: windows::core::HRESULT, log: &std::path::Path) {
        let result = format!("{operation}: HRESULT=0x{:08x}", status.0 as u32);
        println!("{result}");
        let _ = std::fs::OpenOptions::new().create(true).append(true).open(log)
            .and_then(|mut file| std::io::Write::write_all(&mut file, format!("{result}\n").as_bytes()));
        assert!(status.is_ok(), "{operation}: HRESULT=0x{:08x}", status.0 as u32);
    }

    let directory = probe_directory(expected);
    let log = directory.join("desktop-wallpaper.log");
    std::fs::write(&log, "").unwrap();
    check("CoInitializeEx", CoInitializeEx(None, COINIT_APARTMENTTHREADED), &log);
    let _apartment = WallpaperProbeApartment;
    let wallpaper: IDesktopWallpaper = match CoCreateInstance(&DesktopWallpaper, None, CLSCTX_INPROC_SERVER) {
        Ok(wallpaper) => wallpaper,
        Err(error) => {
            let _ = std::fs::write(&log, format!("CoCreateInstance: HRESULT=0x{:08x}", error.code().0 as u32));
            panic!("CoCreateInstance: HRESULT=0x{:08x}", error.code().0 as u32);
        }
    };
    let _ = std::fs::OpenOptions::new().append(true).open(&log)
        .and_then(|mut file| std::io::Write::write_all(&mut file, b"CoCreateInstance: success\n"));
    let table = wallpaper.vtable();
    let raw = wallpaper.as_raw();
    let mut count = 0;
    check("GetMonitorDevicePathCount", (table.GetMonitorDevicePathCount)(raw, &mut count), &log);
    assert!(count > 0, "The current install must expose a monitor");
    let mut position = DESKTOP_WALLPAPER_POSITION::default();
    check("GetPosition", (table.GetPosition)(raw, &mut position), &log);
    let mut color = COLORREF::default();
    check("GetBackgroundColor", (table.GetBackgroundColor)(raw, &mut color), &log);
    for index in 0..count {
        let mut monitor = WallpaperProbeString(PWSTR::null());
        check("GetMonitorDevicePathAt", (table.GetMonitorDevicePathAt)(raw, index, &mut monitor.0), &log);
        assert!(!monitor.0.is_null() && *monitor.0.0 != 0, "The monitor ID must be present");
        let id = PCWSTR(monitor.0.0);
        let mut rect = RECT::default();
        check("GetMonitorRECT", (table.GetMonitorRECT)(raw, id, &mut rect), &log);
        assert!(rect.right > rect.left && rect.bottom > rect.top, "The monitor must have positive area");
        let mut path = WallpaperProbeString(PWSTR::null());
        check("GetWallpaper", (table.GetWallpaper)(raw, id, &mut path.0), &log);
        println!("GetWallpaper: path_present={}", !path.0.is_null() && *path.0.0 != 0);
    }
    std::fs::remove_file(log).unwrap();
}

#[test]
#[ignore = "read-only COM probe; review activation dependencies before running on a shared profile"]
fn desktop_wallpaper_queries_are_accessible_on_a_private_desktop() {
    isolated_probe(concat!(module_path!(), "::desktop_wallpaper_queries_are_accessible_on_a_private_desktop"),
        run_desktop_wallpaper_query_probe, &[]);
}

#[windows::core::implement(IShellBrowser, ICommDlgBrowser)]
struct DesktopViewProbeBrowser {
    hwnd: HWND,
    active: std::rc::Rc<std::cell::RefCell<Option<IShellView>>>,
    files: std::rc::Rc<std::cell::RefCell<DesktopViewProbeFiles>>,
    browse_calls: std::rc::Rc<std::cell::Cell<u32>>,
    folder: PathBuf,
}

#[derive(Default)]
struct DesktopViewProbeFiles(Option<std::process::Child>);

impl Drop for DesktopViewProbeFiles {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn desktop_view_callback(name: &str, code: windows::core::HRESULT) -> windows::core::Result<()> {
    use std::io::Write;
    if let Ok(mut log) = std::fs::OpenOptions::new().create(true).append(true)
        .open(r"C:\sandbox\desktop-view-callbacks.txt") {
        let _ = writeln!(log, "{name}: HRESULT=0x{:08x}", code.0 as u32);
    }
    if code.0 == 0 { Ok(()) } else { Err(windows::core::Error::from_hresult(code)) }
}

fn desktop_view_diagnostic(message: &str) {
    use std::io::Write;
    if let Ok(mut log) = std::fs::OpenOptions::new().create(true).append(true)
        .open(r"C:\sandbox\desktop-view-checks.txt") {
        let _ = writeln!(log, "{message}");
    }
}

unsafe fn desktop_view_window_diagnostic(label: &str, hwnd: HWND) {
    let mut class = [0u16; 128];
    let length = GetClassNameW(hwnd, &mut class).max(0) as usize;
    let mut process = 0;
    let thread = GetWindowThreadProcessId(hwnd, Some(&mut process));
    desktop_view_diagnostic(&format!(
        "{label}: hwnd={hwnd:?}; parent={:?}; root={:?}; owner={:?}; style=0x{:08x}; valid={}; visible={}; enabled={}; class={}; pid={process}; tid={thread}",
        GetAncestor(hwnd, GA_PARENT), GetAncestor(hwnd, GA_ROOT), GetWindow(hwnd, GW_OWNER),
        GetWindowLongPtrW(hwnd, GWL_STYLE) as u32, IsWindow(hwnd).as_bool(), IsWindowVisible(hwnd).as_bool(),
        windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled(hwnd).as_bool(),
        String::from_utf16_lossy(&class[..length])));
}

unsafe fn desktop_view_files_windows(process: u32) -> Option<HWND> {
    unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> BOOL {
        let (expected, count, visible) = &mut *(data.0 as *mut (u32, usize, Option<HWND>));
        let mut process = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut process));
        if process == *expected {
            *count += 1;
            let mut title = [0u16; 512];
            let length = GetWindowTextW(hwnd, &mut title).max(0) as usize;
            desktop_view_diagnostic(&format!("Files window title={:?}", String::from_utf16_lossy(&title[..length])));
            desktop_view_window_diagnostic("Files top-level window", hwnd);
            if visible.is_none() && IsWindowVisible(hwnd).as_bool() { *visible = Some(hwnd); }
        }
        BOOL(1)
    }
    let mut data = (process, 0usize, None);
    let result = EnumWindows(Some(collect), LPARAM(&mut data as *mut _ as isize));
    desktop_view_diagnostic(&format!("Files top-level windows: pid={process}; count={}; enumeration={result:?}", data.1));
    data.2
}

impl windows::Win32::System::Ole::IOleWindow_Impl for DesktopViewProbeBrowser_Impl {
    fn GetWindow(&self) -> windows::core::Result<HWND> {
        desktop_view_callback("GetWindow", S_OK)?;
        Ok(self.hwnd)
    }
    fn ContextSensitiveHelp(&self, _: BOOL) -> windows::core::Result<()> {
        desktop_view_callback("ContextSensitiveHelp", E_NOTIMPL)
    }
}

impl ICommDlgBrowser_Impl for DesktopViewProbeBrowser_Impl {
    fn OnDefaultCommand(&self, view: Option<&IShellView>) -> windows::core::Result<()> {
        unsafe {
            desktop_view_callback("OnDefaultCommand (entered)", S_OK)?;
            let folder: IFolderView = view.ok_or_else(|| windows::core::Error::from_hresult(E_POINTER))?.cast()?;
            let selection: IShellItemArray = folder.Items(SVGIO_SELECTION)?;
            let count = selection.GetCount()?;
            desktop_view_diagnostic(&format!("OnDefaultCommand: selection_count={count}"));
            if count != 1 {
                return desktop_view_callback("OnDefaultCommand (selection not handled)", S_FALSE);
            }
            let item = selection.GetItemAt(0)?;
            if item.GetAttributes(SFGAO_FOLDER)? & SFGAO_FOLDER != SFGAO_FOLDER {
                return desktop_view_callback("OnDefaultCommand (non-folder not handled)", S_FALSE);
            }
            let path = match item.GetDisplayName(SIGDN_FILESYSPATH) {
                Ok(path) => PathBuf::from(WallpaperProbeString(path).0.to_string()?),
                Err(_) => return desktop_view_callback("OnDefaultCommand (virtual folder not handled)", S_FALSE),
            };
            let pidl = SHGetIDListFromObject(&item)?;
            let pidl_bytes = std::slice::from_raw_parts(pidl as *const u8, ILGetSize(Some(pidl)) as usize).to_vec();
            windows::Win32::System::Com::CoTaskMemFree(Some(pidl as *const _));
            desktop_view_diagnostic(&format!("OnDefaultCommand: selected_path={}; absolute_pidl={pidl_bytes:02x?}", path.display()));
            if path != self.folder || !path.is_dir() {
                return desktop_view_callback("OnDefaultCommand (non-fixture folder not handled)", S_FALSE);
            }
            if self.files.borrow().0.is_some() {
                return desktop_view_callback("OnDefaultCommand (duplicate activation)", E_FAIL);
            }
            std::fs::write(r"C:\sandbox\desktop-view-files-handoff.json",
                serde_json::json!({ "callback": "ICommDlgBrowser::OnDefaultCommand", "selection_count": count,
                    "absolute_pidl": pidl_bytes, "path": path, "activation": "selected-folder Enter" }).to_string())
                .map_err(|_| windows::core::Error::from_hresult(E_FAIL))?;
            let child = std::process::Command::new(r"C:\probe\winarchy-files.exe")
                .arg(&path)
                .env("WINARCHY_FILES_THEME", r"C:\probe\files-theme.json")
                .env("WINARCHY_FILES_SETTINGS", r"C:\sandbox\files-settings.json")
                .spawn().map_err(|_| windows::core::Error::from_hresult(E_FAIL))?;
            desktop_view_diagnostic(&format!("Files spawned: pid={}", child.id()));
            self.files.borrow_mut().0 = Some(child);
            desktop_view_callback("OnDefaultCommand (Files handoff)", S_OK)
        }
    }
    fn OnStateChange(&self, _: Option<&IShellView>, change: u32) -> windows::core::Result<()> {
        desktop_view_diagnostic(&format!("OnStateChange: change={change}"));
        desktop_view_callback("OnStateChange", S_OK)
    }
    fn IncludeObject(&self, _: Option<&IShellView>, _: *const windows::Win32::UI::Shell::Common::ITEMIDLIST) -> windows::core::Result<()> {
        desktop_view_callback("IncludeObject (all Desktop items)", S_OK)
    }
}

impl IShellBrowser_Impl for DesktopViewProbeBrowser_Impl {
    fn InsertMenusSB(&self, _: HMENU, widths: *mut windows::Win32::System::Ole::OLEMENUGROUPWIDTHS) -> windows::core::Result<()> {
        if !widths.is_null() { unsafe { *widths = Default::default(); } }
        desktop_view_callback("InsertMenusSB (no host menus)", S_OK)
    }
    fn SetMenuSB(&self, _: HMENU, _: isize, _: HWND) -> windows::core::Result<()> { desktop_view_callback("SetMenuSB (no host menus)", S_OK) }
    fn RemoveMenusSB(&self, _: HMENU) -> windows::core::Result<()> { desktop_view_callback("RemoveMenusSB", S_OK) }
    fn SetStatusTextSB(&self, _: &PCWSTR) -> windows::core::Result<()> { desktop_view_callback("SetStatusTextSB (no status bar)", S_OK) }
    fn EnableModelessSB(&self, _: BOOL) -> windows::core::Result<()> { desktop_view_callback("EnableModelessSB", S_OK) }
    fn TranslateAcceleratorSB(&self, _: *const MSG, _: u16) -> windows::core::Result<()> { desktop_view_callback("TranslateAcceleratorSB (unhandled)", S_FALSE) }
    fn BrowseObject(&self, pidl: *const windows::Win32::UI::Shell::Common::ITEMIDLIST, flags: u32) -> windows::core::Result<()> {
        unsafe {
            self.browse_calls.set(self.browse_calls.get() + 1);
            desktop_view_diagnostic(&format!("BrowseObject: incoming_flags=0x{flags:08x}"));
            if pidl.is_null() || flags & (SBSP_PARENT | SBSP_NAVIGATEBACK | SBSP_NAVIGATEFORWARD) != 0 {
                return desktop_view_callback("BrowseObject (unsupported navigation)", E_INVALIDARG);
            }
            let item: IShellItem = if flags & SBSP_RELATIVE != 0 {
                SHCreateItemWithParent(None, &SHGetDesktopFolder()?, pidl)?
            } else {
                SHCreateItemFromIDList(pidl)?
            };
            let path = WallpaperProbeString(item.GetDisplayName(SIGDN_FILESYSPATH)?);
            let path = PathBuf::from(path.0.to_string()?);
            desktop_view_diagnostic(&format!("BrowseObject: flags=0x{flags:08x}; resolved_path={}", path.display()));
            std::fs::write(r"C:\sandbox\desktop-view-files-handoff.json",
                serde_json::json!({ "flags": flags, "path": path, "activation": "selected-folder Enter" }).to_string())
                .map_err(|_| windows::core::Error::from_hresult(E_FAIL))?;
            if path != self.folder || !path.is_dir() || self.files.borrow().0.is_some() {
                return desktop_view_callback("BrowseObject (unexpected target or duplicate activation)", E_INVALIDARG);
            }
            let child = std::process::Command::new(r"C:\probe\winarchy-files.exe")
                .arg(&path)
                .env("WINARCHY_FILES_THEME", r"C:\probe\files-theme.json")
                .env("WINARCHY_FILES_SETTINGS", r"C:\sandbox\files-settings.json")
                .spawn().map_err(|_| windows::core::Error::from_hresult(E_FAIL))?;
            desktop_view_diagnostic(&format!("Files spawned: pid={}", child.id()));
            self.files.borrow_mut().0 = Some(child);
            desktop_view_callback("BrowseObject (Files handoff)", S_OK)
        }
    }
    fn GetViewStateStream(&self, _: u32) -> windows::core::Result<windows::Win32::System::Com::IStream> {
        desktop_view_callback("GetViewStateStream (no persistence)", E_NOTIMPL)?;
        unreachable!()
    }
    fn GetControlWindow(&self, _: u32) -> windows::core::Result<HWND> {
        desktop_view_callback("GetControlWindow (no controls)", E_NOTIMPL)?;
        unreachable!()
    }
    fn SendControlMsg(&self, _: u32, _: u32, _: WPARAM, _: LPARAM, _: *mut LRESULT) -> windows::core::Result<()> { desktop_view_callback("SendControlMsg (no controls)", E_NOTIMPL) }
    fn QueryActiveShellView(&self) -> windows::core::Result<IShellView> {
        let view = self.active.borrow().clone();
        desktop_view_callback("QueryActiveShellView", if view.is_some() { S_OK } else { E_FAIL })?;
        Ok(view.unwrap())
    }
    fn OnViewWindowActive(&self, _: Option<&IShellView>) -> windows::core::Result<()> { desktop_view_callback("OnViewWindowActive", S_OK) }
    fn SetToolbarItems(&self, _: *const windows::Win32::UI::Controls::TBBUTTON, _: u32, _: u32) -> windows::core::Result<()> { desktop_view_callback("SetToolbarItems (no toolbar)", S_OK) }
}

struct DesktopViewProbeWindow {
    view: IShellView,
    active: std::rc::Rc<std::cell::RefCell<Option<IShellView>>>,
    created: bool,
}

impl DesktopViewProbeWindow {
    unsafe fn destroy(&mut self) -> windows::core::Result<()> {
        self.active.borrow_mut().take();
        if self.created {
            let deactivate = self.view.UIActivate(SVUIA_DEACTIVATE.0 as u32);
            let destroy = self.view.DestroyViewWindow();
            self.created = false;
            for (name, result) in [("UIActivate(DEACTIVATE)", &deactivate), ("DestroyViewWindow", &destroy)] {
                let code = result.as_ref().err().map_or(S_OK, |error| error.code());
                let _ = desktop_view_callback(name, code);
            }
            deactivate.and(destroy)?;
        }
        Ok(())
    }
}

impl Drop for DesktopViewProbeWindow {
    fn drop(&mut self) { unsafe { let _ = self.destroy(); } }
}

unsafe fn wait_for_desktop_item(view: &IShellView, expected: &std::path::Path, timeout: std::time::Duration) -> (bool, Option<u32>) {
    use windows::core::Interface;
    let folder: IFolderView = view.cast().unwrap();
    let deadline = std::time::Instant::now() + timeout;
    let mut query_error = None;
    while std::time::Instant::now() < deadline {
        let mut message = MSG::default();
        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        match folder.Items::<IShellItemArray>(SVGIO_ALLVIEW) {
            Ok(items) => match items.GetCount() {
                Ok(count) => for index in 0..count {
                    if let Ok(item) = items.GetItemAt(index) {
                        if let Ok(path) = item.GetDisplayName(SIGDN_FILESYSPATH) {
                            let path = WallpaperProbeString(path);
                            if PathBuf::from(path.0.to_string().unwrap()) == expected { return (true, query_error); }
                        }
                    }
                },
                Err(error) => query_error = Some(error.code().0 as u32),
            },
            Err(error) => query_error = Some(error.code().0 as u32),
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    (false, query_error)
}

struct DesktopViewProbeMenu {
    owner: HWND,
    menu: HMENU,
    directory: PathBuf,
    deadline: std::time::Instant,
    visible: bool,
    capture_acknowledged: bool,
    navigation_sent: bool,
    initial_highlighted: Option<u32>,
    highlighted: Option<u32>,
    cancelled: bool,
}

unsafe extern "system" fn desktop_view_menu_timer(_: HWND, _: u32, id: usize, _: u32) {
    let state = &mut *(id as *mut DesktopViewProbeMenu);
    let result = (|| -> Result<(), String> {
        if std::time::Instant::now() >= state.deadline {
            return Err("Native menu visibility/capture/navigation timed out".into());
        }
        if !state.visible {
            let mut previous = HWND::default();
            loop {
                let Ok(window) = FindWindowExW(None, previous, w!("#32768"), PCWSTR::null()) else { return Ok(()); };
                previous = window;
                if GetWindowThreadProcessId(window, None) == GetCurrentThreadId() && IsWindowVisible(window).as_bool() {
                    let mut bounds = RECT::default();
                    GetWindowRect(window, &mut bounds).map_err(|error| error.to_string())?;
                    if bounds.right <= bounds.left || bounds.bottom <= bounds.top { continue; }
                    state.visible = true;
                    std::fs::write(state.directory.join("desktop-view-menu-ready.json"), serde_json::json!({
                        "phase": "menu", "menu_hwnd": window.0 as usize, "menu_visible": true,
                        "menu_bounds": [bounds.left, bounds.top, bounds.right, bounds.bottom],
                        "points": [], "skip_pixel_comparison": true
                    }).to_string()).map_err(|error| error.to_string())?;
                    break;
                }
            }
        }
        let ack = state.directory.join("desktop-view-menu-captured.txt");
        if !ack.is_file() { return Ok(()); }
        let capture: serde_json::Value = match std::fs::read(&ack).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()) {
            Some(capture) => capture,
            None => return Ok(()),
        };
        if capture["menu_visible"] != true { return Err("Screenshot helper did not observe a visible menu".into()); }
        let png = std::fs::read(state.directory.join("desktop-view-menu.png")).map_err(|error| error.to_string())?;
        if !png.starts_with(b"\x89PNG\r\n\x1a\n") { return Err("Native menu capture must be PNG".into()); }
        state.capture_acknowledged = true;
        if !state.navigation_sent {
            state.initial_highlighted = (0..GetMenuItemCount(state.menu) as u32)
                .find(|index| GetMenuState(state.menu, *index, MF_BYPOSITION) & MF_HILITE.0 != 0);
            PostMessageW(state.owner, WM_KEYDOWN, WPARAM(0x28), LPARAM(0)).map_err(|error| error.to_string())?;
            state.navigation_sent = true;
            return Ok(());
        }
        state.highlighted = (0..GetMenuItemCount(state.menu) as u32)
            .find(|index| GetMenuState(state.menu, *index, MF_BYPOSITION) & MF_HILITE.0 != 0);
        if state.highlighted.is_some() && state.highlighted != state.initial_highlighted {
            EndMenu().map_err(|error| error.to_string())?;
            state.cancelled = true;
        }
        Ok(())
    })();
    if let Err(error) = result {
        desktop_view_diagnostic(&error);
        // A failed capture must never be followed by a synthetic menu action.
        let _ = TerminateProcess(GetCurrentProcess(), 1);
    }
}

struct DesktopViewProbeBackground {
    context_menu: Option<IContextMenu>,
    color: u32,
    image_active: bool,
    image: Option<DesktopViewProbeImage>,
}

struct DesktopViewProbeImage {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    _picture: windows::Win32::System::Ole::IPicture,
}

impl Drop for DesktopViewProbeImage {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.previous);
            let _ = DeleteObject(self.bitmap);
            let _ = DeleteDC(self.dc);
        }
    }
}

unsafe fn run_native_desktop_view_probe() {
    use windows::core::Interface;
    use windows::Win32::Security::*;
    use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows::Win32::System::RemoteDesktop::*;

    let directory = PathBuf::from(r"C:\sandbox");
    let nonce = std::env::var("WINARCHY_DESKTOP_VIEW_GUEST_NONCE")
        .expect("Guest-only: supply a disposable guest marker and nonce before running");
    assert!(nonce.len() >= 32 && nonce.bytes().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(std::fs::read_to_string(directory.join("desktop-view-guest.nonce")).unwrap().trim(), nonce);

    let mut token = HANDLE::default();
    OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).unwrap();
    let mut length = 0;
    let mut elevation = TOKEN_ELEVATION::default();
    GetTokenInformation(token, TokenElevation, Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),
        std::mem::size_of_val(&elevation) as u32, &mut length).unwrap();
    let mut session = 0u32;
    GetTokenInformation(token, TokenSessionId, Some((&mut session as *mut u32).cast()),
        std::mem::size_of_val(&session) as u32, &mut length).unwrap();
    let _ = GetTokenInformation(token, TokenUser, None, 0, &mut length);
    let mut user = vec![0usize; (length as usize).div_ceil(std::mem::size_of::<usize>())];
    GetTokenInformation(token, TokenUser, Some(user.as_mut_ptr().cast()), length, &mut length).unwrap();
    let mut sid = PWSTR::null();
    ConvertSidToStringSidW((*(user.as_ptr() as *const TOKEN_USER)).User.Sid, &mut sid).unwrap();
    let sid_text = sid.to_string().unwrap();
    let _ = LocalFree(HLOCAL(sid.0.cast()));
    CloseHandle(token).unwrap();
    let current_desktop = desktop_name(GetThreadDesktop(GetCurrentThreadId()).unwrap());
    let identity = format!("TokenElevation={}; SID={sid_text}; SessionId={session}; desktop={current_desktop}",
        elevation.TokenIsElevated);
    println!("{identity}");
    std::fs::write(directory.join("desktop-view-identity.txt"), &identity).unwrap();
    assert_eq!(elevation.TokenIsElevated, 0, "INCONCLUSIVE: an elevated guest cannot prove user-mode parity");
    assert_ne!(session, 0, "The probe requires an interactive guest session");
    let input = OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS).unwrap();
    let input_name = desktop_name(input);
    CloseDesktop(input).unwrap();
    assert_eq!(current_desktop, input_name, "The guest must be able to see the probe");

    unsafe fn explorer_running(session: u32) -> bool {
        let mut processes = std::ptr::null_mut();
        let mut count = 0;
        WTSEnumerateProcessesW(HANDLE::default(), 0, 1, &mut processes, &mut count).unwrap();
        let found = std::slice::from_raw_parts(processes, count as usize).iter().any(|process|
            process.SessionId == session && !process.pProcessName.is_null()
                && process.pProcessName.to_string().unwrap().eq_ignore_ascii_case("explorer.exe"));
        WTSFreeMemory(processes.cast());
        found
    }
    assert_eq!(explorer_running(session), false, "The guest must start without Explorer");
    let progress = directory.join("desktop-view-progress.txt");
    std::fs::write(&progress, "guest-validated\n").unwrap();

    unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        if matches!(message, WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR) {
            use windows::core::Interface;
            let background = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const DesktopViewProbeBackground;
            if let Some(context) = background.as_ref().and_then(|background| background.context_menu.as_ref()) {
                if let Ok(context) = context.cast::<IContextMenu3>() {
                    let mut result = LRESULT(0);
                    if context.HandleMenuMsg2(message, wp, lp, Some(&mut result)).is_ok() { return result; }
                } else if let Ok(context) = context.cast::<IContextMenu2>() {
                    if context.HandleMenuMsg(message, wp, lp).is_ok() { return LRESULT(0); }
                }
            }
        }
        if message == WM_ERASEBKGND {
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            let background = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const DesktopViewProbeBackground;
            if !background.is_null() {
                let background = &*background;
                if background.image_active {
                    if let Some(image) = &background.image {
                        let _ = BitBlt(HDC(wp.0 as *mut _), 0, 0, rect.right, rect.bottom,
                            image.dc, 0, 0, SRCCOPY);
                        return LRESULT(1);
                    }
                }
                let brush = CreateSolidBrush(COLORREF(background.color));
                FillRect(HDC(wp.0 as *mut _), &rect, brush);
                let _ = DeleteObject(brush);
                return LRESULT(1);
            }
        }
        DefWindowProcW(hwnd, message, wp, lp)
    }

    CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
    let _apartment = WallpaperProbeApartment;
    let instance = GetModuleHandleW(PCWSTR::null()).unwrap();
    let class = WNDCLASSW { lpfnWndProc: Some(window_proc), hInstance: instance.into(),
        lpszClassName: w!("WinarchyDesktopViewProbe"), ..Default::default() };
    assert_ne!(RegisterClassW(&class), 0);
    let mut background = Box::new(DesktopViewProbeBackground {
        context_menu: None, color: 0x0000ff00, image_active: false, image: None,
    });
    let host = DesktopPaintWindow {
        hwnd: CreateWindowExW(WINDOW_EX_STYLE(0), class.lpszClassName,
            w!("Winarchy guest Desktop view probe"), WS_OVERLAPPEDWINDOW | WS_VISIBLE | WS_CLIPCHILDREN,
            80, 80, 800, 600, None, None, instance, None).unwrap(),
        dc: HDC::default(),
    };
    let desktop = SHGetDesktopFolder().unwrap();
    let desktop_path = WallpaperProbeString(SHGetKnownFolderPath(&FOLDERID_Desktop, KF_FLAG_DEFAULT, None).unwrap());
    let desktop_path = PathBuf::from(desktop_path.0.to_string().unwrap());
    assert_eq!(std::fs::read(desktop_path.join("WinarchyDesktopProbe.txt")).unwrap(), b"guest desktop fixture\n");
    assert!(desktop_path.join("WinarchyDesktopProbeFolder").is_dir());
    let view: IShellView = desktop.CreateViewObject(host.hwnd)
        .unwrap_or_else(|error| panic!("CreateViewObject(IShellView): HRESULT=0x{:08x}", error.code().0 as u32));
    let active = std::rc::Rc::new(std::cell::RefCell::new(Some(view.clone())));
    let files = std::rc::Rc::new(std::cell::RefCell::new(DesktopViewProbeFiles::default()));
    let browse_calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let browser: IShellBrowser = DesktopViewProbeBrowser {
        hwnd: host.hwnd, active: active.clone(), files: files.clone(), browse_calls: browse_calls.clone(),
        folder: desktop_path.join("WinarchyDesktopProbeFolder"),
    }.into();
    let mut view_window = DesktopViewProbeWindow { view: view.clone(), active, created: false };
    let settings = FOLDERSETTINGS { ViewMode: FVM_ICON.0 as u32,
        fFlags: (FWF_DESKTOP | FWF_TRANSPARENT).0 as u32 };
    let mut rect = RECT::default();
    GetClientRect(host.hwnd, &mut rect).unwrap();
    SetWindowLongPtrW(host.hwnd, GWLP_USERDATA, (&mut *background as *mut DesktopViewProbeBackground) as isize);
    let mut wallpaper = serde_json::json!({
        "status": "INCONCLUSIVE", "source": "SPI_GETDESKWALLPAPER in Rust guest process",
        "identity": identity, "account": std::env::var("USERNAME").unwrap_or_default(),
        "session": session, "path": "", "bytes": null, "sha256": null,
        "spi_hresult": null, "load_hresult": null, "render_hresult": null,
        "dpi_context": format!("{:?}", windows::Win32::UI::HiDpi::GetThreadDpiAwarenessContext()),
        "window_dpi": windows::Win32::UI::HiDpi::GetDpiForWindow(host.hwnd),
        "host_client": [rect.left, rect.top, rect.right, rect.bottom],
    });
    let mut host_bounds = RECT::default();
    GetWindowRect(host.hwnd, &mut host_bounds).unwrap();
    wallpaper["host_window"] = serde_json::json!([host_bounds.left, host_bounds.top, host_bounds.right, host_bounds.bottom]);
    let monitor = MonitorFromWindow(host.hwnd, MONITOR_DEFAULTTONEAREST);
    let mut monitor_info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    assert!(GetMonitorInfoW(monitor, &mut monitor_info).as_bool());
    wallpaper["monitor"] = serde_json::json!([monitor_info.rcMonitor.left, monitor_info.rcMonitor.top,
        monitor_info.rcMonitor.right, monitor_info.rcMonitor.bottom]);
    let mut path_buffer = [0u16; 32768];
    let spi = SystemParametersInfoW(SPI_GETDESKWALLPAPER, path_buffer.len() as u32,
        Some(path_buffer.as_mut_ptr().cast()), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
    wallpaper["spi_hresult"] = serde_json::json!(spi.as_ref().err().map_or(0, |error| error.code().0));
    let path_end = path_buffer.iter().position(|unit| *unit == 0).unwrap_or(path_buffer.len());
    let image_path = PathBuf::from(std::ffi::OsString::from_wide(&path_buffer[..path_end]));
    wallpaper["path"] = serde_json::json!(image_path);
    wallpaper["spi_buffer_utf16_units"] = serde_json::json!(path_buffer.len());
    let image_bytes = if spi.is_ok() && path_end > 0 && path_end < path_buffer.len() {
        match std::fs::read(&image_path) {
            Ok(bytes) => Some(bytes),
            Err(error) => {
                wallpaper["read_error"] = serde_json::json!(error.to_string());
                None
            }
        }
    } else { None };
    if let Some(bytes) = image_bytes {
        wallpaper["bytes"] = serde_json::json!(bytes.len());
        use windows::Win32::System::Ole::{IPicture, OleLoadPicturePath};
        let mut picture_raw = std::ptr::null_mut();
        let loaded = OleLoadPicturePath(PCWSTR(path_buffer.as_ptr()), None::<&windows::core::IUnknown>,
            0, 0, &IPicture::IID, &mut picture_raw);
        wallpaper["load_hresult"] = serde_json::json!(loaded.as_ref().err().map_or(0, |error| error.code().0));
        if loaded.is_ok() {
            let picture = IPicture::from_raw(picture_raw);
            let width = picture.Width();
            let height = picture.Height();
            wallpaper["dimensions_himetric"] = serde_json::json!([width.as_ref().ok(), height.as_ref().ok()]);
            wallpaper["dimensions_hresult"] = serde_json::json!([
                width.as_ref().err().map_or(0, |error| error.code().0),
                height.as_ref().err().map_or(0, |error| error.code().0)]);
            if let (Ok(width), Ok(height)) = (width, height) {
                if width > 0 && height > 0 {
                    let screen_dc = GetDC(host.hwnd);
                    let dc = CreateCompatibleDC(screen_dc);
                    let bitmap = CreateCompatibleBitmap(screen_dc, rect.right, rect.bottom);
                    let previous = SelectObject(dc, bitmap);
                    wallpaper["dimensions_pixels_at_dc_dpi"] = serde_json::json!([
                        i64::from(width) * i64::from(GetDeviceCaps(screen_dc, LOGPIXELSX)) / 2540,
                        i64::from(height) * i64::from(GetDeviceCaps(screen_dc, LOGPIXELSY)) / 2540]);
                    ReleaseDC(host.hwnd, screen_dc);
                    let image = DesktopViewProbeImage { dc, bitmap, previous, _picture: picture };
                    if !dc.is_invalid() && !bitmap.is_invalid() {
                        let painted = PaintDesktop(dc).as_bool();
                        wallpaper["paintdesktop_control"] = serde_json::json!({
                            "success": painted, "colorref": GetPixel(dc, rect.right / 2, rect.bottom / 2).0,
                            "meaning": "PaintDesktop control only; black does not establish SPI image rendering"
                        });
                        let rendered = image._picture.Render(dc, 0, 0, rect.right, rect.bottom,
                            0, height, width, -height, std::ptr::null());
                        wallpaper["render_hresult"] = serde_json::json!(rendered.as_ref().err().map_or(0, |error| error.code().0));
                        if rendered.is_ok() {
                            wallpaper["status"] = serde_json::json!("READY");
                            background.image = Some(image);
                        }
                    }
                }
            }
        }
    }
    if background.image.is_none() {
        wallpaper["reason"] = serde_json::json!("Guest SPI path empty/unreadable, image decode unavailable, or render failed; no substitute image used");
    }
    std::fs::write(directory.join("desktop-view-wallpaper.json"), wallpaper.to_string()).unwrap();
    let created_hwnd = view.CreateViewWindow(None, &settings, &browser, &rect)
        .unwrap_or_else(|error| panic!("CreateViewWindow: HRESULT=0x{:08x}", error.code().0 as u32));
    view_window.created = true;
    desktop_view_window_diagnostic("host after CreateViewWindow", host.hwnd);
    desktop_view_window_diagnostic("CreateViewWindow output", created_hwnd);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut activated = false;
    let context_menu_commands;
    let refresh_api_visible;
    let refresh_f5_visible;
    loop {
        assert!(std::time::Instant::now() < deadline, "The visible Desktop view did not expose both guest fixtures");
        let mut message = MSG::default();
        while std::time::Instant::now() < deadline && PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
            assert_ne!(message.message, WM_QUIT, "The guest closed the probe");
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        {
            if !activated {
                view.UIActivate(SVUIA_ACTIVATE_FOCUS.0 as u32).unwrap();
                view.Refresh().unwrap();
                activated = true;
            }
            let folder: IFolderView = view.cast().unwrap();
            let folder2: IFolderView2 = view.cast().unwrap();
            let folder_flags = folder2.GetCurrentFolderFlags().unwrap();
            std::fs::write(directory.join("desktop-view-flags.txt"), format!("folder_flags={folder_flags:08x}; transparent={}\n",
                folder_flags & FWF_TRANSPARENT.0 as u32 != 0)).unwrap();
            if let Ok(items) = folder.Items::<IShellItemArray>(SVGIO_ALLVIEW) {
                let mut fixtures = Vec::new();
                let mut fixture_indices = Vec::new();
                for index in 0..items.GetCount().unwrap() {
                    assert!(std::time::Instant::now() < deadline, "Desktop item enumeration timed out");
                    let item = items.GetItemAt(index).unwrap();
                    if let Ok(path) = item.GetDisplayName(SIGDN_FILESYSPATH) {
                        let path = WallpaperProbeString(path);
                        let path = PathBuf::from(path.0.to_string().unwrap());
                        if path.parent() == Some(desktop_path.as_path())
                            && path.file_name().unwrap().to_string_lossy().starts_with("WinarchyDesktopProbe") {
                            let is_folder = item.GetAttributes(windows::Win32::System::SystemServices::SFGAO_FOLDER).unwrap().0 != 0;
                            let name = path.file_name().unwrap().to_string_lossy().into_owned();
                            fixtures.push((name.clone(), is_folder));
                            fixture_indices.push((name, index as i32));
                        }
                    }
                }
                fixtures.sort();
                if fixtures.len() >= 2 {
                    std::fs::write(&progress, "guest-validated\nfixtures-enumerated\n").unwrap();
                    desktop_view_diagnostic(&format!("fixtures: expected=file(false),folder(true); observed={fixtures:?}"));
                    assert_eq!(fixtures, [("WinarchyDesktopProbe.txt".to_string(), false),
                        ("WinarchyDesktopProbeFolder".to_string(), true)]);
                    desktop_view_diagnostic("IShellView::GetWindow: begin");
                    let window = view.GetWindow();
                    desktop_view_diagnostic(&format!("IShellView::GetWindow: expected=success; observed={window:?}; CreateViewWindow={created_hwnd:?}"));
                    let hwnd = window.unwrap();
                    desktop_view_window_diagnostic("host before assertions", host.hwnd);
                    desktop_view_window_diagnostic("view before assertions", hwnd);
                    let focus = windows::Win32::UI::Input::KeyboardAndMouse::GetFocus();
                    desktop_view_window_diagnostic("focus before assertions", focus);
                    let visible = IsWindowVisible(hwnd).as_bool();
                    desktop_view_diagnostic(&format!("IsWindowVisible: expected=true; observed={visible}"));
                    assert_eq!(visible, true, "The Desktop view must be visible");
                    let enabled = windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled(hwnd).as_bool();
                    desktop_view_diagnostic(&format!("IsWindowEnabled: expected=true; observed={enabled}"));
                    assert_eq!(enabled, true, "The Desktop view must be enabled");
                    let child = IsChild(host.hwnd, hwnd).as_bool();
                    desktop_view_diagnostic(&format!("IsChild: expected=true; observed={child}; expected_ancestor={:?}; observed_view={hwnd:?}; immediate_parent={:?}",
                        host.hwnd, GetAncestor(hwnd, GA_PARENT)));
                    assert_eq!(child, true, "The Desktop view must be hosted beneath the probe window");
                    desktop_view_diagnostic("focus handoff: locating SysListView32 beneath the view");
                    let list = FindWindowExW(hwnd, None, w!("SysListView32"), PCWSTR::null()).unwrap();
                    desktop_view_window_diagnostic("focus handoff target", list);
                    let handoff = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(list);
                    desktop_view_diagnostic(&format!("SetFocus: target={list:?}; previous_focus={focus:?}; returned_previous={handoff:?}"));
                    let focus = windows::Win32::UI::Input::KeyboardAndMouse::GetFocus();
                    desktop_view_window_diagnostic("focus after handoff", focus);
                    assert_eq!(focus, list, "The host must hand keyboard focus to the native list control");
                    let focused = focus == hwnd || IsChild(hwnd, focus).as_bool();
                    desktop_view_diagnostic(&format!("GetFocus: expected=view-or-descendant; observed_match={focused}; view={hwnd:?}; focus={focus:?}"));
                    assert!(focused, "The Desktop view must receive focus");
                    let mut bounds = RECT::default();
                    desktop_view_diagnostic("GetWindowRect: begin");
                    let bounds_result = GetWindowRect(hwnd, &mut bounds);
                    desktop_view_diagnostic(&format!("GetWindowRect: expected=success,positive-area; observed={bounds_result:?}; bounds={bounds:?}"));
                    bounds_result.unwrap();
                    assert!(bounds.right > bounds.left && bounds.bottom > bounds.top, "The Desktop view must have positive bounds");
                    desktop_view_diagnostic("ItemCount/GetCount: begin");
                    let folder_count = folder.ItemCount(SVGIO_ALLVIEW);
                    let array_count = items.GetCount();
                    desktop_view_diagnostic(&format!("ItemCount/GetCount: expected=equal; folder={folder_count:?}; array={array_count:?}"));
                    assert_eq!(folder_count.unwrap(), array_count.unwrap() as i32);
                    std::fs::write(&progress, "guest-validated\nfixtures-enumerated\nview-focused\n").unwrap();
                    let file_index = fixture_indices.iter().find(|(name, _)| name == "WinarchyDesktopProbe.txt").unwrap().1;
                    folder.SelectItem(file_index, (SVSI_SELECT.0 | SVSI_FOCUSED.0 | SVSI_DESELECTOTHERS.0) as u32).unwrap();
                    assert_eq!(folder.GetFocusedItem().unwrap(), file_index);
                    let selected = folder.Items::<IShellItemArray>(SVGIO_SELECTION).unwrap();
                    assert_eq!(selected.GetCount().unwrap(), 1);
                    let selected_path = WallpaperProbeString(selected.GetItemAt(0).unwrap().GetDisplayName(SIGDN_FILESYSPATH).unwrap());
                    assert_eq!(PathBuf::from(selected_path.0.to_string().unwrap()), desktop_path.join("WinarchyDesktopProbe.txt"));
                    let folder_index = fixture_indices.iter().find(|(name, _)| name == "WinarchyDesktopProbeFolder").unwrap().1;
                    folder.SelectItem(folder_index, (SVSI_SELECT.0 | SVSI_FOCUSED.0) as u32).unwrap();
                    assert_eq!(folder.GetFocusedItem().unwrap(), folder_index);
                    let selected = folder.Items::<IShellItemArray>(SVGIO_SELECTION).unwrap();
                    assert_eq!(selected.GetCount().unwrap(), 2, "The file and folder must both remain selected");
                    let mut selected_paths: Vec<PathBuf> = (0..2).map(|index| {
                        let path = WallpaperProbeString(selected.GetItemAt(index).unwrap().GetDisplayName(SIGDN_FILESYSPATH).unwrap());
                        PathBuf::from(path.0.to_string().unwrap())
                    }).collect();
                    selected_paths.sort();
                    let expected_paths = [desktop_path.join("WinarchyDesktopProbe.txt"), desktop_path.join("WinarchyDesktopProbeFolder")];
                    assert_eq!(selected_paths, expected_paths, "Selection must contain exactly the fixture file and folder");
                    std::fs::write(directory.join("desktop-view-selection.json"),
                        serde_json::json!({ "selected_paths": selected_paths, "count": 2, "exact": true }).to_string()).unwrap();
                    let menu = CreatePopupMenu().unwrap();
                    let context_menu: IContextMenu = view.GetItemObject(SVGIO_SELECTION).unwrap();
                    context_menu.QueryContextMenu(menu, 0, 1, 0x7fff, 0).unwrap();
                    context_menu_commands = GetMenuItemCount(menu);
                    assert!(context_menu_commands > 0, "The selected Desktop file and folder must expose native menu commands");
                    background.context_menu = Some(context_menu);
                    let mut menu_state = Box::new(DesktopViewProbeMenu {
                        owner: host.hwnd, menu, directory: directory.clone(),
                        deadline: std::time::Instant::now() + std::time::Duration::from_secs(30),
                        visible: false, capture_acknowledged: false, navigation_sent: false,
                        initial_highlighted: None, highlighted: None, cancelled: false,
                    });
                    let timer_id = (&mut *menu_state as *mut DesktopViewProbeMenu) as usize;
                    assert_ne!(SetTimer(host.hwnd, timer_id, 50, Some(desktop_view_menu_timer)), 0);
                    let command = TrackPopupMenuEx(menu, (TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON).0,
                        bounds.left + 80, bounds.top + 80, host.hwnd, None).0;
                    KillTimer(host.hwnd, timer_id).unwrap();
                    background.context_menu = None;
                    DestroyMenu(menu).unwrap();
                    std::fs::write(directory.join("desktop-view-menu.json"), serde_json::json!({
                        "visible": menu_state.visible, "capture_acknowledged": menu_state.capture_acknowledged,
                        "navigation_key": "Down", "navigation_sent": menu_state.navigation_sent,
                        "initial_highlighted_index": menu_state.initial_highlighted,
                        "highlighted_index": menu_state.highlighted, "cancelled": menu_state.cancelled,
                        "returned_command": command, "command_invoked": false, "selection_count": 2
                    }).to_string()).unwrap();
                    assert!(menu_state.visible, "The native menu must actually become visible");
                    assert!(menu_state.capture_acknowledged, "Dismissal must follow screenshot acknowledgement");
                    assert!(menu_state.navigation_sent && menu_state.highlighted.is_some(), "Down must highlight a native menu item");
                    assert_ne!(menu_state.highlighted, menu_state.initial_highlighted, "Down must change the highlighted menu item");
                    assert!(menu_state.cancelled, "The timer must dismiss the menu with EndMenu");
                    assert_eq!(command, 0, "TrackPopupMenuEx must return cancellation, never a command");
                    std::fs::write(&progress, "guest-validated\nfixtures-enumerated\nview-focused\nfixture-selected\n").unwrap();
                    SendMessageW(focus, WM_KEYDOWN, WPARAM(0x24), LPARAM(0));
                    assert_eq!(folder.GetFocusedItem().unwrap(), 0, "Home must move keyboard focus to the first Desktop item");
                    std::fs::write(&progress, "guest-validated\nfixtures-enumerated\nview-focused\nfixture-selected\nkeyboard-home\n").unwrap();
                    folder.SelectItem(folder_index, (SVSI_SELECT.0 | SVSI_FOCUSED.0 | SVSI_DESELECTOTHERS.0) as u32).unwrap();
                    assert_eq!(folder.Items::<IShellItemArray>(SVGIO_SELECTION).unwrap().GetCount().unwrap(), 1);
                    assert_eq!(folder.GetFocusedItem().unwrap(), folder_index);
                    windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(focus).unwrap();
                    desktop_view_window_diagnostic("folder Enter focus", windows::Win32::UI::Input::KeyboardAndMouse::GetFocus());
                    let browse_calls_before = browse_calls.get();
                    for (message, lparam) in [(WM_KEYDOWN, 1), (WM_KEYUP, 0xc0000001u32 as isize)] {
                        let message = MSG { hwnd: focus, message, wParam: WPARAM(0x0d),
                            lParam: LPARAM(lparam), time: GetMessageTime() as u32, ..Default::default() };
                        // The Result wrapper discards S_FALSE, which determines whether the host dispatches.
                        let result = (view.vtable().TranslateAccelerator)(view.as_raw(), &message);
                        desktop_view_diagnostic(&format!("folder Enter IShellView::TranslateAccelerator: message=0x{:04x}; HRESULT=0x{:08x}; dispatch={}; BrowseObject_calls={}",
                            message.message, result.0 as u32, result != S_OK, browse_calls.get() - browse_calls_before));
                        if result != S_OK {
                            let _ = TranslateMessage(&message);
                            DispatchMessageW(&message);
                        }
                    }
                    let files_started = std::time::Instant::now();
                    let deadline = files_started + std::time::Duration::from_secs(20);
                    let mut last_child_status = None;
                    let (files_window, files_visible_at) = loop {
                        let mut message = MSG::default();
                        while std::time::Instant::now() < deadline && PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                            let _ = TranslateMessage(&message);
                            DispatchMessageW(&message);
                        }
                        if let Ok(window) = FindWindowW(PCWSTR::null(), w!("Files — Winarchy")) {
                            if IsWindowVisible(window).as_bool() && std::time::Instant::now() < deadline {
                                let visible_at = std::time::Instant::now();
                                desktop_view_diagnostic(&format!("folder Enter outcome: Files_visible=true; hwnd={window:?}; elapsed_ms={}; BrowseObject_calls={}; frame_readiness=unverified",
                                    visible_at.duration_since(files_started).as_millis(), browse_calls.get() - browse_calls_before));
                                break (window, visible_at);
                            }
                        }
                        let timed_out = std::time::Instant::now() >= deadline;
                        let mut diagnostic_window = None;
                        if let Some(child) = files.borrow_mut().0.as_mut() {
                            let status = match child.try_wait() {
                                Ok(Some(status)) => format!("exited: {status}"),
                                Ok(None) => "running".to_owned(),
                                Err(error) => format!("try_wait failed: {error}"),
                            };
                            if timed_out || last_child_status.as_ref() != Some(&status) {
                                desktop_view_diagnostic(&format!("Files child: pid={}; status={status}; timed_out={timed_out}", child.id()));
                                diagnostic_window = desktop_view_files_windows(child.id());
                                last_child_status = Some(status);
                            }
                        } else if timed_out {
                            desktop_view_diagnostic("Files child: no child was spawned");
                        }
                        if timed_out {
                            if let Some(window) = diagnostic_window {
                                desktop_view_diagnostic(&format!("Files timeout diagnostic capture: hwnd={window:?}; expected title missing"));
                                let _ = SetForegroundWindow(window);
                                let _ = windows::Win32::Graphics::Dwm::DwmFlush();
                                std::fs::write(directory.join("desktop-view-files-ready.json"), serde_json::json!({
                                    "phase": "files", "files_hwnd": window.0 as usize,
                                    "points": [], "skip_pixel_comparison": true,
                                    "diagnostic": "expected Files title missing",
                                }).to_string()).unwrap();
                                let capture_deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                                let captured = directory.join("desktop-view-files-captured.txt");
                                while !captured.is_file() && std::time::Instant::now() < capture_deadline {
                                    let mut message = MSG::default();
                                    while std::time::Instant::now() < capture_deadline && PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                                        let _ = TranslateMessage(&message);
                                        DispatchMessageW(&message);
                                    }
                                    std::thread::sleep(std::time::Duration::from_millis(20));
                                }
                                desktop_view_diagnostic(&format!("Files timeout diagnostic capture: acknowledged={}", captured.is_file()));
                            }
                            desktop_view_diagnostic(&format!("folder Enter outcome: Files_visible=false; BrowseObject_calls={}; INCONCLUSIVE: the ICommDlgBrowser default-command handoff did not produce a Files window", browse_calls.get() - browse_calls_before));
                            panic!("INCONCLUSIVE: folder Enter did not show Files within 20s; inspect accelerator HRESULTs and OnDefaultCommand/BrowseObject diagnostics");
                        }
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    };
                    let handoff: serde_json::Value = serde_json::from_slice(
                        &std::fs::read(directory.join("desktop-view-files-handoff.json")).unwrap()).unwrap();
                    assert_eq!(handoff["callback"], "ICommDlgBrowser::OnDefaultCommand");
                    assert_eq!(handoff["selection_count"], 1);
                    assert!(handoff["absolute_pidl"].as_array().unwrap().len() > 2, "The handoff must record a non-empty absolute PIDL");
                    assert_eq!(handoff["path"], desktop_path.join("WinarchyDesktopProbeFolder").to_str().unwrap());
                    let mut files_pid = 0;
                    GetWindowThreadProcessId(files_window, Some(&mut files_pid));
                    assert_eq!(Some(files_pid), files.borrow().0.as_ref().map(std::process::Child::id),
                        "The visible Files window must belong to the launched guest app");
                    let foreground = SetForegroundWindow(files_window).as_bool();
                    windows::Win32::Graphics::Dwm::DwmFlush().unwrap();
                    let settle_deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
                    while std::time::Instant::now() < settle_deadline {
                        let mut message = MSG::default();
                        while std::time::Instant::now() < settle_deadline && PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                            let _ = TranslateMessage(&message);
                            DispatchMessageW(&message);
                        }
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    }
                    windows::Win32::Graphics::Dwm::DwmFlush().unwrap();
                    let capture_requested_at = std::time::Instant::now();
                    desktop_view_diagnostic(&format!("Files delayed capture requested: hwnd={files_window:?}; foreground={foreground}; elapsed_ms={}; visible_for_ms={}; frame_readiness=unverified",
                        capture_requested_at.duration_since(files_started).as_millis(), capture_requested_at.duration_since(files_visible_at).as_millis()));
                    std::fs::write(directory.join("desktop-view-files-ready.json"), serde_json::json!({
                        "phase": "files", "files_hwnd": files_window.0 as usize,
                        "points": [], "skip_pixel_comparison": true,
                        "visible_elapsed_ms": files_visible_at.duration_since(files_started).as_millis(),
                        "capture_elapsed_ms": capture_requested_at.duration_since(files_started).as_millis(),
                        "diagnostic": "two-second message-pumped delay after DwmFlush; frame readiness unverified",
                    }).to_string()).unwrap();
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
                    while !directory.join("desktop-view-files-captured.txt").is_file() {
                        if std::time::Instant::now() >= deadline {
                            desktop_view_diagnostic(&format!("Files delayed capture: hwnd={files_window:?}; acknowledged=false; elapsed_ms={}", files_started.elapsed().as_millis()));
                        }
                        assert!(std::time::Instant::now() < deadline, "Missing Files screenshot acknowledgement");
                        let mut message = MSG::default();
                        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                            let _ = TranslateMessage(&message);
                            DispatchMessageW(&message);
                        }
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    }
                    desktop_view_diagnostic(&format!("Files delayed capture: hwnd={files_window:?}; acknowledged=true; elapsed_ms={}; request_to_ack_ms={}",
                        files_started.elapsed().as_millis(), capture_requested_at.elapsed().as_millis()));
                    assert!(std::fs::read(directory.join("desktop-view-files.png")).unwrap().starts_with(b"\x89PNG\r\n\x1a\n"));
                    PostMessageW(files_window, WM_CLOSE, WPARAM(0), LPARAM(0)).unwrap();
                    *files.borrow_mut() = DesktopViewProbeFiles::default();
                    let _ = SetForegroundWindow(host.hwnd);
                    windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(focus).unwrap();
                    let api_path = desktop_path.join("WinarchyDesktopProbeRefreshApi.txt");
                    assert!(!api_path.exists());
                    std::fs::write(&api_path, b"Refresh API fixture\n").unwrap();
                    view.Refresh().unwrap();
                    let (api_visible, api_error) = wait_for_desktop_item(&view, &api_path, std::time::Duration::from_secs(5));
                    refresh_api_visible = api_visible;
                    let f5_path = desktop_path.join("WinarchyDesktopProbeRefreshF5.txt");
                    assert!(!f5_path.exists());
                    std::fs::write(&f5_path, b"F5 fixture\n").unwrap();
                    SendMessageW(focus, WM_KEYDOWN, WPARAM(0x74), LPARAM(0));
                    SendMessageW(focus, WM_KEYUP, WPARAM(0x74), LPARAM(0));
                    let (f5_visible, f5_error) = wait_for_desktop_item(&view, &f5_path, std::time::Duration::from_secs(5));
                    refresh_f5_visible = f5_visible;
                    std::fs::write(directory.join("desktop-view-refresh.txt"), format!(
                        "refresh_api_visible={refresh_api_visible}; api_error={api_error:?}; refresh_f5_visible={refresh_f5_visible}; f5_error={f5_error:?}\n")).unwrap();
                    std::fs::write(&progress, format!("guest-validated\nfixtures-enumerated\nview-focused\nfixture-selected\nkeyboard-home\nrefresh_api={refresh_api_visible}\nrefresh_f5={refresh_f5_visible}\n")).unwrap();
                    assert!(refresh_api_visible, "IShellView::Refresh must expose the fresh fixture; api_error={api_error:?}");
                    assert!(refresh_f5_visible, "F5 must expose the fresh fixture; f5_error={f5_error:?}");
                    break;
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    use windows::Win32::UI::Controls::{LVHITTESTINFO, LVM_HITTEST, LVM_GETITEMRECT, LVM_GETITEMCOUNT};
    let child = view.GetWindow().unwrap();
    let list = FindWindowExW(child, None, w!("SysListView32"), PCWSTR::null()).unwrap();
    let mut client = RECT::default();
    GetClientRect(list, &mut client).unwrap();
    let count = SendMessageW(list, LVM_GETITEMCOUNT, WPARAM(0), LPARAM(0)).0;
    assert!(count >= 2);
    let mut empty_points = Vec::new();
    for y in (32..client.bottom - 32).rev().step_by(32) {
        for x in (32..client.right - 32).rev().step_by(32) {
            let point = POINT { x, y };
            let mut hit = LVHITTESTINFO { pt: point, ..Default::default() };
            if SendMessageW(list, LVM_HITTEST, WPARAM(0), LPARAM((&mut hit as *mut LVHITTESTINFO) as isize)).0 != -1 { continue; }
            let occupied = (0..count).any(|index| {
                let mut bounds = RECT::default();
                assert_ne!(SendMessageW(list, LVM_GETITEMRECT, WPARAM(index as usize),
                    LPARAM((&mut bounds as *mut RECT) as isize)).0, 0);
                x >= bounds.left - 16 && x <= bounds.right + 16 && y >= bounds.top - 16 && y <= bounds.bottom + 16
            });
            if !occupied { empty_points.push(point); }
        }
    }
    assert!(empty_points.len() >= 8, "The native list must expose multiple empty points away from every icon and label");
    let empty = empty_points[0];
    let mut screen = empty;
    assert!(ClientToScreen(list, &mut screen).as_bool());
    assert!(WindowFromPoint(screen) == list || IsChild(list, WindowFromPoint(screen)).as_bool());
    let mut host_point = screen;
    assert!(ScreenToClient(host.hwnd, &mut host_point).as_bool());
    let screen_points: Vec<POINT> = empty_points.iter().map(|point| {
        let mut point = *point;
        assert!(ClientToScreen(list, &mut point).as_bool());
        assert!(WindowFromPoint(point) == list || IsChild(list, WindowFromPoint(point)).as_bool());
        point
    }).collect();
    let host_points: Vec<POINT> = screen_points.iter().map(|point| {
        let mut point = *point;
        assert!(ScreenToClient(host.hwnd, &mut point).as_bool());
        point
    }).collect();
    wallpaper["empty_screen_points"] = serde_json::json!(screen_points.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>());
    let mut wallpaper_samples = Vec::new();
    let mut samples = Vec::new();
    for (phase, color) in [("green", 0x0000ff00u32), ("magenta", 0x00ff00ff),
        ("wallpaper-child", 0), ("wallpaper-parent", 0), ("parent", 0x00ff8000)] {
        let image_phase = phase.starts_with("wallpaper-");
        if image_phase && background.image.is_none() {
            std::fs::write(directory.join(format!("desktop-view-{phase}-ready.json")),
                serde_json::json!({ "phase": phase, "skip": true, "reason": wallpaper["reason"] }).to_string()).unwrap();
            continue;
        }
        if phase == "wallpaper-parent" || phase == "parent" {
            view_window.destroy().unwrap();
            assert!(!IsWindow(child).as_bool());
        } else {
            assert!(IsWindowVisible(child).as_bool());
            assert!(IsChild(host.hwnd, child).as_bool());
        }
        background.color = color;
        background.image_active = image_phase;
        let child_visible = view_window.created;
        assert!(RedrawWindow(host.hwnd, None, None, RDW_INVALIDATE | RDW_ERASE | RDW_UPDATENOW | RDW_ALLCHILDREN).as_bool());
        windows::Win32::Graphics::Dwm::DwmFlush().unwrap();
        let request = directory.join(format!("desktop-view-{phase}-ready.json"));
        let ack = directory.join(format!("desktop-view-{phase}-captured.txt"));
        std::fs::write(request, serde_json::json!({ "phase": phase, "x": screen.x, "y": screen.y,
            "list_x": empty.x, "list_y": empty.y, "host_x": host_point.x, "host_y": host_point.y,
            "expected_colorref": if image_phase { None } else { Some(color) }, "child_visible": child_visible,
            "points": screen_points.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>() }).to_string()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !ack.is_file() {
            assert!(std::time::Instant::now() < deadline, "Missing screenshot acknowledgement: {phase}");
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                assert_ne!(message.message, WM_QUIT);
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(std::fs::read(directory.join(format!("desktop-view-{phase}.png"))).unwrap()
            .starts_with(b"\x89PNG\r\n\x1a\n"), "Each guest capture must be PNG");
        if child_visible { assert!(IsWindowVisible(child).as_bool()); }
        let dc = GetDC(None);
        assert!(!dc.is_invalid());
        let pixel = GetPixel(dc, screen.x, screen.y);
        if image_phase {
            let pixels: Vec<u32> = screen_points.iter().map(|p| GetPixel(dc, p.x, p.y).0).collect();
            wallpaper_samples.push(pixels);
            if phase == "wallpaper-parent" {
                let image = background.image.as_ref().unwrap();
                let expected: Vec<u32> = host_points.iter().map(|p| GetPixel(image.dc, p.x, p.y).0).collect();
                wallpaper["expected_colorrefs"] = serde_json::json!(expected);
            }
            wallpaper["observed_colorrefs"] = serde_json::json!(wallpaper_samples);
            std::fs::write(directory.join("desktop-view-wallpaper.json"), wallpaper.to_string()).unwrap();
        }
        ReleaseDC(None, dc);
        if !image_phase { samples.push((phase, color, pixel.0)); }
        std::fs::write(directory.join("desktop-view-pixels.txt"), format!("{samples:?}\n")).unwrap();
    }
    std::fs::write(&progress, "direct-view-destroyed\nthree-phase-screenshots-captured\n").unwrap();
    assert_eq!(explorer_running(session), false, "The Desktop view must not start Explorer");
    assert_eq!(samples[2].2, 0x00ff8000, "Parent-only screen control must match the painted color");
    assert_eq!((samples[0].2, samples[1].2), (0x0000ff00, 0x00ff00ff),
        "The empty child point must follow both live parent colors; evidence retained even if opaque");
    if background.image.is_some() {
        let expected: Vec<u32> = host_points.iter().map(|p| GetPixel(background.image.as_ref().unwrap().dc, p.x, p.y).0).collect();
        let nonuniform = expected.iter().any(|color| *color != expected[0]);
        let matches = wallpaper_samples.len() == 2 && wallpaper_samples.iter().all(|pixels| *pixels == expected);
        wallpaper["nonuniform"] = serde_json::json!(nonuniform);
        wallpaper["exact_child_parent_match"] = serde_json::json!(matches);
        wallpaper["status"] = serde_json::json!(if nonuniform && matches { "PASS" } else { "FAIL" });
        std::fs::write(directory.join("desktop-view-wallpaper.json"), wallpaper.to_string()).unwrap();
        assert!(nonuniform, "The actual SPI image must be nonuniform at empty sample coordinates");
        assert!(matches, "Every empty child and parent-only sample must exactly match the decoded SPI image");
    }
    std::fs::write(directory.join("desktop-view-result.txt"), format!(
        "wallpaper={}; visible=true; focus=true; selection=true; multiselection_exact=true; menu_visible=true; menu_captured=true; menu_navigated=true; menu_cancelled=true; returned_command=0; keyboard_home=true; files_handoff=true; files_capture=true; context_menu_commands={context_menu_commands}; refresh_api_visible={refresh_api_visible}; refresh_f5_visible={refresh_f5_visible}; positive_bounds=true; guest_fixture_file=true; guest_fixture_folder=true; explorer=false\n", wallpaper["status"])).unwrap();
}

#[test]
#[ignore = "guest-only COM/UI probe: requires a disposable interactive standard-user guest, marker, nonce and Desktop fixtures"]
fn native_shell_desktop_view_shows_exact_guest_fixtures_without_explorer() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|panic| {
        let _ = std::fs::write(r"C:\sandbox\desktop-view-panic.txt", panic.to_string());
    }));
    let result = std::panic::catch_unwind(|| unsafe { run_native_desktop_view_probe(); });
    std::panic::set_hook(original_hook);
    if let Err(panic) = result { std::panic::resume_unwind(panic); }
}

struct ShowDesktopHelper(PROCESS_INFORMATION);

impl Drop for ShowDesktopHelper {
    fn drop(&mut self) {
        unsafe {
            if WaitForSingleObject(self.0.hProcess, 0) != WAIT_OBJECT_0 {
                let _ = TerminateProcess(self.0.hProcess, 1);
                let _ = WaitForSingleObject(self.0.hProcess, 5_000);
            }
            let _ = CloseHandle(self.0.hThread);
            let _ = CloseHandle(self.0.hProcess);
        }
    }
}

#[test]
#[ignore = "helper only: requires the parent-created private desktop and Job Object"]
fn show_desktop_external_window_helper() {
    unsafe {
        let expected = std::env::var("WINARCHY_POINTER_DESKTOP").expect("Run through isolated_probe");
        probe_directory(&expected);
        let mut contained = BOOL(0);
        IsProcessInJob(GetCurrentProcess(), None, &mut contained).unwrap();
        assert_eq!(contained.as_bool(), true, "The helper must inherit the probe Job Object");
        unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
            if message == WM_DESTROY { PostQuitMessage(0); }
            DefWindowProcW(hwnd, message, wp, lp)
        }
        let instance = GetModuleHandleW(PCWSTR::null()).unwrap();
        let class = WNDCLASSW { lpfnWndProc: Some(window_proc), hInstance: instance.into(),
            lpszClassName: w!("WinarchyShowDesktopProbe"), ..Default::default() };
        assert_ne!(RegisterClassW(&class), 0);
        let title: Vec<u16> = expected.encode_utf16().chain(Some(0)).collect();
        let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), class.lpszClassName,
            PCWSTR(title.as_ptr()), WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            100, 100, 240, 160, None, None, instance, None).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while std::time::Instant::now() < deadline {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                if message.message == WM_QUIT { return; }
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let _ = DestroyWindow(hwnd);
    }
}

unsafe fn run_show_desktop_probe(expected: &str) {
    probe_directory(expected);
    assert!(FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).is_err(),
        "The private desktop must not already have a shell");
    let mut contained = BOOL(0);
    IsProcessInJob(GetCurrentProcess(), None, &mut contained).unwrap();
    assert_eq!(contained.as_bool(), true, "The probe must own a kill-on-close Job Object");
    let exe = std::env::current_exe().unwrap();
    let application: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let test = concat!(module_path!(), "::show_desktop_external_window_helper")
        .strip_prefix(concat!(env!("CARGO_CRATE_NAME"), "::")).unwrap();
    let mut command: Vec<u16> = format!("\"{}\" --exact {test} --ignored --nocapture --test-threads=1", exe.display())
        .encode_utf16().chain(Some(0)).collect();
    let mut desktop: Vec<u16> = format!("WinSta0\\{expected}").encode_utf16().chain(Some(0)).collect();
    let startup = STARTUPINFOW { cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        lpDesktop: PWSTR(desktop.as_mut_ptr()), ..Default::default() };
    let mut process = PROCESS_INFORMATION::default();
    CreateProcessW(PCWSTR(application.as_ptr()), PWSTR(command.as_mut_ptr()), None, None, false,
        CREATE_NO_WINDOW, None, PCWSTR::null(), &startup, &mut process).unwrap();
    let helper = ShowDesktopHelper(process);
    let title: Vec<u16> = expected.encode_utf16().chain(Some(0)).collect();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let target = loop {
        if let Ok(hwnd) = FindWindowW(w!("WinarchyShowDesktopProbe"), PCWSTR(title.as_ptr())) {
            break hwnd;
        }
        assert_eq!(WaitForSingleObject(helper.0.hProcess, 0), WAIT_TIMEOUT, "The helper must remain alive");
        assert!(std::time::Instant::now() < deadline, "The helper must create its visible window");
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let mut pid = 0;
    GetWindowThreadProcessId(target, Some(&mut pid));
    assert_eq!(pid, helper.0.dwProcessId, "The target must belong to the separate helper");
    assert_eq!(IsWindowVisible(target).as_bool(), true);
    assert_eq!(IsIconic(target).as_bool(), false);
    assert_eq!(crate::tray_host::spawn(Theme::default(), 1.0), true);
    let host = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).unwrap();
    GetWindowThreadProcessId(host, Some(&mut pid));
    assert_eq!(pid, std::process::id(), "Commands must target our private shell host");
    for (command, minimized) in [(419, true), (416, false)] {
        PostMessageW(host, WM_COMMAND, WPARAM(command), LPARAM(0)).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while IsIconic(target).as_bool() != minimized && std::time::Instant::now() < deadline {
            assert_eq!(WaitForSingleObject(helper.0.hProcess, 0), WAIT_TIMEOUT, "The helper must remain alive");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(IsWindow(target).as_bool(), true, "The helper window must survive both commands");
        assert_eq!(IsIconic(target).as_bool(), minimized, "WM_COMMAND {command} must change the external window");
    }
    PostMessageW(target, WM_CLOSE, WPARAM(0), LPARAM(0)).unwrap();
    assert_eq!(WaitForSingleObject(helper.0.hProcess, 5_000), WAIT_OBJECT_0, "The helper must close cleanly");
}

#[test]
#[ignore = "creates an isolated desktop and a contained helper; never runs on the input desktop"]
fn show_desktop_commands_minimize_and_restore_an_external_window_without_explorer() {
    isolated_probe(concat!(module_path!(), "::show_desktop_commands_minimize_and_restore_an_external_window_without_explorer"),
        run_show_desktop_probe, &[]);
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
        for report in ["desktop-paint.txt", "desktop-wallpaper.log", "desktop-namespace.txt"] {
            if let Ok(report) = std::fs::read_to_string(probe.directory.join(report)) {
                println!("{report}");
            }
        }
        assert_eq!(code, 0, "The isolated pointer probe must pass");
        let mut files: Vec<_> = std::fs::read_dir(&probe.directory).unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
        files.sort();
        assert_eq!(files, expected_files);
    }
}
