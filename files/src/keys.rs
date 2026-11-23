use std::sync::{Arc, Mutex, OnceLock, atomic::{AtomicBool, Ordering}, mpsc};
use std::time::Duration;
use windows::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    System::{
        Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
        Threading::{GetCurrentProcessId, GetCurrentThreadId},
    },
    UI::{
        Accessibility::{
            CUIAutomation, IUIAutomation, UIA_ComboBoxControlTypeId, UIA_CustomControlTypeId, UIA_EditControlTypeId,
            UIA_GroupControlTypeId, UIA_ListControlTypeId, UIA_ListItemControlTypeId, UIA_MenuControlTypeId,
            UIA_MenuItemControlTypeId, UIA_PaneControlTypeId, UIA_WindowControlTypeId, UIA_CONTROLTYPE_ID,
        },
        Input::KeyboardAndMouse::{GetAsyncKeyState, GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT, VK_XBUTTON1, VK_XBUTTON2},
        WindowsAndMessaging::{CallNextHookEx, GetForegroundWindow, GetWindowThreadProcessId, HC_ACTION, SetWindowsHookExW, WH_KEYBOARD},
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
    ToggleHidden,
    NewTab,
    CloseTab,
    NextTab,
    MoveUp,
    MoveDown,
    Open,
    ContextMenu,
    Confirm,
    Cancel,
    FocusAddress,
    FocusFilter,
    PinUp,
    PinDown,
    Back,
    Forward,
    PrevTab,
    ReopenTab,
    JumpTab(usize),
    Rename,
    NewFolder,
    CopyPath,
    Properties,
    DeletePermanently,
    MoveLeft,
    MoveRight,
    Home,
    End,
    Char(char),
}

pub type Receiver = Arc<Mutex<mpsc::Receiver<Key>>>;

static SENDER: OnceLock<mpsc::Sender<Key>> = OnceLock::new();
static MENU_OPEN: AtomicBool = AtomicBool::new(false);

// While the overlay menu is open its keys must pass whatever control holds focus.
pub fn set_menu_open(open: bool) {
    MENU_OPEN.store(open, Ordering::Relaxed);
}

pub fn ctrl_down() -> bool {
    (unsafe { GetAsyncKeyState(VK_CONTROL.0 as i32) }) < 0
}

pub fn shift_down() -> bool {
    (unsafe { GetKeyState(VK_SHIFT.0 as i32) }) < 0
}

// The hook runs on the UI thread and never consumes the key; the focus check happens off-thread
// because UI Automation calls into this same process would block the UI thread.
pub fn install() -> Result<Receiver, String> {
    let (sender, receiver) = mpsc::channel();
    SENDER
        .set(sender)
        .map_err(|_| "keyboard hook already installed".to_owned())?;
    unsafe { SetWindowsHookExW(WH_KEYBOARD, Some(hook), None, GetCurrentThreadId()) }
        .map_err(|error| error.to_string())?;
    std::thread::spawn(poll_side_buttons);
    Ok(Arc::new(Mutex::new(receiver)))
}

unsafe extern "system" fn hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let key_down = lparam.0 & (1 << 31) == 0;
    if code == HC_ACTION as i32 && key_down {
        let ctrl = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
        let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
        let alt = unsafe { GetKeyState(VK_MENU.0 as i32) } < 0;
        let key = if wparam.0 == 0x79 && shift { Some(Key::ContextMenu) } else { map_key(wparam.0 as u32, ctrl, shift, alt) };
        if let (Some(key), Some(sender)) = (key, SENDER.get()) {
            let _ = sender.send(key);
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

// WinUI runs in pointer-input mode, which stops the legacy mouse messages a mouse hook relies on, so the side buttons are polled.
fn poll_side_buttons() {
    let mut was_down = [false; 2];
    loop {
        std::thread::sleep(Duration::from_millis(15));
        let ours = unsafe {
            let mut process = 0;
            GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut process));
            process == GetCurrentProcessId()
        };
        for (index, (button, key)) in [(VK_XBUTTON1, Key::Back), (VK_XBUTTON2, Key::Forward)].into_iter().enumerate() {
            let down = unsafe { GetAsyncKeyState(button.0 as i32) } < 0;
            if down && !was_down[index] && ours && let Some(sender) = SENDER.get() {
                let _ = sender.send(key);
            }
            was_down[index] = down;
        }
    }
}

fn map_key(virtual_key: u32, ctrl: bool, shift: bool, alt: bool) -> Option<Key> {
    if alt {
        return match (virtual_key, ctrl) {
            (0x26, false) if shift => Some(Key::PinUp),
            (0x28, false) if shift => Some(Key::PinDown),
            (0x25, false) => Some(Key::Back),
            (0x27, false) => Some(Key::Forward),
            (0x26, false) => Some(Key::Up),
            (0x0D, false) => Some(Key::Properties),
            _ => None,
        };
    }
    if ctrl && shift {
        return match virtual_key {
            0x4E => Some(Key::NewFolder),
            0x54 => Some(Key::ReopenTab),
            0x43 => Some(Key::CopyPath),
            0x09 => Some(Key::PrevTab),
            _ => None,
        };
    }
    if ctrl && (0x31..=0x39).contains(&virtual_key) {
        return Some(Key::JumpTab((virtual_key - 0x31) as usize));
    }
    if shift && virtual_key == 0x2E {
        return Some(Key::DeletePermanently);
    }
    match (virtual_key, ctrl) {
        (0x46, true) => Some(Key::FocusFilter),
        (0x71, false) => Some(Key::Rename),
        (0x25, false) => Some(Key::MoveLeft),
        (0x27, false) => Some(Key::MoveRight),
        (0x24, false) => Some(Key::Home),
        (0x23, false) => Some(Key::End),
        (0x30..=0x39 | 0x41..=0x5A, false) => char::from_u32(virtual_key).map(|c| Key::Char(c.to_ascii_lowercase())),
        (0x43, true) => Some(Key::Copy),
        (0x58, true) => Some(Key::Cut),
        (0x56, true) => Some(Key::Paste),
        (0x41, true) => Some(Key::SelectAll),
        (0x5A, true) => Some(Key::Undo),
        (0x59, true) => Some(Key::Redo),
        (0x4E, true) => Some(Key::NewWindow),
        (0x48, true) => Some(Key::ToggleHidden),
        (0x54, true) => Some(Key::NewTab),
        (0x57, true) => Some(Key::CloseTab),
        (0x09, true) => Some(Key::NextTab),
        (0x26, false) => Some(Key::MoveUp),
        (0x28, false) => Some(Key::MoveDown),
        (0x0D, false) => Some(Key::Open),
        (0x5D, false) => Some(Key::ContextMenu),
        (0x1B, false) => Some(Key::Cancel),
        (0x4C, true) => Some(Key::FocusAddress),
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
            Ok(key) => {
                if let Some(key) = resolve(key) {
                    return Some(key);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return None,
        }
    }
}

// Arrows must not steal from menus or lists, and Enter must not double-fire on a focused button, so those
// keys only pass when focus sits on a neutral container.
fn resolve(key: Key) -> Option<Key> {
    // Enter and Escape drive the prompt and address fields, so they matter exactly when a text field has focus.
    if matches!(key, Key::Open | Key::Cancel) && focused_kind() == Some(UIA_EditControlTypeId) {
        return Some(if key == Key::Open { Key::Confirm } else { Key::Cancel });
    }
    key_allowed(key).then_some(key)
}

fn key_allowed(key: Key) -> bool {
    let Some(kind) = focused_kind() else { return false };
    let neutral = [UIA_WindowControlTypeId, UIA_PaneControlTypeId, UIA_CustomControlTypeId, UIA_GroupControlTypeId];
    let navigating = [
        UIA_ComboBoxControlTypeId, UIA_ListControlTypeId, UIA_ListItemControlTypeId, UIA_MenuControlTypeId,
        UIA_MenuItemControlTypeId,
    ];
    match key {
        Key::MoveUp | Key::MoveDown | Key::Open | Key::Cancel if MENU_OPEN.load(Ordering::Relaxed) => true,
        Key::Open => neutral.contains(&kind),
        Key::Cancel => kind != UIA_EditControlTypeId,
        Key::FocusAddress | Key::FocusFilter | Key::Back | Key::Forward | Key::PinUp | Key::PinDown => true,
        Key::MoveUp | Key::MoveDown | Key::MoveLeft | Key::MoveRight | Key::Home | Key::End | Key::Char(_) => {
            kind != UIA_EditControlTypeId && !navigating.contains(&kind)
        }
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
        assert_eq!(map_key(0x43, true, false, false), Some(Key::Copy));
        assert_eq!(map_key(0x43, false, false, false), Some(Key::Char('c')));
        assert_eq!(map_key(0x7A, false, false, false), None);
        assert_eq!(map_key(0x2E, false, false, false), Some(Key::Delete));
        assert_eq!(map_key(0x2E, true, false, false), None);
        assert_eq!(map_key(0x08, false, false, false), Some(Key::Up));
        assert_eq!(map_key(0x41, true, false, false), Some(Key::SelectAll));
        assert_eq!(map_key(0x5A, true, false, false), Some(Key::Undo));
        assert_eq!(map_key(0x59, true, false, false), Some(Key::Redo));
        assert_eq!(map_key(0x4E, true, false, false), Some(Key::NewWindow));
        assert_eq!(map_key(0x54, true, false, false), Some(Key::NewTab));
        assert_eq!(map_key(0x57, true, false, false), Some(Key::CloseTab));
        assert_eq!(map_key(0x09, true, false, false), Some(Key::NextTab));
        assert_eq!(map_key(0x28, false, false, false), Some(Key::MoveDown));
        assert_eq!(map_key(0x26, false, false, false), Some(Key::MoveUp));
        assert_eq!(map_key(0x0D, false, false, false), Some(Key::Open));
        assert_eq!(map_key(0x5D, false, false, false), Some(Key::ContextMenu));
        assert_eq!(map_key(0x25, false, false, true), Some(Key::Back));
        assert_eq!(map_key(0x4E, true, true, false), Some(Key::NewFolder));
        assert_eq!(map_key(0x2E, false, true, false), Some(Key::DeletePermanently));
        assert_eq!(map_key(0x32, true, false, false), Some(Key::JumpTab(1)));
        assert_eq!(map_key(0x41, false, false, false), Some(Key::Char('a')));
    }
}
