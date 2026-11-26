use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_UNICODETEXT;

pub unsafe fn get(owner: HWND) -> Option<String> {
    OpenClipboard(owner).ok()?;
    let text = GetClipboardData(CF_UNICODETEXT.0 as u32).ok().and_then(|data| {
        let memory = HGLOBAL(data.0);
        let pointer = GlobalLock(memory) as *const u16;
        if pointer.is_null() {
            return None;
        }
        let all = std::slice::from_raw_parts(pointer, GlobalSize(memory) / 2);
        let text = String::from_utf16_lossy(&all[..all.iter().position(|c| *c == 0).unwrap_or(all.len())]);
        let _ = GlobalUnlock(memory);
        Some(text)
    });
    let _ = CloseClipboard();
    text
}

pub unsafe fn set(owner: HWND, text: &str) {
    if text.is_empty() || OpenClipboard(owner).is_err() {
        return;
    }
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    if let Ok(memory) = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) {
        let pointer = GlobalLock(memory) as *mut u16;
        if !pointer.is_null() {
            pointer.copy_from_nonoverlapping(wide.as_ptr(), wide.len());
            let _ = GlobalUnlock(memory);
            // el sistema pasa a ser dueño de la memoria
            if EmptyClipboard().is_ok() {
                let _ = SetClipboardData(CF_UNICODETEXT.0 as u32, HANDLE(memory.0));
            }
        }
    }
    let _ = CloseClipboard();
}
