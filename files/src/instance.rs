use std::path::PathBuf;

const PIPE_PREFIX: &str = r"\\.\pipe\winarchy-files-";
const MUTEX: &str = r"Local\winarchy-files-instance";
const CONNECT_ATTEMPTS: u32 = 20;

// One pipe per user, so another account on the machine can neither receive nor block this one's launches.
fn pipe_name() -> String {
    format!("{PIPE_PREFIX}{}", std::env::var("USERNAME").unwrap_or_default())
}

#[derive(Clone)]
pub enum Request {
    Failed,
    Focus,
    Open(PathBuf),
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

// True when this process is the first one. Otherwise `target` was handed to the running window (empty: just bring it forward).
#[cfg(windows)]
pub fn claim(target: Option<&std::path::Path>) -> bool {
    use windows::{
        Win32::{
            Foundation::{ERROR_ALREADY_EXISTS, GENERIC_WRITE, GetLastError},
            Storage::FileSystem::{CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_NONE, OPEN_EXISTING, WriteFile},
            System::Threading::CreateMutexW,
        },
        core::PCWSTR,
    };
    if std::env::var_os("WINARCHY_FILES_MULTI").is_some() {
        return true;
    }
    unsafe {
        // the handle stays open for the life of the process: closing it would release the claim
        let _ = CreateMutexW(None, false, PCWSTR(wide(MUTEX).as_ptr()));
        if GetLastError() != ERROR_ALREADY_EXISTS {
            return true;
        }
        // this launch may hold foreground rights the running window lacks; pass them on so its raise is not refused
        let _ = windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(windows::Win32::UI::WindowsAndMessaging::ASFW_ANY);
        let text = target.map(|path| path.to_string_lossy().into_owned()).unwrap_or_default();
        let name = wide(&pipe_name());
        for _ in 0..CONNECT_ATTEMPTS {
            if let Ok(pipe) = CreateFileW(PCWSTR(name.as_ptr()), GENERIC_WRITE.0, FILE_SHARE_NONE, None, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, None) {
                let sent = WriteFile(pipe, Some(text.as_bytes()), None, None).is_ok();
                let _ = windows::Win32::Foundation::CloseHandle(pipe);
                if sent {
                    return false;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        // the owner never answered (hung or exiting): carry on as an independent window rather than losing the request
        true
    }
}

#[cfg(not(windows))]
pub fn claim(_target: Option<&std::path::Path>) -> bool {
    true
}

// Blocks until another launch forwards a request; `None` is a plain "bring the window forward".
#[cfg(windows)]
pub fn receive() -> Request {
    use windows::{
        Win32::{
            Foundation::CloseHandle,
            Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAGS_AND_ATTRIBUTES, PIPE_ACCESS_INBOUND, ReadFile},
            System::Pipes::{ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT},
        },
        core::PCWSTR,
    };
    unsafe {
        let pipe = CreateNamedPipeW(
            PCWSTR(wide(&pipe_name()).as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(PIPE_ACCESS_INBOUND.0 | FILE_FLAG_FIRST_PIPE_INSTANCE.0),
            PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            0,
            4096,
            0,
            None,
        );
        if pipe.is_invalid() {
            return Request::Failed;
        }
        let _ = ConnectNamedPipe(pipe, None);
        let mut received = Vec::new();
        let mut chunk = vec![0u8; 4096];
        loop {
            let mut read = 0u32;
            if ReadFile(pipe, Some(&mut chunk), Some(&mut read), None).is_err() || read == 0 {
                break;
            }
            received.extend_from_slice(&chunk[..read as usize]);
        }
        let _ = DisconnectNamedPipe(pipe);
        let _ = CloseHandle(pipe);
        let text = String::from_utf8_lossy(&received).into_owned();
        if text.is_empty() { Request::Focus } else { Request::Open(PathBuf::from(text)) }
    }
}

#[cfg(not(windows))]
pub fn receive() -> Request {
    Request::Failed
}

// Raises this process's window above the others, as a launch from Explorer or a shortcut would.
#[cfg(windows)]
pub fn raise() {
    use windows::Win32::{
        Foundation::{HWND, LPARAM},
        System::Threading::{AttachThreadInput, GetCurrentProcessId, GetCurrentThreadId},
        UI::WindowsAndMessaging::{EnumWindows, GetForegroundWindow, GetWindowThreadProcessId, IsIconic, IsWindowVisible, SW_RESTORE, SetForegroundWindow, ShowWindow},
    };
    use windows::core::BOOL;
    unsafe extern "system" fn bring(window: HWND, _: LPARAM) -> BOOL {
        let mut process = 0;
        // SAFETY: plain Win32 calls on a window handle supplied by EnumWindows
        unsafe {
            GetWindowThreadProcessId(window, Some(&mut process));
            if process != GetCurrentProcessId() || !IsWindowVisible(window).as_bool() {
                return true.into();
            }
            if IsIconic(window).as_bool() {
                let _ = ShowWindow(window, SW_RESTORE);
            }
            if !SetForegroundWindow(window).as_bool() {
                // refused by the foreground lock: borrow the foreground thread's input state for one call
                let owner = GetWindowThreadProcessId(GetForegroundWindow(), None);
                let me = GetCurrentThreadId();
                if owner != 0 && owner != me && AttachThreadInput(me, owner, true).as_bool() {
                    let _ = SetForegroundWindow(window);
                    let _ = AttachThreadInput(me, owner, false);
                }
            }
        }
        false.into()
    }
    unsafe {
        let _ = EnumWindows(Some(bring), LPARAM(0));
    }
}

#[cfg(not(windows))]
pub fn raise() {}
