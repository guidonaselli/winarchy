// Cuenta los toasts vivos leyendo la base del servicio de notificaciones: sin Explorer ni identidad de paquete
// UserNotificationListener no concede acceso. winsqlite3.dll viene con Windows; se carga en runtime.
// shortcut: solo cuenta (sin lista ni acciones), mejorar si se quiere un panel de notificaciones propio.

use std::ffi::{c_char, c_int, c_void, CString};
use windows::core::{w, PCSTR};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

const SQLITE_ROW: c_int = 100;
const OPEN_READONLY_URI: c_int = 0x01 | 0x40;
const UNIX_TO_FILETIME: u64 = 116_444_736_000_000_000;

type Open = unsafe extern "system" fn(*const c_char, *mut *mut c_void, c_int, *const c_char) -> c_int;
type Prepare = unsafe extern "system" fn(*mut c_void, *const c_char, c_int, *mut *mut c_void, *mut *const c_char) -> c_int;
type Step = unsafe extern "system" fn(*mut c_void) -> c_int;
type Column = unsafe extern "system" fn(*mut c_void, c_int) -> i64;
type Close = unsafe extern "system" fn(*mut c_void) -> c_int;

pub fn toast_count() -> Option<usize> {
    let local = std::env::var("LOCALAPPDATA").ok()?;
    let path = format!("{}/Microsoft/Windows/Notifications/wpndatabase.db", local.replace('\\', "/"));
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() * 10_000_000 + UNIX_TO_FILETIME;
    count_toasts(&format!("file:///{path}?mode=ro"), now)
}

fn count_toasts(uri: &str, now_filetime: u64) -> Option<usize> {
    unsafe {
        let lib = LoadLibraryW(w!("winsqlite3.dll")).ok()?;
        let sym = |name: &[u8]| GetProcAddress(lib, PCSTR(name.as_ptr()));
        let open: Open = std::mem::transmute(sym(b"sqlite3_open_v2\0")?);
        let prepare: Prepare = std::mem::transmute(sym(b"sqlite3_prepare_v2\0")?);
        let step: Step = std::mem::transmute(sym(b"sqlite3_step\0")?);
        let column: Column = std::mem::transmute(sym(b"sqlite3_column_int64\0")?);
        let finalize: Step = std::mem::transmute(sym(b"sqlite3_finalize\0")?);
        let close: Close = std::mem::transmute(sym(b"sqlite3_close\0")?);

        let uri = CString::new(uri).ok()?;
        let mut db = std::ptr::null_mut();
        if open(uri.as_ptr(), &mut db, OPEN_READONLY_URI, std::ptr::null()) != 0 {
            close(db);
            return None;
        }
        let sql = CString::new(format!("SELECT COUNT(*) FROM Notification WHERE Type='toast' AND (ExpiryTime=0 OR ExpiryTime>{now_filetime})")).ok()?;
        let mut stmt = std::ptr::null_mut();
        let count = if prepare(db, sql.as_ptr(), -1, &mut stmt, std::ptr::null_mut()) == 0 && step(stmt) == SQLITE_ROW {
            Some(column(stmt, 0) as usize)
        } else {
            None
        };
        finalize(stmt);
        close(db);
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_database_yields_none() {
        assert_eq!(count_toasts("file:///C:/definitely/not/here.db?mode=ro", 0), None);
    }

    #[test]
    #[ignore = "lee la base real del servicio de notificaciones de esta máquina"]
    fn reads_the_real_database() {
        assert!(toast_count().is_some());
    }
}
