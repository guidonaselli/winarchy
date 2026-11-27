// Ported from nickel-uwu (commit 298f87a), Copyright (c) 2026 Steven Horton, MIT. See THIRD_PARTY_NOTICES.md.
//! The real windows behind `present`: enumeration, readiness and repositioning.

use crate::{
    present::{Core, Frame, Platform, Snapshot, Window},
    symbols::Symbols,
    views, wrapper,
};
use std::ffi::c_void;
use windows::{
    core::{BOOL, PWSTR},
    Win32::{
        Foundation::{CloseHandle, HWND, LPARAM, RECT},
        Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED},
        Storage::{EnhancedStorage::PKEY_AppUserModel_ID, Packaging::Appx::GetApplicationUserModelId},
        System::{
            Com::{CoTaskMemFree, StructuredStorage::PropVariantToStringAlloc},
            Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
        },
        UI::{
            Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow},
            WindowsAndMessaging::{
                EnumChildWindows, EnumWindows, GetAncestor, GetClassNameW, GetClientRect, GetDesktopWindow, GA_PARENT, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindow,
                IsWindowVisible, SetWindowPos, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOZORDER,
            },
        },
    },
};

pub const FRAME_CLASS: &str = "ApplicationFrameWindow";
pub const CORE_CLASS: &str = "Windows.UI.Core.CoreWindow";

fn handle(window: usize) -> HWND {
    HWND(window as *mut c_void)
}

pub fn class_name(hwnd: HWND) -> String {
    let mut name = [0u16; 128];
    // SAFETY: Windows validates the HWND; the buffer is writable.
    let length = unsafe { GetClassNameW(hwnd, &mut name) }.max(0) as usize;
    String::from_utf16_lossy(&name[..length])
}

fn owner(hwnd: HWND) -> u32 {
    let mut pid = 0;
    // SAFETY: Windows validates the HWND and writes the PID.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

/// Frames and CoreWindows; CoreWindows live either at top level or inside a frame.
fn windows() -> Vec<(Window, bool)> {
    unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> BOOL {
        let class = class_name(hwnd);
        let frame = class == FRAME_CLASS;
        if frame || class == CORE_CLASS {
            // SAFETY: The enumerations below are synchronous and pass this Vec.
            let found = unsafe { &mut *(data.0 as *mut Vec<(Window, bool)>) };
            found.push((Window { hwnd: hwnd.0 as usize, pid: owner(hwnd) }, frame));
            if frame {
                let _ = unsafe { EnumChildWindows(Some(hwnd), Some(visit), data) };
            }
        }
        BOOL(1)
    }
    let mut found: Vec<(Window, bool)> = Vec::new();
    // SAFETY: The Vec outlives the synchronous enumeration.
    let _ = unsafe { EnumWindows(Some(visit), LPARAM(&mut found as *mut _ as isize)) };
    found
}

pub fn existing_cores() -> std::collections::HashSet<Window> {
    windows().into_iter().filter(|(_, frame)| !frame).map(|(window, _)| window).collect()
}

fn process_app_id(pid: u32) -> Option<String> {
    // SAFETY: Windows checks the access right and the buffer length; the handle is closed here.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 1024];
        let mut length = buffer.len() as u32;
        let status = GetApplicationUserModelId(process, &mut length, Some(PWSTR(buffer.as_mut_ptr())));
        let _ = CloseHandle(process);
        (status.0 == 0 && length > 0 && length as usize <= buffer.len()).then(|| String::from_utf16_lossy(&buffer[..length as usize - 1]))
    }
}

fn frame_app_id(window: usize) -> Option<String> {
    // SAFETY: The converted string is a COM task allocation, freed after copying.
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(handle(window)).ok()?;
        let value = store.GetValue(&PKEY_AppUserModel_ID).ok()?;
        let text = PropVariantToStringAlloc(&value).ok()?;
        let result = text.to_string().ok();
        CoTaskMemFree(Some(text.0.cast()));
        result.filter(|id| !id.is_empty())
    }
}

fn parent(window: usize) -> Option<usize> {
    // SAFETY: Windows validates the HWND. GA_PARENT never reports an owner, but names the desktop for a top-level window.
    let parent = unsafe { GetAncestor(handle(window), GA_PARENT) };
    (!parent.is_invalid() && parent != unsafe { GetDesktopWindow() }).then_some(parent.0 as usize)
}

pub fn snapshot() -> Snapshot {
    let all = windows();
    let frames = all.iter().filter(|(_, frame)| *frame).filter_map(|(w, _)| frame_app_id(w.hwnd).map(|app_id| Frame { window: *w, app_id })).collect();
    let cores = all
        .iter()
        .filter(|(_, frame)| !frame)
        .filter_map(|(w, _)| process_app_id(w.pid).map(|app_id| Core { window: *w, app_id, parent: parent(w.hwnd) }))
        .collect();
    Snapshot { frames, cores }
}

fn cloaked(window: Window) -> Result<bool, String> {
    let mut flags = 0u32;
    // SAFETY: DWM validates the HWND; flags is a writable DWORD.
    unsafe { DwmGetWindowAttribute(handle(window.hwnd), DWMWA_CLOAKED, (&mut flags as *mut u32).cast(), size_of::<u32>() as u32) }
        .map_err(|e| format!("cloak state: {e}"))?;
    Ok(flags != 0)
}

pub struct Native<'a>(pub &'a Symbols);

impl Platform for Native<'_> {
    fn client(&mut self, frame: Window) -> Result<usize, String> {
        Ok(wrapper::find(frame.hwnd, self.0)?.client)
    }

    fn present(&mut self, view: &Frame, core: Window, only_view: bool) -> Result<bool, String> {
        let frame = view.window;
        let found = wrapper::find(frame.hwnd, self.0)?;
        if found.client != 0 && found.client != core.hwnd {
            return Err("wrapper already owns another CoreWindow".into());
        }
        // SAFETY: Windows validates these observed handles.
        let usable = unsafe { IsWindow(Some(handle(core.hwnd))).as_bool() && !IsIconic(handle(frame.hwnd)).as_bool() };
        if !usable {
            return Ok(false);
        }
        if [frame, core].iter().any(|w| owner(handle(w.hwnd)) != w.pid) {
            return Err("window owner changed".into());
        }
        // another process can parent a CoreWindow after the enumeration
        let parent = parent(core.hwnd);
        if parent.is_some_and(|p| p != frame.hwnd) {
            return Err("CoreWindow is attached to another frame".into());
        }
        let attached = parent.is_some();
        let frame_cloaked = cloaked(frame)?;
        let core_cloaked = cloaked(core)?;
        // SAFETY: Same observed handles.
        let visible = unsafe { IsWindowVisible(handle(frame.hwnd)).as_bool() && IsWindowVisible(handle(core.hwnd)).as_bool() };
        if found.client == core.hwnd && found.wait_flags == 0 && attached && !frame_cloaked && !core_cloaked && visible {
            return Ok(true);
        }
        wrapper::advance(&found, core.hwnd, attached, self.0)?;
        if (frame_cloaked || core_cloaked) && found.wait_flags == 0 {
            // the private lookup is by AppUserModelID: never uncloak when two frames share it
            if !only_view {
                return Err("cannot uncloak one of several views of the app".into());
            }
            views::uncloak(&view.app_id, self.0)?;
        }
        Ok(false)
    }
}

/// The private frame never resizes its child here, so the CoreWindow follows the frame's client area.
pub fn fit(frame: Window, core: Window) {
    let (frame, core) = (handle(frame.hwnd), handle(core.hwnd));
    // SAFETY: Windows validates both handles; the move is posted, never blocking on the app.
    unsafe {
        if parent(core.0 as usize) != Some(frame.0 as usize) || IsIconic(frame).as_bool() {
            return;
        }
        let (mut client, mut current) = (RECT::default(), RECT::default());
        if GetClientRect(frame, &mut client).is_err() || GetWindowRect(core, &mut current).is_err() {
            return;
        }
        if current.right - current.left != client.right || current.bottom - current.top != client.bottom {
            let _ = SetWindowPos(core, None, 0, 0, client.right, client.bottom, SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS);
        }
    }
}
