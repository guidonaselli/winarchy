use std::{ffi::OsString, os::windows::ffi::{OsStrExt, OsStringExt}, path::PathBuf};
use windows::{
    Win32::{
        Foundation::{GlobalFree, HANDLE, HGLOBAL},
        System::{
            DataExchange::{
                CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
                OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
            },
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
        },
        UI::Shell::{DragQueryFileW, HDROP},
    },
    core::w,
};

const CF_HDROP: u32 = 15;
const CF_UNICODETEXT: u32 = 13;
const DROPEFFECT_COPY: u32 = 1;
const DROPEFFECT_MOVE: u32 = 2;
const DROPFILES_HEADER: usize = 20;

struct Open;

impl Open {
    fn new() -> Result<Self, String> {
        unsafe { OpenClipboard(None) }
            .map(|()| Self)
            .map_err(|error| format!("clipboard is busy: {error}"))
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

// On success the clipboard owns the memory; on failure the caller still must free it.
fn put(format: u32, bytes: &[u8]) -> Result<(), String> {
    unsafe {
        let global = GlobalAlloc(GMEM_MOVEABLE, bytes.len()).map_err(|error| error.to_string())?;
        let target = GlobalLock(global);
        if target.is_null() {
            let _ = GlobalFree(Some(global));
            return Err("could not lock clipboard memory".to_owned());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), target.cast::<u8>(), bytes.len());
        let _ = GlobalUnlock(global);
        SetClipboardData(format, Some(HANDLE(global.0))).map_err(|error| {
            let _ = GlobalFree(Some(HGLOBAL(global.0)));
            error.to_string()
        })?;
    }
    Ok(())
}

fn drop_files(paths: &[PathBuf]) -> Vec<u8> {
    let mut bytes = vec![0u8; DROPFILES_HEADER];
    bytes[0..4].copy_from_slice(&(DROPFILES_HEADER as u32).to_le_bytes());
    bytes[16..20].copy_from_slice(&1u32.to_le_bytes());
    for path in paths {
        for unit in path.as_os_str().encode_wide().chain(std::iter::once(0)) {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
    }
    bytes.extend_from_slice(&[0, 0]);
    bytes
}

pub fn set_files(paths: &[PathBuf], cut: bool) -> Result<(), String> {
    let _clipboard = Open::new()?;
    unsafe { EmptyClipboard() }.map_err(|error| error.to_string())?;
    put(CF_HDROP, &drop_files(paths))?;
    let effect = if cut { DROPEFFECT_MOVE } else { DROPEFFECT_COPY };
    let format = unsafe { RegisterClipboardFormatW(w!("Preferred DropEffect")) };
    put(format, &effect.to_le_bytes())
}

pub fn set_text(text: &str) -> Result<(), String> {
    let _clipboard = Open::new()?;
    unsafe { EmptyClipboard() }.map_err(|error| error.to_string())?;
    let bytes: Vec<u8> = text.encode_utf16().chain(std::iter::once(0)).flat_map(u16::to_le_bytes).collect();
    put(CF_UNICODETEXT, &bytes)
}

pub fn has_files() -> bool {
    unsafe { IsClipboardFormatAvailable(CF_HDROP) }.is_ok()
}

pub fn clear() {
    if let Ok(_clipboard) = Open::new() {
        let _ = unsafe { EmptyClipboard() };
    }
}

// The bool is true when the source asked for a move (cut).
pub fn get_files() -> Result<Option<(Vec<PathBuf>, bool)>, String> {
    if !has_files() {
        return Ok(None);
    }
    let _clipboard = Open::new()?;
    unsafe {
        let drop = GetClipboardData(CF_HDROP).map_err(|error| error.to_string())?;
        let drop = HDROP(drop.0);
        let count = DragQueryFileW(drop, u32::MAX, None);
        let mut paths = Vec::with_capacity(count as usize);
        for index in 0..count {
            let length = DragQueryFileW(drop, index, None) as usize;
            let mut buffer = vec![0u16; length + 1];
            let written = DragQueryFileW(drop, index, Some(&mut buffer)) as usize;
            paths.push(PathBuf::from(OsString::from_wide(&buffer[..written])));
        }
        let format = RegisterClipboardFormatW(w!("Preferred DropEffect"));
        let cut = GetClipboardData(format)
            .ok()
            .map(|handle| HGLOBAL(handle.0))
            .and_then(|global| {
                let data = GlobalLock(global).cast::<u32>();
                let effect = (!data.is_null()).then(|| data.read_unaligned());
                let _ = GlobalUnlock(global);
                effect
            })
            .is_some_and(|effect| effect & DROPEFFECT_MOVE != 0);
        Ok(Some((paths, cut)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_files_is_a_wide_double_nul_list() {
        let bytes = drop_files(&[PathBuf::from("C:/a"), PathBuf::from("C:/b")]);
        assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), 20);
        assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), 1);
        assert_eq!(bytes.len(), 20 + 2 * (4 + 1) * 2 + 2);
        assert_eq!(&bytes[bytes.len() - 4..], &[0, 0, 0, 0]);
    }

    #[test]
    #[ignore = "uses the real system clipboard"]
    fn files_round_trip_through_the_system_clipboard() {
        let paths = vec![PathBuf::from(r"C:\Windows\System32\notepad.exe")];
        set_files(&paths, true).unwrap();
        assert_eq!(get_files().unwrap(), Some((paths, true)));
    }
}
