// Lee la base del servicio de notificaciones (WNS): sin Explorer ni identidad de paquete UserNotificationListener
// no concede acceso y ShellExperienceHost no corre para dibujar toasts. winsqlite3.dll viene con Windows y se carga en runtime.
// shortcut: solo texto del toast (título y cuerpo), sin imágenes ni botones, mejorar si se quiere un centro de notificaciones completo.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use windows::core::{w, PCSTR};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

const SQLITE_ROW: c_int = 100;
const OPEN_READONLY_URI: c_int = 0x01 | 0x40;
const UNIX_TO_FILETIME: u64 = 116_444_736_000_000_000;
const POLL: std::time::Duration = std::time::Duration::from_secs(2);
const TOAST_TIMEOUT_MS: u32 = 7000;

#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub arrival: i64,
    pub title: String,
    pub text: String,
}

struct Sqlite {
    prepare: unsafe extern "system" fn(*mut c_void, *const c_char, c_int, *mut *mut c_void, *mut *const c_char) -> c_int,
    step: unsafe extern "system" fn(*mut c_void) -> c_int,
    int64: unsafe extern "system" fn(*mut c_void, c_int) -> i64,
    text: unsafe extern "system" fn(*mut c_void, c_int) -> *const c_char,
    finalize: unsafe extern "system" fn(*mut c_void) -> c_int,
    close: unsafe extern "system" fn(*mut c_void) -> c_int,
    db: *mut c_void,
}

impl Sqlite {
    unsafe fn open(uri: &str) -> Option<Sqlite> {
        let lib = LoadLibraryW(w!("winsqlite3.dll")).ok()?;
        let sym = |name: &[u8]| GetProcAddress(lib, PCSTR(name.as_ptr()));
        type Open = unsafe extern "system" fn(*const c_char, *mut *mut c_void, c_int, *const c_char) -> c_int;
        let open: Open = std::mem::transmute(sym(b"sqlite3_open_v2\0")?);
        let mut s = Sqlite {
            prepare: std::mem::transmute(sym(b"sqlite3_prepare_v2\0")?),
            step: std::mem::transmute(sym(b"sqlite3_step\0")?),
            int64: std::mem::transmute(sym(b"sqlite3_column_int64\0")?),
            text: std::mem::transmute(sym(b"sqlite3_column_text\0")?),
            finalize: std::mem::transmute(sym(b"sqlite3_finalize\0")?),
            close: std::mem::transmute(sym(b"sqlite3_close\0")?),
            db: std::ptr::null_mut(),
        };
        let uri = CString::new(uri).ok()?;
        if open(uri.as_ptr(), &mut s.db, OPEN_READONLY_URI, std::ptr::null()) != 0 {
            (s.close)(s.db);
            return None;
        }
        Some(s)
    }

    // Llama a `row` por cada fila con el statement posicionado en ella.
    unsafe fn each(&self, sql: &str, mut row: impl FnMut(*mut c_void)) -> Option<()> {
        let sql = CString::new(sql).ok()?;
        let mut stmt = std::ptr::null_mut();
        if (self.prepare)(self.db, sql.as_ptr(), -1, &mut stmt, std::ptr::null_mut()) != 0 {
            return None;
        }
        while (self.step)(stmt) == SQLITE_ROW {
            row(stmt);
        }
        (self.finalize)(stmt);
        Some(())
    }
}

impl Drop for Sqlite {
    fn drop(&mut self) {
        unsafe { (self.close)(self.db) };
    }
}

fn database_uri() -> Option<String> {
    let local = std::env::var("LOCALAPPDATA").ok()?;
    Some(format!("file:///{}/Microsoft/Windows/Notifications/wpndatabase.db?mode=ro", local.replace('\\', "/")))
}

fn now_filetime() -> Option<u64> {
    Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() * 10_000_000 + UNIX_TO_FILETIME)
}

pub fn toast_count() -> Option<usize> {
    count_toasts(&database_uri()?, now_filetime()?)
}

fn count_toasts(uri: &str, now: u64) -> Option<usize> {
    unsafe {
        let db = Sqlite::open(uri)?;
        let mut count = None;
        db.each(&format!("SELECT COUNT(*) FROM Notification WHERE Type='toast' AND (ExpiryTime=0 OR ExpiryTime>{now})"), |s| count = Some((db.int64)(s, 0) as usize))?;
        count
    }
}

fn toasts_after(uri: &str, arrival: i64) -> Option<Vec<Toast>> {
    unsafe {
        let db = Sqlite::open(uri)?;
        let mut found = Vec::new();
        db.each(&format!("SELECT ArrivalTime, CAST(Payload AS TEXT) FROM Notification WHERE Type='toast' AND ArrivalTime>{arrival} ORDER BY ArrivalTime"), |s| {
            let p = (db.text)(s, 1);
            let payload = if p.is_null() { String::new() } else { CStr::from_ptr(p).to_string_lossy().into_owned() };
            let (title, text) = toast_texts(&payload);
            found.push(Toast { arrival: (db.int64)(s, 0), title, text });
        })?;
        Some(found)
    }
}

// Primer y segundo <text> del binding del toast: título y cuerpo.
fn toast_texts(xml: &str) -> (String, String) {
    let mut texts = Vec::new();
    let mut rest = xml;
    while let Some(open) = rest.find("<text") {
        rest = &rest[open + 5..];
        let Some(gt) = rest.find('>') else { break };
        if rest[..gt].ends_with('/') {
            continue;
        }
        let body = &rest[gt + 1..];
        let Some(end) = body.find("</text>") else { break };
        texts.push(decode(body[..end].trim()));
        rest = &body[end..];
    }
    let mut it = texts.into_iter().filter(|t| !t.is_empty());
    let title = it.next().unwrap_or_default();
    (title, it.collect::<Vec<_>>().join("\n"))
}

fn decode(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

// Muestra los toasts nuevos con el popup propio de la bandeja. Parte de la última llegada para no repetir los viejos.
pub fn start_relay() {
    let Some(uri) = database_uri() else { return };
    std::thread::spawn(move || {
        let mut last = toasts_after(&uri, 0).and_then(|t| t.last().map(|t| t.arrival)).unwrap_or(0);
        loop {
            std::thread::sleep(POLL);
            let Some(new) = toasts_after(&uri, last) else { continue };
            for toast in new {
                last = last.max(toast.arrival);
                crate::tray::submit_toast(toast.title, toast.text, TOAST_TIMEOUT_MS);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_database_yields_none() {
        assert_eq!(count_toasts("file:///C:/definitely/not/here.db?mode=ro", 0), None);
        assert_eq!(toasts_after("file:///C:/definitely/not/here.db?mode=ro", 0), None);
    }

    #[test]
    fn toast_title_and_body_come_from_the_generic_binding() {
        let xml = r#"<toast launch="x"><visual><binding template="ToastGeneric"><text>GeForce &amp; Co</text><text hint-maxLines="2">Update available</text><image src="C:\a.png"/></binding></visual></toast>"#;
        assert_eq!(toast_texts(xml), ("GeForce & Co".to_string(), "Update available".to_string()));
    }

    #[test]
    fn a_toast_without_text_is_empty() {
        assert_eq!(toast_texts("<badge value=\"1\"/>"), (String::new(), String::new()));
    }

    #[test]
    #[ignore = "lee la base real del servicio de notificaciones de esta máquina"]
    fn reads_the_real_database() {
        assert!(toast_count().is_some());
        assert!(toasts_after(&database_uri().unwrap(), 0).is_some());
    }
}
