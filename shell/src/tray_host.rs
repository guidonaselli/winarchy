use std::sync::mpsc;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use crate::theme::Theme;

const MAX_COPYDATA: u32 = 4096;
static HOST: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static RESTORE: std::sync::Mutex<Vec<isize>> = std::sync::Mutex::new(Vec::new());

pub fn owns_window(hwnd: HWND) -> bool {
    HOST.load(std::sync::atomic::Ordering::Relaxed) == hwnd.0 as isize
}

#[repr(C)]
struct CopyData {
    dw_data: usize,
    cb_data: u32,
    data: *const core::ffi::c_void,
}

pub fn spawn(theme: Theme, scale: f32) -> bool {
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    if std::thread::Builder::new().name("tray-host".into()).spawn(move || unsafe {
        let instance = match GetModuleHandleW(PCWSTR::null()) {
            Ok(instance) => instance,
            Err(_) => { let _ = ready_tx.send(false); return; }
        };
        let class = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: instance.into(), lpszClassName: w!("Shell_TrayWnd"), ..Default::default() };
        if RegisterClassW(&class) == 0 {
            let _ = ready_tx.send(false);
            return;
        }
        let Ok(hwnd) = CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE, class.lpszClassName, w!(""), WS_POPUP, 0, 0, 1, 1, None, None, instance, None) else {
            let _ = ready_tx.send(false);
            return;
        };
        HOST.store(hwnd.0 as isize, std::sync::atomic::Ordering::Relaxed);
        crate::tray::set_native_active(true);
        if !crate::tray_notifications::attach(hwnd, theme, scale) {
            let _ = DestroyWindow(hwnd);
            HOST.store(0, std::sync::atomic::Ordering::Relaxed);
            crate::tray::set_native_active(false);
            let _ = ready_tx.send(false);
            return;
        }
        let created = RegisterWindowMessageW(w!("TaskbarCreated"));
        if created != 0 {
            let _ = SendNotifyMessageW(HWND_BROADCAST, created, WPARAM(0), LPARAM(0));
        }
        let _ = ready_tx.send(true);
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = DispatchMessageW(&message);
        }
        let _ = DestroyWindow(hwnd);
        HOST.store(0, std::sync::atomic::Ordering::Relaxed);
        crate::tray::set_native_active(false);
    }).is_err() {
        return false;
    }
    ready_rx.recv_timeout(std::time::Duration::from_secs(3)).unwrap_or(false)
}

unsafe extern "system" fn proc(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if message == WM_COMMAND {
        match (wp.0 & 0xFFFF) as u32 {
            419 => minimize_all(),
            416 => restore_all(),
            _ => {}
        }
        return LRESULT(1);
    }
    if message == WM_COPYDATA && lp.0 != 0 {
        let copy = &*(lp.0 as *const CopyData);
        if copy.dw_data == 1 && (964..=MAX_COPYDATA).contains(&copy.cb_data) && !copy.data.is_null() {
            let data = std::slice::from_raw_parts(copy.data.cast::<u8>(), copy.cb_data as usize);
            if crate::tray::ingest_shell_notify(data) {
                crate::tray_flyout::notify();
                crate::cluster::notify();
            }
            return LRESULT(1);
        }
    }
    DefWindowProcW(hwnd, message, wp, lp)
}

unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> BOOL {
    let mut pid = 0;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    let owned = GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| !owner.0.is_null());
    if pid != windows::Win32::System::Threading::GetCurrentProcessId()
        && hwnd != GetShellWindow()
        && IsWindowVisible(hwnd).as_bool()
        && !IsIconic(hwnd).as_bool()
        && ex & WS_EX_TOOLWINDOW.0 == 0
        && !owned
    {
        (*(data.0 as *mut Vec<isize>)).push(hwnd.0 as isize);
    }
    BOOL(1)
}

fn minimize_all() {
    let mut windows = Vec::new();
    unsafe { let _ = EnumWindows(Some(collect), LPARAM(&mut windows as *mut _ as isize)); }
    let mut restore = RESTORE.lock().unwrap();
    if !restore.is_empty() { return; }
    for handle in windows {
        unsafe { let _ = ShowWindowAsync(HWND(handle as *mut _), SW_MINIMIZE); }
        restore.push(handle);
    }
}

fn restore_all() {
    for handle in std::mem::take(&mut *RESTORE.lock().unwrap()) {
        unsafe {
            let hwnd = HWND(handle as *mut _);
            if IsWindow(hwnd).as_bool() { let _ = ShowWindowAsync(hwnd, SW_RESTORE); }
        }
    }
}
