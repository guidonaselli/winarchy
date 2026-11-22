use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::Duration;
use windows::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    System::{
        Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
        Threading::GetCurrentThreadId,
    },
    UI::{
        Accessibility::{
            CUIAutomation, IUIAutomation, UIA_ComboBoxControlTypeId, UIA_CustomControlTypeId, UIA_EditControlTypeId,
            UIA_GroupControlTypeId, UIA_ListControlTypeId, UIA_ListItemControlTypeId, UIA_MenuControlTypeId,
            UIA_MenuItemControlTypeId, UIA_PaneControlTypeId, UIA_WindowControlTypeId, UIA_CONTROLTYPE_ID,
        },
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
    Redo,
    NewWindow,
    NewTab,
    CloseTab,
    NextTab,
    MoveUp,
    MoveDown,
    Open,
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
        (0x59, true) => Some(Key::Redo),
        (0x4E, true) => Some(Key::NewWindow),
        (0x54, true) => Some(Key::NewTab),
        (0x57, true) => Some(Key::CloseTab),
        (0x09, true) => Some(Key::NextTab),
        (0x26, false) => Some(Key::MoveUp),
        (0x28, false) => Some(Key::MoveDown),
        (0x0D, false) => Some(Key::Open),
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
            Ok(key) if key_allowed(key) => return Some(key),
            Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return None,
        }
    }
}

// Arrows must not steal from menus or lists, and Enter must not double-fire on a focused button, so those
// keys only pass when focus sits on a neutral container.
fn key_allowed(key: Key) -> bool {
    let Some(kind) = focused_kind() else { return false };
    let neutral = [UIA_WindowControlTypeId, UIA_PaneControlTypeId, UIA_CustomControlTypeId, UIA_GroupControlTypeId];
    let navigating = [
        UIA_ComboBoxControlTypeId, UIA_ListControlTypeId, UIA_ListItemControlTypeId, UIA_MenuControlTypeId,
        UIA_MenuItemControlTypeId,
    ];
    match key {
        Key::Open => neutral.contains(&kind),
        Key::MoveUp | Key::MoveDown => kind != UIA_EditControlTypeId && !navigating.contains(&kind),
        _ => kind != UIA_EditControlTypeId,
    }
}

// Any failure counts as "typing", so a shortcut never fires on a state that could not be read.
fn focused_kind() -> Option<UIA_CONTROLTYPE_ID> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let automation = CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        automation.GetFocusedElement().and_then(|element| element.CurrentControlType()).ok()
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
        assert_eq!(map_key(0x59, true), Some(Key::Redo));
        assert_eq!(map_key(0x4E, true), Some(Key::NewWindow));
        assert_eq!(map_key(0x54, true), Some(Key::NewTab));
        assert_eq!(map_key(0x57, true), Some(Key::CloseTab));
        assert_eq!(map_key(0x09, true), Some(Key::NextTab));
        assert_eq!(map_key(0x28, false), Some(Key::MoveDown));
        assert_eq!(map_key(0x26, false), Some(Key::MoveUp));
        assert_eq!(map_key(0x0D, false), Some(Key::Open));
    }
}
