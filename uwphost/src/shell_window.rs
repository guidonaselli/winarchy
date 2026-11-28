// Ported from nickel-uwu (commit 298f87a), Copyright (c) 2026 Steven Horton, MIT. See THIRD_PARTY_NOTICES.md.

use crate::{contract::Failure, symbols::system_module};
use windows::{
    core::{s, w},
    Win32::{
        Foundation::{GetLastError, HWND},
        System::LibraryLoader::GetProcAddress,
        UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, GetShellWindow, WINDOW_EX_STYLE, WS_POPUP},
    },
};

/// Hidden window registered as the session's shell window while this process lives.
pub struct ShellWindow(pub HWND);

impl ShellWindow {
    pub fn register() -> Result<Self, Failure> {
        let fail = |reason: String| Failure::ShellWindow(reason);
        // SAFETY: STATIC is a system class; the window belongs to this thread for the guard's lifetime.
        // SetShellWindowEx is an undocumented user32 export with the (HWND, HWND) -> BOOL signature.
        unsafe {
            if !GetShellWindow().is_invalid() {
                return Err(fail("a shell window is already registered".into()));
            }
            let user32 = system_module("user32.dll")?;
            let set = GetProcAddress(user32, s!("SetShellWindowEx")).ok_or_else(|| fail("SetShellWindowEx is missing".into()))?;
            let set: unsafe extern "system" fn(HWND, HWND) -> i32 = std::mem::transmute(set);
            let window = CreateWindowExW(WINDOW_EX_STYLE(0), w!("STATIC"), w!("Winarchy UWP host"), WS_POPUP, 0, 0, 1, 1, None, None, None, None)
                .map_err(|e| fail(format!("create shell window: {e}")))?;
            let guard = Self(window);
            if set(window, HWND::default()) == 0 {
                return Err(fail(format!("SetShellWindowEx failed: {}", GetLastError().0)));
            }
            if GetShellWindow() != window {
                return Err(fail("another window owns the shell registration".into()));
            }
            Ok(guard)
        }
    }
}

impl Drop for ShellWindow {
    fn drop(&mut self) {
        // SAFETY: This thread created the window.
        let _ = unsafe { DestroyWindow(self.0) };
    }
}
