use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::Duration;
use windows::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    System::{
        Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
        Threading::GetCurrentThreadId,
    },
    UI::{
        Accessibility::{CUIAutomation, IUIAutomation, UIA_EditControlTypeId},
        Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL},
        WindowsAndMessaging::{CallNextHookEx, HC_ACTION, SetWindowsHookExW, WH_KEYBOARD},
    },
};
use windows_reactor::CancellationToken;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Key {
    Copy,
    Cut,
    Paste,
    SelectAll,
    Delete,
    Up,
    Refresh,
    Undo,
}

pub type Receiver = Arc<Mutex<mpsc::Receiver<Key>>>;

static SENDER: OnceLock<mpsc::Sender<Key>> = OnceLock::new();

// The hook runs on the UI thread and never consumes the key; the focus check happens off-thread
// because UI Automation calls into this same process would block the UI thread.
pub fn install() -> Result<Receiver, String> {
    let (sender, receiver) = mpsc::channel();
    SENDER
        .set(sender)
        .map_err(|_| "keyboard hook already installed".to_owned())?;
    unsafe { SetWindowsHookExW(WH_KEYBOARD, Some(hook), None, GetCurrentThreadId()) }
        .map_err(|error| error.to_string())?;
    Ok(Arc::new(Mutex::new(receiver)))
}

unsafe extern "system" fn hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let key_down = lparam.0 & (1 << 31) == 0;
    if code == HC_ACTION as i32 && key_down {
        let ctrl = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
        let key = map_key(wparam.0 as u32, ctrl);
        if let (Some(key), Some(sender)) = (key, SENDER.get()) {
            let _ = sender.send(key);
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn map_key(virtual_key: u32, ctrl: bool) -> Option<Key> {
    match (virtual_key, ctrl) {
        (0x43, true) => Some(Key::Copy),
        (0x58, true) => Some(Key::Cut),
        (0x56, true) => Some(Key::Paste),
        (0x41, true) => Some(Key::SelectAll),
        (0x5A, true) => Some(Key::Undo),
        (0x2E, false) => Some(Key::Delete),
        (0x08, false) => Some(Key::Up),
        (0x74, false) => Some(Key::Refresh),
        _ => None,
    }
}

// Blocks until a shortcut arrives that was not typed into a text field.
pub fn next(receiver: &Receiver, cancel: &CancellationToken) -> Option<Key> {
    loop {
        if cancel.is_cancelled() {
            return None;
        }
        let key = receiver
            .lock()
            .ok()?
            .recv_timeout(Duration::from_millis(250));
        match key {
            Ok(key) if !focus_in_text_field() => return Some(key),
            Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return None,
        }
    }
}

// Any failure counts as "typing", so a shortcut never fires on a state that could not be read.
fn focus_in_text_field() -> bool {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(automation) =
            CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
        else {
            return true;
        };
        automation
            .GetFocusedElement()
            .and_then(|element| element.CurrentControlType())
            .map_or(true, |kind| kind == UIA_EditControlTypeId)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_shortcuts_and_ignores_the_rest() {
        assert_eq!(map_key(0x43, true), Some(Key::Copy));
        assert_eq!(map_key(0x43, false), None);
        assert_eq!(map_key(0x2E, false), Some(Key::Delete));
        assert_eq!(map_key(0x2E, true), None);
        assert_eq!(map_key(0x08, false), Some(Key::Up));
        assert_eq!(map_key(0x41, true), Some(Key::SelectAll));
        assert_eq!(map_key(0x5A, true), Some(Key::Undo));
        assert_eq!(map_key(0x59, true), None);
    }
}
